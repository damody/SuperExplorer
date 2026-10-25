//! Dual-layer File Explorer launch interception while SuperExplorer is running.
//!
//! Layer 1 registers a temporary user-scoped `Directory\shell` verb in
//! `HKCU\Software\Classes\Directory\shell` so `ShellExecute("open", "<folder>")`
//! from external applications invokes `SuperExplorer.exe --open-in-tab "<folder>"`
//! directly with zero window flash, and restores the prior registry state on exit.
//!
//! Layer 2 runs a background `SetWinEventHook` + `IShellWindows` STA observer that
//! captures direct `explorer.exe` launches (such as `SHOpenFolderAndSelectItems` or
//! `explorer.exe /select,"<file>"`), extracts both the folder location and any
//! selected item, closes the transient Explorer window, and opens a new tab in the
//! active `SuperExplorer` window with the target item selected.

#![expect(
    unsafe_code,
    reason = "Directory\\shell verb registration and mutex coordination use Win32 APIs"
)]

use std::{
    collections::VecDeque,
    env, fs,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use explorer_model::LocationDescriptor;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_SUCCESS, WAIT_ABANDONED, WAIT_OBJECT_0},
        System::{
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
                REG_VALUE_TYPE, RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW,
                RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
            },
            Threading::{CreateMutexW, INFINITE, ReleaseMutex, WaitForSingleObject},
        },
    },
    core::PCWSTR,
};

const INTERCEPT_OWNER_MUTEX: &str = "Local\\SuperExplorer.ExplorerInterceptOwner.v1";
const DIRECTORY_SHELL_SUBKEY: &str = r"Software\Classes\Directory\shell";
const SUPEREXPLORER_VERB_NAME: &str = "SuperExplorerOpen";
const SUPEREXPLORER_BACKUP_VALUE: &str = "SuperExplorerBackupDefault";

static IN_PROCESS_QUEUE: OnceLock<Mutex<VecDeque<InterceptedTabRequest>>> = OnceLock::new();
static STOPPED: AtomicBool = AtomicBool::new(false);
static STARTED: OnceLock<Mutex<bool>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InterceptedTabRequest {
    pub location: LocationDescriptor,
    pub selected_item: Option<LocationDescriptor>,
}

fn in_process_queue() -> &'static Mutex<VecDeque<InterceptedTabRequest>> {
    IN_PROCESS_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Enqueues a new tab request inside the running `SuperExplorer` process and brings
/// its main window to the foreground.
pub fn enqueue_in_process_tab(
    location: LocationDescriptor,
    selected_item: Option<LocationDescriptor>,
) {
    if let Ok(mut queue) = in_process_queue().lock() {
        queue.push_back(InterceptedTabRequest {
            location,
            selected_item,
        });
    }
    let hwnd = crate::explorer_handoff::main_hwnd();
    if hwnd != 0 {
        crate::win_e_hotkey::activate_hwnd(hwnd);
    }
}

/// Forwards a tab request from a secondary CLI invocation to the running `SuperExplorer`
/// instance if one has a live window.
pub fn forward_tab_to_running_instance(
    location: LocationDescriptor,
    selected_item: Option<LocationDescriptor>,
) -> bool {
    if !crate::launch_coordination::activate_existing_window() {
        return false;
    }
    let dir = intercept_queue_dir();
    if !write_queued_tab_request(
        &dir,
        &InterceptedTabRequest {
            location,
            selected_item,
        },
    ) {
        return false;
    }
    let _ = crate::launch_coordination::activate_existing_window();
    true
}

/// Drains both in-process and cross-process intercepted tab requests for the GPUI
/// service pump.
pub fn drain_intercepted_tabs() -> Vec<(LocationDescriptor, Option<LocationDescriptor>)> {
    let mut out = Vec::new();
    if let Ok(mut queue) = in_process_queue().lock() {
        while let Some(request) = queue.pop_front() {
            out.push((request.location, request.selected_item));
        }
    }
    let cross_process = drain_queued_tab_files(&intercept_queue_dir());
    let had_cross_process = !cross_process.is_empty();
    for request in cross_process {
        out.push((request.location, request.selected_item));
    }
    if had_cross_process {
        let hwnd = crate::explorer_handoff::main_hwnd();
        if hwnd != 0 {
            crate::win_e_hotkey::activate_hwnd(hwnd);
        }
    }
    out
}

