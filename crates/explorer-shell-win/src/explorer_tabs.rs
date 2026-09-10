//! Snapshot every Windows File Explorer window and every tab location.

#![expect(
    unsafe_code,
    reason = "File Explorer tab enumeration uses Win32 COM and window APIs"
)]

use std::{
    ffi::{OsString, c_void},
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use explorer_model::{HistoryEntry, LocationDescriptor};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, IServiceProvider},
            Ole::IOleWindow,
            Threading::{
                GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
            Variant::VARIANT,
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
                UIA_ClassNamePropertyId,
            },
            Shell::{
                IFolderView, IPersistFolder2, IPersistIDList, IShellBrowser, IShellWindows,
                IWebBrowser2, SHGetNameFromIDList, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_FILESYSPATH,
                SIGDN_NORMALDISPLAY, ShellWindows,
            },
            WindowsAndMessaging::{
                EnumWindows, FindWindowExW, GW_OWNER, GetClassNameW, GetWindow, GetWindowTextW,
                GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostMessageW, SC_CLOSE,
                SMTO_ABORTIFHUNG, SW_RESTORE, SendMessageTimeoutW, ShowWindow, WM_CLOSE,
                WM_COMMAND, WM_SYSCOMMAND,
            },
        },
    },
    core::{BOOL, BSTR, GUID, IUnknown, Interface, PCWSTR},
};

const SID_S_TOP_LEVEL_BROWSER: GUID = GUID::from_u128(0x4C96BE40_915C_11CF_99D3_00AA004AE837);
const SELECT_TAB_COMMAND: usize = 0xA221;
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(45);
const TAB_SWITCH_WAIT: Duration = Duration::from_millis(40);
const TAB_RESOLVE_ATTEMPTS: u32 = 4;
const TAB_SWITCH_DEADLINE: Duration = Duration::from_millis(180);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplorerTabSnapshot {
    pub location: LocationDescriptor,
    pub display_title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplorerWindowSnapshot {
    pub hwnd: isize,
    pub tabs: Vec<ExplorerTabSnapshot>,
    pub active_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RawShellTab {
    parent_hwnd: isize,
    tab_hwnd: isize,
    location: LocationDescriptor,
    display_title: String,
}

pub fn snapshot_open_explorer_windows() -> Result<Vec<ExplorerWindowSnapshot>, String> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("explorer-tab-snapshot".into())
        .spawn(move || {
            let apartment = match crate::sta::ApartmentGuard::initialize() {
                Ok(apartment) => apartment,
                Err(error) => {
                    let _ = sender.send(Err(format!("Explorer snapshot STA failed: {error}")));
                    return;
                }
            };
            let result = snapshot_open_explorer_windows_on_sta();
            drop(apartment);
            let _ = sender.send(result);
        })
        .map_err(|error| format!("Explorer snapshot thread failed: {error}"))?;
    receiver
        .recv_timeout(SNAPSHOT_TIMEOUT)
        .map_err(|_| "Explorer tab snapshot timed out".to_owned())?
}

pub fn close_explorer_windows(hwnds: &[isize]) {
    let hwnds = hwnds
        .iter()
        .copied()
        .filter(|hwnd| *hwnd != 0)
        .collect::<Vec<_>>();
    if hwnds.is_empty() {
        return;
    }
    let (sender, receiver) = mpsc::channel();
    let owned = hwnds.clone();
    let _ = thread::Builder::new()
        .name("explorer-window-close".into())
        .spawn(move || {
            win32_close_until_gone(&owned);
            let _ = sender.send(());
        });
    let _ = receiver.recv_timeout(Duration::from_secs(8));
    win32_close_until_gone(&hwnds);
}

fn win32_close_until_gone(hwnds: &[isize]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let mut any_alive = false;
        for hwnd in hwnds.iter().copied() {
            let handle = HWND(hwnd as *mut c_void);
            if !unsafe { IsWindow(Some(handle)) }.as_bool() {
                continue;
            }
            any_alive = true;
            let _ = unsafe { ShowWindow(handle, SW_RESTORE) };
            post_close(handle);
        }
        if !any_alive {
            return;
        }
        thread::sleep(Duration::from_millis(80));
    }
}

pub fn location_from_explorer_path(raw: &str, title: &str) -> HistoryEntry {
    HistoryEntry::new(
        descriptor_from_explorer_path(raw),
        display_title(raw, title),
    )
}

pub fn descriptor_from_explorer_path(raw: &str) -> LocationDescriptor {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned());
    }
    let filesystem = decode_file_url(trimmed).unwrap_or_else(|| trimmed.replace('/', "\\"));
    let parsing = filesystem
        .trim_start_matches("shell:")
        .trim_start_matches("Shell:");
    let guid = normalize_guid_token(parsing);
    if let Some(known) = known_shell_parsing_name(&guid) {
        return LocationDescriptor::ParsingName(known.to_owned());
    }
    if filesystem.starts_with("::")
        || filesystem.starts_with("shell:")
        || filesystem.starts_with("Shell:")
        || filesystem.starts_with("shell:::")
    {
        return LocationDescriptor::ParsingName(
            if filesystem.starts_with("shell:") || filesystem.starts_with("Shell:") {
                filesystem
            } else {
                format!("shell:{filesystem}")
            },
        );
    }
    let path = PathBuf::from(&filesystem);
    if path.has_root() || looks_like_drive_or_unc(&filesystem) {
        LocationDescriptor::file_system(filesystem)
    } else {
        LocationDescriptor::ParsingName(trimmed.to_owned())
    }
}

