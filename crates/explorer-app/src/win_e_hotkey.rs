//! Process-lifetime Win+E interception that opens a SuperExplorer window.

#![expect(
    unsafe_code,
    reason = "Win+E interception uses a process-lifetime low-level keyboard hook"
)]

use std::{
    env,
    process::{Command, Stdio},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread,
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WAIT_ABANDONED, WAIT_OBJECT_0, WPARAM},
        System::Threading::{
            CreateMutexW, GetCurrentThreadId, INFINITE, ReleaseMutex, WaitForSingleObject,
        },
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, VK_CONTROL, VK_E, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
            },
            WindowsAndMessaging::{
                AllowSetForegroundWindow, CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION,
                KBDLLHOOKSTRUCT, MSG, PostThreadMessageW, SW_RESTORE, SetForegroundWindow,
                SetWindowsHookExW, ShowWindow, TranslateMessage, UnhookWindowsHookEx,
                WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
            },
        },
    },
    core::PCWSTR,
};

use crate::explorer_import::WIN_E_ENV;

const HOOK_MUTEX: &str = "Local\\SuperExplorer.WinEHookOwner.v1";
const ASFW_ANY: u32 = u32::MAX;

static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
static WIN_HELD: AtomicBool = AtomicBool::new(false);
static CONSUMED_CHORD: AtomicBool = AtomicBool::new(false);
static ACTIVE_KEY: AtomicU32 = AtomicU32::new(0);
static STOPPED: AtomicBool = AtomicBool::new(false);
static STARTED: OnceLock<Mutex<bool>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WinEDecision {
    consume: bool,
    fire: bool,
}

pub fn start_win_e_hotkey() {
    let started = STARTED.get_or_init(|| Mutex::new(false));
    let Ok(mut guard) = started.lock() else {
        return;
    };
    if *guard {
        return;
    }
    *guard = true;
    drop(guard);
    thread::Builder::new()
        .name("superexplorer-win-e".into())
        .spawn(win_e_owner_loop)
        .ok();
}

