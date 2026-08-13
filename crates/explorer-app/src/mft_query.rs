#![cfg(windows)]

use std::{ffi::c_void, path::Path, ptr, time::Duration};

const PIPE_NAME: &str = r"\\.\pipe\SuperExplorerMftFolderSizeV1";
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
const REQUEST_PATH_MAX_BYTES: usize = 32 * 1024;
const HIERARCHY_HEADER_BYTES: usize = 16;
const HIERARCHY_MAX_NODES: usize = 100_000;
const HIERARCHY_MAX_BYTES: usize = 8 * 1024 * 1024;
const ERROR_PIPE_CONNECTED: u32 = 535;
const ERROR_PIPE_LISTENING: u32 = 536;
const ERROR_NO_DATA: u32 = 232;
const ERROR_MORE_DATA: u32 = 234;
const INVALID_HANDLE_VALUE: isize = -1;
/// The MFT service lazily builds the whole-volume folder aggregate on the first
/// query of a generation. A large volume (C: has ~2.2M entries) takes a few
/// seconds, so the client response window must tolerate the cold build instead
/// of failing after the old 2s budget.
const AGGREGATE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
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
    fn drop(&mut self) {
        if self.0 != 0 && self.0 != INVALID_HANDLE_VALUE {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

struct LocalMemory(*mut c_void);
impl Drop for LocalMemory {
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
    match read_u16(&response, 6).unwrap_or_default() {
        0 | 3 => Ok(FolderAggregateQueryV1 {
            generation: read_u64(&response, 8).unwrap_or_default(),
            logical_bytes: read_u64(&response, 16).unwrap_or_default(),
            allocated_bytes: read_u64(&response, 24).unwrap_or_default(),
            file_count: read_u64(&response, 32).unwrap_or_default(),
            directory_count: read_u64(&response, 40).unwrap_or_default(),
            partial: read_u16(&response, 6) == Some(3),
        }),
        1 => Err("MFT Service has no aggregate for this folder".to_owned()),
        2 => Err("MFT Service cache is temporarily unavailable".to_owned()),
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
    #[cfg(not(test))]
    let name = wide(PIPE_NAME);
    #[cfg(test)]
    let name = wide(test_pipe_name());
    let mut pipe = INVALID_HANDLE_VALUE;
    for _ in 0..attempts.max(1) {
        let _ = unsafe { WaitNamedPipeW(name.as_ptr(), 50) };
        pipe = unsafe {
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
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if pipe == INVALID_HANDLE_VALUE {
        return Err(format!("MFT query pipe unavailable ({})", unsafe {
            GetLastError()
        }));
    }
    Ok(Handle(pipe))
}

pub(crate) fn serve_folder_queries(
    stopped: impl Fn() -> bool,
    query: impl FnMut(char, u64, u16) -> Result<FolderAggregateQueryV1, String>,
) {
    let mut query = query;
    serve_queries(
        stopped,
        move |letter, reference, cache, _| query(letter, reference, cache),
        |_, _, _| Err("MFT hierarchy operation is unavailable".to_owned()),
        || Ok(MftCacheDiagnosticsV1::default()),
        || Ok(Vec::new()),
        |value| Ok(value),
    );
}

pub(crate) fn serve_queries(
    stopped: impl Fn() -> bool,
    mut query: impl FnMut(
        char,
        u64,
        u16,
        Option<std::path::PathBuf>,
    ) -> Result<FolderAggregateQueryV1, String>,
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
    let sddl = wide("D:P(A;;GA;;;SY)(A;;GRGW;;;IU)");
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
                0x0000_0001 | 0x0000_0008,
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

fn encode_response(result: Result<FolderAggregateQueryV1, String>) -> [u8; RESPONSE_BYTES] {
    let mut bytes = [0_u8; RESPONSE_BYTES];
    bytes[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&SCHEMA.to_le_bytes());
    let (status, aggregate) = match result {
        Ok(value) => (if value.partial { 3_u16 } else { 0_u16 }, value),
        Err(error) if error.contains("unavailable") => (2, FolderAggregateQueryV1::default()),
        Err(_) => (1, FolderAggregateQueryV1::default()),
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
    bytes
}

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
    fn local_named_pipe_returns_only_folder_aggregate() {
        TEST_PIPE_NAME
            .set(format!(
                r"\\.\pipe\SuperExplorerMftFolderSizeV1.Test.{}",
                std::process::id()
            ))
            .expect("test query pipe name is configured once per test process");
        let stopped = Arc::new(AtomicBool::new(false));
        let server_stopped = Arc::clone(&stopped);
        let server = std::thread::spawn(move || {
            serve_folder_queries(
                || server_stopped.load(Ordering::Acquire),
                |_letter, _reference, cache_memory_mb| {
                    assert_eq!(cache_memory_mb, 128);
                    Ok(FolderAggregateQueryV1 {
                        generation: 3,
                        logical_bytes: 1_250,
                        allocated_bytes: 4_096,
                        file_count: 7,
                        directory_count: 3,
                        partial: false,
                    })
                },
            );
        });
        let root = std::env::temp_dir();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
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
        stopped.store(true, Ordering::Release);
        let stopped_at = std::time::Instant::now();
        server.join().unwrap();
        assert!(stopped_at.elapsed() < Duration::from_millis(250));
    }
}