pub(crate) fn active_index_for(visual_hwnds: &[isize], original_active: Option<isize>) -> usize {
    original_active
        .and_then(|hwnd| visual_hwnds.iter().position(|candidate| *candidate == hwnd))
        .unwrap_or(0)
}

#[cfg(test)]
pub(crate) fn assemble_windows(
    tabs: Vec<RawShellTab>,
    windows: &[(isize, Vec<isize>)],
) -> Result<Vec<ExplorerWindowSnapshot>, String> {
    use std::collections::HashMap;
    let mut by_tab = HashMap::<isize, RawShellTab>::new();
    for tab in tabs {
        by_tab.insert(tab.tab_hwnd, tab);
    }
    let mut assembled_windows = Vec::new();
    for (parent, order) in windows {
        if order.is_empty() {
            let Some(tab) = by_tab.values().find(|tab| tab.parent_hwnd == *parent) else {
                return Err(format!("Explorer window {parent:#x} has no COM tab"));
            };
            assembled_windows.push(ExplorerWindowSnapshot {
                hwnd: *parent,
                tabs: vec![ExplorerTabSnapshot {
                    location: tab.location.clone(),
                    display_title: tab.display_title.clone(),
                }],
                active_index: 0,
            });
            continue;
        }
        let mut assembled = Vec::with_capacity(order.len());
        for tab_hwnd in order {
            let Some(tab) = by_tab.get(tab_hwnd) else {
                return Err(format!(
                    "Explorer tab {tab_hwnd:#x} in window {parent:#x} has no location"
                ));
            };
            assembled.push(ExplorerTabSnapshot {
                location: tab.location.clone(),
                display_title: tab.display_title.clone(),
            });
        }
        assembled_windows.push(ExplorerWindowSnapshot {
            hwnd: *parent,
            tabs: assembled,
            active_index: 0,
        });
    }
    Ok(assembled_windows)
}

fn snapshot_open_explorer_windows_on_sta() -> Result<Vec<ExplorerWindowSnapshot>, String> {
    let windows = explorer_windows();
    if windows.is_empty() {
        return Ok(Vec::new());
    }
    let mut snapshots = Vec::with_capacity(windows.len());
    let mut last_error = None;
    for (parent, tab_hwnds) in windows {
        match snapshot_one_window(parent, &tab_hwnds) {
            Ok(snapshot) if !snapshot.tabs.is_empty() => snapshots.push(snapshot),
            Ok(_) => {
                last_error = Some(format!("Explorer window {parent:#x} assembled empty"));
            }
            Err(error) => {
                tracing::warn!(
                    hwnd = format!("{parent:#x}"),
                    %error,
                    "File Explorer window tab snapshot failed"
                );
                last_error = Some(error);
            }
        }
    }
    if snapshots.is_empty() {
        return Err(
            last_error.unwrap_or_else(|| "Explorer tab snapshot produced no windows".to_owned())
        );
    }
    Ok(snapshots)
}

fn snapshot_one_window(
    parent: isize,
    tab_hwnds: &[isize],
) -> Result<ExplorerWindowSnapshot, String> {
    let original_active = tab_hwnds.first().copied();
    if let Some(snapshot) = snapshot_from_com(parent, tab_hwnds, original_active) {
        let needed = tab_hwnds.len().max(1);
        if snapshot.tabs.len() >= needed || snapshot.tabs.len() > 1 {
            return Ok(snapshot);
        }
    }
    let visual_count = tab_hwnds.len().max(1);
    let handle = HWND(parent as *mut c_void);
    let mut visual_hwnds = Vec::with_capacity(visual_count);
    let mut tabs = Vec::with_capacity(visual_count);
    let mut previous_active = original_active;
    for index in 0..visual_count {
        let mut active_hwnd = first_tab_hwnd(parent).unwrap_or(parent);
        if visual_count > 1 {
            for _ in 0..TAB_RESOLVE_ATTEMPTS {
                previous_active =
                    Some(select_explorer_tab_and_wait(handle, index, previous_active));
                active_hwnd = first_tab_hwnd(parent).unwrap_or(parent);
                if !visual_hwnds.contains(&active_hwnd) {
                    break;
                }
            }
            if visual_hwnds.contains(&active_hwnd) {
                return Err(format!(
                    "Explorer window {parent:#x} tab {index} did not become active"
                ));
            }
        }
        let tab = resolve_tab_location(parent, active_hwnd).ok_or_else(|| {
            format!("Explorer tab {active_hwnd:#x} in window {parent:#x} has no location")
        })?;
        visual_hwnds.push(active_hwnd);
        tabs.push(tab);
    }
    let active_index = active_index_for(&visual_hwnds, original_active);
    if visual_count > 1 {
        select_explorer_tab(handle, active_index);
    }
    Ok(ExplorerWindowSnapshot {
        hwnd: parent,
        tabs,
        active_index,
    })
}

