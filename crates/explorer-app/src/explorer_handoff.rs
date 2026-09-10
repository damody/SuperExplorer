//! Convert every SuperExplorer window back into File Explorer, then quit.

#![expect(
    unsafe_code,
    reason = "handoff coordination uses named events and process/window APIs"
)]

use std::{
    env, fs,
    mem::size_of,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use explorer_model::LocationDescriptor;
use explorer_shell_win::ExplorerHandoffWindow;
use serde::{Deserialize, Serialize};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_ABANDONED, WAIT_OBJECT_0, WPARAM},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                CreateEventW, CreateMutexW, INFINITE, OpenProcess, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, ReleaseMutex,
                ResetEvent, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
            },
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
        },
    },
    core::{BOOL, PCWSTR},
};

use crate::explorer_import::ImportedTabPayload;

const DUMP_EVENT: &str = "Local\\SuperExplorer.HandoffDump.v1";
const QUIT_EVENT: &str = "Local\\SuperExplorer.HandoffQuit.v1";
const OWNER_MUTEX: &str = "Local\\SuperExplorer.HandoffOwner.v1";
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(45);

static LIVE_WINDOW: Mutex<Option<HandoffWindowFile>> = Mutex::new(None);
static MAIN_HWND: AtomicU64 = AtomicU64::new(0);
static STOPPED: AtomicBool = AtomicBool::new(false);
static STARTED: OnceLock<Mutex<bool>> = OnceLock::new();
static DUMP_THREAD: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct HandoffWindowFile {
    hwnd: u64,
    tabs: Vec<ImportedTabPayload>,
    active: usize,
}

pub fn publish_live_window(tabs: Vec<(LocationDescriptor, String)>, active: usize) {
    let payload = HandoffWindowFile {
        hwnd: MAIN_HWND.load(Ordering::Acquire),
        tabs: tabs
            .into_iter()
            .map(|(location, display_title)| ImportedTabPayload {
                location,
                display_title,
            })
            .collect(),
        active,
    };
    if let Ok(mut slot) = LIVE_WINDOW.lock() {
        *slot = Some(payload);
    }
}

pub fn set_main_hwnd(hwnd: u64) {
    MAIN_HWND.store(hwnd, Ordering::Release);
    if let Ok(mut slot) = LIVE_WINDOW.lock() {
        if let Some(window) = slot.as_mut() {
            window.hwnd = hwnd;
        }
    }
}

pub fn start_handoff_server() {
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
    thread::Builder::new()
        .name("superexplorer-handoff".into())
        .spawn(handoff_server_loop)
        .ok();
}

pub fn stop_handoff_server() {
    STOPPED.store(true, Ordering::Release);
    if let Some(event) = open_or_create_event(DUMP_EVENT) {
        let _ = unsafe { SetEvent(event) };
        let _ = unsafe { CloseHandle(event) };
    }
}

pub fn handoff_all_windows(
    tabs: Vec<(LocationDescriptor, String)>,
    active: usize,
) -> Result<(), String> {
    publish_live_window(tabs, active);
    let owner = named_mutex(OWNER_MUTEX)?;
    let wait = unsafe { WaitForSingleObject(owner, 0) };
    if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
        let _ = unsafe { CloseHandle(owner) };
        return Err("another SuperExplorer window is already converting back".to_owned());
    }
    let result = run_handoff();
    let _ = unsafe { ReleaseMutex(owner) };
    let _ = unsafe { CloseHandle(owner) };
    result
}

fn run_handoff() -> Result<(), String> {
    let dir = handoff_dir();
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|error| format!("handoff directory: {error}"))?;
    let pids = superexplorer_pids()?;
    let dump = open_or_create_event(DUMP_EVENT).ok_or("handoff dump event")?;
    let quit = open_or_create_event(QUIT_EVENT).ok_or("handoff quit event")?;
    let _ = unsafe { ResetEvent(dump) };
    let _ = unsafe { ResetEvent(quit) };
    write_live_window_file(&dir);
    let deadline = Instant::now() + HANDOFF_TIMEOUT;
    while Instant::now() < deadline {
        let _ = unsafe { SetEvent(dump) };
        if pids.iter().all(|pid| dir.join(format!("window-{pid}.json")).is_file()) {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = unsafe { ResetEvent(dump) };
    let missing = pids
        .iter()
        .filter(|pid| !dir.join(format!("window-{pid}.json")).is_file())
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "SuperExplorer windows did not report tabs before handoff: {missing:?}"
        ));
    }
    let windows = read_handoff_windows(&dir);
    if windows.is_empty() {
        return Err("no SuperExplorer windows reported tabs to convert".to_owned());
    }
    let opened = explorer_shell_win::open_file_explorer_windows(&windows)
        .map_err(|error| format!("open File Explorer: {error}"))?;
    if opened.len() < windows.len() {
        tracing::warn!(
            expected = windows.len(),
            opened = opened.len(),
            "File Explorer handoff opened fewer windows than SuperExplorer windows"
        );
    }
    let _ = unsafe { SetEvent(quit) };
    close_sibling_windows(&windows);
    let _ = fs::remove_dir_all(&dir);
    let _ = unsafe { CloseHandle(dump) };
    let _ = unsafe { CloseHandle(quit) };
    Ok(())
}

