//! Durable USN Journal cursor and delta protocol shared by the privileged
//! MFT service and the unprivileged Host reader.

#![cfg(windows)]

use std::{
    collections::HashMap,
    ffi::c_void,
    fs::{self, File},
    io::{Read as _, Write as _},
    mem::size_of,
    os::windows::ffi::OsStrExt as _,
    path::{Path, PathBuf},
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_IO_PENDING, HANDLE},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{
            IO::{
                CancelIoEx, DeviceIoControl, GetOverlappedResult, GetOverlappedResultEx, OVERLAPPED,
            },
            Ioctl::{
                FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V0,
                USN_JOURNAL_DATA_V0, USN_REASON_BASIC_INFO_CHANGE, USN_REASON_DATA_EXTEND,
                USN_REASON_DATA_OVERWRITE, USN_REASON_DATA_TRUNCATION, USN_REASON_EA_CHANGE,
                USN_REASON_FILE_CREATE, USN_REASON_FILE_DELETE, USN_REASON_HARD_LINK_CHANGE,
                USN_REASON_RENAME_NEW_NAME, USN_REASON_RENAME_OLD_NAME,
                USN_REASON_REPARSE_POINT_CHANGE, USN_REASON_SECURITY_CHANGE,
            },
            Threading::CreateEventW,
        },
    },
    core::PCWSTR,
};

const CHECKPOINT_MAGIC: &[u8; 8] = b"SEMFTCP2";
const DELTA_MAGIC: &[u8; 8] = b"SEMFTDL2";
const STATUS_MAGIC: &[u8; 8] = b"SEMFTST2";
const COMMIT_MARKER: u64 = 0x5345_4D46_5443_4D54;
const SCHEMA_V2: u32 = 2;
const MAX_RECORD_BYTES: usize = 64 * 1024 * 1024;
const MAX_CHANGES: usize = 100_000;
const MAX_NAME_BYTES: usize = 64 * 1024;
pub(crate) const JOURNAL_BUFFER_BYTES: usize = 1024 * 1024;
pub(crate) const PENDING_CHANGE_LIMIT: usize = 100_000;
pub(crate) const PENDING_BYTE_LIMIT: usize = 16 * 1024 * 1024;