/// Parses an `--open-in-tab` argument or `/select,<path>` target into a folder
/// [`LocationDescriptor`] and an optional item to select.
pub fn resolve_open_in_tab_request(raw: &str) -> (LocationDescriptor, Option<LocationDescriptor>) {
    let trimmed = raw.trim().trim_matches('"').trim();
    if trimmed.is_empty() {
        return (
            LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned()),
            None,
        );
    }
    if let Some(rest) = strip_select_prefix(trimmed) {
        let item_raw = rest.trim().trim_matches('"').trim();
        if !item_raw.is_empty() {
            let item_path = PathBuf::from(item_raw);
            if let Some(parent) = item_path.parent().filter(|p| !p.as_os_str().is_empty()) {
                return (
                    LocationDescriptor::file_system(parent.to_path_buf()),
                    Some(LocationDescriptor::file_system(item_path)),
                );
            }
        }
    }
    let descriptor = explorer_shell_win::descriptor_from_explorer_path(trimmed);
    if let LocationDescriptor::FileSystem(path) = &descriptor
        && fs::metadata(path).is_ok_and(|meta| meta.is_file())
        && let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty())
    {
        return (
            LocationDescriptor::file_system(parent.to_path_buf()),
            Some(LocationDescriptor::file_system(path.clone())),
        );
    }
    (descriptor, None)
}

fn strip_select_prefix(value: &str) -> Option<&str> {
    let prefixes = ["/select,", "-select,"];
    for prefix in prefixes {
        if value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return Some(&value[prefix.len()..]);
        }
    }
    None
}

/// Starts both layers of File Explorer interception for the lifetime of this process.
pub fn start_explorer_intercept() {
    let started = STARTED.get_or_init(|| Mutex::new(false));
    let Ok(mut guard) = started.lock() else {
        return;
    };
    if *guard {
        return;
    }
    *guard = true;
    drop(guard);
    STOPPED.store(false, Ordering::Release);
    if let Ok(exe) = env::current_exe() {
        register_directory_shell_verb(&exe);
    }
    thread::Builder::new()
        .name("superexplorer-explorer-intercept".into())
        .spawn(intercept_owner_loop)
        .ok();
}

/// Stops File Explorer interception and restores the `HKCU\Software\Classes\Directory\shell`
/// default verb.
pub fn stop_explorer_intercept() {
    STOPPED.store(true, Ordering::Release);
    unregister_directory_shell_verb();
    if let Some(guard) = STARTED.get()
        && let Ok(mut started) = guard.lock()
    {
        *started = false;
    }
}

