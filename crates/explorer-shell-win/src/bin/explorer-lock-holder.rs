#![cfg_attr(not(windows), allow(dead_code))]
#![cfg_attr(windows, windows_subsystem = "windows")]
#![allow(
    unsafe_code,
    reason = "the owned Restart Manager test helper needs one Win32 window"
)]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::{fs::OpenOptions, io::Write as _, os::windows::fs::OpenOptionsExt as _};
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, WPARAM},
            System::{
                Console::{
                    CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT, SetConsoleCtrlHandler,
                },
                LibraryLoader::GetModuleHandleW,
                Recovery::{REGISTER_APPLICATION_RESTART_FLAGS, RegisterApplicationRestart},
                Threading::GetCurrentThreadId,
            },
            UI::WindowsAndMessaging::{
                CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG,
                PostQuitMessage, PostThreadMessageW, RegisterClassW, TranslateMessage,
                WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_ENDSESSION, WM_NCCREATE,
                WM_QUERYENDSESSION, WM_QUIT, WNDCLASSW, WS_OVERLAPPED, WS_VISIBLE,
            },
        },
        core::{BOOL, w},
    };

    static MAIN_THREAD_ID: AtomicU32 = AtomicU32::new(0);

    unsafe extern "system" fn console_control(control: u32) -> BOOL {
        if matches!(
            control,
            CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT
        ) {
            let thread = MAIN_THREAD_ID.load(Ordering::Acquire);
            if thread != 0 {
                let _ = unsafe {
                    PostThreadMessageW(thread, WM_QUIT, WPARAM::default(), LPARAM::default())
                };
            }
            BOOL(1)
        } else {
            BOOL(0)
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_NCCREATE => {
                let _ = lparam.0 as *const CREATESTRUCTW;
                LRESULT(1)
            }
            WM_QUERYENDSESSION => {
                // This controlled helper has no unsaved state, so it voluntarily exits after
                // consenting to Restart Manager's graceful session-end request.
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    std::process::exit(0);
                });
                LRESULT(1)
            }
            WM_ENDSESSION if wparam.0 != 0 => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            WM_CLOSE | WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    let path = std::env::args_os().nth(1).ok_or("missing lock path")?;
    let _handle = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)?;
    let instance = unsafe { GetModuleHandleW(None) }?;
    MAIN_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
    unsafe { SetConsoleCtrlHandler(Some(console_control), true) }?;
    unsafe { RegisterApplicationRestart(w!(""), REGISTER_APPLICATION_RESTART_FLAGS::default()) }?;
    let class_name = w!("SuperExplorerOwnedLockHolder");
    let class = WNDCLASSW {
        hInstance: instance.into(),
        lpszClassName: class_name,
        lpfnWndProc: Some(window_proc),
        ..WNDCLASSW::default()
    };
    if unsafe { RegisterClassW(&raw const class) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("SuperExplorer controlled lock holder"),
            WS_OVERLAPPED | WS_VISIBLE,
            -32_000,
            -32_000,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }?;
    if window.0.is_null() {
        return Err("failed to create lock-holder window".into());
    }
    println!("READY {}", std::process::id());
    std::io::stdout().flush()?;
    let mut message = MSG::default();
    while unsafe { GetMessageW(&raw mut message, None, 0, 0) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&raw const message);
            DispatchMessageW(&raw const message);
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