fn snapshot_from_com(
    parent: isize,
    tab_hwnds: &[isize],
    original_active: Option<isize>,
) -> Option<ExplorerWindowSnapshot> {
    let collected = collect_shell_tabs().ok()?;
    snapshot_from_collected(&collected, parent, tab_hwnds, original_active)
}

pub(crate) fn snapshot_from_collected(
    collected: &[RawShellTab],
    parent: isize,
    tab_hwnds: &[isize],
    original_active: Option<isize>,
) -> Option<ExplorerWindowSnapshot> {
    let for_parent = collected
        .iter()
        .filter(|tab| tab.parent_hwnd == parent)
        .collect::<Vec<_>>();
    if for_parent.is_empty() {
        return None;
    }
    let mut tabs = Vec::new();
    if !tab_hwnds.is_empty()
        && tab_hwnds
            .iter()
            .all(|hwnd| for_parent.iter().any(|tab| tab.tab_hwnd == *hwnd))
    {
        for hwnd in tab_hwnds {
            let tab = for_parent.iter().find(|tab| tab.tab_hwnd == *hwnd)?;
            tabs.push(ExplorerTabSnapshot {
                location: tab.location.clone(),
                display_title: tab.display_title.clone(),
            });
        }
    } else {
        // Win11 tabbed Explorer reports one IShellWindows item per tab, but
        // IWebBrowser2.HWND is the shared CabinetWClass. Keep every COM tab.
        tabs.extend(for_parent.iter().map(|tab| ExplorerTabSnapshot {
            location: tab.location.clone(),
            display_title: tab.display_title.clone(),
        }));
    }
    if tabs.is_empty() {
        return None;
    }
    let title = window_title(HWND(parent as *mut c_void));
    let active_index = active_index_for(tab_hwnds, original_active).min(tabs.len() - 1);
    let active_index = index_matching_window_title(&tabs, &title).unwrap_or(active_index);
    Some(ExplorerWindowSnapshot {
        hwnd: parent,
        tabs,
        active_index,
    })
}

pub(crate) fn index_matching_window_title(
    tabs: &[ExplorerTabSnapshot],
    title: &str,
) -> Option<usize> {
    let prefix = window_title_leaf(title);
    if prefix.is_empty() {
        return None;
    }
    tabs.iter().position(|tab| {
        let display = tab.display_title.trim();
        if !display.is_empty()
            && (prefix.eq_ignore_ascii_case(display)
                || prefix.contains(display)
                || display.contains(&prefix))
        {
            return true;
        }
        tab.location.path().is_some_and(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| prefix.eq_ignore_ascii_case(name) || prefix.contains(name))
        })
    })
}

pub(crate) fn window_title_leaf(title: &str) -> String {
    let trimmed = title.trim();
    let without_app = trimmed
        .rsplit_once(" - ")
        .map(|(leaf, _)| leaf)
        .unwrap_or(trimmed);
    without_app
        .split(" 和 ")
        .next()
        .unwrap_or(without_app)
        .split(" and ")
        .next()
        .unwrap_or(without_app)
        .trim()
        .to_owned()
}

fn resolve_tab_location(parent: isize, tab_hwnd: isize) -> Option<ExplorerTabSnapshot> {
    for _ in 0..TAB_RESOLVE_ATTEMPTS {
        if let Some(tab) = location_from_collected_tabs(parent, tab_hwnd) {
            return Some(tab);
        }
        thread::sleep(TAB_SWITCH_WAIT);
    }
    location_from_uia(parent)
}

fn location_from_collected_tabs(parent: isize, tab_hwnd: isize) -> Option<ExplorerTabSnapshot> {
    let tabs = collect_shell_tabs().ok()?;
    if let Some(tab) = tabs.iter().find(|tab| tab.tab_hwnd == tab_hwnd) {
        return Some(ExplorerTabSnapshot {
            location: tab.location.clone(),
            display_title: fallback_title(&tab.display_title, tab_hwnd, &tab.location),
        });
    }
    let active = first_tab_hwnd(parent);
    tabs.into_iter()
        .find(|tab| tab.parent_hwnd == parent && Some(tab.tab_hwnd) == active)
        .map(|tab| ExplorerTabSnapshot {
            display_title: fallback_title(&tab.display_title, tab_hwnd, &tab.location),
            location: tab.location,
        })
}

fn fallback_title(title: &str, tab_hwnd: isize, location: &LocationDescriptor) -> String {
    let trimmed = title.trim();
    if !trimmed.is_empty() {
        return trimmed.to_owned();
    }
    let window_title = window_title(HWND(tab_hwnd as *mut c_void));
    if !window_title.is_empty() {
        return window_title;
    }
    match location {
        LocationDescriptor::FileSystem(path) => path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map_or_else(|| path.display().to_string(), str::to_owned),
        LocationDescriptor::ParsingName(name) => name.clone(),
        _ => String::new(),
    }
}

fn location_from_uia(parent: isize) -> Option<ExplorerTabSnapshot> {
    let raw = uia_location_text(parent)?;
    let entry = location_from_explorer_path(&raw, &window_title(HWND(parent as *mut c_void)));
    Some(ExplorerTabSnapshot {
        display_title: entry.display_title,
        location: entry.location,
    })
}