fn intercept_owner_loop() {
    let wide: Vec<u16> = INTERCEPT_OWNER_MUTEX.encode_utf16().chain([0]).collect();
    let Ok(mutex) = (unsafe { CreateMutexW(None, false, PCWSTR(wide.as_ptr())) }) else {
        return;
    };
    loop {
        if STOPPED.load(Ordering::Acquire) {
            break;
        }
        let wait = unsafe { WaitForSingleObject(mutex, INFINITE) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            break;
        }
        if STOPPED.load(Ordering::Acquire) {
            let _ = unsafe { ReleaseMutex(mutex) };
            break;
        }
        if let Ok(exe) = env::current_exe() {
            register_directory_shell_verb(&exe);
        }
        explorer_shell_win::run_live_explorer_interceptor(
            || STOPPED.load(Ordering::Acquire),
            crate::explorer_handoff::is_handoff_in_progress,
            |tab| {
                enqueue_in_process_tab(tab.location, tab.selected_item);
            },
        );
        let _ = unsafe { ReleaseMutex(mutex) };
        if STOPPED.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    let _ = unsafe { CloseHandle(mutex) };
}

pub(crate) fn format_open_in_tab_command(exe: &Path) -> String {
    format!("\"{}\" --open-in-tab \"%1\"", exe.display())
}

fn register_directory_shell_verb(exe: &Path) {
    let command = format_open_in_tab_command(exe);
    let command_subkey = format!(r"{DIRECTORY_SHELL_SUBKEY}\{SUPEREXPLORER_VERB_NAME}\command");
    let Some(cmd_key) = create_hkcu_key(&command_subkey) else {
        return;
    };
    let _ = set_reg_sz(cmd_key, None, &command);
    let _ = unsafe { RegCloseKey(cmd_key) };

    let Some(shell_key) = create_hkcu_key(DIRECTORY_SHELL_SUBKEY) else {
        return;
    };
    let current_default = query_reg_sz(shell_key, None).unwrap_or_default();
    if current_default != SUPEREXPLORER_VERB_NAME {
        let _ = set_reg_sz(
            shell_key,
            Some(SUPEREXPLORER_BACKUP_VALUE),
            &current_default,
        );
    }
    let _ = set_reg_sz(shell_key, None, SUPEREXPLORER_VERB_NAME);
    let _ = unsafe { RegCloseKey(shell_key) };
}

fn unregister_directory_shell_verb() {
    if let Some(shell_key) = open_hkcu_key_rw(DIRECTORY_SHELL_SUBKEY) {
        let current_default = query_reg_sz(shell_key, None).unwrap_or_default();
        let backup = query_reg_sz(shell_key, Some(SUPEREXPLORER_BACKUP_VALUE));
        if current_default == SUPEREXPLORER_VERB_NAME {
            match backup.as_deref() {
                Some(prior) if !prior.is_empty() && prior != SUPEREXPLORER_VERB_NAME => {
                    let _ = set_reg_sz(shell_key, None, prior);
                }
                _ => {
                    let _ = delete_reg_value(shell_key, None);
                }
            }
        }
        let _ = delete_reg_value(shell_key, Some(SUPEREXPLORER_BACKUP_VALUE));
        let verb_wide: Vec<u16> = SUPEREXPLORER_VERB_NAME.encode_utf16().chain([0]).collect();
        let _ = unsafe { RegDeleteTreeW(shell_key, PCWSTR(verb_wide.as_ptr())) };
        let _ = unsafe { RegCloseKey(shell_key) };
    }
}

fn create_hkcu_key(subkey: &str) -> Option<HKEY> {
    let wide: Vec<u16> = subkey.encode_utf16().chain([0]).collect();
    let mut key = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wide.as_ptr()),
            Some(0),
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_READ | KEY_WRITE,
            None,
            &raw mut key,
            None,
        )
    };
    (status == ERROR_SUCCESS).then_some(key)
}

fn open_hkcu_key_rw(subkey: &str) -> Option<HKEY> {
    let wide: Vec<u16> = subkey.encode_utf16().chain([0]).collect();
    let mut key = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wide.as_ptr()),
            Some(0),
            KEY_READ | KEY_WRITE,
            &raw mut key,
        )
    };
    (status == ERROR_SUCCESS).then_some(key)
}

fn query_reg_sz(key: HKEY, name: Option<&str>) -> Option<String> {
    let wide: Option<Vec<u16>> = name.map(|value| value.encode_utf16().chain([0]).collect());
    let name_ptr = wide
        .as_ref()
        .map_or(PCWSTR::null(), |buf| PCWSTR(buf.as_ptr()));
    let mut kind = REG_VALUE_TYPE::default();
    let mut byte_len = 0_u32;
    let status = unsafe {
        RegQueryValueExW(
            key,
            name_ptr,
            None,
            Some(&raw mut kind),
            None,
            Some(&raw mut byte_len),
        )
    };
    if status != ERROR_SUCCESS || kind != REG_SZ || byte_len == 0 {
        return None;
    }
    let mut buffer = vec![0_u8; byte_len as usize];
    let status = unsafe {
        RegQueryValueExW(
            key,
            name_ptr,
            None,
            Some(&raw mut kind),
            Some(buffer.as_mut_ptr()),
            Some(&raw mut byte_len),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let u16_len = (byte_len as usize) / 2;
    let wide_slice = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u16>(), u16_len) };
    let trimmed_len = wide_slice
        .iter()
        .position(|&ch| ch == 0)
        .unwrap_or(wide_slice.len());
    Some(String::from_utf16_lossy(&wide_slice[..trimmed_len]))
}

