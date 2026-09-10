//! Recreate File Explorer windows from SuperExplorer tab locations.

#![expect(
    unsafe_code,
    reason = "opening File Explorer tabs uses Win32 COM and window messages"
)]

use std::{
    ffi::c_void,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use explorer_model::LocationDescriptor;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance},
            Variant::VARIANT,
        },
        UI::{
            Shell::{IShellWindows, IWebBrowser2, ShellExecuteW, ShellWindows},
            WindowsAndMessaging::{
                AllowSetForegroundWindow, EnumWindows, FindWindowExW, GW_OWNER, GetClassNameW,
                GetWindow, IsWindowVisible, PostMessageW, SW_SHOWNORMAL, SendMessageW,
                SetForegroundWindow, WM_COMMAND,
            },
        },
    },
    core::{BOOL, BSTR, GUID, Interface, PCWSTR},
};

const NEW_TAB_COMMAND: usize = 0xA21B;
const OPEN_TIMEOUT: Duration = Duration::from_secs(45);
const WAIT_SLICE: Duration = Duration::from_millis(40);
const ASFW_ANY: u32 = u32::MAX;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplorerHandoffWindow {
    pub tabs: Vec<LocationDescriptor>,
    pub active: usize,
}

pub fn explorer_open_target(location: &LocationDescriptor) -> Option<String> {
    match location {
        LocationDescriptor::FileSystem(path) => match location.file_system_kind() {
            Some(explorer_model::FileSystemKind::Adb)
            | Some(explorer_model::FileSystemKind::Sftp)
            | Some(explorer_model::FileSystemKind::Ftp)
            | Some(explorer_model::FileSystemKind::Gdrive) => None,
            Some(explorer_model::FileSystemKind::Local)
            | Some(explorer_model::FileSystemKind::Wsl)
            | None => Some(path.to_string_lossy().into_owned()),
        },
        LocationDescriptor::ParsingName(name) => {
            let trimmed = name.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        LocationDescriptor::KnownFolder(bytes) => Some(format!(
            "shell:::{{{}}}",
            guid_string(u128::from_be_bytes(*bytes))
        )),
        LocationDescriptor::ShellNamespace(_) | LocationDescriptor::Virtual(_) => None,
    }
}

pub fn open_file_explorer_windows(windows: &[ExplorerHandoffWindow]) -> Result<Vec<isize>, String> {
    if windows.is_empty() {
        return Ok(Vec::new());
    }
    let (sender, receiver) = mpsc::channel();
    let owned = windows.to_vec();
    thread::Builder::new()
        .name("explorer-handoff-open".into())
        .spawn(move || {
            let apartment = match crate::sta::ApartmentGuard::initialize() {
                Ok(apartment) => apartment,
                Err(error) => {
                    let _ = sender.send(Err(format!("File Explorer handoff STA failed: {error}")));
                    return;
                }
            };
            let result = open_file_explorer_windows_on_sta(&owned);
            drop(apartment);
            let _ = sender.send(result);
        })
        .map_err(|error| format!("File Explorer handoff thread failed: {error}"))?;
    receiver
        .recv_timeout(OPEN_TIMEOUT)
        .map_err(|_| "File Explorer handoff timed out".to_owned())?
}

pub fn explorer_is_showing_target(target: &str) -> bool {
    let expected = normalize_target(target);
    shell_window_targets()
        .iter()
        .any(|observed| targets_match(&expected, observed))
}

fn open_file_explorer_windows_on_sta(
    windows: &[ExplorerHandoffWindow],
) -> Result<Vec<isize>, String> {
    let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    let mut opened = Vec::with_capacity(windows.len());
    let mut expected = Vec::new();
    for window in windows {
        let (targets, active) = handoff_targets(window);
        expected.extend(targets.iter().cloned());
        opened.push(open_one_explorer_window(&targets, active)?);
    }
    retry_missing_targets(&expected);
    Ok(opened)
}

fn retry_missing_targets(expected: &[String]) {
    for target in expected {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && !explorer_is_showing_target(target) {
            thread::sleep(WAIT_SLICE);
        }
        if explorer_is_showing_target(target) {
            continue;
        }
        let _ = launch_explorer(target, true);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && !explorer_is_showing_target(target) {
            thread::sleep(WAIT_SLICE);
        }
    }
}

fn handoff_targets(window: &ExplorerHandoffWindow) -> (Vec<String>, usize) {
    let mut targets = Vec::new();
    let mut active = 0;
    for (index, tab) in window.tabs.iter().enumerate() {
        let Some(target) = explorer_open_target(tab) else {
            continue;
        };
        if index == window.active {
            active = targets.len();
        }
        targets.push(target);
    }
    if targets.is_empty() {
        targets.push("shell:MyComputerFolder".to_owned());
        active = 0;
    }
    let active = active.min(targets.len().saturating_sub(1));
    (targets, active)
}

fn open_one_explorer_window(targets: &[String], active: usize) -> Result<isize, String> {
    let first = targets
        .first()
        .cloned()
        .unwrap_or_else(|| "shell:MyComputerFolder".to_owned());
    let before = cabinet_hwnds();
    launch_explorer(&first, true)?;
    let hwnd = wait_for_new_cabinet(&before, Duration::from_secs(8)).unwrap_or(0);
    if hwnd != 0 {
        let _ = unsafe { SetForegroundWindow(HWND(hwnd as *mut c_void)) };
    }
    wait_until_showing(&first, Duration::from_secs(8));
    for target in targets.iter().skip(1) {
        let tabs_before = if hwnd != 0 {
            tab_hwnds_of(hwnd)
        } else {
            Vec::new()
        };
        if hwnd != 0 {
            let _ = unsafe { SetForegroundWindow(HWND(hwnd as *mut c_void)) };
            request_new_tab(hwnd);
            let new_tab = wait_for_new_tab(hwnd, &tabs_before, Duration::from_secs(2));
            if !navigate_tab(hwnd, new_tab, target) {
                launch_explorer(target, false)?;
            }
        } else {
            launch_explorer(target, false)?;
        }
        wait_until_showing(target, Duration::from_secs(4));
    }
    if hwnd != 0 && targets.len() > 1 {
        select_tab(hwnd, active.min(targets.len() - 1));
    }
    Ok(hwnd)
}

fn launch_explorer(target: &str, _new_window: bool) -> Result<(), String> {
    let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    shell_execute_open(target)
}

fn shell_execute_open(target: &str) -> Result<(), String> {
    let target_wide: Vec<u16> = target.encode_utf16().chain([0]).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            windows::core::w!("open"),
            PCWSTR(target_wide.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err(format!(
            "explorer.exe failed to start for {target} ({})",
            result.0 as isize
        ));
    }
    Ok(())
}

fn wait_until_showing(target: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && !explorer_is_showing_target(target) {
        thread::sleep(WAIT_SLICE);
    }
}

fn wait_for_new_tab(parent: isize, before: &[isize], timeout: Duration) -> Option<isize> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(hwnd) = tab_hwnds_of(parent)
            .into_iter()
            .find(|hwnd| !before.contains(hwnd))
        {
            return Some(hwnd);
        }
        thread::sleep(WAIT_SLICE);
    }
    None
}

