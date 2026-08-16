//! Bounded, discover-only process current-directory ownership for Windows.
#![allow(
    unsafe_code,
    reason = "audited remote process reads are required to discover Windows current directories"
)]

use std::{
    ffi::c_void,
    mem::{MaybeUninit, size_of},
    os::windows::ffi::OsStringExt as _,
    path::{Path, PathBuf},
    time::Instant,
};

use explorer_common::{ExplorerError, ExplorerErrorKind, RoadmapLimits};
use explorer_model::{
    LockOwner, LockOwnerApplicationType, LockOwnerEligibility, LockOwnerIdentity,
};
use windows::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE},
    System::{
        Diagnostics::{
            Debug::ReadProcessMemory,
            ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
        },
        Threading::{
            GetCurrentProcessId, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_VM_READ,
        },
    },
};

const MAX_PROCESS_CANDIDATES: usize = 4_096;
const MAX_CURRENT_DIRECTORY_UTF16: usize = 32_768;
const PROCESS_BASIC_INFORMATION_CLASS: u32 = 0;
const PROCESS_WOW64_INFORMATION_CLASS: u32 = 26;
const PEB64_PROCESS_PARAMETERS_OFFSET: usize = 0x20;
const PEB32_PROCESS_PARAMETERS_OFFSET: usize = 0x10;
const PARAMETERS64_CURRENT_DIRECTORY_OFFSET: usize = 0x38;
const PARAMETERS32_CURRENT_DIRECTORY_OFFSET: usize = 0x24;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        process_handle: HANDLE,
        process_information_class: u32,
        process_information: *mut c_void,
        process_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ProcessBasicInformation {
    reserved1: *mut c_void,
    peb_base_address: *mut c_void,
    reserved2: [*mut c_void; 2],
    unique_process_id: usize,
    reserved3: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeString64 {
    length: u16,
    maximum_length: u16,
    padding: u32,
    buffer: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeString32 {
    length: u16,
    maximum_length: u16,
    buffer: u32,
}

struct OwnedHandle(
    HANDLE,
    #[cfg(test)] Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
);

impl OwnedHandle {
    const fn new(handle: HANDLE) -> Self {
        Self(
            handle,
            #[cfg(test)]
            None,
        )
    }

    #[cfg(test)]
    fn tracked(handle: HANDLE, drops: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> Self {
        Self(handle, Some(drops))
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: this object uniquely owns a live snapshot or process handle.
        let _ = unsafe { CloseHandle(self.0) };
        #[cfg(test)]
        if let Some(drops) = &self.1 {
            drops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// Owners projected for one input resource index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentDirectoryOwnerMatch {
    pub resource_index: usize,
    pub owners: Vec<LockOwner>,
}

/// Batch-level terminal for the one-snapshot current-directory source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CurrentDirectoryOwnerBatchTerminal {
    Complete(Vec<CurrentDirectoryOwnerMatch>),
    Cancelled,
    DeadlineElapsed,
    Unavailable(ExplorerError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProbeStop {
    Cancelled,
    DeadlineElapsed,
}

fn check_probe_control(
    is_cancelled: &(dyn Fn() -> bool + Sync),
    deadline: Option<Instant>,
) -> Result<(), ProbeStop> {
    if is_cancelled() {
        return Err(ProbeStop::Cancelled);
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(ProbeStop::DeadlineElapsed);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NormalizedWindowsPath {
    components: Vec<String>,
}

impl NormalizedWindowsPath {
    fn parse(path: &Path) -> Option<Self> {
        let mut text = path.as_os_str().to_string_lossy().replace('/', "\\");
        if text.starts_with("\\\\?\\UNC\\") || text.starts_with("\\\\?\\unc\\") {
            text = format!("\\\\{}", &text[8..]);
        } else if text.starts_with("\\\\?\\") {
            text = text[4..].to_owned();
        }

        let is_unc = text.starts_with("\\\\");
        let is_drive_absolute = text.len() >= 3
            && text.as_bytes().get(1) == Some(&b':')
            && text.as_bytes().get(2) == Some(&b'\\');
        if !is_unc && !is_drive_absolute {
            return None;
        }

        let mut components = Vec::new();
        if is_unc {
            components.push("unc".to_owned());
        }
        for component in text.split('\\').filter(|component| !component.is_empty()) {
            if matches!(component, "." | "..") {
                return None;
            }
            components.push(component.to_lowercase());
        }
        if (is_unc && components.len() < 3) || (!is_unc && components.is_empty()) {
            return None;
        }
        Some(Self { components })
    }

    fn contains(&self, candidate: &Self) -> bool {
        self.components.len() <= candidate.components.len()
            && self
                .components
                .iter()
                .zip(&candidate.components)
                .all(|(left, right)| left == right)
    }
}

#[cfg(test)]
fn current_directory_occupies(
    resource: &Path,
    resource_is_directory: bool,
    current_directory: &Path,
) -> bool {
    if !resource_is_directory {
        return false;
    }
    let Some(resource) = NormalizedWindowsPath::parse(resource) else {
        return false;
    };
    let Some(current_directory) = NormalizedWindowsPath::parse(current_directory) else {
        return false;
    };
    resource.contains(&current_directory)
}

fn next_candidate_count(current: usize) -> Result<usize, ExplorerError> {
    let next = current.saturating_add(1);
    if next > MAX_PROCESS_CANDIDATES {
        return Err(discovery_error(
            "enumerate process snapshot",
            "process candidate count exceeded the bounded contract",
            None,
        ));
    }
    Ok(next)
}

fn project_inspected_candidate(
    directories: &[(bool, Option<NormalizedWindowsPath>)],
    matches: &mut [CurrentDirectoryOwnerMatch],
    inspected: Option<(PathBuf, LockOwner)>,
) {
    let Some((current_directory, owner)) = inspected else {
        return;
    };
    let Some(candidate) = NormalizedWindowsPath::parse(&current_directory) else {
        return;
    };
    for (resource_index, (is_directory, resource)) in directories.iter().enumerate() {
        if *is_directory
            && resource
                .as_ref()
                .is_some_and(|resource| resource.contains(&candidate))
        {
            matches[resource_index].owners.push(owner.clone());
        }
    }
}

fn finalize_current_directory_matches(matches: &mut [CurrentDirectoryOwnerMatch]) {
    let maximum_owners = RoadmapLimits::default().lock_recovery_max_owners;
    for item in matches {
        item.owners.sort_by(|left, right| {
            left.identity
                .process_id
                .cmp(&right.identity.process_id)
                .then_with(|| {
                    left.identity
                        .creation_time_100ns
                        .cmp(&right.identity.creation_time_100ns)
                })
                .then_with(|| {
                    left.display_name
                        .to_lowercase()
                        .cmp(&right.display_name.to_lowercase())
                })
                .then_with(|| application_type_rank(left).cmp(&application_type_rank(right)))
        });
        item.owners.dedup_by_key(|owner| {
            (
                owner.identity.process_id,
                owner.identity.creation_time_100ns,
            )
        });
        item.owners.truncate(maximum_owners);
    }
}

/// Discovers process current directories once for the complete authorized resource batch.
pub fn discover_current_directory_owners_read_only(
    resources: &[PathBuf],
    is_cancelled: &(dyn Fn() -> bool + Sync),
    deadline: Option<Instant>,
) -> CurrentDirectoryOwnerBatchTerminal {
    if is_cancelled() {
        return CurrentDirectoryOwnerBatchTerminal::Cancelled;
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return CurrentDirectoryOwnerBatchTerminal::DeadlineElapsed;
    }
    let directories = resources
        .iter()
        .map(|path| {
            let is_directory = std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir());
            (is_directory, NormalizedWindowsPath::parse(path))
        })
        .collect::<Vec<_>>();
    let mut matches = (0..resources.len())
        .map(|resource_index| CurrentDirectoryOwnerMatch {
            resource_index,
            owners: Vec::new(),
        })
        .collect::<Vec<_>>();
    if directories
        .iter()
        .all(|(is_directory, normalized)| !is_directory || normalized.is_none())
    {
        return CurrentDirectoryOwnerBatchTerminal::Complete(matches);
    }

    // SAFETY: the returned snapshot handle is immediately wrapped in unique RAII ownership.
    let snapshot = match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) } {
        Ok(snapshot) => OwnedHandle::new(snapshot),
        Err(error) => {
            return CurrentDirectoryOwnerBatchTerminal::Unavailable(discovery_error(
                "create process snapshot",
                "Toolhelp could not create a bounded process snapshot",
                Some(error.code().0),
            ));
        }
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    // SAFETY: `entry` is writable and has the required size for this snapshot call.
    if let Err(error) = unsafe { Process32FirstW(snapshot.0, &raw mut entry) } {
        return CurrentDirectoryOwnerBatchTerminal::Unavailable(discovery_error(
            "enumerate process snapshot",
            "Toolhelp could not read the first process entry",
            Some(error.code().0),
        ));
    }

    let current_process_id = unsafe { GetCurrentProcessId() };
    let mut candidates = 0_usize;
    loop {
        if is_cancelled() {
            return CurrentDirectoryOwnerBatchTerminal::Cancelled;
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return CurrentDirectoryOwnerBatchTerminal::DeadlineElapsed;
        }
        candidates = match next_candidate_count(candidates) {
            Ok(candidates) => candidates,
            Err(error) => return CurrentDirectoryOwnerBatchTerminal::Unavailable(error),
        };
        let inspected = if entry.th32ProcessID != 0 && entry.th32ProcessID != current_process_id {
            inspect_process(&entry, is_cancelled, deadline)
        } else {
            Ok(None)
        };
        let inspected = match inspected {
            Ok(value) => value,
            Err(ProbeStop::Cancelled) => return CurrentDirectoryOwnerBatchTerminal::Cancelled,
            Err(ProbeStop::DeadlineElapsed) => {
                return CurrentDirectoryOwnerBatchTerminal::DeadlineElapsed;
            }
        };
        project_inspected_candidate(&directories, &mut matches, inspected);
        if is_cancelled() {
            return CurrentDirectoryOwnerBatchTerminal::Cancelled;
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return CurrentDirectoryOwnerBatchTerminal::DeadlineElapsed;
        }
        // SAFETY: `entry` remains writable and belongs to the live snapshot.
        if unsafe { Process32NextW(snapshot.0, &raw mut entry) }.is_err() {
            break;
        }
    }

    finalize_current_directory_matches(&mut matches);
    CurrentDirectoryOwnerBatchTerminal::Complete(matches)
}

fn inspect_process(
    entry: &PROCESSENTRY32W,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    deadline: Option<Instant>,
) -> Result<Option<(PathBuf, LockOwner)>, ProbeStop> {
    check_probe_control(is_cancelled, deadline)?;
    // SAFETY: access is read/query-only and the returned handle is uniquely owned below.
    let Ok(handle) = (unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            false,
            entry.th32ProcessID,
        )
    }) else {
        return Ok(None);
    };
    let handle = OwnedHandle::new(handle);
    check_probe_control(is_cancelled, deadline)?;
    let Some(current_directory) = query_current_directory(handle.0, is_cancelled, deadline)? else {
        return Ok(None);
    };
    check_probe_control(is_cancelled, deadline)?;
    let Some(creation_time_100ns) = process_creation_time(handle.0, is_cancelled, deadline)? else {
        return Ok(None);
    };
    let identity = LockOwnerIdentity {
        process_id: entry.th32ProcessID,
        creation_time_100ns,
    };
    let length = entry
        .szExeFile
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(entry.szExeFile.len());
    let display_name = String::from_utf16_lossy(&entry.szExeFile[..length]);
    if display_name.is_empty() {
        return Ok(None);
    }
    Ok(Some((
        current_directory,
        LockOwner {
            identity,
            display_name,
            application_type: LockOwnerApplicationType::Console,
            restartable: false,
            eligibility: LockOwnerEligibility::Protected,
        },
    )))
}

fn query_current_directory(
    handle: HANDLE,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    deadline: Option<Instant>,
) -> Result<Option<PathBuf>, ProbeStop> {
    check_probe_control(is_cancelled, deadline)?;
    let mut wow64_peb = 0_usize;
    let mut returned = 0_u32;
    // SAFETY: the output buffer is writable for the declared size.
    let wow64_status = unsafe {
        NtQueryInformationProcess(
            handle,
            PROCESS_WOW64_INFORMATION_CLASS,
            (&raw mut wow64_peb).cast(),
            u32::try_from(size_of::<usize>()).unwrap_or(u32::MAX),
            &raw mut returned,
        )
    };
    check_probe_control(is_cancelled, deadline)?;
    if wow64_status >= 0 && wow64_peb != 0 {
        return query_current_directory32(handle, wow64_peb, is_cancelled, deadline);
    }

    let mut information = MaybeUninit::<ProcessBasicInformation>::zeroed();
    // SAFETY: the output buffer is writable for the declared size.
    let status = unsafe {
        NtQueryInformationProcess(
            handle,
            PROCESS_BASIC_INFORMATION_CLASS,
            information.as_mut_ptr().cast(),
            u32::try_from(size_of::<ProcessBasicInformation>()).unwrap_or(u32::MAX),
            &raw mut returned,
        )
    };
    check_probe_control(is_cancelled, deadline)?;
    if status < 0 {
        return Ok(None);
    }
    // SAFETY: successful `NtQueryInformationProcess` initialized the complete structure.
    let information = unsafe { information.assume_init() };
    query_current_directory64(
        handle,
        information.peb_base_address as usize,
        is_cancelled,
        deadline,
    )
}

fn query_current_directory64(
    handle: HANDLE,
    peb: usize,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    deadline: Option<Instant>,
) -> Result<Option<PathBuf>, ProbeStop> {
    let Some(parameters): Option<u64> = read_remote(
        handle,
        checked_address(peb, PEB64_PROCESS_PARAMETERS_OFFSET).unwrap_or(0),
        is_cancelled,
        deadline,
    )?
    else {
        return Ok(None);
    };
    let Some(descriptor): Option<UnicodeString64> = read_remote(
        handle,
        checked_address(
            usize::try_from(parameters).unwrap_or(0),
            PARAMETERS64_CURRENT_DIRECTORY_OFFSET,
        )
        .unwrap_or(0),
        is_cancelled,
        deadline,
    )?
    else {
        return Ok(None);
    };
    read_remote_unicode(
        handle,
        usize::try_from(descriptor.buffer).unwrap_or(0),
        descriptor.length,
        descriptor.maximum_length,
        is_cancelled,
        deadline,
    )
}

fn query_current_directory32(
    handle: HANDLE,
    peb: usize,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    deadline: Option<Instant>,
) -> Result<Option<PathBuf>, ProbeStop> {
    let Some(parameters): Option<u32> = read_remote(
        handle,
        checked_address(peb, PEB32_PROCESS_PARAMETERS_OFFSET).unwrap_or(0),
        is_cancelled,
        deadline,
    )?
    else {
        return Ok(None);
    };
    let Some(descriptor): Option<UnicodeString32> = read_remote(
        handle,
        checked_address(
            usize::try_from(parameters).unwrap_or(0),
            PARAMETERS32_CURRENT_DIRECTORY_OFFSET,
        )
        .unwrap_or(0),
        is_cancelled,
        deadline,
    )?
    else {
        return Ok(None);
    };
    read_remote_unicode(
        handle,
        usize::try_from(descriptor.buffer).unwrap_or(0),
        descriptor.length,
        descriptor.maximum_length,
        is_cancelled,
        deadline,
    )
}

fn checked_address(base: usize, offset: usize) -> Option<usize> {
    let address = base.checked_add(offset)?;
    (address != 0).then_some(address)
}

fn read_remote<T: Copy>(
    handle: HANDLE,
    address: usize,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    deadline: Option<Instant>,
) -> Result<Option<T>, ProbeStop> {
    check_probe_control(is_cancelled, deadline)?;
    if checked_address(address, size_of::<T>()).is_none() {
        return Ok(None);
    }
    let mut value = MaybeUninit::<T>::uninit();
    let mut bytes_read = 0_usize;
    // SAFETY: `value` is writable for exactly `size_of::<T>()`; the remote address is never
    // dereferenced locally and Windows validates its readability in the target process.
    let result = unsafe {
        ReadProcessMemory(
            handle,
            address as *const c_void,
            value.as_mut_ptr().cast(),
            size_of::<T>(),
            Some(&raw mut bytes_read),
        )
    };
    check_probe_control(is_cancelled, deadline)?;
    if result.is_err() {
        return Ok(None);
    }
    if bytes_read != size_of::<T>() {
        return Ok(None);
    }
    // SAFETY: an exact successful copy initialized all bytes of `T`, and every use is a plain
    // integer/pointer-layout structure with no Rust references or invalid value constraints.
    Ok(Some(unsafe { value.assume_init() }))
}

fn read_remote_unicode(
    handle: HANDLE,
    buffer: usize,
    length_bytes: u16,
    maximum_length_bytes: u16,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    deadline: Option<Instant>,
) -> Result<Option<PathBuf>, ProbeStop> {
    check_probe_control(is_cancelled, deadline)?;
    let Some(length) = checked_remote_unicode_length(buffer, length_bytes, maximum_length_bytes)
    else {
        return Ok(None);
    };
    if checked_address(buffer, length).is_none() {
        return Ok(None);
    }
    let mut wide = vec![0_u16; length / size_of::<u16>()];
    let mut bytes_read = 0_usize;
    // SAFETY: `wide` owns a writable `length`-byte region and Windows validates the remote range.
    let result = unsafe {
        ReadProcessMemory(
            handle,
            buffer as *const c_void,
            wide.as_mut_ptr().cast(),
            length,
            Some(&raw mut bytes_read),
        )
    };
    check_probe_control(is_cancelled, deadline)?;
    if result.is_err() {
        return Ok(None);
    }
    if bytes_read != length {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(&wide))))
}

fn checked_remote_unicode_length(
    buffer: usize,
    length_bytes: u16,
    maximum_length_bytes: u16,
) -> Option<usize> {
    let length = usize::from(length_bytes);
    let maximum = usize::from(maximum_length_bytes);
    (buffer != 0
        && length != 0
        && length % size_of::<u16>() == 0
        && length <= maximum
        && length / size_of::<u16>() <= MAX_CURRENT_DIRECTORY_UTF16)
        .then_some(length)
}

fn process_creation_time(
    handle: HANDLE,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    deadline: Option<Instant>,
) -> Result<Option<u64>, ProbeStop> {
    check_probe_control(is_cancelled, deadline)?;
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: all four FILETIME outputs are writable for this process query.
    let result = unsafe {
        GetProcessTimes(
            handle,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    };
    check_probe_control(is_cancelled, deadline)?;
    if result.is_err() {
        return Ok(None);
    }
    Ok(Some(
        (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
    ))
}

const fn application_type_rank(owner: &LockOwner) -> u8 {
    match owner.application_type {
        LockOwnerApplicationType::Unknown => 0,
        LockOwnerApplicationType::MainWindow => 1,
        LockOwnerApplicationType::OtherWindow => 2,
        LockOwnerApplicationType::Service => 3,
        LockOwnerApplicationType::Explorer => 4,
        LockOwnerApplicationType::Console => 5,
        LockOwnerApplicationType::Critical => 6,
    }
}

fn discovery_error(
    operation: &'static str,
    detail: &'static str,
    code: Option<i32>,
) -> ExplorerError {
    let mut error = ExplorerError::new(
        ExplorerErrorKind::Availability,
        operation,
        true,
        "Windows process current-directory information is temporarily unavailable.",
        detail,
    );
    if let Some(code) = code {
        error = error.with_native_code(code);
    }
    error
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write as _,
        os::windows::process::CommandExt as _,
        process::{Command, Stdio},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::*;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn candidate_owner(process_id: u32, creation_time_100ns: u64, name: &str) -> LockOwner {
        LockOwner {
            identity: LockOwnerIdentity {
                process_id,
                creation_time_100ns,
            },
            display_name: name.to_owned(),
            application_type: LockOwnerApplicationType::Console,
            restartable: false,
            eligibility: LockOwnerEligibility::Protected,
        }
    }

    #[derive(Clone, Copy)]
    enum InjectedHandleTerminal {
        Success,
        TypedError,
        Cancelled,
        Deadline,
        Panic,
    }

    fn run_tracked_handle_scope(
        terminal: InjectedHandleTerminal,
        drops: Arc<AtomicUsize>,
    ) -> Result<(), ProbeStop> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .expect("create tracked snapshot");
        let _snapshot = OwnedHandle::tracked(snapshot, drops.clone());
        let process = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                false,
                GetCurrentProcessId(),
            )
        }
        .expect("open tracked process handle");
        let _process = OwnedHandle::tracked(process, drops);
        match terminal {
            InjectedHandleTerminal::Success => Ok(()),
            InjectedHandleTerminal::TypedError | InjectedHandleTerminal::Cancelled => {
                Err(ProbeStop::Cancelled)
            }
            InjectedHandleTerminal::Deadline => Err(ProbeStop::DeadlineElapsed),
            InjectedHandleTerminal::Panic => panic!("injected tracked-handle panic"),
        }
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "IsWow64Process2"]
        fn is_wow64_process_2(
            process: HANDLE,
            process_machine: *mut u16,
            native_machine: *mut u16,
        ) -> i32;
    }

    #[test]
    fn component_ancestry_handles_local_unc_extended_and_prefix_boundaries() {
        assert!(current_directory_occupies(
            Path::new(r"D:\"),
            true,
            Path::new(r"d:\folder")
        ));
        assert!(current_directory_occupies(
            Path::new(r"D:\AI\\ComfyUI\"),
            true,
            Path::new(r"d:\ai\ComfyUI")
        ));
        assert!(current_directory_occupies(
            Path::new(r"D:\AI_Pic\ComfyUI"),
            true,
            Path::new(r"d:\ai_pic\ComfyUI\subfolder")
        ));
        assert!(!current_directory_occupies(
            Path::new(r"D:\AI"),
            true,
            Path::new(r"D:\AI_Picture")
        ));
        assert!(current_directory_occupies(
            Path::new(r"\\server\share\\parent\"),
            true,
            Path::new(r"\\?\UNC\SERVER\SHARE\parent\child")
        ));
        assert!(!current_directory_occupies(
            Path::new(r"\\server\share"),
            true,
            Path::new(r"\\server\share-two\child")
        ));
        assert!(current_directory_occupies(
            Path::new(r"\\?\D:\root"),
            true,
            Path::new(r"D:\root\child")
        ));
    }

    #[test]
    fn matcher_rejects_files_relative_paths_and_unresolved_traversal() {
        assert!(!current_directory_occupies(
            Path::new(r"D:\root\file.txt"),
            false,
            Path::new(r"D:\root")
        ));
        assert!(!current_directory_occupies(
            Path::new(r"D:relative"),
            true,
            Path::new(r"D:\relative")
        ));
        assert!(!current_directory_occupies(
            Path::new(r"D:\root\..\other"),
            true,
            Path::new(r"D:\other")
        ));
    }

    #[test]
    fn checked_remote_contract_rejects_overflow_and_malformed_unicode() {
        assert!(checked_address(usize::MAX, 1).is_none());
        assert!(checked_address(1, 1).is_some());
        assert_eq!(checked_remote_unicode_length(1, 4, 4), Some(4));
        assert_eq!(checked_remote_unicode_length(0, 4, 4), None);
        assert_eq!(checked_remote_unicode_length(1, 0, 4), None);
        assert_eq!(checked_remote_unicode_length(1, 3, 4), None);
        assert_eq!(checked_remote_unicode_length(1, 6, 4), None);
        assert_eq!(MAX_PROCESS_CANDIDATES, 4_096);
        assert_eq!(MAX_CURRENT_DIRECTORY_UTF16, 32_768);
    }

    #[test]
    fn candidate_overflow_is_unavailable_before_partial_projection() {
        let mut count = 0;
        for expected in 1..=MAX_PROCESS_CANDIDATES {
            count = next_candidate_count(count).expect("bounded candidate");
            assert_eq!(count, expected);
        }
        let error = next_candidate_count(count).expect_err("4,097th candidate must fail closed");
        assert_eq!(error.kind, ExplorerErrorKind::Availability);
        assert_eq!(count, MAX_PROCESS_CANDIDATES);
    }

    #[test]
    fn access_denied_and_exit_races_skip_only_the_affected_candidates() {
        let directories = vec![(true, NormalizedWindowsPath::parse(Path::new(r"D:\fixture")))];
        let mut matches = vec![CurrentDirectoryOwnerMatch {
            resource_index: 0,
            owners: Vec::new(),
        }];
        project_inspected_candidate(
            &directories,
            &mut matches,
            Some((
                PathBuf::from(r"D:\fixture\first"),
                candidate_owner(10, 100, "first.exe"),
            )),
        );
        // `inspect_process` maps both an access-denied OpenProcess result and a process that
        // exits during a later read to `None`; each must be a local skip, not a batch terminal.
        project_inspected_candidate(&directories, &mut matches, None);
        project_inspected_candidate(&directories, &mut matches, None);
        project_inspected_candidate(
            &directories,
            &mut matches,
            Some((
                PathBuf::from(r"D:\fixture\last"),
                candidate_owner(20, 200, "last.exe"),
            )),
        );
        finalize_current_directory_matches(&mut matches);
        assert_eq!(
            matches[0]
                .owners
                .iter()
                .map(|owner| owner.identity.process_id)
                .collect::<Vec<_>>(),
            vec![10, 20]
        );
    }

    #[test]
    fn candidate_projection_is_independent_of_snapshot_order() {
        let directories = vec![(true, NormalizedWindowsPath::parse(Path::new(r"D:\fixture")))];
        let candidates = [
            (
                PathBuf::from(r"D:\fixture\z"),
                candidate_owner(30, 300, "z.exe"),
            ),
            (
                PathBuf::from(r"D:\fixture\a"),
                candidate_owner(10, 100, "a.exe"),
            ),
            (
                PathBuf::from(r"D:\fixture\m"),
                candidate_owner(20, 200, "m.exe"),
            ),
        ];
        let project = |order: &[usize]| {
            let mut matches = vec![CurrentDirectoryOwnerMatch {
                resource_index: 0,
                owners: Vec::new(),
            }];
            for index in order {
                project_inspected_candidate(
                    &directories,
                    &mut matches,
                    Some(candidates[*index].clone()),
                );
            }
            finalize_current_directory_matches(&mut matches);
            matches
        };
        assert_eq!(project(&[0, 1, 2]), project(&[2, 0, 1]));
    }

    #[test]
    fn snapshot_and_process_handles_close_on_success_and_typed_error() {
        for terminal in [
            InjectedHandleTerminal::Success,
            InjectedHandleTerminal::TypedError,
        ] {
            let drops = Arc::new(AtomicUsize::new(0));
            let _ = run_tracked_handle_scope(terminal, drops.clone());
            assert_eq!(drops.load(Ordering::SeqCst), 2);
        }
    }

    #[test]
    fn snapshot_and_process_handles_close_on_cancellation_and_deadline() {
        for terminal in [
            InjectedHandleTerminal::Cancelled,
            InjectedHandleTerminal::Deadline,
        ] {
            let drops = Arc::new(AtomicUsize::new(0));
            let _ = run_tracked_handle_scope(terminal, drops.clone());
            assert_eq!(drops.load(Ordering::SeqCst), 2);
        }
    }

    #[test]
    fn snapshot_and_process_handles_close_after_injected_panic() {
        let drops = Arc::new(AtomicUsize::new(0));
        let unwind_drops = drops.clone();
        let result = std::panic::catch_unwind(move || {
            let _ = run_tracked_handle_scope(InjectedHandleTerminal::Panic, unwind_drops);
        });
        assert!(result.is_err());
        assert_eq!(drops.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn remote_read_control_checks_cancel_and_deadline_before_native_access() {
        assert_eq!(
            read_remote::<u64>(HANDLE::default(), 1, &|| true, None),
            Err(ProbeStop::Cancelled)
        );
        assert_eq!(
            read_remote::<u64>(
                HANDLE::default(),
                1,
                &|| false,
                Some(
                    Instant::now()
                        .checked_sub(Duration::from_millis(1))
                        .unwrap(),
                ),
            ),
            Err(ProbeStop::DeadlineElapsed)
        );
    }

    #[test]
    fn batch_control_terminals_dominate_before_snapshot_creation() {
        let current = std::env::current_dir().expect("current directory");
        assert_eq!(
            discover_current_directory_owners_read_only(
                std::slice::from_ref(&current),
                &|| true,
                None,
            ),
            CurrentDirectoryOwnerBatchTerminal::Cancelled
        );
        assert_eq!(
            discover_current_directory_owners_read_only(
                &[current],
                &|| false,
                Some(
                    Instant::now()
                        .checked_sub(Duration::from_millis(1))
                        .unwrap(),
                ),
            ),
            CurrentDirectoryOwnerBatchTerminal::DeadlineElapsed
        );
    }

    #[test]
    fn current_process_is_excluded_from_the_batch_snapshot() {
        let current = std::env::current_dir().expect("current directory");
        let deadline = Instant::now() + Duration::from_secs(5);
        let CurrentDirectoryOwnerBatchTerminal::Complete(items) =
            discover_current_directory_owners_read_only(&[current], &|| false, Some(deadline))
        else {
            panic!("current-process exclusion probe must complete");
        };
        let current_process_id = unsafe { GetCurrentProcessId() };
        assert!(items.iter().all(|item| {
            item.owners
                .iter()
                .all(|owner| owner.identity.process_id != current_process_id)
        }));
    }

    #[test]
    fn live_cmd_current_directory_projects_to_exact_and_parent_folders() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("superexplorer-cwd-owner-{nonce}"));
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).expect("create current-directory fixture");
        let cmd = PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot"))
            .join("System32")
            .join("cmd.exe");
        let mut child = Command::new(cmd)
            .args(["/D", "/Q", "/C", "ping -n 31 127.0.0.1 >nul"])
            .current_dir(&nested)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("start native cmd.exe fixture");
        let process_id = child.id();

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut observed = false;
        while Instant::now() < deadline {
            let result = discover_current_directory_owners_read_only(
                &[root.clone(), nested.clone()],
                &|| false,
                Some(deadline),
            );
            if let CurrentDirectoryOwnerBatchTerminal::Complete(items) = result {
                observed = items.len() == 2
                    && items.iter().all(|item| {
                        item.owners
                            .iter()
                            .any(|owner| owner.identity.process_id == process_id)
                    });
                if observed {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }

        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            observed,
            "native cmd.exe current directory must occupy both nested and parent resources"
        );
    }

    #[test]
    fn live_cmd_owner_disappears_after_moving_outside_subtree_and_after_exit() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("superexplorer-cwd-refresh-{nonce}"));
        let nested = root.join("nested");
        let outside = std::env::temp_dir().join(format!("superexplorer-cwd-outside-{nonce}"));
        std::fs::create_dir_all(&nested).expect("create nested fixture");
        std::fs::create_dir_all(&outside).expect("create outside fixture");
        let cmd = PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot"))
            .join("System32")
            .join("cmd.exe");
        let mut child = Command::new(cmd)
            .args(["/D", "/Q", "/K"])
            .current_dir(&nested)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("start controlled cmd.exe fixture");
        let process_id = child.id();
        let resources = [root.clone(), nested.clone()];
        let observes = |expected: bool| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let CurrentDirectoryOwnerBatchTerminal::Complete(items) =
                    discover_current_directory_owners_read_only(
                        &resources,
                        &|| false,
                        Some(deadline),
                    )
                {
                    let present = items.iter().all(|item| {
                        item.owners
                            .iter()
                            .any(|owner| owner.identity.process_id == process_id)
                    });
                    if present == expected {
                        return true;
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
            false
        };

        assert!(
            observes(true),
            "controlled process must initially occupy subtree"
        );
        let stdin = child.stdin.as_mut().expect("controlled stdin");
        writeln!(stdin, "cd /d \"{}\"", outside.display()).expect("move current directory");
        stdin.flush().expect("flush move command");
        assert!(
            observes(false),
            "a process that moves outside the subtree must no longer be projected"
        );
        writeln!(stdin, "exit").expect("exit command");
        stdin.flush().expect("flush exit command");
        child.wait().expect("controlled process exit");
        assert!(
            observes(false),
            "an exited process must not remain in a fresh discovery snapshot"
        );
        std::fs::remove_dir_all(root).expect("remove root fixture");
        std::fs::remove_dir_all(outside).expect("remove outside fixture");
    }

    #[test]
    fn live_wow64_cmd_current_directory_projects_to_exact_and_parent_folders() {
        if !cfg!(target_arch = "x86_64") {
            return;
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("superexplorer-cwd-owner-wow64-{nonce}"));
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).expect("create WOW64 current-directory fixture");
        let cmd = PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot"))
            .join("SysWOW64")
            .join("cmd.exe");
        assert!(cmd.is_file(), "x64 Windows must provide SysWOW64 cmd.exe");
        let mut child = Command::new(cmd)
            .args(["/D", "/Q", "/C", "ping -n 31 127.0.0.1 >nul"])
            .current_dir(&nested)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("start WOW64 cmd.exe fixture");
        let process_id = child.id();
        let process = OwnedHandle::new(
            unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                    false,
                    process_id,
                )
            }
            .expect("open WOW64 fixture read-only"),
        );
        let mut process_machine = 0_u16;
        let mut native_machine = 0_u16;
        let wow64_ok = unsafe {
            is_wow64_process_2(process.0, &raw mut process_machine, &raw mut native_machine)
        };
        assert_ne!(wow64_ok, 0, "IsWow64Process2 must succeed");
        assert_ne!(process_machine, 0, "fixture must be a WOW64 process");
        assert_ne!(native_machine, 0, "native architecture must be reported");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut observed = false;
        while Instant::now() < deadline {
            let result = discover_current_directory_owners_read_only(
                &[root.clone(), nested.clone()],
                &|| false,
                Some(deadline),
            );
            if let CurrentDirectoryOwnerBatchTerminal::Complete(items) = result {
                observed = items.len() == 2
                    && items.iter().all(|item| {
                        item.owners
                            .iter()
                            .any(|owner| owner.identity.process_id == process_id)
                    });
                if observed {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }

        drop(process);
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            observed,
            "WOW64 cmd.exe current directory must occupy nested and parent resources"
        );
    }
}
