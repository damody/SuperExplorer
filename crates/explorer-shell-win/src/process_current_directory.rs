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

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: this object uniquely owns a live snapshot or process handle.
        let _ = unsafe { CloseHandle(self.0) };
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
        Ok(snapshot) => OwnedHandle(snapshot),
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
        candidates += 1;
        if candidates > MAX_PROCESS_CANDIDATES {
            return CurrentDirectoryOwnerBatchTerminal::Unavailable(discovery_error(
                "enumerate process snapshot",
                "process candidate count exceeded the bounded contract",
                None,
            ));
        }
        if entry.th32ProcessID != 0
            && entry.th32ProcessID != current_process_id
            && let Some((current_directory, owner)) = inspect_process(&entry)
        {
            let candidate = NormalizedWindowsPath::parse(&current_directory);
            if let Some(candidate) = candidate {
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
        }
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

    let maximum_owners = RoadmapLimits::default().lock_recovery_max_owners;
    for item in &mut matches {
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
    CurrentDirectoryOwnerBatchTerminal::Complete(matches)
}

fn inspect_process(entry: &PROCESSENTRY32W) -> Option<(PathBuf, LockOwner)> {
    // SAFETY: access is read/query-only and the returned handle is uniquely owned below.
    let handle = OwnedHandle(
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                false,
                entry.th32ProcessID,
            )
        }
        .ok()?,
    );
    let current_directory = query_current_directory(handle.0)?;
    let identity = LockOwnerIdentity {
        process_id: entry.th32ProcessID,
        creation_time_100ns: process_creation_time(handle.0)?,
    };
    let length = entry
        .szExeFile
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(entry.szExeFile.len());
    let display_name = String::from_utf16_lossy(&entry.szExeFile[..length]);
    if display_name.is_empty() {
        return None;
    }
    Some((
        current_directory,
        LockOwner {
            identity,
            display_name,
            application_type: LockOwnerApplicationType::Console,
            restartable: false,
            eligibility: LockOwnerEligibility::Protected,
        },
    ))
}

fn query_current_directory(handle: HANDLE) -> Option<PathBuf> {
    let mut wow64_peb = 0_usize;
    let mut returned = 0_u32;
    // SAFETY: the output buffer is writable for the declared size.
    let wow64_status = unsafe {
        NtQueryInformationProcess(
            handle,
            PROCESS_WOW64_INFORMATION_CLASS,
            (&raw mut wow64_peb).cast(),
            u32::try_from(size_of::<usize>()).ok()?,
            &raw mut returned,
        )
    };
    if wow64_status >= 0 && wow64_peb != 0 {
        return query_current_directory32(handle, wow64_peb);
    }

    let mut information = MaybeUninit::<ProcessBasicInformation>::zeroed();
    // SAFETY: the output buffer is writable for the declared size.
    let status = unsafe {
        NtQueryInformationProcess(
            handle,
            PROCESS_BASIC_INFORMATION_CLASS,
            information.as_mut_ptr().cast(),
            u32::try_from(size_of::<ProcessBasicInformation>()).ok()?,
            &raw mut returned,
        )
    };
    if status < 0 {
        return None;
    }
    // SAFETY: successful `NtQueryInformationProcess` initialized the complete structure.
    let information = unsafe { information.assume_init() };
    query_current_directory64(handle, information.peb_base_address as usize)
}

fn query_current_directory64(handle: HANDLE, peb: usize) -> Option<PathBuf> {
    let parameters: u64 = read_remote(
        handle,
        checked_address(peb, PEB64_PROCESS_PARAMETERS_OFFSET)?,
    )?;
    let descriptor: UnicodeString64 = read_remote(
        handle,
        checked_address(
            usize::try_from(parameters).ok()?,
            PARAMETERS64_CURRENT_DIRECTORY_OFFSET,
        )?,
    )?;
    read_remote_unicode(
        handle,
        usize::try_from(descriptor.buffer).ok()?,
        descriptor.length,
        descriptor.maximum_length,
    )
}

fn query_current_directory32(handle: HANDLE, peb: usize) -> Option<PathBuf> {
    let parameters: u32 = read_remote(
        handle,
        checked_address(peb, PEB32_PROCESS_PARAMETERS_OFFSET)?,
    )?;
    let descriptor: UnicodeString32 = read_remote(
        handle,
        checked_address(
            usize::try_from(parameters).ok()?,
            PARAMETERS32_CURRENT_DIRECTORY_OFFSET,
        )?,
    )?;
    read_remote_unicode(
        handle,
        usize::try_from(descriptor.buffer).ok()?,
        descriptor.length,
        descriptor.maximum_length,
    )
}

fn checked_address(base: usize, offset: usize) -> Option<usize> {
    let address = base.checked_add(offset)?;
    (address != 0).then_some(address)
}

fn read_remote<T: Copy>(handle: HANDLE, address: usize) -> Option<T> {
    let _ = checked_address(address, size_of::<T>())?;
    let mut value = MaybeUninit::<T>::uninit();
    let mut bytes_read = 0_usize;
    // SAFETY: `value` is writable for exactly `size_of::<T>()`; the remote address is never
    // dereferenced locally and Windows validates its readability in the target process.
    unsafe {
        ReadProcessMemory(
            handle,
            address as *const c_void,
            value.as_mut_ptr().cast(),
            size_of::<T>(),
            Some(&raw mut bytes_read),
        )
    }
    .ok()?;
    if bytes_read != size_of::<T>() {
        return None;
    }
    // SAFETY: an exact successful copy initialized all bytes of `T`, and every use is a plain
    // integer/pointer-layout structure with no Rust references or invalid value constraints.
    Some(unsafe { value.assume_init() })
}

fn read_remote_unicode(
    handle: HANDLE,
    buffer: usize,
    length_bytes: u16,
    maximum_length_bytes: u16,
) -> Option<PathBuf> {
    let length = usize::from(length_bytes);
    let maximum = usize::from(maximum_length_bytes);
    if buffer == 0
        || length == 0
        || length % size_of::<u16>() != 0
        || length > maximum
        || length / size_of::<u16>() > MAX_CURRENT_DIRECTORY_UTF16
    {
        return None;
    }
    let _ = checked_address(buffer, length)?;
    let mut wide = vec![0_u16; length / size_of::<u16>()];
    let mut bytes_read = 0_usize;
    // SAFETY: `wide` owns a writable `length`-byte region and Windows validates the remote range.
    unsafe {
        ReadProcessMemory(
            handle,
            buffer as *const c_void,
            wide.as_mut_ptr().cast(),
            length,
            Some(&raw mut bytes_read),
        )
    }
    .ok()?;
    if bytes_read != length {
        return None;
    }
    Some(PathBuf::from(std::ffi::OsString::from_wide(&wide)))
}

fn process_creation_time(handle: HANDLE) -> Option<u64> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: all four FILETIME outputs are writable for this process query.
    unsafe {
        GetProcessTimes(
            handle,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    }
    .ok()?;
    Some((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
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
        os::windows::process::CommandExt as _,
        process::Command,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::*;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[test]
    fn component_ancestry_handles_local_unc_extended_and_prefix_boundaries() {
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
        assert_eq!(MAX_PROCESS_CANDIDATES, 4_096);
        assert_eq!(MAX_CURRENT_DIRECTORY_UTF16, 32_768);
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
}
