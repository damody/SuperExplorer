//! Import open File Explorer windows/tabs into SuperExplorer processes.

#![expect(
    unsafe_code,
    reason = "spawning imported SuperExplorer windows must grant foreground rights"
)]

use std::{
    env, fs,
    process::{Command, Stdio},
};

use explorer_model::{ExplorerWindowState, HistoryEntry, LocationDescriptor, TabState};
use explorer_shell_win::{ExplorerWindowSnapshot, snapshot_open_explorer_windows};
use serde::{Deserialize, Serialize};

pub const INITIAL_TABS_ENV: &str = "SUPEREXPLORER_INITIAL_TABS";
pub const INITIAL_TABS_FILE_ENV: &str = "SUPEREXPLORER_INITIAL_TABS_FILE";
pub const WIN_E_ENV: &str = "SUPEREXPLORER_WIN_E";
pub const IMPORT_PATH_PREFIX_ENV: &str = "SUPEREXPLORER_IMPORT_PATH_PREFIX";
const ENV_PAYLOAD_SOFT_LIMIT: usize = 24 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImportedTabsPayload {
    pub tabs: Vec<ImportedTabPayload>,
    pub active: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImportedTabPayload {
    pub location: LocationDescriptor,
    pub display_title: String,
}

#[derive(Clone, Debug)]
pub struct LaunchImport {
    pub this_window: Option<ExplorerWindowState>,
    pub this_pc: bool,
}

pub fn consume_launch_import(first_ordinary_process: bool) -> LaunchImport {
    if env::var_os(WIN_E_ENV).is_some() {
        return LaunchImport {
            this_window: None,
            this_pc: true,
        };
    }
    if let Some(payload) = read_initial_tabs() {
        return LaunchImport {
            this_window: window_from_payload(&payload),
            this_pc: false,
        };
    }
    if !first_ordinary_process {
        return LaunchImport {
            this_window: None,
            this_pc: false,
        };
    }
    match snapshot_open_explorer_windows() {
        Ok(windows) => {
            let windows = windows
                .into_iter()
                .filter(window_allowed_for_import)
                .collect::<Vec<_>>();
            if windows.is_empty() {
                return LaunchImport {
                    this_window: None,
                    this_pc: false,
                };
            }
            tracing::info!(
                windows = windows.len(),
                tabs = windows
                    .iter()
                    .map(|window| window.tabs.len())
                    .sum::<usize>(),
                "Imported open File Explorer windows into SuperExplorer"
            );
            let this_window = window_from_snapshot(&windows[0]);
            let mut imported_hwnds = Vec::new();
            if this_window.is_some() {
                imported_hwnds.push(windows[0].hwnd);
            }
            imported_hwnds.extend(spawn_extra_windows(&windows[1..]));
            explorer_shell_win::close_explorer_windows(&imported_hwnds);
            LaunchImport {
                this_window,
                this_pc: false,
            }
        }
        Err(error) => {
            tracing::warn!(%error, "File Explorer tab import failed");
            LaunchImport {
                this_window: None,
                this_pc: false,
            }
        }
    }
}

pub fn this_pc_entry() -> HistoryEntry {
    HistoryEntry::new(
        LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned()),
        "This PC",
    )
}

fn window_allowed_for_import(window: &ExplorerWindowSnapshot) -> bool {
    let Ok(prefix) = env::var(IMPORT_PATH_PREFIX_ENV) else {
        return true;
    };
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return true;
    }
    let prefix = prefix
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    window.tabs.iter().all(|tab| {
        tab.location.path().is_some_and(|path| {
            let path = path
                .to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_ascii_lowercase();
            path == prefix || path.starts_with(&format!("{prefix}\\"))
        })
    })
}

fn read_initial_tabs() -> Option<ImportedTabsPayload> {
    if let Some(payload) = read_initial_tabs_file() {
        return Some(payload);
    }
    let raw = env::var(INITIAL_TABS_ENV).ok()?;
    serde_json::from_str(&raw).ok()
}

