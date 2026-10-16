//! Keep the UI at the desktop user's privileges even when launched by setup.
//! This module belongs only to the UI binary, never the MFT helper or service.
#![expect(
    unsafe_code,
    reason = "checking tokens and inheriting the desktop process token require Win32 calls"
)]

use anyhow::{Context, Result, ensure};
use std::{mem::size_of, os::windows::ffi::OsStrExt};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::{
            Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock, GetCommandLineW},
            Threading::{
                CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
                DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess,
                InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST, OpenProcess,
                OpenProcessToken, PROC_THREAD_ATTRIBUTE_PARENT_PROCESS, PROCESS_CREATE_PROCESS,
                PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, ResumeThread,
                STARTUPINFOEXW, STARTUPINFOW, TerminateProcess, UpdateProcThreadAttribute,
            },
        },
        UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId},
    },
    core::{PCWSTR, PWSTR, w},
};

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: this wrapper exclusively owns a successfully opened handle.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn open_token(process: HANDLE) -> Result<OwnedHandle> {
    let mut token = HANDLE::default();
    // SAFETY: process is live; token is a writable output owned on success.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.context("open process token")?;
    Ok(OwnedHandle(token))
}

fn is_elevated(token: HANDLE) -> Result<bool> {
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0;
    // SAFETY: token is queryable and the output matches TokenElevation's size.
    unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    }
    .context("query process elevation")?;
    Ok(elevation.TokenIsElevated != 0)
}

/// Return true once a replacement has been started; the elevated caller must exit.
/// Match an ordinary desktop when available. An already elevated desktop has
/// no ordinary shell token to borrow; keep its current privilege level without
/// relaunching or changing the machine's security configuration.
pub(super) fn relaunch_if_elevated() -> Result<bool> {
    // SAFETY: GetCurrentProcess returns a borrowed pseudo-handle; it is not closed.
    let current = open_token(unsafe { GetCurrentProcess() })?;
    if !is_elevated(current.0)? {
        return Ok(false);
    }

    // SAFETY: these calls return the shell window and write its owning process ID.
    let shell = unsafe { GetShellWindow() };
    ensure!(
        !shell.is_invalid(),
        "no interactive desktop shell is available"
    );
    let mut shell_pid = 0;
    unsafe { GetWindowThreadProcessId(shell, Some(&mut shell_pid)) };
    ensure!(shell_pid != 0, "desktop shell has no process ID");
    // SAFETY: the PID came from the shell window; the handle is owned on success.
    let shell_process = OwnedHandle(
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_CREATE_PROCESS,
                false,
                shell_pid,
            )
        }
        .context("open desktop shell process")?,
    );
    let shell_token = open_token(shell_process.0)?;
    if !should_relaunch(true, is_elevated(shell_token.0)?) {
        explorer_common::record_process_error_message(
            explorer_common::ErrorSeverity::Warning,
            "application",
            "startup_privileges",
            "Desktop shell is elevated; continuing with the current desktop privileges instead of failing startup",
            Some(file!()),
        );
        return Ok(false);
    }
    let executable = std::env::current_exe().context("resolve SuperExplorer executable")?;
    // SAFETY: the process command line is a live, NUL-terminated UTF-16 string.
    // Copy it exactly so quoted paths, Unicode, and diagnostics arguments survive.
    let raw_command_line = unsafe { GetCommandLineW() };
    let command_line = unsafe { raw_command_line.as_wide() }.to_vec();
    let process =
        launch_with_desktop_parent(shell_process.0, shell_token.0, &executable, &command_line)?;
    drop(process);
    Ok(true)
}

struct DesktopEnvironment(*mut core::ffi::c_void);

impl Drop for DesktopEnvironment {
    fn drop(&mut self) {
        // SAFETY: this environment was allocated by CreateEnvironmentBlock.
        let _ = unsafe { DestroyEnvironmentBlock(self.0) };
    }
}

struct ProcessAttributes {
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
    // Pointer-aligned storage must outlive both the list and process creation.
    _storage: Vec<usize>,
}

impl Drop for ProcessAttributes {
    fn drop(&mut self) {
        // SAFETY: list was initialized successfully and its storage is still live.
        unsafe { DeleteProcThreadAttributeList(self.list) };
    }
}