fn handoff_server_loop() {
    let dump = match open_or_create_event(DUMP_EVENT) {
        Some(event) => event,
        None => return,
    };
    let quit = match open_or_create_event(QUIT_EVENT) {
        Some(event) => event,
        None => {
            let _ = unsafe { CloseHandle(dump) };
            return;
        }
    };
    DUMP_THREAD.store(
        unsafe { windows::Win32::System::Threading::GetCurrentThreadId() },
        Ordering::Release,
    );
    let handles = [dump, quit];
    while !STOPPED.load(Ordering::Acquire) {
        let wait = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
        if STOPPED.load(Ordering::Acquire) {
            break;
        }
        if wait.0 == WAIT_OBJECT_0.0 + 1 {
            close_this_window();
            break;
        }
        if wait == WAIT_OBJECT_0 {
            write_live_window_file(&handoff_dir());
            let mut quit_seen = false;
            while !STOPPED.load(Ordering::Acquire)
                && unsafe { WaitForSingleObject(dump, 0) } == WAIT_OBJECT_0
            {
                if unsafe { WaitForSingleObject(quit, 50) } == WAIT_OBJECT_0 {
                    quit_seen = true;
                    break;
                }
            }
            if quit_seen {
                close_this_window();
                break;
            }
        }
    }
    DUMP_THREAD.store(0, Ordering::Release);
    let _ = unsafe { CloseHandle(dump) };
    let _ = unsafe { CloseHandle(quit) };
}

fn write_live_window_file(dir: &Path) {
    let Some(mut window) = live_window() else {
        return;
    };
    if window.hwnd == 0 {
        window.hwnd = MAIN_HWND.load(Ordering::Acquire);
    }
    let _ = fs::create_dir_all(dir);
    let path = dir.join(format!("window-{}.json", std::process::id()));
    if let Ok(encoded) = serde_json::to_string(&window) {
        let _ = fs::write(path, encoded);
    }
}

fn live_window() -> Option<HandoffWindowFile> {
    LIVE_WINDOW.lock().ok().and_then(|slot| slot.clone())
}

fn read_handoff_windows(dir: &Path) -> Vec<ExplorerHandoffWindow> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut windows = Vec::new();
    for entry in entries.flatten() {
        let Ok(raw) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(file) = serde_json::from_str::<HandoffWindowFile>(&raw) else {
            continue;
        };
        if file.tabs.is_empty() {
            continue;
        }
        windows.push(ExplorerHandoffWindow {
            tabs: file.tabs.into_iter().map(|tab| tab.location).collect(),
            active: file.active,
        });
    }
    windows
}

fn close_sibling_windows(windows: &[ExplorerHandoffWindow]) {
    let self_pid = std::process::id();
    for pid in superexplorer_pids().unwrap_or_default() {
        if pid == self_pid {
            continue;
        }
        close_process_windows(pid);
    }
    let _ = windows;
}

fn close_this_window() {
    let hwnd = MAIN_HWND.load(Ordering::Acquire);
    if hwnd != 0 {
        let handle = HWND(hwnd as *mut std::ffi::c_void);
        let _ = unsafe { PostMessageW(Some(handle), WM_CLOSE, WPARAM(0), LPARAM(0)) };
    }
}

fn close_process_windows(pid: u32) {
    unsafe {
        let _ = EnumWindows(Some(enum_close_pid_windows), LPARAM(pid as isize));
    }
}

unsafe extern "system" fn enum_close_pid_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let target = lparam.0 as u32;
    let mut process_id = 0_u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut process_id)) };
    if process_id == target && unsafe { IsWindowVisible(hwnd) }.as_bool() {
        let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
    }
    BOOL(1)
}

fn handoff_dir() -> PathBuf {
    env::temp_dir().join("SuperExplorerHandoff")
}

fn named_mutex(name: &str) -> Result<HANDLE, String> {
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    unsafe { CreateMutexW(None, false, PCWSTR(wide.as_ptr())) }
        .map_err(|error| format!("handoff mutex: {error}"))
}

fn open_or_create_event(name: &str) -> Option<HANDLE> {
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let created = unsafe { CreateEventW(None, true, false, PCWSTR(wide.as_ptr())) };
    created.ok()
}

fn superexplorer_pids() -> Result<Vec<u32>, String> {
    let exe = env::current_exe().map_err(|error| error.to_string())?;
    let expected = exe
        .canonicalize()
        .unwrap_or(exe)
        .to_string_lossy()
        .to_ascii_lowercase();
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|error| format!("process snapshot: {error}"))?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut pids = Vec::new();
    let mut more = unsafe { Process32FirstW(snapshot, &raw mut entry) }.is_ok();
    while more {
        if process_image_matches(entry.th32ProcessID, &expected) {
            pids.push(entry.th32ProcessID);
        }
        more = unsafe { Process32NextW(snapshot, &raw mut entry) }.is_ok();
    }
    let _ = unsafe { CloseHandle(snapshot) };
    Ok(pids)
}

fn process_image_matches(pid: u32, expected: &str) -> bool {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) };
    let Ok(process) = process else {
        return false;
    };
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &raw mut length,
        )
    }
    .is_ok();
    let _ = unsafe { CloseHandle(process) };
    if !ok {
        return false;
    }
    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    PathBuf::from(path)
        .canonicalize()
        .map(|path| path.to_string_lossy().to_ascii_lowercase() == expected)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_window_round_trips_tabs_and_active_index() {
        publish_live_window(
            vec![
                (
                    LocationDescriptor::file_system(r"D:\SuperExplorer"),
                    "SuperExplorer".into(),
                ),
                (
                    LocationDescriptor::ParsingName("shell:MyComputerFolder".into()),
                    "本機".into(),
                ),
            ],
            1,
        );
        set_main_hwnd(42);
        let window = live_window().expect("published window");
        assert_eq!(window.hwnd, 42);
        assert_eq!(window.tabs.len(), 2);
        assert_eq!(window.active, 1);
        assert_eq!(
            explorer_shell_win::explorer_open_target(&window.tabs[0].location).as_deref(),
            Some(r"D:\SuperExplorer")
        );
    }
}