fn read_initial_tabs_file() -> Option<ImportedTabsPayload> {
    let path = env::var(INITIAL_TABS_FILE_ENV).ok()?;
    let raw = fs::read_to_string(&path).ok()?;
    let payload = serde_json::from_str(&raw).ok();
    let _ = fs::remove_file(path);
    payload
}

fn spawn_extra_windows(windows: &[ExplorerWindowSnapshot]) -> Vec<isize> {
    let Ok(exe) = env::current_exe() else {
        return Vec::new();
    };
    let mut imported = Vec::new();
    for (index, window) in windows.iter().enumerate() {
        let Some(payload) = payload_from_snapshot(window) else {
            tracing::warn!(hwnd = window.hwnd, "Skipped Explorer window with no tabs");
            continue;
        };
        match spawn_imported_window(&exe, &payload, index) {
            Ok(()) => imported.push(window.hwnd),
            Err(error) => tracing::warn!(
                hwnd = window.hwnd,
                %error,
                "Failed to spawn SuperExplorer window for imported Explorer tabs"
            ),
        }
    }
    imported
}

fn spawn_imported_window(
    exe: &std::path::Path,
    payload: &ImportedTabsPayload,
    index: usize,
) -> Result<(), String> {
    let encoded =
        serde_json::to_string(payload).map_err(|error| format!("encode imported tabs: {error}"))?;
    let mut command = Command::new(exe);
    command
        .env_remove(WIN_E_ENV)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if encoded.len() > ENV_PAYLOAD_SOFT_LIMIT {
        let path = env::temp_dir().join(format!(
            "superexplorer-import-{}-{index}.json",
            std::process::id()
        ));
        fs::write(&path, encoded.as_bytes())
            .map_err(|error| format!("write imported tabs file: {error}"))?;
        command
            .env(INITIAL_TABS_FILE_ENV, &path)
            .env_remove(INITIAL_TABS_ENV);
    } else {
        command
            .env(INITIAL_TABS_ENV, encoded)
            .env_remove(INITIAL_TABS_FILE_ENV);
    }
    // SAFETY: ASFW_ANY / the child pid are integers; the call only grants
    // foreground permission for the imported SuperExplorer window.
    let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(u32::MAX) };
    let child = command
        .spawn()
        .map_err(|error| format!("spawn SuperExplorer: {error}"))?;
    let _ =
        unsafe { windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(child.id()) };
    drop(child);
    Ok(())
}

fn payload_from_snapshot(window: &ExplorerWindowSnapshot) -> Option<ImportedTabsPayload> {
    if window.tabs.is_empty() {
        return None;
    }
    Some(ImportedTabsPayload {
        tabs: window
            .tabs
            .iter()
            .map(|tab| ImportedTabPayload {
                location: tab.location.clone(),
                display_title: tab.display_title.clone(),
            })
            .collect(),
        active: window.active_index.min(window.tabs.len() - 1),
    })
}

fn window_from_snapshot(window: &ExplorerWindowSnapshot) -> Option<ExplorerWindowState> {
    window_from_payload(&payload_from_snapshot(window)?)
}