fn uia_location_text(parent: isize) -> Option<String> {
    let (sender, receiver) = mpsc::channel();
    let _ = thread::Builder::new()
        .name("explorer-tab-uia".into())
        .spawn(move || {
            let apartment = crate::sta::ApartmentGuard::initialize().ok();
            let path = read_breadcrumb_path(parent);
            drop(apartment);
            let _ = sender.send(path);
        });
    receiver
        .recv_timeout(Duration::from_millis(800))
        .ok()
        .flatten()
}

fn read_breadcrumb_path(parent: isize) -> Option<String> {
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL) }.ok()?;
    let element = unsafe { automation.ElementFromHandle(HWND(parent as *mut c_void)) }.ok()?;
    existing_path_from_uia_breadcrumbs(&automation, &element)
}

fn existing_path_from_uia_breadcrumbs(
    automation: &IUIAutomation,
    element: &IUIAutomationElement,
) -> Option<String> {
    let names = uia_breadcrumb_names(automation, element)?;
    let joined = drive_path_from_breadcrumb_names(&names)?;
    if std::path::Path::new(&joined).exists() {
        return Some(joined);
    }
    let mapped = replace_known_localized_components(&joined);
    std::path::Path::new(&mapped).exists().then_some(mapped)
}

pub(crate) fn replace_known_localized_components(path: &str) -> String {
    path.split('\\')
        .map(|component| match component {
            "使用者" | "用户" => "Users".to_owned(),
            "文件" | "文档" => "Documents".to_owned(),
            "桌面" => "Desktop".to_owned(),
            "下載" | "下载" => "Downloads".to_owned(),
            "圖片" | "图片" => "Pictures".to_owned(),
            "音樂" | "音乐" => "Music".to_owned(),
            "影片" | "视频" => "Videos".to_owned(),
            other => other.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\\")
}

fn uia_breadcrumb_names(
    automation: &IUIAutomation,
    element: &IUIAutomationElement,
) -> Option<Vec<String>> {
    let class = property_condition(
        automation,
        UIA_ClassNamePropertyId,
        "FileExplorerExtensions.BreadcrumbBarItemControl",
    )?;
    let items = unsafe { element.FindAll(TreeScope_Descendants, &class) }.ok()?;
    let length = unsafe { items.Length() }.ok().unwrap_or(0);
    if length <= 0 {
        return None;
    }
    let mut names = Vec::with_capacity(length as usize);
    for index in 0..length {
        let item = unsafe { items.GetElement(index) }.ok()?;
        let name = unsafe { item.CurrentName() }.ok()?.to_string();
        if !name.trim().is_empty() {
            names.push(name);
        }
    }
    (!names.is_empty()).then_some(names)
}

fn property_condition(
    automation: &IUIAutomation,
    property: windows::Win32::UI::Accessibility::UIA_PROPERTY_ID,
    value: &str,
) -> Option<windows::Win32::UI::Accessibility::IUIAutomationCondition> {
    let variant = VARIANT::from(BSTR::from(value));
    unsafe { automation.CreatePropertyCondition(property, &variant) }.ok()
}

fn extract_drive_letter(name: &str) -> Option<char> {
    let bytes = name.as_bytes();
    for index in 0..bytes.len().saturating_sub(3) {
        if bytes[index] == b'('
            && bytes[index + 1].is_ascii_alphabetic()
            && bytes[index + 2] == b':'
            && bytes.get(index + 3) == Some(&b')')
        {
            return Some((bytes[index + 1] as char).to_ascii_uppercase());
        }
    }
    None
}

fn drive_path_from_breadcrumb_names(names: &[String]) -> Option<String> {
    let mut drive = None;
    let mut rest = Vec::new();
    for name in names {
        if let Some(letter) = extract_drive_letter(name) {
            drive = Some(letter);
            rest.clear();
            continue;
        }
        if drive.is_some() {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                rest.push(trimmed);
            }
        }
    }
    let letter = drive?;
    if rest.is_empty() {
        return Some(format!("{letter}:\\"));
    }
    Some(format!("{letter}:\\{}", rest.join("\\")))
}

fn collect_shell_tabs() -> Result<Vec<RawShellTab>, String> {
    let windows: IShellWindows = unsafe { CoCreateInstance(&ShellWindows, None, CLSCTX_ALL) }
        .map_err(|error| format!("IShellWindows unavailable: {error}"))?;
    let count = unsafe { windows.Count() }.unwrap_or(0);
    let mut tabs = Vec::new();
    for index in 0..count {
        let item = unsafe { windows.Item(&VARIANT::from(index)) };
        let Ok(dispatch) = item else {
            continue;
        };
        let Ok(browser) = dispatch.cast::<IWebBrowser2>() else {
            continue;
        };
        let Ok(parent) = (unsafe { browser.HWND() }) else {
            continue;
        };
        let parent_hwnd = parent.0 as isize;
        if !is_file_explorer_window(HWND(parent_hwnd as *mut c_void)) {
            continue;
        }
        let Ok(unknown) = dispatch.cast::<IUnknown>() else {
            continue;
        };
        let tab_hwnd = tab_hwnd_from_browser(&unknown).unwrap_or(parent_hwnd);
        let Ok((location, title)) = tab_location(&browser, &unknown) else {
            continue;
        };
        tabs.push(RawShellTab {
            parent_hwnd,
            tab_hwnd,
            location,
            display_title: title,
        });
    }
    Ok(tabs)
}

fn tab_hwnd_from_browser(dispatch: &IUnknown) -> Option<isize> {
    let browser = shell_browser(dispatch)?;
    let ole = browser.cast::<IOleWindow>().ok()?;
    unsafe { ole.GetWindow() }.ok().map(|hwnd| hwnd.0 as isize)
}

fn shell_browser(dispatch: &IUnknown) -> Option<IShellBrowser> {
    let provider = dispatch.cast::<IServiceProvider>().ok()?;
    unsafe { provider.QueryService(&SID_S_TOP_LEVEL_BROWSER) }
        .or_else(|_| unsafe { provider.QueryService(&IShellBrowser::IID) })
        .ok()
}

fn tab_location(
    browser: &IWebBrowser2,
    dispatch: &IUnknown,
) -> Result<(LocationDescriptor, String), String> {
    if let Ok(url) = unsafe { browser.LocationURL() } {
        let url = url.to_string();
        if usable_location_url(&url) {
            let title = unsafe { browser.LocationName() }
                .map(|name| name.to_string())
                .unwrap_or_default();
            let entry = location_from_explorer_path(&url, &title);
            return Ok((entry.location, entry.display_title));
        }
    }
    if let Some(entry) = persist_id_list_location(dispatch) {
        return Ok((entry.location, entry.display_title));
    }
    if let Some(entry) = folder_view_location(dispatch) {
        return Ok((entry.location, entry.display_title));
    }
    Err("Explorer tab did not expose a location".to_owned())
}

fn persist_id_list_location(dispatch: &IUnknown) -> Option<HistoryEntry> {
    let browser = shell_browser(dispatch)?;
    let view = unsafe { browser.QueryActiveShellView() }.ok()?;
    let persist = view.cast::<IPersistIDList>().ok()?;
    let pidl = unsafe { persist.GetIDList() }.ok()?;
    names_from_pidl(pidl)
}

fn folder_view_location(dispatch: &IUnknown) -> Option<HistoryEntry> {
    let browser = shell_browser(dispatch)?;
    let view = unsafe { browser.QueryActiveShellView() }.ok()?;
    let folder_view = view.cast::<IFolderView>().ok()?;
    let persist: IPersistFolder2 = unsafe { folder_view.GetFolder() }.ok()?;
    let pidl = unsafe { persist.GetCurFolder() }.ok()?;
    names_from_pidl(pidl)
}

fn names_from_pidl(
    pidl: *mut windows::Win32::UI::Shell::Common::ITEMIDLIST,
) -> Option<HistoryEntry> {
    if pidl.is_null() {
        return None;
    }
    let parsing = unsafe { SHGetNameFromIDList(pidl, SIGDN_DESKTOPABSOLUTEPARSING) }.ok();
    let display = unsafe { SHGetNameFromIDList(pidl, SIGDN_NORMALDISPLAY) }.ok();
    let filesystem = unsafe { SHGetNameFromIDList(pidl, SIGDN_FILESYSPATH) }.ok();
    unsafe { CoTaskMemFree(Some(pidl.cast())) };
    let raw = filesystem
        .and_then(pwstr_to_string)
        .filter(|value| !value.is_empty())
        .or_else(|| parsing.and_then(pwstr_to_string))?;
    let title = display.and_then(pwstr_to_string).unwrap_or_default();
    Some(location_from_explorer_path(&raw, &title))
}

fn pwstr_to_string(value: windows::core::PWSTR) -> Option<String> {
    if value.0.is_null() {
        return None;
    }
    let owned = unsafe { crate::native::CoTaskMem::from_raw(value.0) }?;
    let mut len = 0;
    while unsafe { *owned.as_ptr().add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { std::slice::from_raw_parts(owned.as_ptr(), len) };
    Some(String::from_utf16_lossy(slice))
}

fn explorer_windows() -> Vec<(isize, Vec<isize>)> {
    let mut windows = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_explorer_windows),
            LPARAM(&raw mut windows as isize),
        );
    }
    windows
        .into_iter()
        .map(|hwnd| (hwnd, tab_hwnds_of(hwnd)))
        .collect()
}

