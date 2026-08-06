//! Privileged, read-only NTFS index refresher installed as a Windows service.

#![cfg(windows)]

#[path = "../mft_size_map.rs"]
mod mft_size_map;

use std::{
    ffi::c_void,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const SERVICE_NAME: &str = "SuperExplorerMft";
const SERVICE_RUNNING: u32 = 4;
const SERVICE_STOPPED: u32 = 1;
const SERVICE_STOP_PENDING: u32 = 3;
const SERVICE_ACCEPT_STOP: u32 = 1;
const SERVICE_CONTROL_STOP: u32 = 1;
const SERVICE_WIN32_OWN_PROCESS: u32 = 0x10;

static STOPPED: AtomicBool = AtomicBool::new(false);

#[repr(C)]
struct ServiceTableEntryW {
    name: *mut u16,
    main: Option<unsafe extern "system" fn(u32, *mut *mut u16)>,
}

#[repr(C)]
struct ServiceStatus {
    service_type: u32,
    current_state: u32,
    controls_accepted: u32,
    win32_exit_code: u32,
    service_specific_exit_code: u32,
    checkpoint: u32,
    wait_hint: u32,
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn StartServiceCtrlDispatcherW(table: *const ServiceTableEntryW) -> i32;
    fn RegisterServiceCtrlHandlerW(
        name: *const u16,
        handler: Option<unsafe extern "system" fn(u32)>,
    ) -> *mut c_void;
    fn SetServiceStatus(handle: *mut c_void, status: *const ServiceStatus) -> i32;
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

unsafe extern "system" fn control_handler(control: u32) {
    if control == SERVICE_CONTROL_STOP {
        STOPPED.store(true, Ordering::Release);
    }
}

unsafe extern "system" fn service_main(_: u32, _: *mut *mut u16) {
    let name = wide(SERVICE_NAME);
    // SAFETY: SCM owns the service callback lifetime and the UTF-16 name is terminated.
    let handle = unsafe { RegisterServiceCtrlHandlerW(name.as_ptr(), Some(control_handler)) };
    if handle.is_null() {
        return;
    }
    report(handle, SERVICE_RUNNING, SERVICE_ACCEPT_STOP);
    while !STOPPED.load(Ordering::Acquire) {
        refresh_fixed_volumes();
        for _ in 0..300 {
            if STOPPED.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    report(handle, SERVICE_STOP_PENDING, 0);
    report(handle, SERVICE_STOPPED, 0);
}

fn report(handle: *mut c_void, state: u32, controls: u32) {
    let status = ServiceStatus {
        service_type: SERVICE_WIN32_OWN_PROCESS,
        current_state: state,
        controls_accepted: controls,
        win32_exit_code: 0,
        service_specific_exit_code: 0,
        checkpoint: 0,
        wait_hint: 0,
    };
    // SAFETY: handle came from SCM and status points to initialized storage.
    let _ = unsafe { SetServiceStatus(handle, &raw const status) };
}

fn cache_root() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("SuperExplorer")
        .join("MftIndex")
}

fn refresh_fixed_volumes() {
    let cache = cache_root();
    if std::fs::create_dir_all(&cache).is_err() {
        return;
    }
    for letter in b'A'..=b'Z' {
        if STOPPED.load(Ordering::Acquire) {
            return;
        }
        let root = PathBuf::from(format!("{}:\\", char::from(letter)));
        if !root.exists() {
            continue;
        }
        let Ok(index) = mft_size_map::read_volume_index(&root, || STOPPED.load(Ordering::Acquire))
        else {
            continue;
        };
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = cache.join(format!("{}.{}.tmp", char::from(letter), stamp));
        let destination = cache.join(format!("{}.semftidx", char::from(letter)));
        if mft_size_map::write_service_index(&temporary, &index).is_ok() {
            let _ = std::fs::remove_file(&destination);
            let _ = std::fs::rename(&temporary, &destination);
        }
    }
}

fn main() {
    let mut name = wide(SERVICE_NAME);
    let table = [
        ServiceTableEntryW {
            name: name.as_mut_ptr(),
            main: Some(service_main),
        },
        ServiceTableEntryW {
            name: std::ptr::null_mut(),
            main: None,
        },
    ];
    // SAFETY: table is terminated and remains alive until the dispatcher returns.
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        // Developer smoke mode: one refresh when launched outside SCM.
        refresh_fixed_volumes();
    }
}