fn window_from_payload(payload: &ImportedTabsPayload) -> Option<ExplorerWindowState> {
    if payload.tabs.is_empty() {
        return None;
    }
    let fallback = this_pc_entry();
    let tabs = payload
        .tabs
        .iter()
        .map(|tab| {
            TabState::new(HistoryEntry::new(
                tab.location.clone(),
                tab.display_title.clone(),
            ))
        })
        .collect::<Vec<_>>();
    let active = payload.active.min(tabs.len() - 1);
    let active_id = tabs[active].id;
    ExplorerWindowState::from_restored_tabs(tabs, active_id, fallback).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_shell_win::ExplorerTabSnapshot;

    #[test]
    fn payload_round_trips_every_tab_and_active_index() {
        let payload = ImportedTabsPayload {
            tabs: vec![
                ImportedTabPayload {
                    location: LocationDescriptor::file_system(r"D:\SuperExplorer"),
                    display_title: "SuperExplorer".into(),
                },
                ImportedTabPayload {
                    location: LocationDescriptor::ParsingName("shell:MyComputerFolder".into()),
                    display_title: "本機".into(),
                },
            ],
            active: 1,
        };
        let encoded = serde_json::to_string(&payload).unwrap();
        let decoded: ImportedTabsPayload = serde_json::from_str(&encoded).unwrap();
        let window = window_from_payload(&decoded).unwrap();
        assert_eq!(window.tabs().len(), 2);
        assert_eq!(
            window.tabs()[0].history.current().unwrap().display_title,
            "SuperExplorer"
        );
        assert_eq!(
            window.active_tab().history.current().unwrap().display_title,
            "本機"
        );
    }

    #[test]
    fn payload_file_round_trips_every_tab() {
        let payload = ImportedTabsPayload {
            tabs: vec![ImportedTabPayload {
                location: LocationDescriptor::file_system(r"D:\SuperExplorer"),
                display_title: "SuperExplorer".into(),
            }],
            active: 0,
        };
        let path = env::temp_dir().join(format!(
            "superexplorer-import-test-{}.json",
            std::process::id()
        ));
        fs::write(&path, serde_json::to_string(&payload).unwrap()).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let decoded: ImportedTabsPayload = serde_json::from_str(&raw).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(decoded, payload);
    }

    #[test]
    fn snapshot_payload_keeps_visual_order_and_active_tab() {
        let snapshot = ExplorerWindowSnapshot {
            hwnd: 1,
            tabs: vec![
                ExplorerTabSnapshot {
                    location: LocationDescriptor::file_system(r"D:\a"),
                    display_title: "a".into(),
                },
                ExplorerTabSnapshot {
                    location: LocationDescriptor::file_system(r"D:\b"),
                    display_title: "b".into(),
                },
                ExplorerTabSnapshot {
                    location: LocationDescriptor::file_system(r"D:\c"),
                    display_title: "c".into(),
                },
            ],
            active_index: 2,
        };
        let payload = payload_from_snapshot(&snapshot).unwrap();
        assert_eq!(payload.tabs.len(), 3);
        assert_eq!(payload.active, 2);
        assert_eq!(payload.tabs[0].display_title, "a");
        assert_eq!(payload.tabs[2].display_title, "c");
        let window = window_from_payload(&payload).unwrap();
        assert_eq!(
            window.active_tab().history.current().unwrap().display_title,
            "c"
        );
    }

    #[test]
    fn import_prefix_keeps_only_windows_under_that_folder() {
        let allowed = ExplorerWindowSnapshot {
            hwnd: 1,
            tabs: vec![ExplorerTabSnapshot {
                location: LocationDescriptor::file_system(r"D:\tmp\roundtrip\alpha"),
                display_title: "alpha".into(),
            }],
            active_index: 0,
        };
        let mixed = ExplorerWindowSnapshot {
            hwnd: 2,
            tabs: vec![ExplorerTabSnapshot {
                location: LocationDescriptor::file_system(r"D:\UE5.8"),
                display_title: "UE5.8".into(),
            }],
            active_index: 0,
        };
        unsafe {
            env::set_var(IMPORT_PATH_PREFIX_ENV, r"D:\tmp\roundtrip");
        }
        let keep = window_allowed_for_import(&allowed);
        let skip = window_allowed_for_import(&mixed);
        unsafe {
            env::remove_var(IMPORT_PATH_PREFIX_ENV);
        }
        assert!(keep);
        assert!(!skip);
    }

    #[test]
    fn empty_snapshot_is_not_imported() {
        let snapshot = ExplorerWindowSnapshot {
            hwnd: 1,
            tabs: Vec::new(),
            active_index: 0,
        };
        assert!(payload_from_snapshot(&snapshot).is_none());
        assert!(window_from_snapshot(&snapshot).is_none());
    }

    #[test]
    fn live_snapshot_converts_every_tab_into_window_state() {
        let windows = snapshot_open_explorer_windows().unwrap_or_else(|_| Vec::new());
        for window in windows {
            let payload = payload_from_snapshot(&window).expect("non-empty snapshot");
            assert_eq!(payload.tabs.len(), window.tabs.len());
            let state = window_from_payload(&payload).expect("imported window");
            assert_eq!(state.tabs().len(), window.tabs.len());
            assert!(
                state.active_tab().history.current().is_some(),
                "active imported tab must have a location"
            );
        }
    }

    #[test]
    #[ignore = "opens live File Explorer windows; run explicitly when Explorer is healthy"]
    fn live_isolated_round_trip_under_prefix() {
        use explorer_shell_win::{
            ExplorerHandoffWindow, close_explorer_windows, open_file_explorer_windows,
        };
        use std::{
            process::{Command, Stdio},
            time::Duration,
        };

        let exe = debug_super_explorer_exe();
        assert!(
            exe.is_file(),
            "build SuperExplorer.exe first: {}",
            exe.display()
        );
        terminate_matching_exe(&exe);

        let root = std::path::PathBuf::from(format!(r"D:\se-rt-{}", std::process::id()));
        let folders = ["alpha", "beta", "gamma"].map(|name| root.join(name));
        for folder in &folders {
            fs::create_dir_all(folder).unwrap();
            fs::write(folder.join("marker.txt"), name_of(folder)).unwrap();
        }
        let prefix = root.to_string_lossy().into_owned();

        let user_before = snapshot_keys_outside_prefix(&prefix);
        let opened = open_file_explorer_windows(
            &folders
                .iter()
                .map(|folder| ExplorerHandoffWindow {
                    tabs: vec![LocationDescriptor::file_system(folder)],
                    active: 0,
                })
                .collect::<Vec<_>>(),
        );
        let opened = match opened {
            Ok(hwnds) => hwnds,
            Err(error) => {
                let _ = fs::remove_dir_all(&root);
                panic!("open isolated Explorers: {error}");
            }
        };

        if !wait_until(Duration::from_secs(15), || {
            prefix_window_count(&prefix) >= folders.len()
        }) {
            close_explorer_windows(&opened);
            let _ = fs::remove_dir_all(&root);
            panic!("prefix Explorers never appeared in the snapshot under {prefix}");
        }

        let isolated = root.join(".isolated-app");
        fs::create_dir_all(isolated.join("logs")).unwrap();
        let child = Command::new(&exe)
            .env("LOCALAPPDATA", isolated.join("appdata"))
            .env("EXPLORER_LOG_DIR", isolated.join("logs"))
            .env(IMPORT_PATH_PREFIX_ENV, &prefix)
            .env_remove("SUPEREXPLORER_UITEST_HANDOFF_AFTER_MS")
            .env_remove(WIN_E_ENV)
            .env_remove(INITIAL_TABS_ENV)
            .env_remove(INITIAL_TABS_FILE_ENV)
            .env_remove("EXPLORER_AUTO_CLOSE_MS")
            .env_remove("EXPLORER_VISUAL_FIXTURE")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let child = match child {
            Ok(child) => child,
            Err(error) => {
                close_explorer_windows(&opened);
                let _ = fs::remove_dir_all(&root);
                panic!("launch SuperExplorer: {error}");
            }
        };
        let started = child.id();
        drop(child);

        if !wait_until(Duration::from_secs(30), || {
            prefix_window_count(&prefix) == 0 && matching_exe_count(&exe) >= 1
        }) {
            terminate_matching_exe(&exe);
            close_explorer_windows(&opened);
            let leftover = prefix_window_count(&prefix);
            let se = matching_exe_count(&exe);
            let _ = fs::remove_dir_all(&root);
            panic!(
                "import did not take prefix Explorers (started={started} leftover={leftover} se={se})"
            );
        }

        let after = snapshot_open_explorer_windows().unwrap_or_default();
        let leftover_prefix = prefix_window_count(&prefix);
        let user_after = keys_outside_prefix(&after, &prefix);
        let lost_user = user_before
            .difference(&user_after)
            .cloned()
            .collect::<Vec<_>>();
        terminate_matching_exe(&exe);
        close_prefix_windows(&prefix, &after);
        close_explorer_windows(&opened);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(
            leftover_prefix, 0,
            "File Explorer windows were still open after SuperExplorer imported them"
        );
        assert!(
            folders
                .iter()
                .all(|folder| !snapshot_contains_path(&after, folder)),
            "imported test folders were still listed in File Explorer"
        );
        assert!(
            lost_user.is_empty(),
            "user Explorer folders disappeared: {lost_user:?}"
        );
    }

    fn debug_super_explorer_exe() -> std::path::PathBuf {
        let mut path = env::current_exe().expect("current exe");
        path.pop();
        if path
            .file_name()
            .is_some_and(|name| name == "deps" || name == "examples")
        {
            path.pop();
        }
        path.push("SuperExplorer.exe");
        path
    }

    fn name_of(path: &std::path::Path) -> String {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("marker")
            .to_owned()
    }

    fn wait_until(timeout: std::time::Duration, mut ready: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if ready() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        ready()
    }

    fn prefix_window_count(prefix: &str) -> usize {
        snapshot_open_explorer_windows()
            .unwrap_or_default()
            .into_iter()
            .filter(|window| window_under_prefix(window, prefix))
            .count()
    }

    fn window_under_prefix(window: &ExplorerWindowSnapshot, prefix: &str) -> bool {
        let prefix = normalize_prefix(prefix);
        !window.tabs.is_empty()
            && window.tabs.iter().all(|tab| {
                tab.location.path().is_some_and(|path| {
                    let path = normalize_prefix(&path.to_string_lossy());
                    path == prefix || path.starts_with(&format!("{prefix}\\"))
                })
            })
    }

    fn normalize_prefix(value: &str) -> String {
        value
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    }

    fn snapshot_keys_outside_prefix(prefix: &str) -> std::collections::HashSet<String> {
        keys_outside_prefix(
            &snapshot_open_explorer_windows().unwrap_or_default(),
            prefix,
        )
    }

    fn keys_outside_prefix(
        windows: &[ExplorerWindowSnapshot],
        prefix: &str,
    ) -> std::collections::HashSet<String> {
        let prefix = normalize_prefix(prefix);
        windows
            .iter()
            .filter(|window| !window_under_prefix(window, &prefix))
            .flat_map(|window| window.tabs.iter())
            .filter_map(|tab| {
                tab.location.path().map(|path| {
                    path.to_string_lossy()
                        .replace('/', "\\")
                        .trim_end_matches('\\')
                        .to_ascii_lowercase()
                })
            })
            .collect()
    }

    fn snapshot_contains_path(
        windows: &[ExplorerWindowSnapshot],
        expected: &std::path::Path,
    ) -> bool {
        let expected = normalize_prefix(&expected.to_string_lossy());
        windows.iter().flat_map(|window| window.tabs.iter()).any(|tab| {
            tab.location.path().is_some_and(|path| {
                normalize_prefix(&path.to_string_lossy()) == expected
            })
        })
    }

    fn close_prefix_windows(prefix: &str, windows: &[ExplorerWindowSnapshot]) {
        let hwnds = windows
            .iter()
            .filter(|window| window_under_prefix(window, prefix))
            .map(|window| window.hwnd)
            .collect::<Vec<_>>();
        explorer_shell_win::close_explorer_windows(&hwnds);
    }

    fn matching_exe_count(exe: &std::path::Path) -> usize {
        matching_exe_pids(exe).len()
    }

    fn matching_exe_pids(exe: &std::path::Path) -> Vec<u32> {
        let expected = exe
            .canonicalize()
            .unwrap_or_else(|_| exe.to_path_buf())
            .to_string_lossy()
            .to_ascii_lowercase();
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_Process | ForEach-Object { '{0}|{1}' -f $_.ProcessId, $_.ExecutablePath }",
            ])
            .output()
            .ok();
        let Some(output) = output else {
            return Vec::new();
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let (pid, path) = line.split_once('|')?;
                let path = std::path::PathBuf::from(path.trim())
                    .canonicalize()
                    .ok()?
                    .to_string_lossy()
                    .to_ascii_lowercase();
                if path == expected {
                    pid.trim().parse().ok()
                } else {
                    None
                }
            })
            .collect()
    }

    fn terminate_matching_exe(exe: &std::path::Path) {
        for pid in matching_exe_pids(exe) {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .status();
        }
    }
}