fn tab_hwnds_of(parent: isize) -> Vec<isize> {
    let mut tabs = Vec::new();
    let mut child = HWND::default();
    let handle = HWND(parent as *mut c_void);
    loop {
        child = unsafe {
            FindWindowExW(
                Some(handle),
                if child.0.is_null() { None } else { Some(child) },
                PCWSTR(windows::core::w!("ShellTabWindowClass").as_ptr()),
                None,
            )
        }
        .unwrap_or_default();
        if child.0.is_null() {
            break;
        }
        tabs.push(child.0 as isize);
    }
    tabs
}

fn navigate_tab(parent: isize, tab: Option<isize>, target: &str) -> bool {
    let Some(browser) = browser_for_window(parent, tab) else {
        return false;
    };
    let url = BSTR::from(target);
    unsafe { browser.Navigate(&url, None, None, None, None) }.is_ok()
}

fn browser_for_window(parent: isize, tab: Option<isize>) -> Option<IWebBrowser2> {
    let windows =
        unsafe { CoCreateInstance::<_, IShellWindows>(&ShellWindows, None, CLSCTX_ALL) }.ok()?;
    let count = unsafe { windows.Count() }.unwrap_or(0);
    for index in 0..count {
        let Ok(dispatch) = (unsafe { windows.Item(&VARIANT::from(index)) }) else {
            continue;
        };
        let Ok(browser) = dispatch.cast::<IWebBrowser2>() else {
            continue;
        };
        let Ok(hwnd) = (unsafe { browser.HWND() }) else {
            continue;
        };
        if hwnd.0 as isize != parent {
            continue;
        }
        if tab.is_none() {
            return Some(browser);
        }
        let ole_tab = dispatch
            .cast::<windows::Win32::System::Ole::IOleWindow>()
            .ok()
            .and_then(|ole| unsafe { ole.GetWindow() }.ok())
            .map(|handle| handle.0 as isize);
        if ole_tab == tab {
            return Some(browser);
        }
    }
    None
}

