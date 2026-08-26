#![cfg(windows)]

use std::{ffi::c_void, path::Path, ptr, time::Duration};

const PIPE_NAME: &str = r"\\.\pipe\SuperExplorerMftFolderSizeV1";
const PIPE_SECURITY_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GRGW;;;IU)";
const PIPE_TYPE_MESSAGE_READMODE_REJECT_REMOTE: u32 = 0x0000_0001 | 0x0000_0008;
#[cfg(test)]
static TEST_PIPE_NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();

#[cfg(test)]
fn test_pipe_name() -> &'static str {
    TEST_PIPE_NAME.get().map_or(PIPE_NAME, String::as_str)
}
const MAGIC: u32 = 0x5146_4D53;
const SCHEMA: u16 = 1;
const REQUEST_BYTES: usize = 24;
const RESPONSE_BYTES: usize = 48;
const RESPONSE_STATUS_DETAILED_ERROR: u16 = 4;
const ERROR_DETAIL_MAX_BYTES: usize = 3 * 1024;
const DIAGNOSTICS_RESPONSE_BYTES: usize = 64;
const DIAGNOSTICS_BREAKDOWN_RESPONSE_BYTES: usize = 64;
const DURABILITY_DIAGNOSTICS_HEADER_BYTES: usize = 16;
const DURABILITY_DIAGNOSTICS_RECORD_BYTES: usize = 144;
const DURABILITY_DIAGNOSTICS_MAX_VOLUMES: usize = 26;
const REQUEST_KIND_FOLDER: u16 = 0;
const REQUEST_KIND_DIAGNOSTICS: u16 = 1;
const REQUEST_KIND_DIAGNOSTICS_BREAKDOWN: u16 = 2;
const REQUEST_KIND_SET_LRU_LIMIT: u16 = 3;
const REQUEST_KIND_HIERARCHY: u16 = 4;
const REQUEST_KIND_DURABILITY_DIAGNOSTICS: u16 = 5;
const REQUEST_KIND_FOLDER_PATH: u16 = 6;
const REQUEST_KIND_HIERARCHY_PATH: u16 = 7;
const REQUEST_KIND_FOLDER_BATCH_PATH: u16 = 8;
const REQUEST_PATH_MAX_BYTES: usize = 32 * 1024;
const FOLDER_BATCH_MAX_ITEMS: usize = 256;
const FOLDER_BATCH_ITEM_HEADER_BYTES: usize = 20;
const FOLDER_BATCH_MAX_BYTES: usize =
    FOLDER_BATCH_MAX_ITEMS * (FOLDER_BATCH_ITEM_HEADER_BYTES + REQUEST_PATH_MAX_BYTES);
const FOLDER_BATCH_RESPONSE_BYTES: usize = 72;
const FOLDER_BATCH_FRAME_ITEM: u16 = 0;
const FOLDER_BATCH_FRAME_END: u16 = 1;
const FOLDER_BATCH_PARALLELISM: usize = 4;
const HIERARCHY_HEADER_BYTES: usize = 16;
const HIERARCHY_MAX_NODES: usize = 100_000;
const HIERARCHY_MAX_BYTES: usize = 8 * 1024 * 1024;
const ERROR_PIPE_CONNECTED: u32 = 535;
const ERROR_PIPE_LISTENING: u32 = 536;
const ERROR_NO_DATA: u32 = 232;
const ERROR_MORE_DATA: u32 = 234;
const INVALID_HANDLE_VALUE: isize = -1;
/// Folder-size cells have a strict terminal-state contract: a current request
/// must display either an exact result or `Unavailable` within ten seconds.
const AGGREGATE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const CONTROL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FolderAggregateQueryV1 {
    pub generation: u64,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
    /// True when an independently-budgeted MFT structure was trimmed. The
    /// numeric fields are a known lower bound and must never be shown as exact.
    pub partial: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FolderBatchRequestV1 {
    pub request_id: u64,
    pub path: std::path::PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FolderBatchResultV1 {
    pub request_id: u64,
    pub result: Result<FolderAggregateQueryV1, String>,
}

#[derive(Clone, Debug)]
struct PreparedFolderBatchItemV1 {
    request_id: u64,
    letter: char,
    reference: u64,
    path: std::path::PathBuf,
    path_bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MftCacheDiagnosticsV1 {
    pub generation: u64,
    pub lru_bytes: u64,
    pub limit_bytes: u64,
    pub entry_count: u64,
    pub persisted_index_bytes: u64,
    pub hits: u64,
    pub misses: u64,
    pub volume_index_bytes: Option<u64>,
    pub file_data_bytes: Option<u64>,
    pub aggregate_bytes: Option<u64>,
    pub persisted_index_limit_bytes: Option<u64>,
    pub volume_index_limit_bytes: Option<u64>,
    pub file_data_limit_bytes: Option<u64>,
    pub aggregate_limit_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MftVolumeDiagnosticsV1 {
    pub volume: u8,
    pub mode: u8,
    pub schema: u8,
    pub migration_state: u8,
    pub recovery_reason: u8,
    pub transaction_last_outcome: u8,
    pub checkpoint_last_outcome: u8,
    pub exact: bool,
    pub observed_journal_id: u64,
    pub observed_next_usn: i64,
    pub observed_generation: u64,
    pub durable_journal_id: u64,
    pub durable_next_usn: i64,
    pub durable_generation: u64,
    pub pending_count: u64,
    pub pending_bytes: u64,
    pub last_successful_commit_ms: u64,
    pub focus_lease_count: u64,
    pub focus_expiry_remaining_ms: u64,
    pub main_bytes: u64,
    pub wal_bytes: u64,
    pub transaction_attempts: u64,
    pub transaction_failures: u64,
    pub checkpoint_attempts: u64,
    pub checkpoint_failures: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MftCacheBudgetLimitsV1 {
    pub persisted_index_mb: u16,
    pub volume_index_mb: u16,
    pub file_data_mb: u16,
    pub aggregate_mb: u16,
    pub lru_mb: u16,
}

impl MftCacheBudgetLimitsV1 {
    pub(crate) fn normalized(self) -> Self {
        let normalize = |value: u16| value.clamp(128, 16_384);
        Self {
            persisted_index_mb: self.persisted_index_mb.clamp(256, 16_384),
            volume_index_mb: normalize(self.volume_index_mb),
            file_data_mb: self.file_data_mb.clamp(64, 16_384),
            aggregate_mb: normalize(self.aggregate_mb),
            lru_mb: normalize(self.lru_mb),
        }
    }
}

#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: *mut c_void,
    inherit_handle: i32,
}

#[link(name = "kernel32")]
#[expect(
    unsafe_code,
    reason = "MFT query transport uses Win32 named-pipe, buffer, and handle APIs"
)]
// SAFETY: These declarations mirror the documented Win32 ABI. Call sites validate
// handles, buffer lengths, pointer lifetimes, and API-specific return values.
unsafe extern "system" {
    fn CreateNamedPipeW(
        name: *const u16,
        open_mode: u32,
        pipe_mode: u32,
        max_instances: u32,
        out_buffer_size: u32,
        in_buffer_size: u32,
        default_timeout: u32,
        security_attributes: *const SecurityAttributes,
    ) -> isize;
    fn ConnectNamedPipe(pipe: isize, overlapped: *mut c_void) -> i32;
    fn DisconnectNamedPipe(pipe: isize) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security_attributes: *const SecurityAttributes,
        creation: u32,
        flags: u32,
        template: isize,
    ) -> isize;
    fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
    fn ReadFile(
        file: isize,
        buffer: *mut c_void,
        bytes: u32,
        read: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn WriteFile(
        file: isize,
        buffer: *const c_void,
        bytes: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn PeekNamedPipe(
        pipe: isize,
        buffer: *mut c_void,
        buffer_size: u32,
        bytes_read: *mut u32,
        bytes_available: *mut u32,
        bytes_left_this_message: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: isize) -> i32;
    fn GetLastError() -> u32;
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
}

#[link(name = "advapi32")]
#[expect(
    unsafe_code,
    reason = "constructing the MFT query pipe ACL requires the Win32 SDDL converter"
)]
// SAFETY: The declaration matches ConvertStringSecurityDescriptorToSecurityDescriptorW;
// callers provide a terminated SDDL string and release the returned allocation with LocalFree.
unsafe extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        descriptor: *const u16,
        revision: u32,
        converted: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
}

struct Handle(isize);
impl Drop for Handle {
    #[expect(
        unsafe_code,
        reason = "releasing a raw MFT query pipe handle requires Win32 CloseHandle"
    )]
    // SAFETY: Handle owns a non-null Win32 handle exactly once; Drop closes it once.
    fn drop(&mut self) {
        if self.0 != 0 && self.0 != INVALID_HANDLE_VALUE {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

struct LocalMemory(*mut c_void);
impl Drop for LocalMemory {
    #[expect(
        unsafe_code,
        reason = "releasing the query-pipe SDDL allocation requires Win32 LocalFree"
    )]
    // SAFETY: LocalMemory exclusively owns memory returned by the SDDL converter.
    fn drop(&mut self) {
        if !self.0.is_null() {
            let _ = unsafe { LocalFree(self.0) };
        }
    }
}