pub(crate) const RELEVANT_REASON_MASK: u32 = USN_REASON_DATA_OVERWRITE
    | USN_REASON_DATA_EXTEND
    | USN_REASON_DATA_TRUNCATION
    | USN_REASON_FILE_CREATE
    | USN_REASON_FILE_DELETE
    | USN_REASON_RENAME_OLD_NAME
    | USN_REASON_RENAME_NEW_NAME
    | USN_REASON_HARD_LINK_CHANGE
    | USN_REASON_BASIC_INFO_CHANGE
    | USN_REASON_EA_CHANGE
    | USN_REASON_SECURITY_CHANGE
    | USN_REASON_REPARSE_POINT_CHANGE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VolumeIdentityV2 {
    pub(crate) serial: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct JournalMetadataV2 {
    pub(crate) journal_id: u64,
    pub(crate) first_usn: i64,
    pub(crate) next_usn: i64,
    pub(crate) lowest_valid_usn: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MftCheckpointV2 {
    pub(crate) schema: u32,
    pub(crate) volume: VolumeIdentityV2,
    pub(crate) journal_id: u64,
    pub(crate) next_usn: i64,
    pub(crate) generation: u64,
}

impl MftCheckpointV2 {
    pub(crate) fn new(
        volume: VolumeIdentityV2,
        journal_id: u64,
        next_usn: i64,
        generation: u64,
    ) -> Self {
        Self {
            schema: SCHEMA_V2,
            volume,
            journal_id,
            next_usn,
            generation,
        }
    }

    pub(crate) fn compatible_with(
        self,
        volume: VolumeIdentityV2,
        journal: JournalMetadataV2,
    ) -> bool {
        self.schema == SCHEMA_V2
            && self.volume == volume
            && self.journal_id == journal.journal_id
            && self.next_usn >= journal.first_usn.max(journal.lowest_valid_usn)
            && self.next_usn <= journal.next_usn
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum MftChangeKindV2 {
    Upsert = 1,
    Delete = 2,
    Invalidate = 3,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MftChangeV2 {
    pub(crate) kind: MftChangeKindV2,
    pub(crate) reference: u64,
    pub(crate) parent_reference: u64,
    pub(crate) name: String,
    pub(crate) logical_bytes: u64,
    pub(crate) allocated_bytes: u64,
    pub(crate) is_directory: bool,
    pub(crate) reason: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MftDeltaV2 {
    pub(crate) schema: u32,
    pub(crate) volume: VolumeIdentityV2,
    pub(crate) journal_id: u64,
    pub(crate) generation: u64,
    pub(crate) start_usn: i64,
    pub(crate) next_usn: i64,
    pub(crate) changes: Vec<MftChangeV2>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum MftServiceModeV2 {
    Initializing = 1,
    Journal = 2,
    Recovering = 3,
    Error = 4,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MftServiceStatusV2 {
    pub(crate) mode: MftServiceModeV2,
    pub(crate) generation: u64,
    pub(crate) journal_id: u64,
    pub(crate) committed_usn: i64,
    pub(crate) pending_count: u64,
    pub(crate) pending_bytes: u64,
    pub(crate) queue_high_water: u64,
    pub(crate) published_unix_ms: u64,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UsnEventV2 {
    pub(crate) reference: u64,
    pub(crate) parent_reference: u64,
    pub(crate) usn: i64,
    pub(crate) reason: u32,
    pub(crate) attributes: u32,
    pub(crate) name: String,
}

struct HandleGuard(HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        // SAFETY: this guard exclusively owns the handle.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn open_volume(root: &Path, overlapped: bool) -> Result<HandleGuard, String> {
    let device = crate::mft_size_map::volume_device_path(root)?;
    let wide = std::ffi::OsStr::new(&device)
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    // SAFETY: the UTF-16 string is terminated and alive for the call.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            if overlapped {
                FILE_FLAG_OVERLAPPED
            } else {
                Default::default()
            },
            None,
        )
    }
    .map_err(|error| error.to_string())?;
    Ok(HandleGuard(handle))
}

pub(crate) fn query_journal(root: &Path) -> Result<JournalMetadataV2, String> {
    let handle = open_volume(root, false)?;
    let mut data = USN_JOURNAL_DATA_V0::default();
    let mut returned = 0_u32;
    // SAFETY: `data` is valid writable storage and the synchronous call has no OVERLAPPED pointer.
    unsafe {
        DeviceIoControl(
            handle.0,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some((&raw mut data).cast::<c_void>()),
            size_of::<USN_JOURNAL_DATA_V0>() as u32,
            Some(&mut returned),
            None,
        )
    }
    .map_err(|error| error.to_string())?;
    if returned < size_of::<USN_JOURNAL_DATA_V0>() as u32 {
        return Err("USN journal metadata is truncated".to_owned());
    }
    Ok(JournalMetadataV2 {
        journal_id: data.UsnJournalID,
        first_usn: data.FirstUsn,
        next_usn: data.NextUsn,
        lowest_valid_usn: data.LowestValidUsn,
    })
}

pub(crate) fn read_journal_once(
    root: &Path,
    checkpoint: MftCheckpointV2,
) -> Result<(i64, Vec<UsnEventV2>), String> {
    let handle = open_volume(root, true)?;
    let input = READ_USN_JOURNAL_DATA_V0 {
        StartUsn: checkpoint.next_usn,
        ReasonMask: RELEVANT_REASON_MASK,
        ReturnOnlyOnClose: 1,
        Timeout: 0,
        BytesToWaitFor: 1,
        UsnJournalID: checkpoint.journal_id,
    };
    let mut output = vec![0_u8; JOURNAL_BUFFER_BYTES];
    let mut returned = 0_u32;
    let event = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
        .map_err(|error| error.to_string())?;
    let event = HandleGuard(event);
    let mut overlapped = OVERLAPPED::default();
    overlapped.hEvent = event.0;
    // SAFETY: all buffers and OVERLAPPED storage remain alive until completion or cancellation.
    let submitted = unsafe {
        DeviceIoControl(
            handle.0,
            FSCTL_READ_USN_JOURNAL,
            Some((&raw const input).cast::<c_void>()),
            size_of::<READ_USN_JOURNAL_DATA_V0>() as u32,
            Some(output.as_mut_ptr().cast::<c_void>()),
            output.len() as u32,
            Some(&mut returned),
            Some(&raw mut overlapped),
        )
    };
    if let Err(error) = submitted
        && error.code() != ERROR_IO_PENDING.to_hresult()
    {
        return Err(error.to_string());
    }
    let completed = unsafe {
        GetOverlappedResultEx(
            handle.0,
            &raw const overlapped,
            &raw mut returned,
            1000,
            false,
        )
    };
    if completed.is_err() {
        // SAFETY: this function owns the handle and OVERLAPPED and drains cancellation before return.
        let _ = unsafe { CancelIoEx(handle.0, Some(&raw const overlapped)) };
        let _ = unsafe {
            GetOverlappedResult(handle.0, &raw const overlapped, &raw mut returned, true)
        };
        return Ok((checkpoint.next_usn, Vec::new()));
    }
    let returned = returned as usize;
    if returned < 8 {
        return Ok((checkpoint.next_usn, Vec::new()));
    }
    let next_usn = read_i64_at(&output, 0).ok_or("USN response has no cursor")?;
    let mut events = Vec::new();
    let mut offset = 8_usize;
    while offset.saturating_add(60) <= returned {
        let record_length = read_u32_at(&output, offset).unwrap_or_default() as usize;
        if record_length < 60 || offset.saturating_add(record_length) > returned {
            return Err("USN response contains an invalid record length".to_owned());
        }
        let major = read_u16_at(&output, offset + 4).unwrap_or_default();
        if major == 2 {
            let name_length = read_u16_at(&output, offset + 56).unwrap_or_default() as usize;
            let name_offset = read_u16_at(&output, offset + 58).unwrap_or_default() as usize;
            let start = offset.saturating_add(name_offset);
            let end = start.saturating_add(name_length);
            if name_length <= MAX_NAME_BYTES
                && name_length.is_multiple_of(2)
                && end <= offset + record_length
            {
                let name_utf16 = output[start..end]
                    .chunks_exact(2)
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .collect::<Vec<_>>();
                events.push(UsnEventV2 {
                    // Preserve the NTFS sequence bits for journal coalescing;
                    // record-number reuse must not merge an old delete with a
                    // later create. Consumers normalize only when addressing
                    // the current MFT index.
                    reference: read_u64_at(&output, offset + 8).unwrap_or_default(),
                    parent_reference: read_u64_at(&output, offset + 16).unwrap_or_default(),
                    usn: read_i64_at(&output, offset + 24).unwrap_or_default(),
                    reason: read_u32_at(&output, offset + 40).unwrap_or_default(),
                    attributes: read_u32_at(&output, offset + 52).unwrap_or_default(),
                    name: String::from_utf16_lossy(&name_utf16),
                });
            }
        }
        offset += record_length;
    }
    Ok((next_usn, events))
}

pub(crate) fn normalize_event(event: &UsnEventV2) -> MftChangeKindV2 {
    if event.reason & USN_REASON_FILE_DELETE != 0 {
        MftChangeKindV2::Delete
    } else if event.reason
        & (USN_REASON_FILE_CREATE
            | USN_REASON_RENAME_OLD_NAME
            | USN_REASON_RENAME_NEW_NAME
            | USN_REASON_DATA_OVERWRITE
            | USN_REASON_DATA_EXTEND
            | USN_REASON_DATA_TRUNCATION)
        != 0
    {
        MftChangeKindV2::Upsert
    } else if event.reason
        & (USN_REASON_HARD_LINK_CHANGE
            | USN_REASON_REPARSE_POINT_CHANGE
            | USN_REASON_BASIC_INFO_CHANGE
            | USN_REASON_EA_CHANGE
            | USN_REASON_SECURITY_CHANGE)
        != 0
    {
        // The folder-size index counts one materialized MFT record. Resolve its
        // current parent/name/sizes instead of rebuilding an entire volume for a
        // routine hard-link or reparse metadata transition.
        MftChangeKindV2::Upsert
    } else {
        MftChangeKindV2::Invalidate
    }
}

pub(crate) fn coalesce_events(
    events: impl IntoIterator<Item = UsnEventV2>,
) -> HashMap<u64, UsnEventV2> {
    let mut pending = HashMap::new();
    for event in events {
        pending
            .entry(event.reference)
            .and_modify(|current: &mut UsnEventV2| {
                current.reason |= event.reason;
                current.parent_reference = event.parent_reference;
                current.usn = current.usn.max(event.usn);
                current.attributes = event.attributes;
                if !event.name.is_empty() {
                    current.name.clone_from(&event.name);
                }
            })
            .or_insert(event);
    }
    pending
}

pub(crate) fn coalesced_bytes(pending: &HashMap<u64, UsnEventV2>) -> usize {
    pending
        .values()
        .map(|event| event.name.len().saturating_add(64))
        .sum()
}

pub(crate) fn publication_due(
    first_age: std::time::Duration,
    quiet_age: std::time::Duration,
) -> bool {
    quiet_age >= std::time::Duration::from_secs(5)
        // Leave one second of headroom for the overlapped journal-read cadence so
        // the externally observed mutation-to-checkpoint latency stays <= 10 s.
        || first_age >= std::time::Duration::from_secs(9)
}

pub(crate) fn checkpoint_path(cache: &Path, letter: char, generation: u64) -> PathBuf {
    cache.join(format!("{letter}.{generation:020}.semftcp"))
}

pub(crate) fn delta_path(cache: &Path, letter: char, generation: u64) -> PathBuf {
    cache.join(format!("{letter}.{generation:020}.semftdelta"))
}

pub(crate) fn status_path(cache: &Path, letter: char) -> PathBuf {
    cache.join(format!("{letter}.semftstatus"))
}

pub(crate) fn latest_checkpoint(
    cache: &Path,
    letter: char,
) -> Result<Option<MftCheckpointV2>, String> {
    let prefix = format!("{letter}.");
    let mut candidates = fs::read_dir(cache)
        .map_err(|error| error.to_string())?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name.starts_with(&prefix) && name.ends_with(".semftcp")).then_some(entry.path())
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    for path in candidates.into_iter().rev() {
        if let Ok(checkpoint) = read_checkpoint(&path) {
            return Ok(Some(checkpoint));
        }
    }
    Ok(None)
}

pub(crate) fn publish_delta_and_checkpoint(
    cache: &Path,
    letter: char,
    delta: &MftDeltaV2,
    checkpoint: &MftCheckpointV2,
) -> Result<(), String> {
    if delta.generation != checkpoint.generation || delta.next_usn != checkpoint.next_usn {
        return Err("delta/checkpoint commit boundary mismatch".to_owned());
    }
    atomic_create(
        &delta_path(cache, letter, delta.generation),
        &encode_delta(delta)?,
    )?;
    atomic_create(
        &checkpoint_path(cache, letter, checkpoint.generation),
        &encode_checkpoint(checkpoint),
    )
}

pub(crate) fn publish_initial_checkpoint(
    cache: &Path,
    letter: char,
    checkpoint: &MftCheckpointV2,
) -> Result<(), String> {
    atomic_create(
        &checkpoint_path(cache, letter, checkpoint.generation),
        &encode_checkpoint(checkpoint),
    )
}

pub(crate) fn write_status(
    cache: &Path,
    letter: char,
    status: &MftServiceStatusV2,
) -> Result<(), String> {
    atomic_replace(&status_path(cache, letter), &encode_status(status)?)
}

pub(crate) fn read_checkpoint(path: &Path) -> Result<MftCheckpointV2, String> {
    let bytes = read_bounded(path)?;
    decode_checkpoint(&bytes)
}

pub(crate) fn read_delta(path: &Path) -> Result<MftDeltaV2, String> {
    let bytes = read_bounded(path)?;
    decode_delta(&bytes)
}

pub(crate) fn read_status(path: &Path) -> Result<MftServiceStatusV2, String> {
    let bytes = read_bounded(path)?;
    decode_status(&bytes)
}

pub(crate) fn deltas_after(
    cache: &Path,
    letter: char,
    generation: u64,
    through_generation: u64,
) -> Result<Vec<MftDeltaV2>, String> {
    let mut deltas = Vec::new();
    for current in generation.saturating_add(1)..=through_generation {
        deltas.push(read_delta(&delta_path(cache, letter, current))?);
    }
    Ok(deltas)
}

pub(crate) fn remove_volume_sidecars(cache: &Path, letter: char) -> Result<(), String> {
    let prefix = format!("{letter}.");
    for entry in fs::read_dir(cache)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let sidecar = name.starts_with(&prefix)
            && (name.ends_with(".semftcp") || name.ends_with(".semftdelta"));
        if sidecar {
            fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn atomic_create(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.exists() {
        let existing = read_bounded(path)?;
        return (existing == bytes).then_some(()).ok_or_else(|| {
            "immutable MFT generation already exists with different bytes".to_owned()
        });
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    write_synced(&temporary, bytes)?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) if path.exists() => {
            let _ = fs::remove_file(&temporary);
            let existing = read_bounded(path)?;
            (existing == bytes)
                .then_some(())
                .ok_or_else(|| format!("immutable MFT generation collision: {error}"))
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error.to_string())
        }
    }
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    write_synced(&temporary, bytes)?;
    let _ = fs::remove_file(path);
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::create(path).map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_RECORD_BYTES as u64 {
        return Err("MFT journal record exceeds the safety limit".to_owned());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_RECORD_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err("MFT journal record exceeds the safety limit".to_owned());
    }
    Ok(bytes)
}

fn encode_checkpoint(checkpoint: &MftCheckpointV2) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(CHECKPOINT_MAGIC);
    push_u32(&mut bytes, checkpoint.schema);
    push_u64(&mut bytes, checkpoint.volume.serial);
    push_u64(&mut bytes, checkpoint.journal_id);
    push_i64(&mut bytes, checkpoint.next_usn);
    push_u64(&mut bytes, checkpoint.generation);
    finish_record(bytes)
}

fn decode_checkpoint(bytes: &[u8]) -> Result<MftCheckpointV2, String> {
    let payload = verify_record(bytes, CHECKPOINT_MAGIC)?;
    let mut cursor = Cursor::new(payload, CHECKPOINT_MAGIC.len());
    let checkpoint = MftCheckpointV2 {
        schema: cursor.u32()?,
        volume: VolumeIdentityV2 {
            serial: cursor.u64()?,
        },
        journal_id: cursor.u64()?,
        next_usn: cursor.i64()?,
        generation: cursor.u64()?,
    };
    if checkpoint.schema != SCHEMA_V2 || !cursor.finished() {
        return Err("unsupported or malformed MFT checkpoint".to_owned());
    }
    Ok(checkpoint)
}

fn encode_delta(delta: &MftDeltaV2) -> Result<Vec<u8>, String> {
    if delta.changes.len() > MAX_CHANGES {
        return Err("MFT delta exceeds the change-count limit".to_owned());
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(DELTA_MAGIC);
    push_u32(&mut bytes, delta.schema);
    push_u64(&mut bytes, delta.volume.serial);
    push_u64(&mut bytes, delta.journal_id);
    push_u64(&mut bytes, delta.generation);
    push_i64(&mut bytes, delta.start_usn);
    push_i64(&mut bytes, delta.next_usn);
    push_u32(&mut bytes, delta.changes.len() as u32);
    for change in &delta.changes {
        let name = change.name.as_bytes();
        if name.len() > MAX_NAME_BYTES {
            return Err("MFT delta name exceeds the safety limit".to_owned());
        }
        bytes.push(change.kind as u8);
        bytes.push(u8::from(change.is_directory));
        bytes.extend_from_slice(&[0, 0]);
        push_u32(&mut bytes, change.reason);
        push_u64(&mut bytes, change.reference);
        push_u64(&mut bytes, change.parent_reference);
        push_u64(&mut bytes, change.logical_bytes);
        push_u64(&mut bytes, change.allocated_bytes);
        push_u32(&mut bytes, name.len() as u32);
        bytes.extend_from_slice(name);
    }
    if bytes.len() > MAX_RECORD_BYTES.saturating_sub(16) {
        return Err("MFT delta exceeds the byte limit".to_owned());
    }
    Ok(finish_record(bytes))
}

fn decode_delta(bytes: &[u8]) -> Result<MftDeltaV2, String> {
    let payload = verify_record(bytes, DELTA_MAGIC)?;
    let mut cursor = Cursor::new(payload, DELTA_MAGIC.len());
    let schema = cursor.u32()?;
    if schema != SCHEMA_V2 {
        return Err("unsupported MFT delta schema".to_owned());
    }
    let volume = VolumeIdentityV2 {
        serial: cursor.u64()?,
    };
    let journal_id = cursor.u64()?;
    let generation = cursor.u64()?;
    let start_usn = cursor.i64()?;
    let next_usn = cursor.i64()?;
    let count = cursor.u32()? as usize;
    if count > MAX_CHANGES {
        return Err("MFT delta exceeds the change-count limit".to_owned());
    }
    let mut changes = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = match cursor.u8()? {
            1 => MftChangeKindV2::Upsert,
            2 => MftChangeKindV2::Delete,
            3 => MftChangeKindV2::Invalidate,
            _ => return Err("MFT delta has an invalid change kind".to_owned()),
        };
        let is_directory = cursor.u8()? != 0;
        cursor.skip(2)?;
        let reason = cursor.u32()?;
        let reference = cursor.u64()?;
        let parent_reference = cursor.u64()?;
        let logical_bytes = cursor.u64()?;
        let allocated_bytes = cursor.u64()?;
        let name_length = cursor.u32()? as usize;
        if name_length > MAX_NAME_BYTES {
            return Err("MFT delta name exceeds the safety limit".to_owned());
        }
        let name = std::str::from_utf8(cursor.bytes(name_length)?)
            .map_err(|_| "MFT delta name is not UTF-8".to_owned())?
            .to_owned();
        changes.push(MftChangeV2 {
            kind,
            reference,
            parent_reference,
            name,
            logical_bytes,
            allocated_bytes,
            is_directory,
            reason,
        });
    }
    if !cursor.finished() || next_usn < start_usn {
        return Err("MFT delta boundary is malformed".to_owned());
    }
    Ok(MftDeltaV2 {
        schema,
        volume,
        journal_id,
        generation,
        start_usn,
        next_usn,
        changes,
    })
}

fn encode_status(status: &MftServiceStatusV2) -> Result<Vec<u8>, String> {
    if status.reason.len() > MAX_NAME_BYTES {
        return Err("MFT status reason exceeds the safety limit".to_owned());
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(STATUS_MAGIC);
    push_u32(&mut bytes, SCHEMA_V2);
    bytes.push(status.mode as u8);
    bytes.extend_from_slice(&[0, 0, 0]);
    push_u64(&mut bytes, status.generation);
    push_u64(&mut bytes, status.journal_id);
    push_i64(&mut bytes, status.committed_usn);
    push_u64(&mut bytes, status.pending_count);
    push_u64(&mut bytes, status.pending_bytes);
    push_u64(&mut bytes, status.queue_high_water);
    push_u64(&mut bytes, status.published_unix_ms);
    push_u32(&mut bytes, status.reason.len() as u32);
    bytes.extend_from_slice(status.reason.as_bytes());
    Ok(finish_record(bytes))
}

fn decode_status(bytes: &[u8]) -> Result<MftServiceStatusV2, String> {
    let payload = verify_record(bytes, STATUS_MAGIC)?;
    let mut cursor = Cursor::new(payload, STATUS_MAGIC.len());
    if cursor.u32()? != SCHEMA_V2 {
        return Err("unsupported MFT status schema".to_owned());
    }
    let mode = match cursor.u8()? {
        1 => MftServiceModeV2::Initializing,
        2 => MftServiceModeV2::Journal,
        3 => MftServiceModeV2::Recovering,
        4 => MftServiceModeV2::Error,
        _ => return Err("MFT status has an invalid mode".to_owned()),
    };
    cursor.skip(3)?;
    let generation = cursor.u64()?;
    let journal_id = cursor.u64()?;
    let committed_usn = cursor.i64()?;
    let pending_count = cursor.u64()?;
    let pending_bytes = cursor.u64()?;
    let queue_high_water = cursor.u64()?;
    let published_unix_ms = cursor.u64()?;
    let reason_length = cursor.u32()? as usize;
    if reason_length > MAX_NAME_BYTES {
        return Err("MFT status reason exceeds the safety limit".to_owned());
    }
    let reason = std::str::from_utf8(cursor.bytes(reason_length)?)
        .map_err(|_| "MFT status reason is not UTF-8".to_owned())?
        .to_owned();
    if !cursor.finished() {
        return Err("MFT status has trailing bytes".to_owned());
    }
    Ok(MftServiceStatusV2 {
        mode,
        generation,
        journal_id,
        committed_usn,
        pending_count,
        pending_bytes,
        queue_high_water,
        published_unix_ms,
        reason,
    })
}

fn finish_record(mut bytes: Vec<u8>) -> Vec<u8> {
    let checksum = checksum(&bytes);
    push_u64(&mut bytes, checksum);
    push_u64(&mut bytes, COMMIT_MARKER);
    bytes
}

fn verify_record<'a>(bytes: &'a [u8], magic: &[u8; 8]) -> Result<&'a [u8], String> {
    if bytes.len() < magic.len() + 16 || &bytes[..magic.len()] != magic {
        return Err("MFT journal record has an invalid header".to_owned());
    }
    let marker_offset = bytes.len() - 8;
    let checksum_offset = marker_offset - 8;
    if read_u64_at(bytes, marker_offset) != Some(COMMIT_MARKER) {
        return Err("MFT journal record is not committed".to_owned());
    }
    let expected = read_u64_at(bytes, checksum_offset).ok_or("MFT journal checksum missing")?;
    if checksum(&bytes[..checksum_offset]) != expected {
        return Err("MFT journal record checksum mismatch".to_owned());
    }
    Ok(&bytes[..checksum_offset])
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

pub(crate) fn normalize_reference(reference: u64) -> u64 {
    reference & 0x0000_FFFF_FFFF_FFFF
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_i64(bytes: &mut Vec<u8>, value: i64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn read_u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn read_i64_at(bytes: &[u8], offset: usize) -> Option<i64> {
    Some(i64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], offset: usize) -> Self {
        Self { bytes, offset }
    }

    fn bytes(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self.offset.saturating_add(length);
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| "MFT journal record is truncated".to_owned())?;
        self.offset = end;
        Ok(result)
    }

    fn skip(&mut self, length: usize) -> Result<(), String> {
        let _ = self.bytes(length)?;
        Ok(())
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.bytes(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?
                .try_into()
                .map_err(|_| "invalid u32".to_owned())?,
        ))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.bytes(8)?
                .try_into()
                .map_err(|_| "invalid u64".to_owned())?,
        ))
    }

    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(
            self.bytes(8)?
                .try_into()
                .map_err(|_| "invalid i64".to_owned())?,
        ))
    }

    fn finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_and_delta_round_trip_and_reject_corruption() {
        let checkpoint = MftCheckpointV2::new(VolumeIdentityV2 { serial: 7 }, 11, 101, 3);
        let bytes = encode_checkpoint(&checkpoint);
        assert_eq!(decode_checkpoint(&bytes).unwrap(), checkpoint);
        let mut corrupt = bytes;
        corrupt[12] ^= 1;
        assert!(decode_checkpoint(&corrupt).is_err());

        let delta = MftDeltaV2 {
            schema: SCHEMA_V2,
            volume: checkpoint.volume,
            journal_id: checkpoint.journal_id,
            generation: checkpoint.generation,
            start_usn: 90,
            next_usn: checkpoint.next_usn,
            changes: vec![MftChangeV2 {
                kind: MftChangeKindV2::Upsert,
                reference: 2,
                parent_reference: 1,
                name: "file.txt".to_owned(),
                logical_bytes: 12,
                allocated_bytes: 4096,
                is_directory: false,
                reason: USN_REASON_DATA_EXTEND,
            }],
        };
        assert_eq!(decode_delta(&encode_delta(&delta).unwrap()).unwrap(), delta);

        let status = MftServiceStatusV2 {
            mode: MftServiceModeV2::Journal,
            generation: 3,
            journal_id: 11,
            committed_usn: 101,
            pending_count: 2,
            pending_bytes: 80,
            queue_high_water: 9,
            published_unix_ms: 12,
            reason: String::new(),
        };
        assert_eq!(
            decode_status(&encode_status(&status).unwrap()).unwrap(),
            status
        );
    }

    #[test]
    fn immutable_delta_is_visible_before_its_checkpoint() {
        let temporary = tempfile::tempdir().unwrap();
        let volume = VolumeIdentityV2 { serial: 7 };
        let delta = MftDeltaV2 {
            schema: SCHEMA_V2,
            volume,
            journal_id: 11,
            generation: 1,
            start_usn: 20,
            next_usn: 30,
            changes: Vec::new(),
        };
        let checkpoint = MftCheckpointV2::new(volume, 11, 30, 1);
        publish_delta_and_checkpoint(temporary.path(), 'C', &delta, &checkpoint).unwrap();
        assert_eq!(
            read_delta(&delta_path(temporary.path(), 'C', 1)).unwrap(),
            delta
        );
        assert_eq!(
            latest_checkpoint(temporary.path(), 'C').unwrap(),
            Some(checkpoint)
        );

        let mismatch = MftCheckpointV2::new(volume, 11, 31, 1);
        assert!(publish_delta_and_checkpoint(temporary.path(), 'D', &delta, &mismatch).is_err());
        assert!(!checkpoint_path(temporary.path(), 'D', 1).exists());
    }

    #[test]
    fn cursor_compatibility_rejects_wrong_journal_and_retained_range() {
        let volume = VolumeIdentityV2 { serial: 7 };
        let metadata = JournalMetadataV2 {
            journal_id: 11,
            first_usn: 20,
            next_usn: 100,
            lowest_valid_usn: 25,
        };
        assert!(MftCheckpointV2::new(volume, 11, 25, 0).compatible_with(volume, metadata));
        assert!(!MftCheckpointV2::new(volume, 12, 25, 0).compatible_with(volume, metadata));
        assert!(!MftCheckpointV2::new(volume, 11, 24, 0).compatible_with(volume, metadata));
    }

    #[test]
    fn coalescing_retains_final_topology_and_combines_reasons() {
        let first = UsnEventV2 {
            reference: 2,
            parent_reference: 1,
            usn: 10,
            reason: USN_REASON_RENAME_OLD_NAME,
            attributes: 0,
            name: "old".to_owned(),
        };
        let second = UsnEventV2 {
            reference: 2,
            parent_reference: 3,
            usn: 11,
            reason: USN_REASON_RENAME_NEW_NAME,
            attributes: 0,
            name: "new".to_owned(),
        };
        let pending = coalesce_events([first, second]);
        let event = &pending[&2];
        assert_eq!(event.parent_reference, 3);
        assert_eq!(event.name, "new");
        assert_eq!(
            event.reason & (USN_REASON_RENAME_OLD_NAME | USN_REASON_RENAME_NEW_NAME),
            USN_REASON_RENAME_OLD_NAME | USN_REASON_RENAME_NEW_NAME
        );
    }

    #[test]
    fn decisive_change_reason_wins_over_ambiguous_companion_flags() {
        let event = UsnEventV2 {
            reference: 7,
            parent_reference: 5,
            usn: 11,
            reason: USN_REASON_FILE_CREATE | USN_REASON_DATA_EXTEND | USN_REASON_HARD_LINK_CHANGE,
            attributes: 0,
            name: "created.bin".to_owned(),
        };
        assert_eq!(normalize_event(&event), MftChangeKindV2::Upsert);

        let hard_link = UsnEventV2 {
            reason: USN_REASON_HARD_LINK_CHANGE,
            ..event
        };
        assert_eq!(normalize_event(&hard_link), MftChangeKindV2::Upsert);

        let unknown = UsnEventV2 {
            reason: 0,
            ..hard_link
        };
        assert_eq!(normalize_event(&unknown), MftChangeKindV2::Invalidate);
    }

    #[test]
    fn coalesced_queue_accounting_is_bounded_by_unique_references() {
        let event = UsnEventV2 {
            reference: 7,
            parent_reference: 5,
            usn: 1,
            reason: USN_REASON_DATA_EXTEND,
            attributes: 0,
            name: "hot.bin".to_owned(),
        };
        let pending = coalesce_events(std::iter::repeat_n(event, 100_000));
        assert_eq!(pending.len(), 1);
        assert_eq!(coalesced_bytes(&pending), 71);
    }

    #[test]
    fn publication_uses_quiet_debounce_with_a_maximum_deadline() {
        use std::time::Duration;
        assert!(!publication_due(
            Duration::from_secs(4),
            Duration::from_secs(4)
        ));
        assert!(publication_due(
            Duration::from_secs(6),
            Duration::from_secs(5)
        ));
        assert!(publication_due(Duration::from_secs(9), Duration::ZERO));
    }
}