fn request_new_tab(parent: isize) {
    let tab = first_tab_hwnd(parent).unwrap_or(parent);
    let handle = HWND(tab as *mut c_void);
    let _ = unsafe {
        PostMessageW(
            Some(handle),
            WM_COMMAND,
            WPARAM(NEW_TAB_COMMAND),
            LPARAM(0),
        )
    };
}

fn select_tab(parent: isize, index: usize) {
    let handle = HWND(parent as *mut c_void);
    let _ = unsafe {
        SendMessageW(
            handle,
            WM_COMMAND,
            Some(WPARAM(0xA221)),
            Some(LPARAM((index + 1) as isize)),
        )
    };
}

fn first_tab_hwnd(parent: isize) -> Option<isize> {
    let child = unsafe {
        FindWindowExW(
            Some(HWND(parent as *mut c_void)),
            None,
            PCWSTR(windows::core::w!("ShellTabWindowClass").as_ptr()),
            None,
        )
    }
    .unwrap_or_default();
    (!child.0.is_null()).then_some(child.0 as isize)
}

fn wait_for_new_cabinet(before: &[isize], timeout: Duration) -> Option<isize> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(hwnd) = cabinet_hwnds()
            .into_iter()
            .find(|hwnd| !before.contains(hwnd))
        {
            return Some(hwnd);
        }
        thread::sleep(WAIT_SLICE);
    }
    None
}

fn cabinet_hwnds() -> Vec<isize> {
    let mut windows = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_cabinet_windows),
            LPARAM(&raw mut windows as isize),
        );
    }
    windows
}

unsafe extern "system" fn enum_cabinet_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = unsafe { &mut *(lparam.0 as *mut Vec<isize>) };
    if is_cabinet(hwnd) {
        windows.push(hwnd.0 as isize);
    }
    BOOL(1)
}

fn is_cabinet(hwnd: HWND) -> bool {
    if hwnd.0.is_null() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return false;
    }
    if !unsafe { GetWindow(hwnd, GW_OWNER) }
        .map(|owner| owner.0.is_null())
        .unwrap_or(true)
    {
        return false;
    }
    let mut class = [0u16; 64];
    let class_len = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
    let class = String::from_utf16_lossy(&class[..class_len]);
    class == "CabinetWClass" || class == "ExploreWClass"
}