pub(crate) fn query_folder(
    path: &Path,
    cache_memory_mb: u16,
) -> Result<FolderAggregateQueryV1, String> {
    let canonical = path
        .canonicalize()
        .map_err(|_| "MFT query path is unavailable".to_owned())?;
    let letter = canonical
        .components()
        .find_map(|component| match component {
            std::path::Component::Prefix(prefix) => match prefix.kind() {
                std::path::Prefix::Disk(letter) | std::path::Prefix::VerbatimDisk(letter) => {
                    Some(char::from(letter).to_ascii_uppercase())
                }
                _ => None,
            },
            _ => None,
        })
        .ok_or_else(|| "MFT query requires a drive-letter volume".to_owned())?;
    let reference = crate::mft_size_map::file_reference_number(&canonical)?;
    let mut request = [0_u8; REQUEST_BYTES];
    request[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    request[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    request[6..8].copy_from_slice(&(letter as u16).to_le_bytes());
    request[8..16].copy_from_slice(&reference.to_le_bytes());
    request[16..18].copy_from_slice(&cache_memory_mb.to_le_bytes());
    request[18..20].copy_from_slice(&REQUEST_KIND_FOLDER_PATH.to_le_bytes());
    let path_bytes = canonical.to_string_lossy().into_owned().into_bytes();
    let path_length = u16::try_from(path_bytes.len())
        .ok()
        .filter(|length| usize::from(*length) <= REQUEST_PATH_MAX_BYTES)
        .ok_or_else(|| "MFT query path exceeds the protocol bound".to_owned())?;
    request[20..22].copy_from_slice(&path_length.to_le_bytes());

    let pipe = connect(100)?;
    write_all(pipe.0, &request)?;
    write_all(pipe.0, &path_bytes)?;
    let mut response = [0_u8; RESPONSE_BYTES];
    read_exact(pipe.0, &mut response, || false, AGGREGATE_RESPONSE_TIMEOUT)?;
    if read_u32(&response, 0) != Some(MAGIC) || read_u16(&response, 4) != Some(SCHEMA) {
        return Err("MFT query response schema mismatch".to_owned());
    }
    let error_detail = if let Some(length) = detailed_error_length(&response)? {
        let mut bytes = vec![0_u8; length];
        read_exact(pipe.0, &mut bytes, || false, AGGREGATE_RESPONSE_TIMEOUT)?;
        Some(
            String::from_utf8(bytes)
                .map_err(|_| "MFT Service detailed error is not valid UTF-8".to_owned())?,
        )
    } else {
        None
    };
    decode_folder_response(&canonical, &response, error_detail.as_deref())
}

/// Sends one visible-first folder batch and publishes each terminal result as
/// soon as the service completes it. Numeric results are exact-only.
pub(crate) fn query_folders_batch(
    requests: &[FolderBatchRequestV1],
    cache_memory_mb: u16,
    cancelled: impl Fn() -> bool,
    mut publish: impl FnMut(FolderBatchResultV1) -> Result<(), String>,
) -> Result<(), String> {
    if requests.is_empty() {
        return Ok(());
    }
    if requests.len() > FOLDER_BATCH_MAX_ITEMS {
        return Err(format!(
            "MFT folder batch exceeds item bound: count={} limit={FOLDER_BATCH_MAX_ITEMS}",
            requests.len()
        ));
    }
    let mut seen = std::collections::HashSet::with_capacity(requests.len());
    let mut prepared = Vec::with_capacity(requests.len());
    for request in requests {
        if !seen.insert(request.request_id) {
            return Err(format!(
                "MFT folder batch contains duplicate request id {}",
                request.request_id
            ));
        }
        match prepare_folder_batch_item(request) {
            Ok(item) => prepared.push(item),
            Err(error) => publish(FolderBatchResultV1 {
                request_id: request.request_id,
                result: Err(error),
            })?,
        }
    }
    if prepared.is_empty() || cancelled() {
        return Ok(());
    }

    let mut payload = Vec::new();
    for item in &prepared {
        payload.extend_from_slice(&item.request_id.to_le_bytes());
        payload.extend_from_slice(&(item.letter as u16).to_le_bytes());
        payload.extend_from_slice(&(item.path_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(&item.reference.to_le_bytes());
        payload.extend_from_slice(&item.path_bytes);
    }
    if payload.len() > FOLDER_BATCH_MAX_BYTES {
        return Err("MFT folder batch payload exceeds protocol bound".to_owned());
    }
    static NEXT_BATCH_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let batch_id = NEXT_BATCH_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut header = [0_u8; REQUEST_BYTES];
    header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    header[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    header[6..8].copy_from_slice(&(prepared.len() as u16).to_le_bytes());
    header[8..16].copy_from_slice(&batch_id.to_le_bytes());
    header[16..18].copy_from_slice(&cache_memory_mb.to_le_bytes());
    header[18..20].copy_from_slice(&REQUEST_KIND_FOLDER_BATCH_PATH.to_le_bytes());
    header[20..24].copy_from_slice(&(payload.len() as u32).to_le_bytes());

    let deadline = std::time::Instant::now() + AGGREGATE_RESPONSE_TIMEOUT;
    let pipe = connect_until(deadline)?;
    write_all_until(pipe.0, &header, &cancelled, deadline)?;
    write_all_until(pipe.0, &payload, &cancelled, deadline)?;
    let mut unfinished = prepared
        .iter()
        .map(|item| (item.request_id, item.path.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    loop {
        if cancelled() {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(format!(
                "MFT Service batch deadline exceeded with {} unfinished folders",
                unfinished.len()
            ));
        }
        let mut response = [0_u8; FOLDER_BATCH_RESPONSE_BYTES];
        read_exact(pipe.0, &mut response, &cancelled, remaining)?;
        if read_u32(&response, 0) != Some(MAGIC)
            || read_u16(&response, 4) != Some(SCHEMA)
            || read_u64(&response, 8) != Some(batch_id)
        {
            return Err("MFT folder batch response identity mismatch".to_owned());
        }
        match read_u16(&response, 6) {
            Some(FOLDER_BATCH_FRAME_END) => {
                return unfinished.is_empty().then_some(()).ok_or_else(|| {
                    format!(
                        "MFT Service ended batch with {} unfinished folders",
                        unfinished.len()
                    )
                });
            }
            Some(FOLDER_BATCH_FRAME_ITEM) => {}
            _ => return Err("MFT folder batch response frame is invalid".to_owned()),
        }
        let request_id = read_u64(&response, 16)
            .ok_or_else(|| "MFT folder batch response is missing request id".to_owned())?;
        let path = unfinished.remove(&request_id).ok_or_else(|| {
            format!("MFT folder batch returned unknown or duplicate request id {request_id}")
        })?;
        let status = read_u16(&response, 24).unwrap_or_default();
        let detail_length = usize::from(read_u16(&response, 26).unwrap_or_default());
        let detail =
            if status == RESPONSE_STATUS_DETAILED_ERROR {
                if !(1..=ERROR_DETAIL_MAX_BYTES).contains(&detail_length) {
                    return Err("MFT folder batch detailed error length is invalid".to_owned());
                }
                let mut bytes = vec![0_u8; detail_length];
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                read_exact(pipe.0, &mut bytes, &cancelled, remaining)?;
                Some(String::from_utf8(bytes).map_err(|_| {
                    "MFT Service batch detailed error is not valid UTF-8".to_owned()
                })?)
            } else {
                None
            };
        let result = decode_batch_folder_response(&path, &response, detail.as_deref());
        publish(FolderBatchResultV1 { request_id, result })?;
    }
}

fn prepare_folder_batch_item(
    request: &FolderBatchRequestV1,
) -> Result<PreparedFolderBatchItemV1, String> {
    // UI snapshots already provide absolute filesystem identities. Avoid
    // canonicalize here: resolving every child serially can consume most of
    // the ten-second interactive budget before the batch reaches the service.
    // The file-reference handle below is the authoritative identity check.
    let absolute = if request.path.is_absolute() {
        request.path.clone()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("MFT query current directory is unavailable: {error}"))?
            .join(&request.path)
    };
    let letter = drive_letter(&absolute)
        .ok_or_else(|| "MFT query requires a drive-letter volume".to_owned())?;
    let reference = crate::mft_size_map::file_reference_number(&absolute).map_err(|error| {
        format!(
            "MFT query path identity is unavailable: path={} error={error}",
            absolute.display()
        )
    })?;
    let path_bytes = absolute.to_string_lossy().into_owned().into_bytes();
    if path_bytes.is_empty()
        || path_bytes.len() > REQUEST_PATH_MAX_BYTES
        || path_bytes.len() > usize::from(u16::MAX)
    {
        return Err("MFT query path exceeds the protocol bound".to_owned());
    }
    Ok(PreparedFolderBatchItemV1 {
        request_id: request.request_id,
        letter,
        reference,
        path: absolute,
        path_bytes,
    })
}

fn drive_letter(path: &Path) -> Option<char> {
    path.components().find_map(|component| match component {
        std::path::Component::Prefix(prefix) => match prefix.kind() {
            std::path::Prefix::Disk(letter) | std::path::Prefix::VerbatimDisk(letter) => {
                Some(char::from(letter).to_ascii_uppercase())
            }
            _ => None,
        },
        _ => None,
    })
}

fn decode_batch_folder_response(
    canonical: &Path,
    response: &[u8],
    error_detail: Option<&str>,
) -> Result<FolderAggregateQueryV1, String> {
    match read_u16(response, 24).unwrap_or_default() {
        0 => Ok(FolderAggregateQueryV1 {
            generation: read_u64(response, 32).unwrap_or_default(),
            logical_bytes: read_u64(response, 40).unwrap_or_default(),
            allocated_bytes: read_u64(response, 48).unwrap_or_default(),
            file_count: read_u64(response, 56).unwrap_or_default(),
            directory_count: read_u64(response, 64).unwrap_or_default(),
            partial: false,
        }),
        3 => Err(format!(
            "MFT Service returned a partial aggregate; exact folder size is unavailable (path={})",
            canonical.display()
        )),
        RESPONSE_STATUS_DETAILED_ERROR => match error_detail.filter(|detail| !detail.is_empty()) {
            Some(detail) => Err(detail.to_owned()),
            None => Err("MFT Service batch detailed error payload is missing".to_owned()),
        },
        _ => Err(format!(
            "MFT Service rejected folder batch item: path={}",
            canonical.display()
        )),
    }
}

fn detailed_error_length(response: &[u8]) -> Result<Option<usize>, String> {
    if read_u16(response, 6) != Some(RESPONSE_STATUS_DETAILED_ERROR) {
        return Ok(None);
    }
    read_u32(response, 8)
        .map(|value| value as usize)
        .filter(|length| (1..=ERROR_DETAIL_MAX_BYTES).contains(length))
        .map(Some)
        .ok_or_else(|| "MFT Service detailed error length is invalid".to_owned())
}

fn decode_folder_response(
    canonical: &Path,
    response: &[u8],
    error_detail: Option<&str>,
) -> Result<FolderAggregateQueryV1, String> {
    match read_u16(response, 6).unwrap_or_default() {
        0 => Ok(FolderAggregateQueryV1 {
            generation: read_u64(response, 8).unwrap_or_default(),
            logical_bytes: read_u64(response, 16).unwrap_or_default(),
            allocated_bytes: read_u64(response, 24).unwrap_or_default(),
            file_count: read_u64(response, 32).unwrap_or_default(),
            directory_count: read_u64(response, 40).unwrap_or_default(),
            partial: false,
        }),
        3 => Err(format!(
            "MFT Service returned a partial aggregate; exact folder size is unavailable (path={}, generation={}, logical_bytes={}, allocated_bytes={}, file_count={}, directory_count={})",
            canonical.display(),
            read_u64(response, 8).unwrap_or_default(),
            read_u64(response, 16).unwrap_or_default(),
            read_u64(response, 24).unwrap_or_default(),
            read_u64(response, 32).unwrap_or_default(),
            read_u64(response, 40).unwrap_or_default(),
        )),
        1 => Err("MFT Service has no aggregate for this folder".to_owned()),
        2 => Err("MFT Service cache is temporarily unavailable".to_owned()),
        RESPONSE_STATUS_DETAILED_ERROR => match error_detail.filter(|detail| !detail.is_empty()) {
            Some(detail) => Err(detail.to_owned()),
            None => Err("MFT Service detailed error payload is missing".to_owned()),
        },
        _ => Err("MFT Service rejected the query".to_owned()),
    }
}

pub(crate) fn query_hierarchy(
    path: &Path,
    cache_memory_mb: u16,
) -> Result<Vec<crate::mft_size_map::MftProjectedNodeV1>, String> {
    let canonical = path
        .canonicalize()
        .map_err(|_| "MFT query path is unavailable".to_owned())?;
    let letter = canonical
        .components()
        .find_map(|component| match component {
            std::path::Component::Prefix(prefix) => match prefix.kind() {
                std::path::Prefix::Disk(letter) | std::path::Prefix::VerbatimDisk(letter) => {
                    Some(char::from(letter).to_ascii_uppercase())
                }
                _ => None,
            },
            _ => None,
        })
        .ok_or_else(|| "MFT query requires a drive-letter volume".to_owned())?;
    let reference = crate::mft_size_map::file_reference_number(&canonical)?;
    let mut request = [0_u8; REQUEST_BYTES];
    request[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    request[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    request[6..8].copy_from_slice(&(letter as u16).to_le_bytes());
    request[8..16].copy_from_slice(&reference.to_le_bytes());
    request[16..18].copy_from_slice(&cache_memory_mb.to_le_bytes());
    request[18..20].copy_from_slice(&REQUEST_KIND_HIERARCHY_PATH.to_le_bytes());
    let path_bytes = canonical.to_string_lossy().into_owned().into_bytes();
    let path_length = u16::try_from(path_bytes.len())
        .ok()
        .filter(|length| usize::from(*length) <= REQUEST_PATH_MAX_BYTES)
        .ok_or_else(|| "MFT hierarchy path exceeds the protocol bound".to_owned())?;
    request[20..22].copy_from_slice(&path_length.to_le_bytes());
    let pipe = connect(100)?;
    write_all(pipe.0, &request)?;
    write_all(pipe.0, &path_bytes)?;
    let mut header = [0_u8; HIERARCHY_HEADER_BYTES];
    read_exact(pipe.0, &mut header, || false, AGGREGATE_RESPONSE_TIMEOUT)?;
    if read_u32(&header, 0) != Some(MAGIC) || read_u16(&header, 4) != Some(SCHEMA) {
        return Err("MFT hierarchy response schema mismatch".to_owned());
    }
    let count = read_u32(&header, 8).unwrap_or_default() as usize;
    let payload_bytes = read_u32(&header, 12).unwrap_or_default() as usize;
    if count > HIERARCHY_MAX_NODES || payload_bytes > HIERARCHY_MAX_BYTES {
        return Err("MFT hierarchy response exceeds bounds".to_owned());
    }
    let mut payload = vec![0_u8; payload_bytes];
    read_exact(pipe.0, &mut payload, || false, AGGREGATE_RESPONSE_TIMEOUT)?;
    if read_u16(&header, 6) != Some(0) {
        return Err(String::from_utf8(payload)
            .ok()
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| "MFT hierarchy is unavailable or partial".to_owned()));
    }
    decode_hierarchy_payload(&payload, count)
}

fn decode_hierarchy_payload(
    payload: &[u8],
    count: usize,
) -> Result<Vec<crate::mft_size_map::MftProjectedNodeV1>, String> {
    let mut offset = 0_usize;
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        if offset.saturating_add(35) > payload.len() {
            return Err("truncated MFT hierarchy".to_owned());
        }
        let reference =
            read_u64(payload, offset).ok_or_else(|| "invalid hierarchy reference".to_owned())?;
        let parent_raw =
            read_u64(payload, offset + 8).ok_or_else(|| "invalid hierarchy parent".to_owned())?;
        let logical_bytes = read_u64(payload, offset + 16).unwrap_or_default();
        let allocated_bytes = read_u64(payload, offset + 24).unwrap_or_default();
        let is_directory = payload[offset + 32] != 0;
        let name_len = read_u16(payload, offset + 33).unwrap_or_default() as usize;
        offset += 35;
        if offset.saturating_add(name_len) > payload.len() {
            return Err("truncated hierarchy name".to_owned());
        }
        let name = std::str::from_utf8(&payload[offset..offset + name_len])
            .map_err(|_| "hierarchy name is not UTF-8".to_owned())?
            .to_owned();
        offset += name_len;
        nodes.push(crate::mft_size_map::MftProjectedNodeV1 {
            reference,
            parent_reference: (parent_raw != u64::MAX).then_some(parent_raw),
            name,
            logical_bytes,
            allocated_bytes,
            is_directory,
        });
    }
    if offset != payload.len() {
        return Err("MFT hierarchy payload has trailing bytes".to_owned());
    }
    Ok(nodes)
}

pub(crate) fn query_diagnostics() -> Result<MftCacheDiagnosticsV1, String> {
    let mut request = [0_u8; REQUEST_BYTES];
    request[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    request[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    request[18..20].copy_from_slice(&REQUEST_KIND_DIAGNOSTICS.to_le_bytes());
    let pipe = connect(5)?;
    write_all(pipe.0, &request)?;
    let mut response = [0_u8; DIAGNOSTICS_RESPONSE_BYTES];
    read_exact(pipe.0, &mut response, || false, CONTROL_RESPONSE_TIMEOUT)?;
    if read_u32(&response, 0) != Some(MAGIC)
        || read_u16(&response, 4) != Some(SCHEMA)
        || read_u16(&response, 6) != Some(0)
    {
        return Err("MFT diagnostics response schema mismatch".to_owned());
    }
    let mut diagnostics = MftCacheDiagnosticsV1 {
        generation: read_u64(&response, 8).unwrap_or_default(),
        lru_bytes: read_u64(&response, 16).unwrap_or_default(),
        limit_bytes: read_u64(&response, 24).unwrap_or_default(),
        entry_count: read_u64(&response, 32).unwrap_or_default(),
        persisted_index_bytes: read_u64(&response, 40).unwrap_or_default(),
        hits: read_u64(&response, 48).unwrap_or_default(),
        misses: read_u64(&response, 56).unwrap_or_default(),
        volume_index_bytes: None,
        file_data_bytes: None,
        aggregate_bytes: None,
        persisted_index_limit_bytes: None,
        volume_index_limit_bytes: None,
        file_data_limit_bytes: None,
        aggregate_limit_bytes: None,
    };
    if let Ok(breakdown) = query_diagnostics_breakdown() {
        diagnostics.volume_index_bytes = Some(breakdown[0]);
        diagnostics.file_data_bytes = Some(breakdown[1]);
        diagnostics.aggregate_bytes = Some(breakdown[2]);
        diagnostics.persisted_index_limit_bytes = Some(breakdown[3]);
        diagnostics.volume_index_limit_bytes = Some(breakdown[4]);
        diagnostics.file_data_limit_bytes = Some(breakdown[5]);
        diagnostics.aggregate_limit_bytes = Some(breakdown[6]);
    }
    Ok(diagnostics)
}

pub(crate) fn query_durability_diagnostics() -> Result<Vec<MftVolumeDiagnosticsV1>, String> {
    let mut request = [0_u8; REQUEST_BYTES];
    request[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    request[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    request[18..20].copy_from_slice(&REQUEST_KIND_DURABILITY_DIAGNOSTICS.to_le_bytes());
    let pipe = connect(5)?;
    write_all(pipe.0, &request)?;
    let mut header = [0_u8; DURABILITY_DIAGNOSTICS_HEADER_BYTES];
    read_exact(pipe.0, &mut header, || false, CONTROL_RESPONSE_TIMEOUT)?;
    if read_u32(&header, 0) != Some(MAGIC)
        || read_u16(&header, 4) != Some(SCHEMA)
        || read_u16(&header, 6) != Some(0)
        || read_u16(&header, 10) != Some(DURABILITY_DIAGNOSTICS_RECORD_BYTES as u16)
    {
        return Err("MFT durability diagnostics response schema mismatch".to_owned());
    }
    let count = usize::from(read_u16(&header, 8).unwrap_or_default());
    if count > DURABILITY_DIAGNOSTICS_MAX_VOLUMES {
        return Err("MFT durability diagnostics volume count is invalid".to_owned());
    }
    let mut payload = vec![0_u8; count.saturating_mul(DURABILITY_DIAGNOSTICS_RECORD_BYTES)];
    read_exact(pipe.0, &mut payload, || false, CONTROL_RESPONSE_TIMEOUT)?;
    let mut volumes = Vec::with_capacity(count);
    for record in payload.chunks_exact(DURABILITY_DIAGNOSTICS_RECORD_BYTES) {
        let values = [
            8_usize, 16, 24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 120, 128, 136,
        ]
        .map(|offset| read_u64(record, offset).unwrap_or_default());
        volumes.push(MftVolumeDiagnosticsV1 {
            volume: record[0],
            mode: record[1],
            schema: record[2],
            migration_state: record[3],
            recovery_reason: record[4],
            transaction_last_outcome: record[5],
            checkpoint_last_outcome: record[6],
            exact: record[7] != 0,
            observed_journal_id: values[0],
            observed_next_usn: i64::from_le_bytes(values[1].to_le_bytes()),
            observed_generation: values[2],
            durable_journal_id: values[3],
            durable_next_usn: i64::from_le_bytes(values[4].to_le_bytes()),
            durable_generation: values[5],
            pending_count: values[6],
            pending_bytes: values[7],
            last_successful_commit_ms: values[8],
            focus_lease_count: values[9],
            focus_expiry_remaining_ms: values[10],
            main_bytes: values[11],
            wal_bytes: values[12],
            transaction_attempts: values[13],
            transaction_failures: values[14],
            checkpoint_attempts: values[15],
            checkpoint_failures: values[16],
        });
    }
    Ok(volumes)
}

pub(crate) fn set_cache_budgets(
    limits: MftCacheBudgetLimitsV1,
) -> Result<MftCacheBudgetLimitsV1, String> {
    let limits = limits.normalized();
    let mut request = [0_u8; REQUEST_BYTES];
    request[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    request[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    for (offset, value) in [
        (6, limits.persisted_index_mb),
        (8, limits.volume_index_mb),
        (10, limits.file_data_mb),
        (12, limits.aggregate_mb),
        (14, limits.lru_mb),
    ] {
        request[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    request[18..20].copy_from_slice(&REQUEST_KIND_SET_LRU_LIMIT.to_le_bytes());
    let pipe = connect(100)?;
    write_all(pipe.0, &request)?;
    let mut response = [0_u8; RESPONSE_BYTES];
    read_exact(pipe.0, &mut response, || false, CONTROL_RESPONSE_TIMEOUT)?;
    decode_cache_budget_response(&response)
}

fn decode_cache_budget_response(response: &[u8]) -> Result<MftCacheBudgetLimitsV1, String> {
    if response.len() != RESPONSE_BYTES
        || read_u32(response, 0) != Some(MAGIC)
        || read_u16(response, 4) != Some(SCHEMA)
        || read_u16(response, 6) != Some(0)
    {
        return Err("MFT Service rejected the cache limit".to_owned());
    }
    Ok(MftCacheBudgetLimitsV1 {
        persisted_index_mb: read_u16(response, 8)
            .ok_or_else(|| "invalid persisted limit".to_owned())?,
        volume_index_mb: read_u16(response, 10).ok_or_else(|| "invalid volume limit".to_owned())?,
        file_data_mb: read_u16(response, 12).ok_or_else(|| "invalid file-data limit".to_owned())?,
        aggregate_mb: read_u16(response, 14).ok_or_else(|| "invalid aggregate limit".to_owned())?,
        lru_mb: read_u16(response, 16).ok_or_else(|| "invalid LRU limit".to_owned())?,
    }
    .normalized())
}

fn query_diagnostics_breakdown() -> Result<[u64; 7], String> {
    let mut request = [0_u8; REQUEST_BYTES];
    request[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    request[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    request[18..20].copy_from_slice(&REQUEST_KIND_DIAGNOSTICS_BREAKDOWN.to_le_bytes());
    let pipe = connect(2)?;
    write_all(pipe.0, &request)?;
    let mut response = [0_u8; DIAGNOSTICS_BREAKDOWN_RESPONSE_BYTES];
    read_exact(pipe.0, &mut response, || false, CONTROL_RESPONSE_TIMEOUT)?;
    if read_u32(&response, 0) != Some(MAGIC)
        || read_u16(&response, 4) != Some(SCHEMA)
        || read_u16(&response, 6) != Some(0)
    {
        return Err("MFT diagnostics breakdown is unavailable".to_owned());
    }
    Ok([8, 16, 24, 32, 40, 48, 56].map(|offset| read_u64(&response, offset).unwrap_or_default()))
}

fn connect(attempts: usize) -> Result<Handle, String> {
    connect_until(
        std::time::Instant::now()
            + Duration::from_millis((attempts.max(1) as u64).saturating_mul(100)),
    )
}

#[expect(
    unsafe_code,
    reason = "connecting to the MFT query pipe requires Win32 wait, open, and last-error APIs"
)]
// SAFETY: The pipe name is NUL-terminated, access flags match CreateFileW, returned handles
// are checked before ownership, and GetLastError is read immediately after failed calls.
fn connect_until(deadline: std::time::Instant) -> Result<Handle, String> {
    #[cfg(not(test))]
    let name = wide(PIPE_NAME);
    #[cfg(test)]
    let name = wide(test_pipe_name());
    let pipe = loop {
        let _ = unsafe { WaitNamedPipeW(name.as_ptr(), 50) };
        let pipe = unsafe {
            CreateFileW(
                name.as_ptr(),
                0x8000_0000 | 0x4000_0000,
                0,
                ptr::null(),
                3,
                0,
                0,
            )
        };
        if pipe != INVALID_HANDLE_VALUE {
            break pipe;
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("MFT query pipe unavailable ({})", unsafe {
                GetLastError()
            }));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(Handle(pipe))
}

pub(crate) fn serve_folder_queries(
    stopped: impl Fn() -> bool,
    query: impl Fn(char, u64, u16) -> Result<FolderAggregateQueryV1, String> + Sync,
) {
    serve_queries(
        stopped,
        move |letter, reference, cache, _| query(letter, reference, cache),
        |_, _, _| Err("MFT hierarchy operation is unavailable".to_owned()),
        || Ok(MftCacheDiagnosticsV1::default()),
        || Ok(Vec::new()),
        |value| Ok(value),
    );
}

#[expect(
    unsafe_code,
    reason = "serving MFT queries requires raw Win32 ACL and named-pipe lifecycle APIs"
)]
// SAFETY: Security-descriptor storage outlives pipe creation; all pipe handles are owned by
// Handle, buffers use their exact Rust lengths, and every Win32 result is checked.
pub(crate) fn serve_queries(
    stopped: impl Fn() -> bool,
    query: impl Fn(char, u64, u16, Option<std::path::PathBuf>) -> Result<FolderAggregateQueryV1, String>
    + Sync,
    mut query_hierarchy: impl FnMut(
        char,
        u64,
        Option<std::path::PathBuf>,
    ) -> Result<Vec<crate::mft_size_map::MftProjectedNodeV1>, String>,
    mut diagnostics: impl FnMut() -> Result<MftCacheDiagnosticsV1, String>,
    mut durability_diagnostics: impl FnMut() -> Result<Vec<MftVolumeDiagnosticsV1>, String>,
    mut set_lru_limit: impl FnMut(MftCacheBudgetLimitsV1) -> Result<MftCacheBudgetLimitsV1, String>,
) {
    #[cfg(not(test))]
    let name = wide(PIPE_NAME);
    #[cfg(test)]
    let name = wide(test_pipe_name());
    let sddl = wide(PIPE_SECURITY_SDDL);
    let mut descriptor = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &raw mut descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return;
    }
    let descriptor = LocalMemory(descriptor);
    let attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        descriptor: descriptor.0,
        inherit_handle: 0,
    };
    while !stopped() {
        let pipe = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                0x0000_0003,
                PIPE_TYPE_MESSAGE_READMODE_REJECT_REMOTE,
                8,
                4_096,
                4_096,
                100,
                &raw const attributes,
            )
        };
        if pipe == INVALID_HANDLE_VALUE {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        let pipe = Handle(pipe);
        loop {
            if stopped() {
                return;
            }
            let connected = unsafe { ConnectNamedPipe(pipe.0, ptr::null_mut()) };
            let error = unsafe { GetLastError() };
            if connected != 0 || error == ERROR_PIPE_CONNECTED {
                break;
            }
            if error != ERROR_PIPE_LISTENING && error != ERROR_NO_DATA {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut request = [0_u8; REQUEST_BYTES];
        if read_exact(pipe.0, &mut request, &stopped, CONTROL_RESPONSE_TIMEOUT).is_ok() {
            let valid =
                read_u32(&request, 0) == Some(MAGIC) && read_u16(&request, 4) == Some(SCHEMA);
            let request_kind = read_u16(&request, 18);
            let letter = read_u16(&request, 6)
                .and_then(|value| char::from_u32(u32::from(value)))
                .filter(char::is_ascii_alphabetic)
                .map(|value| value.to_ascii_uppercase());
            let reference = read_u64(&request, 8);
            let cache_memory_mb =
                read_u16(&request, 16).map(explorer_model::normalized_mft_folder_cache_memory_mb);
            let requested_path = if valid
                && matches!(
                    request_kind,
                    Some(REQUEST_KIND_FOLDER_PATH) | Some(REQUEST_KIND_HIERARCHY_PATH)
                ) {
                let length = read_u16(&request, 20).map(usize::from).unwrap_or_default();
                if length == 0 || length > REQUEST_PATH_MAX_BYTES {
                    None
                } else {
                    let mut bytes = vec![0_u8; length];
                    read_exact(pipe.0, &mut bytes, &stopped, CONTROL_RESPONSE_TIMEOUT)
                        .ok()
                        .and_then(|_| String::from_utf8(bytes).ok())
                        .map(std::path::PathBuf::from)
                }
            } else {
                None
            };
            if valid && request_kind == Some(REQUEST_KIND_FOLDER_BATCH_PATH) {
                let _ = serve_folder_batch(pipe.0, &request, &query, &stopped);
                wait_for_client_close(pipe.0, &stopped);
                let _ = unsafe { DisconnectNamedPipe(pipe.0) };
                continue;
            }
            if valid && request_kind == Some(REQUEST_KIND_DIAGNOSTICS) {
                let response = encode_diagnostics_response(diagnostics());
                let _ = write_all(pipe.0, &response);
                wait_for_client_close(pipe.0, &stopped);
                let _ = unsafe { DisconnectNamedPipe(pipe.0) };
                continue;
            }
            if valid && request_kind == Some(REQUEST_KIND_DIAGNOSTICS_BREAKDOWN) {
                let response = encode_diagnostics_breakdown_response(diagnostics());
                let _ = write_all(pipe.0, &response);
                wait_for_client_close(pipe.0, &stopped);
                let _ = unsafe { DisconnectNamedPipe(pipe.0) };
                continue;
            }
            if valid && request_kind == Some(REQUEST_KIND_DURABILITY_DIAGNOSTICS) {
                let response = encode_durability_diagnostics_response(durability_diagnostics());
                let _ = write_all(pipe.0, &response);
                wait_for_client_close(pipe.0, &stopped);
                let _ = unsafe { DisconnectNamedPipe(pipe.0) };
                continue;
            }
            if valid && request_kind == Some(REQUEST_KIND_SET_LRU_LIMIT) {
                let result = [6, 8, 10, 12, 14]
                    .map(|offset| read_u16(&request, offset))
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .and_then(|values| (values.len() == 5).then_some(values))
                    .ok_or_else(|| "invalid cache limits".to_owned())
                    .map(|values| MftCacheBudgetLimitsV1 {
                        persisted_index_mb: values[0],
                        volume_index_mb: values[1],
                        file_data_mb: values[2],
                        aggregate_mb: values[3],
                        lru_mb: values[4],
                    })
                    .and_then(&mut set_lru_limit);
                let mut response = [0_u8; RESPONSE_BYTES];
                response[0..4].copy_from_slice(&MAGIC.to_le_bytes());
                response[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
                match result {
                    Ok(value) => {
                        for (offset, limit) in [
                            (8, value.persisted_index_mb),
                            (10, value.volume_index_mb),
                            (12, value.file_data_mb),
                            (14, value.aggregate_mb),
                            (16, value.lru_mb),
                        ] {
                            response[offset..offset + 2].copy_from_slice(&limit.to_le_bytes());
                        }
                    }
                    Err(_) => response[6..8].copy_from_slice(&1_u16.to_le_bytes()),
                }
                let _ = write_all(pipe.0, &response);
                wait_for_client_close(pipe.0, &stopped);
                let _ = unsafe { DisconnectNamedPipe(pipe.0) };
                continue;
            }
            if valid
                && matches!(
                    request_kind,
                    Some(REQUEST_KIND_HIERARCHY) | Some(REQUEST_KIND_HIERARCHY_PATH)
                )
                && (request_kind != Some(REQUEST_KIND_HIERARCHY_PATH) || requested_path.is_some())
            {
                let result = letter
                    .zip(reference)
                    .ok_or_else(|| "invalid hierarchy identity".to_owned())
                    .and_then(|(letter, reference)| {
                        query_hierarchy(letter, reference, requested_path)
                    });
                let response = encode_hierarchy_response(result);
                let _ = write_all(pipe.0, &response);
                wait_for_client_close(pipe.0, &stopped);
                let _ = unsafe { DisconnectNamedPipe(pipe.0) };
                continue;
            }
            let result = if valid
                && matches!(
                    request_kind,
                    Some(REQUEST_KIND_FOLDER) | Some(REQUEST_KIND_FOLDER_PATH)
                )
                && (request_kind != Some(REQUEST_KIND_FOLDER_PATH) || requested_path.is_some())
            {
                letter
                    .zip(reference)
                    .zip(cache_memory_mb)
                    .ok_or_else(|| "invalid query identity".to_owned())
                    .and_then(|((letter, reference), cache_memory_mb)| {
                        query(letter, reference, cache_memory_mb, requested_path)
                    })
            } else {
                Err("invalid query protocol".to_owned())
            };
            let response = encode_response(result);
            let _ = write_all(pipe.0, &response);
            wait_for_client_close(pipe.0, &stopped);
        }
        let _ = unsafe { DisconnectNamedPipe(pipe.0) };
    }
}

#[derive(Clone, Debug)]
struct ServiceFolderBatchItemV1 {
    request_id: u64,
    letter: char,
    reference: u64,
    path: std::path::PathBuf,
}

fn serve_folder_batch(
    handle: isize,
    header: &[u8; REQUEST_BYTES],
    query: &(
         impl Fn(char, u64, u16, Option<std::path::PathBuf>) -> Result<FolderAggregateQueryV1, String>
         + Sync
     ),
    stopped: &impl Fn() -> bool,
) -> Result<(), String> {
    let count = usize::from(read_u16(header, 6).unwrap_or_default());
    let batch_id = read_u64(header, 8).unwrap_or_default();
    let cache_memory_mb = read_u16(header, 16)
        .map(explorer_model::normalized_mft_folder_cache_memory_mb)
        .ok_or_else(|| "MFT folder batch cache limit is invalid".to_owned())?;
    let payload_length = read_u32(header, 20).unwrap_or_default() as usize;
    if count == 0
        || count > FOLDER_BATCH_MAX_ITEMS
        || payload_length == 0
        || payload_length > FOLDER_BATCH_MAX_BYTES
    {
        return Err(format!(
            "MFT folder batch envelope is invalid: count={count} payload_bytes={payload_length}"
        ));
    }
    let mut payload = vec![0_u8; payload_length];
    read_exact(handle, &mut payload, stopped, CONTROL_RESPONSE_TIMEOUT)?;
    let items = decode_folder_batch_payload(&payload, count)?;
    let next = std::sync::atomic::AtomicUsize::new(0);
    let connection_open = std::sync::atomic::AtomicBool::new(true);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| -> Result<(), String> {
        for _ in 0..FOLDER_BATCH_PARALLELISM.min(items.len()) {
            let sender = sender.clone();
            let items = &items;
            let next = &next;
            let connection_open = &connection_open;
            scope.spawn(move || {
                while connection_open.load(std::sync::atomic::Ordering::Acquire) {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    let result = query(
                        item.letter,
                        item.reference,
                        cache_memory_mb,
                        Some(item.path.clone()),
                    );
                    if sender.send((item.request_id, result)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
        for (request_id, result) in receiver {
            let response = encode_folder_batch_response(batch_id, request_id, result);
            if let Err(error) = write_all(handle, &response) {
                connection_open.store(false, std::sync::atomic::Ordering::Release);
                return Err(error);
            }
        }
        let end = encode_folder_batch_end(batch_id);
        write_all(handle, &end)
    })
}

fn decode_folder_batch_payload(
    payload: &[u8],
    expected_count: usize,
) -> Result<Vec<ServiceFolderBatchItemV1>, String> {
    let mut offset = 0_usize;
    let mut seen = std::collections::HashSet::with_capacity(expected_count);
    let mut items = Vec::with_capacity(expected_count);
    while offset < payload.len() && items.len() < expected_count {
        let header_end = offset.saturating_add(FOLDER_BATCH_ITEM_HEADER_BYTES);
        let item_header = payload
            .get(offset..header_end)
            .ok_or_else(|| "MFT folder batch item header is truncated".to_owned())?;
        let request_id = read_u64(item_header, 0)
            .ok_or_else(|| "MFT folder batch item request id is missing".to_owned())?;
        let letter = read_u16(item_header, 8)
            .and_then(|value| char::from_u32(u32::from(value)))
            .filter(char::is_ascii_alphabetic)
            .map(|value| value.to_ascii_uppercase())
            .ok_or_else(|| "MFT folder batch drive letter is invalid".to_owned())?;
        let path_length = usize::from(read_u16(item_header, 10).unwrap_or_default());
        let reference = read_u64(item_header, 12)
            .ok_or_else(|| "MFT folder batch file reference is missing".to_owned())?;
        if path_length == 0 || path_length > REQUEST_PATH_MAX_BYTES || !seen.insert(request_id) {
            return Err("MFT folder batch item bounds or identity is invalid".to_owned());
        }
        let path_start = header_end;
        let path_end = path_start.saturating_add(path_length);
        let path = payload
            .get(path_start..path_end)
            .ok_or_else(|| "MFT folder batch path is truncated".to_owned())?;
        let path = std::str::from_utf8(path)
            .map(std::path::PathBuf::from)
            .map_err(|_| "MFT folder batch path is not valid UTF-8".to_owned())?;
        if drive_letter(&path) != Some(letter) {
            return Err("MFT folder batch path/volume identity mismatch".to_owned());
        }
        items.push(ServiceFolderBatchItemV1 {
            request_id,
            letter,
            reference,
            path,
        });
        offset = path_end;
    }
    if items.len() != expected_count || offset != payload.len() {
        return Err(format!(
            "MFT folder batch payload count mismatch: expected={expected_count} decoded={} trailing_bytes={}",
            items.len(),
            payload.len().saturating_sub(offset)
        ));
    }
    Ok(items)
}

fn encode_folder_batch_response(
    batch_id: u64,
    request_id: u64,
    result: Result<FolderAggregateQueryV1, String>,
) -> Vec<u8> {
    let mut bytes = [0_u8; FOLDER_BATCH_RESPONSE_BYTES];
    bytes[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    bytes[6..8].copy_from_slice(&FOLDER_BATCH_FRAME_ITEM.to_le_bytes());
    bytes[8..16].copy_from_slice(&batch_id.to_le_bytes());
    bytes[16..24].copy_from_slice(&request_id.to_le_bytes());
    let (status, aggregate, detail) = match result {
        Ok(value) if !value.partial => (0, value, None),
        Ok(value) => (3, value, None),
        Err(error) => (
            RESPONSE_STATUS_DETAILED_ERROR,
            FolderAggregateQueryV1::default(),
            Some(bounded_error_detail(error)),
        ),
    };
    bytes[24..26].copy_from_slice(&status.to_le_bytes());
    for (offset, value) in [
        (32, aggregate.generation),
        (40, aggregate.logical_bytes),
        (48, aggregate.allocated_bytes),
        (56, aggregate.file_count),
        (64, aggregate.directory_count),
    ] {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    let mut response = bytes.to_vec();
    if let Some(detail) = detail {
        response[26..28].copy_from_slice(&(detail.len() as u16).to_le_bytes());
        response.extend_from_slice(detail.as_bytes());
    }
    response
}

fn encode_folder_batch_end(batch_id: u64) -> [u8; FOLDER_BATCH_RESPONSE_BYTES] {
    let mut bytes = [0_u8; FOLDER_BATCH_RESPONSE_BYTES];
    bytes[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    bytes[6..8].copy_from_slice(&FOLDER_BATCH_FRAME_END.to_le_bytes());
    bytes[8..16].copy_from_slice(&batch_id.to_le_bytes());
    bytes
}

#[expect(
    unsafe_code,
    reason = "detecting MFT query client closure requires Win32 PeekNamedPipe"
)]
// SAFETY: handle remains owned and open for this loop; optional output buffers are null and
// the byte-count output points to initialized writable storage.
fn wait_for_client_close(handle: isize, stopped: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + CONTROL_RESPONSE_TIMEOUT;
    while !stopped() && std::time::Instant::now() < deadline {
        let connected = unsafe {
            PeekNamedPipe(
                handle,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if connected == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn encode_hierarchy_response(
    result: Result<Vec<crate::mft_size_map::MftProjectedNodeV1>, String>,
) -> Vec<u8> {
    let mut payload = Vec::new();
    let mut count = 0_usize;
    let status = match result {
        Ok(nodes) => {
            for node in nodes.into_iter().take(HIERARCHY_MAX_NODES) {
                let name = node.name.as_bytes();
                if name.len() > usize::from(u16::MAX)
                    || payload.len().saturating_add(35).saturating_add(name.len())
                        > HIERARCHY_MAX_BYTES
                {
                    payload.clear();
                    count = 0;
                    break;
                }
                payload.extend_from_slice(&node.reference.to_le_bytes());
                payload.extend_from_slice(&node.parent_reference.unwrap_or(u64::MAX).to_le_bytes());
                payload.extend_from_slice(&node.logical_bytes.to_le_bytes());
                payload.extend_from_slice(&node.allocated_bytes.to_le_bytes());
                payload.push(u8::from(node.is_directory));
                payload.extend_from_slice(&(name.len() as u16).to_le_bytes());
                payload.extend_from_slice(name);
                count += 1;
            }
            u16::from(count == 0)
        }
        Err(error) => {
            let error = error.chars().take(1024).collect::<String>();
            payload.extend_from_slice(error.as_bytes());
            1
        }
    };
    let mut response = Vec::with_capacity(HIERARCHY_HEADER_BYTES + payload.len());
    response.extend_from_slice(&MAGIC.to_le_bytes());
    response.extend_from_slice(&SCHEMA.to_le_bytes());
    response.extend_from_slice(&status.to_le_bytes());
    response.extend_from_slice(&(count as u32).to_le_bytes());
    response.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    response.extend_from_slice(&payload);
    response
}

fn encode_durability_diagnostics_response(
    result: Result<Vec<MftVolumeDiagnosticsV1>, String>,
) -> Vec<u8> {
    let (status, mut volumes) = match result {
        Ok(volumes) if volumes.len() <= DURABILITY_DIAGNOSTICS_MAX_VOLUMES => (0_u16, volumes),
        _ => (1_u16, Vec::new()),
    };
    volumes.sort_by_key(|volume| volume.volume);
    let mut bytes = Vec::with_capacity(
        DURABILITY_DIAGNOSTICS_HEADER_BYTES
            + volumes
                .len()
                .saturating_mul(DURABILITY_DIAGNOSTICS_RECORD_BYTES),
    );
    bytes.extend_from_slice(&MAGIC.to_le_bytes());
    bytes.extend_from_slice(&SCHEMA.to_le_bytes());
    bytes.extend_from_slice(&status.to_le_bytes());
    bytes.extend_from_slice(&(volumes.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&(DURABILITY_DIAGNOSTICS_RECORD_BYTES as u16).to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    for volume in volumes {
        let mut record = [0_u8; DURABILITY_DIAGNOSTICS_RECORD_BYTES];
        record[0] = volume.volume;
        record[1] = volume.mode;
        record[2] = volume.schema;
        record[3] = volume.migration_state;
        record[4] = volume.recovery_reason;
        record[5] = volume.transaction_last_outcome;
        record[6] = volume.checkpoint_last_outcome;
        record[7] = u8::from(volume.exact);
        for (offset, value) in [
            (8, volume.observed_journal_id),
            (
                16,
                u64::from_le_bytes(volume.observed_next_usn.to_le_bytes()),
            ),
            (24, volume.observed_generation),
            (32, volume.durable_journal_id),
            (
                40,
                u64::from_le_bytes(volume.durable_next_usn.to_le_bytes()),
            ),
            (48, volume.durable_generation),
            (56, volume.pending_count),
            (64, volume.pending_bytes),
            (72, volume.last_successful_commit_ms),
            (80, volume.focus_lease_count),
            (88, volume.focus_expiry_remaining_ms),
            (96, volume.main_bytes),
            (104, volume.wal_bytes),
            (112, volume.transaction_attempts),
            (120, volume.transaction_failures),
            (128, volume.checkpoint_attempts),
            (136, volume.checkpoint_failures),
        ] {
            record[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&record);
    }
    bytes
}

fn encode_diagnostics_breakdown_response(
    result: Result<MftCacheDiagnosticsV1, String>,
) -> [u8; DIAGNOSTICS_BREAKDOWN_RESPONSE_BYTES] {
    let mut bytes = [0_u8; DIAGNOSTICS_BREAKDOWN_RESPONSE_BYTES];
    bytes[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    let (status, diagnostics) = result
        .map(|value| (0_u16, value))
        .unwrap_or_else(|_| (1, MftCacheDiagnosticsV1::default()));
    bytes[6..8].copy_from_slice(&status.to_le_bytes());
    for (offset, value) in [
        (8, diagnostics.volume_index_bytes.unwrap_or_default()),
        (16, diagnostics.file_data_bytes.unwrap_or_default()),
        (24, diagnostics.aggregate_bytes.unwrap_or_default()),
        (
            32,
            diagnostics.persisted_index_limit_bytes.unwrap_or_default(),
        ),
        (40, diagnostics.volume_index_limit_bytes.unwrap_or_default()),
        (48, diagnostics.file_data_limit_bytes.unwrap_or_default()),
        (56, diagnostics.aggregate_limit_bytes.unwrap_or_default()),
    ] {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn encode_diagnostics_response(
    result: Result<MftCacheDiagnosticsV1, String>,
) -> [u8; DIAGNOSTICS_RESPONSE_BYTES] {
    let mut bytes = [0_u8; DIAGNOSTICS_RESPONSE_BYTES];
    bytes[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    let (status, diagnostics) = result
        .map(|value| (0_u16, value))
        .unwrap_or_else(|_| (1, MftCacheDiagnosticsV1::default()));
    bytes[6..8].copy_from_slice(&status.to_le_bytes());
    for (offset, value) in [
        (8, diagnostics.generation),
        (16, diagnostics.lru_bytes),
        (24, diagnostics.limit_bytes),
        (32, diagnostics.entry_count),
        (40, diagnostics.persisted_index_bytes),
        (48, diagnostics.hits),
        (56, diagnostics.misses),
    ] {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn encode_response(result: Result<FolderAggregateQueryV1, String>) -> Vec<u8> {
    let mut bytes = [0_u8; RESPONSE_BYTES];
    bytes[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    let (status, aggregate, error_detail) = match result {
        Ok(value) => (if value.partial { 3_u16 } else { 0_u16 }, value, None),
        Err(error) => (
            RESPONSE_STATUS_DETAILED_ERROR,
            FolderAggregateQueryV1::default(),
            Some(bounded_error_detail(error)),
        ),
    };
    bytes[6..8].copy_from_slice(&status.to_le_bytes());
    for (offset, value) in [
        (8, aggregate.generation),
        (16, aggregate.logical_bytes),
        (24, aggregate.allocated_bytes),
        (32, aggregate.file_count),
        (40, aggregate.directory_count),
    ] {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    let mut response = bytes.to_vec();
    if let Some(detail) = error_detail {
        let detail = detail.as_bytes();
        response[8..12].copy_from_slice(&(detail.len() as u32).to_le_bytes());
        response.extend_from_slice(detail);
    }
    response
}

fn bounded_error_detail(mut error: String) -> String {
    if error.len() <= ERROR_DETAIL_MAX_BYTES {
        return error;
    }
    let mut boundary = ERROR_DETAIL_MAX_BYTES;
    while !error.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    error.truncate(boundary);
    error
}

#[expect(
    unsafe_code,
    reason = "reading framed MFT query requests requires Win32 ReadFile and last-error inspection"
)]
// SAFETY: Each ReadFile call receives a writable slice pointer and its exact remaining length;
// the handle stays valid, and error state is consumed immediately.
fn read_exact(
    handle: isize,
    bytes: &mut [u8],
    stopped: impl Fn() -> bool,
    timeout: Duration,
) -> Result<(), String> {
    let mut offset = 0;
    let deadline = std::time::Instant::now() + timeout;
    while offset < bytes.len() && !stopped() {
        let mut read = 0_u32;
        let ok = unsafe {
            ReadFile(
                handle,
                bytes[offset..].as_mut_ptr().cast(),
                (bytes.len() - offset) as u32,
                &raw mut read,
                ptr::null_mut(),
            )
        };
        let error = (ok == 0).then(|| unsafe { GetLastError() });
        if read > 0 && (ok != 0 || error == Some(ERROR_MORE_DATA)) {
            offset += read as usize;
        } else {
            let error = error.unwrap_or_else(|| unsafe { GetLastError() });
            if error != ERROR_NO_DATA {
                return Err(format!("MFT query read failed ({error})"));
            }
            if std::time::Instant::now() >= deadline {
                return Err("MFT query read timed out".to_owned());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    (offset == bytes.len())
        .then_some(())
        .ok_or_else(|| "MFT query read was interrupted".to_owned())
}

#[expect(
    unsafe_code,
    reason = "writing framed MFT query responses requires Win32 WriteFile"
)]
// SAFETY: Each WriteFile call receives a readable slice pointer and exact remaining length;
// the pipe handle remains valid for the duration of the synchronous call.
fn write_all(handle: isize, bytes: &[u8]) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        let mut written = 0_u32;
        let ok = unsafe {
            WriteFile(
                handle,
                bytes[offset..].as_ptr().cast(),
                (bytes.len() - offset) as u32,
                &raw mut written,
                ptr::null_mut(),
            )
        };
        if ok == 0 || written == 0 {
            return Err(format!("MFT query write failed ({})", unsafe {
                GetLastError()
            }));
        }
        offset += written as usize;
    }
    Ok(())
}

#[expect(
    unsafe_code,
    reason = "bounded MFT response writes require Win32 WriteFile and last-error inspection"
)]
// SAFETY: Buffer and byte-count pointers remain valid during each synchronous WriteFile call;
// the handle is borrowed for the loop and errors are inspected immediately.
fn write_all_until(
    handle: isize,
    bytes: &[u8],
    stopped: impl Fn() -> bool,
    deadline: std::time::Instant,
) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() && !stopped() {
        let mut written = 0_u32;
        let ok = unsafe {
            WriteFile(
                handle,
                bytes[offset..].as_ptr().cast(),
                (bytes.len() - offset) as u32,
                &raw mut written,
                ptr::null_mut(),
            )
        };
        if written > 0 {
            offset += written as usize;
            continue;
        }
        let error = unsafe { GetLastError() };
        if ok == 0 && error == ERROR_NO_DATA && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        return Err(format!("MFT query write failed ({error})"));
    }
    (offset == bytes.len())
        .then_some(())
        .ok_or_else(|| "MFT query write was interrupted".to_owned())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn response_protocol_is_fixed_and_bounded() {
        let expected = FolderAggregateQueryV1 {
            generation: 7,
            logical_bytes: 11,
            allocated_bytes: 13,
            file_count: 17,
            directory_count: 19,
            partial: false,
        };
        let bytes = encode_response(Ok(expected));
        assert_eq!(bytes.len(), RESPONSE_BYTES);
        assert_eq!(read_u16(&bytes, 6), Some(0));
        assert_eq!(read_u64(&bytes, 8), Some(7));
        assert_eq!(read_u64(&bytes, 40), Some(19));
    }

    #[test]
    fn partial_response_uses_typed_status_without_changing_fixed_frame() {
        let expected = FolderAggregateQueryV1 {
            generation: 9,
            logical_bytes: 1_024,
            allocated_bytes: 4_096,
            file_count: 4,
            directory_count: 2,
            partial: true,
        };
        let bytes = encode_response(Ok(expected));
        assert_eq!(bytes.len(), RESPONSE_BYTES);
        assert_eq!(read_u16(&bytes, 6), Some(3));
        assert_eq!(read_u64(&bytes, 16), Some(1_024));
        let error = decode_folder_response(Path::new(r"D:\fixture"), &bytes, None).unwrap_err();
        assert!(error.contains("partial aggregate"));
        assert!(error.contains(r"D:\fixture"));
        assert!(error.contains("logical_bytes=1024"));
    }

    #[test]
    fn detailed_error_response_round_trips_bounded_utf8_payload() {
        let expected = "active-volume exact recovery failed: volume=D measured_volume_index_bytes=42 configured_volume_index_bytes=41";
        let bytes = encode_response(Err(expected.to_owned()));
        assert_eq!(read_u16(&bytes, 6), Some(RESPONSE_STATUS_DETAILED_ERROR));
        let length = read_u32(&bytes, 8).unwrap() as usize;
        assert_eq!(length, expected.len());
        assert_eq!(&bytes[RESPONSE_BYTES..], expected.as_bytes());
        assert_eq!(
            decode_folder_response(
                Path::new(r"D:\fixture"),
                &bytes[..RESPONSE_BYTES],
                Some(expected),
            ),
            Err(expected.to_owned())
        );

        let oversized = "錯".repeat(ERROR_DETAIL_MAX_BYTES);
        let bytes = encode_response(Err(oversized));
        let length = read_u32(&bytes, 8).unwrap() as usize;
        assert!(length <= ERROR_DETAIL_MAX_BYTES);
        assert!(std::str::from_utf8(&bytes[RESPONSE_BYTES..]).is_ok());
    }

    #[test]
    fn legacy_generic_errors_do_not_require_a_detail_payload() {
        let mut response = [0_u8; RESPONSE_BYTES];
        response[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        response[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
        response[6..8].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(
            decode_folder_response(Path::new(r"D:\fixture"), &response, None),
            Err("MFT Service has no aggregate for this folder".to_owned())
        );
        assert_eq!(detailed_error_length(&response), Ok(None));
    }

    #[test]
    fn detailed_error_length_rejects_zero_and_overflow() {
        let mut response = [0_u8; RESPONSE_BYTES];
        response[6..8].copy_from_slice(&RESPONSE_STATUS_DETAILED_ERROR.to_le_bytes());
        assert!(detailed_error_length(&response).is_err());
        response[8..12].copy_from_slice(&((ERROR_DETAIL_MAX_BYTES as u32) + 1).to_le_bytes());
        assert!(detailed_error_length(&response).is_err());
        response[8..12].copy_from_slice(&(ERROR_DETAIL_MAX_BYTES as u32).to_le_bytes());
        assert_eq!(
            detailed_error_length(&response),
            Ok(Some(ERROR_DETAIL_MAX_BYTES))
        );
    }

    #[test]
    fn folder_batch_codec_rejects_duplicate_truncated_and_cross_volume_items() {
        let item = |request_id: u64, letter: char, path: &str| {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&request_id.to_le_bytes());
            bytes.extend_from_slice(&(letter as u16).to_le_bytes());
            bytes.extend_from_slice(&(path.len() as u16).to_le_bytes());
            bytes.extend_from_slice(&99_u64.to_le_bytes());
            bytes.extend_from_slice(path.as_bytes());
            bytes
        };
        let mut duplicate = item(7, 'D', r"D:\one");
        duplicate.extend_from_slice(&item(7, 'D', r"D:\two"));
        assert!(decode_folder_batch_payload(&duplicate, 2).is_err());

        let truncated = item(8, 'D', r"D:\three");
        assert!(decode_folder_batch_payload(&truncated[..truncated.len() - 1], 1).is_err());

        let mismatch = item(9, 'C', r"D:\four");
        assert!(
            decode_folder_batch_payload(&mismatch, 1)
                .unwrap_err()
                .contains("path/volume identity mismatch")
        );
    }

    #[test]
    fn folder_batch_response_keeps_request_identity_and_bounded_detail() {
        let exact = FolderAggregateQueryV1 {
            generation: 4,
            logical_bytes: 5,
            allocated_bytes: 6,
            file_count: 7,
            directory_count: 8,
            partial: false,
        };
        let response = encode_folder_batch_response(11, 22, Ok(exact));
        assert_eq!(response.len(), FOLDER_BATCH_RESPONSE_BYTES);
        assert_eq!(read_u64(&response, 8), Some(11));
        assert_eq!(read_u64(&response, 16), Some(22));
        assert_eq!(read_u64(&response, 40), Some(5));

        let response = encode_folder_batch_response(11, 23, Err("錯".repeat(4_000)));
        assert_eq!(
            read_u16(&response, 24),
            Some(RESPONSE_STATUS_DETAILED_ERROR)
        );
        let detail_length = usize::from(read_u16(&response, 26).unwrap());
        assert!(detail_length <= ERROR_DETAIL_MAX_BYTES);
        assert!(std::str::from_utf8(&response[FOLDER_BATCH_RESPONSE_BYTES..]).is_ok());
        assert_eq!(response.len(), FOLDER_BATCH_RESPONSE_BYTES + detail_length);

        let end = encode_folder_batch_end(11);
        assert_eq!(read_u16(&end, 6), Some(FOLDER_BATCH_FRAME_END));
        assert_eq!(read_u64(&end, 8), Some(11));
    }

    #[test]
    fn diagnostics_protocol_is_fixed_aggregate_and_path_free() {
        let expected = MftCacheDiagnosticsV1 {
            generation: 2,
            lru_bytes: 3,
            limit_bytes: 4,
            entry_count: 5,
            persisted_index_bytes: 6,
            hits: 7,
            misses: 8,
            volume_index_bytes: Some(9),
            file_data_bytes: Some(10),
            aggregate_bytes: Some(11),
            persisted_index_limit_bytes: Some(12),
            volume_index_limit_bytes: Some(13),
            file_data_limit_bytes: Some(14),
            aggregate_limit_bytes: Some(15),
        };
        let bytes = encode_diagnostics_response(Ok(expected));
        assert_eq!(bytes.len(), DIAGNOSTICS_RESPONSE_BYTES);
        assert_eq!(read_u64(&bytes, 8), Some(2));
        assert_eq!(read_u64(&bytes, 56), Some(8));
        let breakdown = encode_diagnostics_breakdown_response(Ok(expected));
        assert_eq!(breakdown.len(), DIAGNOSTICS_BREAKDOWN_RESPONSE_BYTES);
        assert_eq!(read_u64(&breakdown, 8), Some(9));
        assert_eq!(read_u64(&breakdown, 16), Some(10));
        assert_eq!(read_u64(&breakdown, 24), Some(11));
        assert_eq!(read_u64(&breakdown, 32), Some(12));
        assert_eq!(read_u64(&breakdown, 56), Some(15));
        assert!(
            !format!("{expected:?}")
                .to_ascii_lowercase()
                .contains("path")
        );
    }

    #[test]
    fn diagnostics_pipe_security_is_local_system_and_interactive_user_only() {
        assert_eq!(
            PIPE_SECURITY_SDDL, "D:P(A;;GA;;;SY)(A;;GRGW;;;IU)",
            "the diagnostics endpoint must not grant anonymous, network, or broad authenticated-user access"
        );
        assert_ne!(
            PIPE_TYPE_MESSAGE_READMODE_REJECT_REMOTE & 0x0000_0008,
            0,
            "PIPE_REJECT_REMOTE_CLIENTS must remain enabled"
        );
    }

    #[test]
    fn durability_diagnostics_protocol_is_fixed_sorted_bounded_and_path_free() {
        let volume = |letter, seed| MftVolumeDiagnosticsV1 {
            volume: letter,
            mode: 1,
            schema: 1,
            migration_state: 2,
            recovery_reason: 3,
            transaction_last_outcome: 4,
            checkpoint_last_outcome: 5,
            exact: true,
            observed_journal_id: seed,
            observed_next_usn: -7,
            observed_generation: seed + 1,
            durable_journal_id: seed + 2,
            durable_next_usn: -9,
            durable_generation: seed + 3,
            pending_count: seed + 4,
            pending_bytes: seed + 5,
            last_successful_commit_ms: seed + 6,
            focus_lease_count: seed + 7,
            focus_expiry_remaining_ms: seed + 8,
            main_bytes: seed + 9,
            wal_bytes: seed + 10,
            transaction_attempts: seed + 11,
            transaction_failures: seed + 12,
            checkpoint_attempts: seed + 13,
            checkpoint_failures: seed + 14,
        };
        let bytes =
            encode_durability_diagnostics_response(Ok(vec![volume(b'Z', 100), volume(b'C', 10)]));
        assert_eq!(
            bytes.len(),
            DURABILITY_DIAGNOSTICS_HEADER_BYTES + 2 * DURABILITY_DIAGNOSTICS_RECORD_BYTES
        );
        assert_eq!(read_u16(&bytes, 6), Some(0));
        assert_eq!(read_u16(&bytes, 8), Some(2));
        assert_eq!(
            read_u16(&bytes, 10),
            Some(DURABILITY_DIAGNOSTICS_RECORD_BYTES as u16)
        );
        let first = &bytes[DURABILITY_DIAGNOSTICS_HEADER_BYTES..];
        assert_eq!(first[0], b'C');
        assert_eq!(read_u64(first, 8), Some(10));
        assert_eq!(read_u64(first, 136), Some(24));
        assert!(!bytes.windows(3).any(|window| window == b":\\"));

        let overflow = encode_durability_diagnostics_response(Ok(vec![
            MftVolumeDiagnosticsV1::default();
            DURABILITY_DIAGNOSTICS_MAX_VOLUMES + 1
        ]));
        assert_eq!(overflow.len(), DURABILITY_DIAGNOSTICS_HEADER_BYTES);
        assert_eq!(read_u16(&overflow, 6), Some(1));
        assert_eq!(read_u16(&overflow, 8), Some(0));
    }

    #[test]
    fn cache_budget_protocol_normalizes_five_independent_limits() {
        let limits = MftCacheBudgetLimitsV1 {
            persisted_index_mb: 1,
            volume_index_mb: u16::MAX,
            file_data_mb: 1,
            aggregate_mb: 16_384,
            lru_mb: 2_048,
        }
        .normalized();
        assert_eq!(limits.persisted_index_mb, 256);
        assert_eq!(limits.volume_index_mb, 16_384);
        assert_eq!(limits.file_data_mb, 64);
        assert_eq!(limits.aggregate_mb, 16_384);
        assert_eq!(limits.lru_mb, 2_048);
    }

    #[test]
    fn cache_budget_response_accepts_boundaries_and_rejects_malformed_or_old_service_frames() {
        let mut response = [0_u8; RESPONSE_BYTES];
        response[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        response[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
        for (offset, value) in [
            (8, 256_u16),
            (10, 16_384),
            (12, 64),
            (14, 128),
            (16, 16_384),
        ] {
            response[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        assert_eq!(
            decode_cache_budget_response(&response).unwrap(),
            MftCacheBudgetLimitsV1 {
                persisted_index_mb: 256,
                volume_index_mb: 16_384,
                file_data_mb: 64,
                aggregate_mb: 128,
                lru_mb: 16_384,
            }
        );

        assert!(decode_cache_budget_response(&response[..RESPONSE_BYTES - 1]).is_err());
        response[0] ^= 0xff;
        assert!(decode_cache_budget_response(&response).is_err());
        response[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        response[4..6].copy_from_slice(&0_u16.to_le_bytes());
        assert!(decode_cache_budget_response(&response).is_err());
        response[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
        response[6..8].copy_from_slice(&1_u16.to_le_bytes());
        assert!(decode_cache_budget_response(&response).is_err());
    }

    #[test]
    fn stopped_read_is_reported_as_interrupted_without_waiting_for_timeout() {
        let mut response = [0_u8; RESPONSE_BYTES];
        let started = std::time::Instant::now();
        let error = read_exact(
            INVALID_HANDLE_VALUE,
            &mut response,
            || true,
            CONTROL_RESPONSE_TIMEOUT,
        )
        .unwrap_err();
        assert!(error.contains("interrupted"));
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn local_named_pipe_round_trips_detailed_error_then_folder_aggregate() {
        TEST_PIPE_NAME
            .set(format!(
                r"\\.\pipe\SuperExplorerMftFolderSizeV1.Test.{}",
                std::process::id()
            ))
            .expect("test query pipe name is configured once per test process");
        let stopped = Arc::new(AtomicBool::new(false));
        let server_stopped = Arc::clone(&stopped);
        let active_queries = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let maximum_queries = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let server_active_queries = Arc::clone(&active_queries);
        let server_maximum_queries = Arc::clone(&maximum_queries);
        let server = std::thread::spawn(move || {
            let calls = std::sync::atomic::AtomicU8::new(0);
            serve_queries(
                || server_stopped.load(Ordering::Acquire),
                |_letter, _reference, cache_memory_mb, requested_path| {
                    assert_eq!(cache_memory_mb, 128);
                    let call = calls.fetch_add(1, Ordering::AcqRel);
                    if call == 0 {
                        return Err(
                            "active-volume exact recovery failed: volume=D stage=budget_or_rebuild"
                                .to_owned(),
                        );
                    }
                    let name = requested_path
                        .as_deref()
                        .and_then(Path::file_name)
                        .and_then(|value| value.to_str())
                        .unwrap_or_default();
                    if name == "batch-error" {
                        return Err("synthetic per-item failure".to_owned());
                    }
                    let (delay_ms, logical_bytes) = match name {
                        "batch-slow" => (120, 3_000),
                        "batch-medium" => (60, 2_000),
                        "batch-fast" => (10, 1_000),
                        _ => (0, 1_250),
                    };
                    let batch_query = name.starts_with("batch-");
                    if batch_query {
                        let active = server_active_queries.fetch_add(1, Ordering::AcqRel) + 1;
                        server_maximum_queries.fetch_max(active, Ordering::AcqRel);
                    }
                    std::thread::sleep(Duration::from_millis(delay_ms));
                    if batch_query {
                        server_active_queries.fetch_sub(1, Ordering::AcqRel);
                    }
                    Ok(FolderAggregateQueryV1 {
                        generation: 3,
                        logical_bytes,
                        allocated_bytes: 4_096,
                        file_count: 7,
                        directory_count: 3,
                        partial: false,
                    })
                },
                |_, _, _| Err("not used".to_owned()),
                || Ok(MftCacheDiagnosticsV1::default()),
                || Ok(Vec::new()),
                Ok,
            );
        });
        let root = std::env::temp_dir();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Err(error) = query_folder(&root, 64)
                && error.contains("active-volume exact recovery failed: volume=D")
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "detailed service error did not round-trip before deadline"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let result = loop {
            if let Ok(result) = query_folder(&root, 64)
                && result.logical_bytes == 1_250
            {
                break result;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "test pipe instance was not selected before deadline"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(result.logical_bytes, 1_250);
        assert_eq!(result.file_count, 7);

        let fixture =
            std::env::temp_dir().join(format!("superexplorer-mft-batch-{}", std::process::id()));
        let slow = fixture.join("batch-slow");
        let fast = fixture.join("batch-fast");
        let medium = fixture.join("batch-medium");
        let failed = fixture.join("batch-error");
        std::fs::create_dir_all(&slow).unwrap();
        std::fs::create_dir_all(&fast).unwrap();
        std::fs::create_dir_all(&medium).unwrap();
        std::fs::create_dir_all(&failed).unwrap();
        let requests = [
            FolderBatchRequestV1 {
                request_id: 11,
                path: slow,
            },
            FolderBatchRequestV1 {
                request_id: 12,
                path: fast,
            },
            FolderBatchRequestV1 {
                request_id: 13,
                path: medium,
            },
            FolderBatchRequestV1 {
                request_id: 14,
                path: failed,
            },
        ];
        let mut completion_order = Vec::new();
        query_folders_batch(
            &requests,
            64,
            || false,
            |completion| {
                completion_order.push((completion.request_id, completion.result.is_ok()));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            completion_order,
            vec![(14, false), (12, true), (13, true), (11, true)]
        );
        assert!((2..=FOLDER_BATCH_PARALLELISM).contains(&maximum_queries.load(Ordering::Acquire)));
        assert_eq!(active_queries.load(Ordering::Acquire), 0);
        std::fs::remove_dir_all(&fixture).unwrap();
        stopped.store(true, Ordering::Release);
        let stopped_at = std::time::Instant::now();
        server.join().unwrap();
        assert!(stopped_at.elapsed() < Duration::from_millis(250));
    }

    #[test]
    #[ignore = "requires the installed LocalSystem MFT Service and real NTFS folders"]
    fn real_installed_service_batches_return_an_exact_child_per_root_within_ten_seconds() {
        let roots = std::env::var("SUPEREXPLORER_REAL_BATCH_ROOTS")
            .unwrap_or_else(|_| r"D:\;D:\SuperExplorer;D:\UE_5.7".to_owned());
        for root in roots.split(';').map(Path::new) {
            eprintln!(
                "REAL_MFT_BATCH_BEFORE root={} diagnostics={:?}",
                root.display(),
                query_durability_diagnostics()
            );
            let children = std::fs::read_dir(root)
                .unwrap_or_else(|error| panic!("cannot enumerate {}: {error}", root.display()))
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .take(24)
                .enumerate()
                .map(|(index, path)| FolderBatchRequestV1 {
                    request_id: index as u64 + 1,
                    path,
                })
                .collect::<Vec<_>>();
            assert!(
                !children.is_empty(),
                "{} has no child folders to query",
                root.display()
            );
            let started = std::time::Instant::now();
            let mut exact = Vec::new();
            let mut errors = Vec::new();
            let outcome = query_folders_batch(
                &children,
                1_024,
                || false,
                |completion| {
                    match completion.result {
                        Ok(aggregate) => {
                            assert!(!aggregate.partial);
                            exact.push((
                                completion.request_id,
                                aggregate.logical_bytes,
                                started.elapsed(),
                            ));
                        }
                        Err(error) => errors.push((completion.request_id, error)),
                    }
                    Ok(())
                },
            );
            assert!(
                !exact.is_empty(),
                "{} returned no exact child within ten seconds: terminal={outcome:?} errors={errors:?} diagnostics={:?}",
                root.display(),
                query_durability_diagnostics(),
            );
            let (request_id, logical_bytes, elapsed) = exact[0];
            eprintln!(
                "REAL_MFT_BATCH_EXACT root={} child_request_id={} logical_bytes={} first_exact_ms={} exact_count={} terminal={outcome:?}",
                root.display(),
                request_id,
                logical_bytes,
                elapsed.as_millis(),
                exact.len(),
            );
        }
    }
}