fn set_reg_sz(key: HKEY, name: Option<&str>, value: &str) -> bool {
    let name_wide: Option<Vec<u16>> = name.map(|n| n.encode_utf16().chain([0]).collect());
    let name_ptr = name_wide
        .as_ref()
        .map_or(PCWSTR::null(), |buf| PCWSTR(buf.as_ptr()));
    let value_wide: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let bytes = unsafe {
        std::slice::from_raw_parts(value_wide.as_ptr().cast::<u8>(), value_wide.len() * 2)
    };
    let status = unsafe { RegSetValueExW(key, name_ptr, Some(0), REG_SZ, Some(bytes)) };
    status == ERROR_SUCCESS
}

fn delete_reg_value(key: HKEY, name: Option<&str>) -> bool {
    let name_wide: Option<Vec<u16>> = name.map(|n| n.encode_utf16().chain([0]).collect());
    let name_ptr = name_wide
        .as_ref()
        .map_or(PCWSTR::null(), |buf| PCWSTR(buf.as_ptr()));
    let status = unsafe { RegDeleteValueW(key, name_ptr) };
    status == ERROR_SUCCESS
}

fn intercept_queue_dir() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("SuperExplorer")
        .join("intercept-tabs")
}

fn write_queued_tab_request(dir: &Path, request: &InterceptedTabRequest) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let Ok(bytes) = serde_json::to_vec(request) else {
        return false;
    };
    let id = Uuid::new_v4();
    let tmp_path = dir.join(format!("req-{id}.tmp"));
    let final_path = dir.join(format!("req-{id}.json"));
    if fs::write(&tmp_path, bytes).is_err() {
        return false;
    }
    if fs::rename(&tmp_path, &final_path).is_err() {
        let _ = fs::remove_file(&tmp_path);
        return false;
    }
    true
}

fn drain_queued_tab_files(dir: &Path) -> Vec<InterceptedTabRequest> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
        .collect::<Vec<_>>();
    files.sort();
    let mut out = Vec::new();
    for path in files {
        if let Ok(raw) = fs::read_to_string(&path)
            && let Ok(request) = serde_json::from_str::<InterceptedTabRequest>(&raw)
        {
            out.push(request);
        }
        let _ = fs::remove_file(path);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_open_in_tab_request_parses_folders_and_select_targets() {
        let (folder, selected) = resolve_open_in_tab_request(r"D:\SuperExplorer\crates");
        assert_eq!(
            folder,
            LocationDescriptor::file_system(r"D:\SuperExplorer\crates")
        );
        assert_eq!(selected, None);

        let (folder, selected) =
            resolve_open_in_tab_request(r#"/select,"D:\SuperExplorer\Cargo.toml""#);
        assert_eq!(folder, LocationDescriptor::file_system(r"D:\SuperExplorer"));
        assert_eq!(
            selected,
            Some(LocationDescriptor::file_system(
                r"D:\SuperExplorer\Cargo.toml"
            ))
        );
    }

    #[test]
    fn queued_tab_files_round_trip_and_delete() {
        let temp = tempfile::tempdir().expect("tempdir");
        let first = InterceptedTabRequest {
            location: LocationDescriptor::file_system(r"D:\SuperExplorer"),
            selected_item: Some(LocationDescriptor::file_system(
                r"D:\SuperExplorer\Cargo.toml",
            )),
        };
        assert!(write_queued_tab_request(temp.path(), &first));
        let drained = drain_queued_tab_files(temp.path());
        assert_eq!(drained, vec![first]);
        assert!(drain_queued_tab_files(temp.path()).is_empty());
    }

    #[test]
    fn format_open_in_tab_command_quotes_executable_and_placeholder() {
        let cmd = format_open_in_tab_command(Path::new(
            r"C:\Program Files\SuperExplorer\SuperExplorer.exe",
        ));
        assert_eq!(
            cmd,
            r#""C:\Program Files\SuperExplorer\SuperExplorer.exe" --open-in-tab "%1""#
        );
    }
}