fn shell_window_targets() -> Vec<String> {
    let Ok(windows) = (unsafe { CoCreateInstance::<_, IShellWindows>(&ShellWindows, None, CLSCTX_ALL) })
    else {
        return Vec::new();
    };
    let count = unsafe { windows.Count() }.unwrap_or(0);
    let mut targets = Vec::new();
    for index in 0..count {
        let Ok(dispatch) = (unsafe { windows.Item(&VARIANT::from(index)) }) else {
            continue;
        };
        let Ok(browser) = dispatch.cast::<IWebBrowser2>() else {
            continue;
        };
        if let Ok(url) = unsafe { browser.LocationURL() } {
            let url = url.to_string();
            if !url.trim().is_empty() {
                targets.push(normalize_target(&url));
            }
        }
        if let Ok(name) = unsafe { browser.LocationName() } {
            let name = name.to_string();
            if !name.trim().is_empty() {
                targets.push(normalize_target(&name));
            }
        }
    }
    targets
}

fn normalize_target(value: &str) -> String {
    let trimmed = value.trim();
    let decoded = if let Some(rest) = trimmed.strip_prefix("file://") {
        let rest = rest
            .strip_prefix("localhost")
            .or_else(|| rest.strip_prefix("LOCALHOST"))
            .unwrap_or(rest);
        let path = if rest.starts_with('/') {
            percent_decode(&rest[1..])
        } else {
            percent_decode(rest)
        };
        path.replace('/', "\\")
    } else {
        trimmed.replace('/', "\\")
    };
    decoded.trim_end_matches('\\').to_ascii_lowercase()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""),
                16,
            ) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn targets_match(expected: &str, observed: &str) -> bool {
    expected == observed
        || expected.ends_with(observed)
        || observed.ends_with(expected)
        || expected.replace("%20", " ") == *observed
}

fn guid_string(value: u128) -> String {
    let guid = GUID::from_u128(value);
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer_tabs::descriptor_from_explorer_path;

    #[test]
    fn filesystem_and_shell_locations_become_explorer_targets() {
        assert_eq!(
            explorer_open_target(&LocationDescriptor::file_system(r"D:\SuperExplorer")),
            Some(r"D:\SuperExplorer".to_owned())
        );
        assert_eq!(
            explorer_open_target(&LocationDescriptor::ParsingName(
                "shell:MyComputerFolder".to_owned()
            )),
            Some("shell:MyComputerFolder".to_owned())
        );
        assert_eq!(
            explorer_open_target(&LocationDescriptor::ParsingName(String::new())),
            None
        );
    }

    #[test]
    fn empty_window_falls_back_to_this_pc() {
        let (targets, active) = handoff_targets(&ExplorerHandoffWindow {
            tabs: Vec::new(),
            active: 0,
        });
        assert_eq!(targets, vec!["shell:MyComputerFolder".to_owned()]);
        assert_eq!(active, 0);
    }

    #[test]
    fn file_urls_match_filesystem_paths() {
        assert!(targets_match(
            &normalize_target(r"D:\SuperExplorer"),
            &normalize_target("file:///D:/SuperExplorer")
        ));
        assert!(targets_match(
            &normalize_target(r"C:\Users\Damody\AppData\Local"),
            &normalize_target("file:///C:/Users/Damody/AppData/Local")
        ));
    }

    #[test]
    fn descriptor_round_trip_keeps_this_pc() {
        assert_eq!(
            descriptor_from_explorer_path("shell:MyComputerFolder"),
            LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned())
        );
    }

    #[test]
    fn live_open_creates_explorer_window_then_closes_it() {
        use crate::close_explorer_windows;
        let _lock = crate::live_explorer_lock();
        let root = std::path::PathBuf::from(format!(r"D:\se-handoff-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let opened = open_file_explorer_windows(&[ExplorerHandoffWindow {
            tabs: vec![LocationDescriptor::file_system(&root)],
            active: 0,
        }]);
        let hwnds = opened.expect("open File Explorer");
        assert!(!hwnds.is_empty(), "handoff opened no Explorer window");
        close_explorer_windows(&hwnds);
        let _ = std::fs::remove_dir_all(&root);
    }
}