fn launch_with_desktop_parent(
    desktop: HANDLE,
    token: HANDLE,
    executable: &std::path::Path,
    command_line: &[u16],
) -> Result<OwnedHandle> {
    ensure!(
        !is_elevated(token)?,
        "replacement desktop token is elevated"
    );
    let mut bytes = 0;
    // SAFETY: the first call only obtains the required buffer size.
    let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut bytes) };
    ensure!(bytes > 0, "desktop process attribute size is unavailable");
    let mut storage = vec![0_usize; bytes.div_ceil(size_of::<usize>())];
    let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
    // SAFETY: the aligned buffer has the requested size and remains owned below.
    unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &mut bytes) }
        .context("initialize desktop process attributes")?;
    let attributes = ProcessAttributes {
        list,
        _storage: storage,
    };
    // The specified parent supplies the child's ordinary token and device map.
    // This avoids CreateProcessWithTokenW's dependency on Secondary Logon.
    // SAFETY: desktop permits PROCESS_CREATE_PROCESS; its address and the list
    // remain live until CreateProcessW has consumed the attribute.
    unsafe {
        UpdateProcThreadAttribute(
            attributes.list,
            0,
            PROC_THREAD_ATTRIBUTE_PARENT_PROCESS as usize,
            Some((&raw const desktop).cast()),
            size_of::<HANDLE>(),
            None,
            None,
        )
    }
    .context("set ordinary desktop process parent")?;
    let mut environment = core::ptr::null_mut();
    // SAFETY: token is queryable and environment is an owned output on success.
    unsafe { CreateEnvironmentBlock(&mut environment, Some(token), false) }
        .context("create ordinary desktop environment")?;
    let environment = DesktopEnvironment(environment);
    let directory = executable
        .parent()
        .context("resolve executable directory")?;
    let wide =
        |value: &std::ffi::OsStr| -> Vec<u16> { value.encode_wide().chain(Some(0)).collect() };
    let application = wide(executable.as_os_str());
    let directory = wide(directory.as_os_str());
    let mut command_line = command_line.to_vec();
    command_line.push(0);
    let startup = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW {
            cb: size_of::<STARTUPINFOEXW>() as u32,
            lpDesktop: PWSTR(w!("winsta0\\default").as_ptr().cast_mut()),
            ..Default::default()
        },
        lpAttributeList: attributes.list,
    };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: all buffers and attributes remain live; handles are not inherited.
    // Start suspended so we can verify the actual child token before any UI runs.
    unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT
                | CREATE_UNICODE_ENVIRONMENT
                | CREATE_SUSPENDED
                | CREATE_NO_WINDOW,
            Some(environment.0.cast_const()),
            PCWSTR(directory.as_ptr()),
            &startup.StartupInfo,
            &mut process,
        )
    }
    .context("launch SuperExplorer with ordinary desktop privileges")?;
    let process_handle = OwnedHandle(process.hProcess);
    let thread = OwnedHandle(process.hThread);
    let verified = (|| -> Result<()> {
        let child_token = open_token(process_handle.0)?;
        ensure!(
            !is_elevated(child_token.0)?,
            "replacement process is unexpectedly elevated"
        );
        // SAFETY: this is the primary thread of our suspended child.
        if unsafe { ResumeThread(thread.0) } == u32::MAX {
            return Err(windows::core::Error::from_win32())
                .context("resume ordinary desktop process");
        }
        Ok(())
    })();
    if let Err(error) = verified {
        // SAFETY: only our newly created, still-suspended child is terminated.
        let _ = unsafe { TerminateProcess(process_handle.0, 1) };
        return Err(error);
    }
    Ok(process_handle)
}

fn should_relaunch(current_elevated: bool, desktop_elevated: bool) -> bool {
    current_elevated && !desktop_elevated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_desktop_continues_without_a_relaunch_loop() {
        assert!(!should_relaunch(true, true));
        assert!(!should_relaunch(false, true));
    }

    #[test]
    fn elevated_setup_uses_an_ordinary_desktop_when_available() {
        assert!(should_relaunch(true, false));
        assert!(!should_relaunch(false, false));
    }

    #[test]
    fn ordinary_startup_does_not_launch_a_replacement() {
        let token = open_token(unsafe { GetCurrentProcess() }).unwrap();
        if !is_elevated(token.0).unwrap() {
            assert!(!relaunch_if_elevated().unwrap());
        }
    }

    #[test]
    fn invalid_token_is_an_error_instead_of_assuming_ordinary_privileges() {
        assert!(is_elevated(HANDLE::default()).is_err());
    }

    #[test]
    fn desktop_parent_starts_an_ordinary_child_with_an_explicit_working_directory() {
        use windows::Win32::{
            Foundation::WAIT_OBJECT_0,
            System::Threading::{GetExitCodeProcess, WaitForSingleObject},
        };
        let shell = unsafe { GetShellWindow() };
        assert!(!shell.is_invalid(), "requires an interactive desktop");
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(shell, Some(&mut pid)) };
        let desktop = OwnedHandle(
            unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_CREATE_PROCESS,
                    false,
                    pid,
                )
            }
            .unwrap(),
        );
        let token = open_token(desktop.0).unwrap();
        if is_elevated(token.0).unwrap() {
            // The policy deliberately keeps an already elevated desktop unchanged.
            return;
        }
        let executable = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32")
            .join("cmd.exe");
        let command: Vec<u16> = format!("\"{}\" /d /c exit 0", executable.display())
            .encode_utf16()
            .collect();
        let process =
            launch_with_desktop_parent(desktop.0, token.0, &executable, &command).unwrap();
        assert_eq!(
            unsafe { WaitForSingleObject(process.0, 5000) },
            WAIT_OBJECT_0
        );
        let mut exit = u32::MAX;
        unsafe { GetExitCodeProcess(process.0, &mut exit) }.unwrap();
        assert_eq!(exit, 0);
    }
}