fn tab_hwnds_of(hwnd: isize) -> Vec<isize> {
    let mut tabs = Vec::new();
    let mut child = HWND::default();
    let parent = HWND(hwnd as *mut c_void);
    loop {
        child = unsafe {
            FindWindowExW(
                Some(parent),
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

fn first_tab_hwnd(parent: isize) -> Option<isize> {
    tab_hwnds_of(parent).into_iter().next()
}

unsafe extern "system" fn enum_explorer_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = unsafe { &mut *(lparam.0 as *mut Vec<isize>) };
    if is_file_explorer_window(hwnd) {
        windows.push(hwnd.0 as isize);
    }
    BOOL(1)
}

fn is_file_explorer_window(hwnd: HWND) -> bool {
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
    if class != "CabinetWClass" && class != "ExploreWClass" {
        return false;
    }
    let mut process_id = 0_u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut process_id)) };
    process_id != 0
        && process_id != unsafe { GetCurrentProcessId() }
        && is_explorer_process(process_id)
}

fn is_explorer_process(process_id: u32) -> bool {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) };
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
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(process) };
    if !ok {
        return false;
    }
    let path = OsString::from_wide(&buffer[..length as usize]);
    PathBuf::from(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("explorer.exe"))
}

fn select_explorer_tab_and_wait(hwnd: HWND, index: usize, previous_active: Option<isize>) -> isize {
    select_explorer_tab(hwnd, index);
    let parent = hwnd.0 as isize;
    let deadline = Instant::now() + TAB_SWITCH_DEADLINE;
    loop {
        let active = first_tab_hwnd(parent).unwrap_or(parent);
        if previous_active != Some(active) || Instant::now() >= deadline {
            return active;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn select_explorer_tab(hwnd: HWND, index: usize) {
    send_timeout(
        hwnd,
        WM_COMMAND,
        WPARAM(SELECT_TAB_COMMAND),
        LPARAM((index + 1) as isize),
    );
}

fn post_close(hwnd: HWND) {
    let _ = unsafe {
        PostMessageW(
            Some(hwnd),
            WM_SYSCOMMAND,
            WPARAM(SC_CLOSE as usize),
            LPARAM(0),
        )
    };
    let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
}

fn send_timeout(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) {
    let mut result = 0usize;
    let _ = unsafe {
        SendMessageTimeoutW(
            hwnd,
            message,
            wparam,
            lparam,
            SMTO_ABORTIFHUNG,
            200,
            Some(&raw mut result),
        )
    };
}

fn window_title(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..length])
        .trim()
        .to_owned()
}

fn display_title(raw: &str, title: &str) -> String {
    let trimmed = title.trim();
    if !trimmed.is_empty() {
        return trimmed.to_owned();
    }
    PathBuf::from(raw.replace('/', "\\"))
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map_or_else(|| raw.to_owned(), str::to_owned)
}

fn usable_location_url(url: &str) -> bool {
    let trimmed = url.trim();
    !trimmed.is_empty()
        && !trimmed.eq_ignore_ascii_case("file:")
        && !trimmed.eq_ignore_ascii_case("file:/")
        && !trimmed.eq_ignore_ascii_case("file://")
        && !trimmed.eq_ignore_ascii_case("file:///")
}

fn decode_file_url(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix("file://")?;
    let rest = rest
        .strip_prefix("localhost")
        .or_else(|| rest.strip_prefix("LOCALHOST"))
        .unwrap_or(rest);
    let path = if rest.starts_with('/') {
        percent_decode(&rest[1..])
    } else {
        percent_decode(rest)
    };
    Some(path.replace('/', "\\"))
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

fn looks_like_drive_or_unc(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() >= 2 && bytes[1] == b':') || value.starts_with(r"\\")
}

fn normalize_guid_token(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("shell:")
        .trim_start_matches("::")
        .trim_matches(|ch| ch == '{' || ch == '}')
        .to_ascii_uppercase()
}

fn known_shell_parsing_name(guid: &str) -> Option<&'static str> {
    match guid {
        "20D04FE0-3AEA-1069-A2D8-08002B30309D" | "MYCOMPUTERFOLDER" => {
            Some("shell:MyComputerFolder")
        }
        "645FF040-5081-101B-9F08-00AA002F954E" | "RECYCLEBINFOLDER" => {
            Some("shell:RecycleBinFolder")
        }
        "F02C1A0D-BE21-4350-88B0-7367FC96EF3C"
        | "208D2C60-3AEA-1069-A2D8-08002B30309D"
        | "NETWORKPLACESFOLDER" => Some("shell:NetworkPlacesFolder"),
        "031E4825-7B94-4DC3-B131-E946B44C8DD5" | "LIBRARIES" => Some("shell:Libraries"),
        "F874310E-B6B7-47DC-BC84-B9E6B38F5903"
        | "59031A47-3F72-44A7-89C5-5595FE6B30EE"
        | "HOMEFOLDER" => Some("shell:HomeFolder"),
        "679F85CB-0220-4080-B29B-5540CC05AAB6" => Some("shell:HomeFolder"),
        "374DE290-123F-4565-9164-39C4925E467B" | "DOWNLOADS" => Some("shell:Downloads"),
        "FDD39AD0-238F-46AF-ADB4-6C85480369C7" | "PERSONAL" => Some("shell:Personal"),
        "B4BFCC3A-DB2C-424C-B029-7FE99A87C641" | "DESKTOP" => Some("shell:Desktop"),
        "33E28130-4E1E-4676-835A-98395C3BC3BB" | "MYPICTURES" => Some("shell:My Pictures"),
        "4BD8D571-6D19-48D3-BE97-422220080E43" | "MYMUSIC" => Some("shell:My Music"),
        "18989B1D-99B5-455B-841C-AB7C74E4DDFC" | "MYVIDEO" => Some("shell:My Video"),
        "26EE0668-A00A-44D7-9371-BEB064C98683" | "CONTROLPANELFOLDER" => {
            Some("shell:ControlPanelFolder")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filesystem_and_file_urls_become_paths() {
        assert_eq!(
            descriptor_from_explorer_path(r"D:\SuperExplorer"),
            LocationDescriptor::file_system(r"D:\SuperExplorer")
        );
        assert_eq!(
            descriptor_from_explorer_path("file:///D:/SuperExplorer/crates"),
            LocationDescriptor::file_system(r"D:\SuperExplorer\crates")
        );
        assert_eq!(
            descriptor_from_explorer_path("file:///C:/Users/Damody/My%20Documents"),
            LocationDescriptor::file_system(r"C:\Users\Damody\My Documents")
        );
        assert_eq!(
            descriptor_from_explorer_path("file://localhost/D:/SuperExplorer"),
            LocationDescriptor::file_system(r"D:\SuperExplorer")
        );
    }

    #[test]
    fn this_pc_recycle_home_and_guid_paths_map_to_known_shell_names() {
        assert_eq!(
            descriptor_from_explorer_path("::{20D04FE0-3AEA-1069-A2D8-08002B30309D}"),
            LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned())
        );
        assert_eq!(
            descriptor_from_explorer_path("shell:MyComputerFolder"),
            LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned())
        );
        assert_eq!(
            descriptor_from_explorer_path("::{645FF040-5081-101B-9F08-00AA002F954E}"),
            LocationDescriptor::ParsingName("shell:RecycleBinFolder".to_owned())
        );
        assert_eq!(
            descriptor_from_explorer_path("::{F874310E-B6B7-47DC-BC84-B9E6B38F5903}"),
            LocationDescriptor::ParsingName("shell:HomeFolder".to_owned())
        );
        assert_eq!(
            descriptor_from_explorer_path("::{374DE290-123F-4565-9164-39C4925E467B}"),
            LocationDescriptor::ParsingName("shell:Downloads".to_owned())
        );
    }

    #[test]
    fn assemble_windows_keeps_caller_order_and_rejects_missing_tabs() {
        let tabs = vec![
            RawShellTab {
                parent_hwnd: 1,
                tab_hwnd: 11,
                location: LocationDescriptor::file_system(r"D:\a"),
                display_title: "a".into(),
            },
            RawShellTab {
                parent_hwnd: 1,
                tab_hwnd: 12,
                location: LocationDescriptor::file_system(r"D:\b"),
                display_title: "b".into(),
            },
            RawShellTab {
                parent_hwnd: 2,
                tab_hwnd: 21,
                location: LocationDescriptor::ParsingName("shell:MyComputerFolder".into()),
                display_title: "本機".into(),
            },
        ];
        let order = vec![(1, vec![12, 11]), (2, vec![21])];
        let windows = assemble_windows(tabs.clone(), &order).unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].tabs[0].display_title, "b");
        assert_eq!(windows[0].tabs[1].display_title, "a");
        assert_eq!(windows[1].tabs[0].display_title, "本機");
        let missing = vec![(1, vec![12, 11, 13]), (2, vec![21])];
        assert!(assemble_windows(tabs, &missing).is_err());
    }

    #[test]
    fn visual_order_records_the_originally_active_tab() {
        assert_eq!(active_index_for(&[11, 12, 13], Some(13)), 2);
        assert_eq!(active_index_for(&[11, 12, 13], Some(11)), 0);
        assert_eq!(active_index_for(&[11, 12, 13], Some(99)), 0);
        assert_eq!(active_index_for(&[11, 12, 13], None), 0);
    }

    #[test]
    fn live_snapshot_does_not_panic_when_explorer_is_absent_or_present() {
        let snapshot = snapshot_open_explorer_windows();
        assert!(
            snapshot.is_ok(),
            "live Explorer snapshot failed: {:?}",
            snapshot.err()
        );
        if let Ok(windows) = snapshot {
            for window in windows {
                assert!(!window.tabs.is_empty());
                assert!(window.active_index < window.tabs.len());
            }
        }
    }

    #[test]
    fn live_snapshot_keeps_every_tab_listed_by_com_for_a_window() {
        let windows = snapshot_open_explorer_windows().unwrap_or_default();
        for window in &windows {
            assert!(!window.tabs.is_empty());
            let unique = window
                .tabs
                .iter()
                .map(|tab| format!("{:?}:{}", tab.location, tab.display_title))
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                unique.len(),
                window.tabs.len(),
                "duplicate tab snapshot in hwnd {:#x}",
                window.hwnd
            );
        }
    }

    #[test]
    fn com_tabs_sharing_cabinet_hwnd_are_all_kept() {
        let collected = vec![
            RawShellTab {
                parent_hwnd: 10,
                tab_hwnd: 10,
                location: LocationDescriptor::file_system(r"D:\SuperExplorer"),
                display_title: "SuperExplorer".into(),
            },
            RawShellTab {
                parent_hwnd: 10,
                tab_hwnd: 10,
                location: LocationDescriptor::file_system(r"D:\code\omoba"),
                display_title: "omoba".into(),
            },
        ];
        let snapshot = snapshot_from_collected(&collected, 10, &[11, 12], Some(11)).unwrap();
        assert_eq!(snapshot.tabs.len(), 2);
        assert_eq!(snapshot.tabs[0].display_title, "SuperExplorer");
        assert_eq!(snapshot.tabs[1].display_title, "omoba");
    }

    #[test]
    fn window_title_selects_the_active_tab_leaf() {
        let tabs = vec![
            ExplorerTabSnapshot {
                location: LocationDescriptor::file_system(r"D:\SuperExplorer"),
                display_title: "SuperExplorer".into(),
            },
            ExplorerTabSnapshot {
                location: LocationDescriptor::file_system(r"D:\code\omoba"),
                display_title: "omoba".into(),
            },
        ];
        assert_eq!(
            index_matching_window_title(&tabs, "omoba 和 1 個其他索引標籤 - 檔案總管"),
            Some(1)
        );
        assert_eq!(
            index_matching_window_title(&tabs, "SuperExplorer - 檔案總管"),
            Some(0)
        );
        assert_eq!(
            window_title_leaf("omoba 和 1 個其他索引標籤 - 檔案總管"),
            "omoba"
        );
    }

    #[test]
    fn localized_user_profile_crumbs_map_to_users() {
        assert_eq!(
            replace_known_localized_components(r"C:\使用者\Damody\AppData\Local"),
            r"C:\Users\Damody\AppData\Local"
        );
        assert_eq!(
            replace_known_localized_components(r"C:\用户\Damody\Documents"),
            r"C:\Users\Damody\Documents"
        );
    }

    #[test]
    fn breadcrumb_names_join_drive_letter_and_components() {
        assert_eq!(extract_drive_letter("新增磁碟區 (D:)"), Some('D'));
        assert_eq!(extract_drive_letter("Local Disk (C:)"), Some('C'));
        assert_eq!(extract_drive_letter("此電腦"), None);
        assert_eq!(
            drive_path_from_breadcrumb_names(&[
                "此電腦".into(),
                "新增磁碟區 (D:)".into(),
                "UE5.8".into()
            ]),
            Some(r"D:\UE5.8".into())
        );
        assert_eq!(
            drive_path_from_breadcrumb_names(&[
                "此電腦".into(),
                "新增磁碟區 (D:)".into(),
                "code".into(),
                "omoba".into(),
                "omfue".into()
            ]),
            Some(r"D:\code\omoba\omfue".into())
        );
        assert_eq!(
            drive_path_from_breadcrumb_names(&["此電腦".into(), "本機磁碟 (C:)".into()]),
            Some(r"C:\".into())
        );
        assert_eq!(drive_path_from_breadcrumb_names(&["此電腦".into()]), None);
    }

    #[test]
    fn live_snapshot_sees_folder_opened_on_d() {
        let _lock = crate::live_explorer_lock();
        let root = PathBuf::from(format!(r"D:\se-live-{}", std::process::id()));
        let folder = root.join("alpha");
        std::fs::create_dir_all(&folder).unwrap();
        let opened = crate::open_file_explorer_windows(&[crate::ExplorerHandoffWindow {
            tabs: vec![LocationDescriptor::file_system(&folder)],
            active: 0,
        }]);
        let hwnds = match opened {
            Ok(hwnds) => hwnds,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&root);
                panic!("open File Explorer for snapshot: {error}");
            }
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut seen = false;
        while Instant::now() < deadline {
            if snapshot_open_explorer_windows()
                .ok()
                .is_some_and(|windows| snapshot_contains_path(&windows, &folder))
            {
                seen = true;
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }
        close_explorer_windows(&hwnds);
        let _ = std::fs::remove_dir_all(&root);
        assert!(seen, "snapshot did not see {}", folder.display());
    }

    #[test]
    fn snapshot_path_match_is_case_insensitive_and_accepts_file_names() {
        let windows = vec![ExplorerWindowSnapshot {
            hwnd: 1,
            tabs: vec![ExplorerTabSnapshot {
                location: LocationDescriptor::file_system(r"D:\se-tab-import\Alpha"),
                display_title: "Alpha".into(),
            }],
            active_index: 0,
        }];
        assert!(snapshot_contains_path(
            &windows,
            std::path::Path::new(r"D:\se-tab-import\alpha")
        ));
        assert!(!snapshot_contains_path(
            &windows,
            std::path::Path::new(r"D:\other\beta")
        ));
    }

    fn snapshot_contains_path(
        windows: &[ExplorerWindowSnapshot],
        expected: &std::path::Path,
    ) -> bool {
        let expected_name = expected
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let expected_l = expected
            .to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase();
        windows
            .iter()
            .flat_map(|window| window.tabs.iter())
            .any(|tab| {
                let title_match = tab.display_title.eq_ignore_ascii_case(expected_name);
                let path_match = tab.location.path().is_some_and(|path| {
                    let path = path
                        .to_string_lossy()
                        .replace('/', "\\")
                        .trim_end_matches('\\')
                        .to_ascii_lowercase();
                    path == expected_l
                        || path.ends_with(&expected_l)
                        || path.ends_with(&format!("\\{expected_name}"))
                });
                title_match || path_match
            })
    }
}