fn win_e_owner_loop() {
    let name: Vec<u16> = HOOK_MUTEX.encode_utf16().chain([0]).collect();
    let mutex = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) };
    let Ok(mutex) = mutex else {
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
        let installed = run_hook_until_quit();
        let _ = unsafe { ReleaseMutex(mutex) };
        if STOPPED.load(Ordering::Acquire) {
            break;
        }
        if !installed {
            thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    let _ = unsafe { CloseHandle(mutex) };
}

fn run_hook_until_quit() -> bool {
    HOOK_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0) };
    let Ok(hook) = hook else {
        HOOK_THREAD.store(0, Ordering::Release);
        return false;
    };
    let mut message = MSG::default();
    while unsafe { GetMessageW(&raw mut message, None, 0, 0) }.as_bool() {
        if STOPPED.load(Ordering::Acquire) {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    let _ = unsafe { UnhookWindowsHookEx(hook) };
    HOOK_THREAD.store(0, Ordering::Release);
    true
}

pub fn activate_hwnd(hwnd: u64) {
    if hwnd == 0 {
        return;
    }
    let handle = HWND(hwnd as *mut std::ffi::c_void);
    let _ = unsafe { ShowWindow(handle, SW_RESTORE) };
    let _ = unsafe { SetForegroundWindow(handle) };
}

pub fn stop_win_e_hotkey() {
    STOPPED.store(true, Ordering::Release);
    let thread_id = HOOK_THREAD.load(Ordering::Acquire);
    if thread_id != 0 {
        let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
    }
}

fn reduce_win_e(
    vk: u32,
    key_down: bool,
    key_up: bool,
    windows_down: bool,
    control: bool,
    alt: bool,
    shift: bool,
    win_held: bool,
    consumed: bool,
    active_key: u32,
) -> (WinEDecision, bool, bool, u32) {
    let is_win = vk == u32::from(VK_LWIN.0) || vk == u32::from(VK_RWIN.0);
    let mut next_win_held = win_held;
    let mut next_consumed = consumed;
    let mut next_active = active_key;
    if key_down && is_win {
        next_win_held = true;
        next_consumed = false;
        return (
            WinEDecision {
                consume: false,
                fire: false,
            },
            next_win_held,
            next_consumed,
            next_active,
        );
    }
    if key_down
        && (windows_down || win_held)
        && vk == u32::from(VK_E.0)
        && !control
        && !alt
        && !shift
    {
        let fire = active_key != vk;
        next_active = vk;
        next_consumed = true;
        return (
            WinEDecision {
                consume: true,
                fire,
            },
            next_win_held,
            next_consumed,
            next_active,
        );
    }
    if key_up && vk == u32::from(VK_E.0) && active_key == vk {
        next_active = 0;
        return (
            WinEDecision {
                consume: true,
                fire: false,
            },
            next_win_held,
            next_consumed,
            next_active,
        );
    }
    if key_up && is_win {
        next_win_held = false;
        let consume = consumed;
        next_consumed = false;
        next_active = 0;
        return (
            WinEDecision {
                consume,
                fire: false,
            },
            next_win_held,
            next_consumed,
            next_active,
        );
    }
    (
        WinEDecision {
            consume: false,
            fire: false,
        },
        next_win_held,
        next_consumed,
        next_active,
    )
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 || lparam.0 == 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let message = wparam.0 as u32;
    let key_down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let key_up = message == WM_KEYUP || message == WM_SYSKEYUP;
    let windows_down = unsafe { GetAsyncKeyState(i32::from(VK_LWIN.0)) } < 0
        || unsafe { GetAsyncKeyState(i32::from(VK_RWIN.0)) } < 0;
    let control = unsafe { GetAsyncKeyState(i32::from(VK_CONTROL.0)) } < 0;
    let alt = unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) } < 0;
    let shift = unsafe { GetAsyncKeyState(i32::from(VK_SHIFT.0)) } < 0;
    let (decision, win_held, consumed, active) = reduce_win_e(
        event.vkCode,
        key_down,
        key_up,
        windows_down,
        control,
        alt,
        shift,
        WIN_HELD.load(Ordering::Acquire),
        CONSUMED_CHORD.load(Ordering::Acquire),
        ACTIVE_KEY.load(Ordering::Acquire),
    );
    WIN_HELD.store(win_held, Ordering::Release);
    CONSUMED_CHORD.store(consumed, Ordering::Release);
    ACTIVE_KEY.store(active, Ordering::Release);
    if decision.fire {
        launch_win_e_window();
    }
    if decision.consume {
        return LRESULT(1);
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn launch_win_e_window() {
    let Ok(exe) = env::current_exe() else {
        return;
    };
    let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    let child = Command::new(exe)
        .env(WIN_E_ENV, "1")
        .env_remove(crate::explorer_import::INITIAL_TABS_ENV)
        .env_remove(crate::explorer_import::INITIAL_TABS_FILE_ENV)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Ok(child) = child {
        let _ = unsafe { AllowSetForegroundWindow(child.id()) };
        drop(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win_e_is_consumed_once_and_cancels_start_menu_release() {
        let e = u32::from(VK_E.0);
        let win = u32::from(VK_LWIN.0);
        let (down_win, held, consumed, active) =
            reduce_win_e(win, true, false, true, false, false, false, false, false, 0);
        assert!(!down_win.consume);
        let (down_e, held, consumed, active) = reduce_win_e(
            e, true, false, true, false, false, false, held, consumed, active,
        );
        assert!(down_e.consume);
        assert!(down_e.fire);
        let (repeat, _, consumed, active) = reduce_win_e(
            e, true, false, true, false, false, false, held, consumed, active,
        );
        assert!(repeat.consume);
        assert!(!repeat.fire);
        let (up_e, held, consumed, active) = reduce_win_e(
            e, false, true, true, false, false, false, held, consumed, active,
        );
        assert!(up_e.consume);
        let (up_win, _, _, _) = reduce_win_e(
            win, false, true, false, false, false, false, held, consumed, active,
        );
        assert!(up_win.consume);
        assert!(!up_win.fire);
    }

    #[test]
    fn standalone_windows_key_is_not_stolen() {
        let win = u32::from(VK_LWIN.0);
        let (down, held, consumed, active) =
            reduce_win_e(win, true, false, true, false, false, false, false, false, 0);
        assert!(!down.consume);
        let (up, _, _, _) = reduce_win_e(
            win, false, true, false, false, false, false, held, consumed, active,
        );
        assert!(!up.consume);
    }
}
