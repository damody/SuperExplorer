//! Keep the UI at the desktop user's privileges even when launched by setup.
//! This module belongs only to the UI binary, never the MFT helper or service.
#![expect(
    unsafe_code,
    reason = "checking tokens and launching with the desktop token require Win32 calls"
)]

use anyhow::{Context, Result, ensure};
use std::{mem::size_of, os::windows::ffi::OsStrExt};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{
            DuplicateTokenEx, GetTokenInformation, SecurityImpersonation, TOKEN_ASSIGN_PRIMARY,
            TOKEN_DUPLICATE, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation, TokenPrimary,
        },
        System::{
            Environment::GetCommandLineW,
            Threading::{
                CREATE_PROCESS_LOGON_FLAGS, CreateProcessWithTokenW, GetCurrentProcess,
                OpenProcess, OpenProcessToken, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION,
                PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
            },
        },
        UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId},
    },
    core::{PCWSTR, PWSTR},
};

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: this wrapper exclusively owns a successfully opened handle.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn open_token(process: HANDLE, duplicate: bool) -> Result<OwnedHandle> {
    let access = if duplicate {
        TOKEN_QUERY | TOKEN_DUPLICATE
    } else {
        TOKEN_QUERY
    };
    let mut token = HANDLE::default();
    // SAFETY: process is live; token is a writable output owned on success.
    unsafe { OpenProcessToken(process, access, &mut token) }.context("open process token")?;
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
/// Fail closed if no ordinary desktop token is available, preventing relaunch loops.
pub(super) fn relaunch_if_elevated() -> Result<bool> {
    // SAFETY: GetCurrentProcess returns a borrowed pseudo-handle; it is not closed.
    let current = open_token(unsafe { GetCurrentProcess() }, false)?;
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
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, shell_pid) }
            .context("open desktop shell process")?,
    );
    let shell_token = open_token(shell_process.0, true)?;
    ensure!(!is_elevated(shell_token.0)?, "desktop shell is elevated");

    let mut primary = HANDLE::default();
    // SAFETY: shell_token permits duplication; primary is writable output.
    unsafe {
        DuplicateTokenEx(
            shell_token.0,
            TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut primary,
        )
    }
    .context("duplicate ordinary desktop token")?;
    let primary = OwnedHandle(primary);
    let executable = std::env::current_exe().context("resolve SuperExplorer executable")?;
    let executable: Vec<u16> = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: the process command line is a live, NUL-terminated UTF-16 string.
    // Copy it exactly so quoted paths, Unicode, and diagnostics arguments survive.
    let raw_command_line = unsafe { GetCommandLineW() };
    let mut command_line = unsafe { raw_command_line.as_wide() }.to_vec();
    command_line.push(0);
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: buffers live through the call, command_line is mutable and terminated,
    // and output handles are taken into exclusive ownership on success. A null
    // environment uses the desktop user's profile instead of the installer's.
    unsafe {
        CreateProcessWithTokenW(
            primary.0,
            CREATE_PROCESS_LOGON_FLAGS(0),
            PCWSTR(executable.as_ptr()),
            Some(PWSTR(command_line.as_mut_ptr())),
            PROCESS_CREATION_FLAGS(0),
            None,
            PCWSTR::null(),
            &startup,
            &mut process,
        )
    }
    .context("launch SuperExplorer with ordinary desktop privileges")?;
    let _process = OwnedHandle(process.hProcess);
    let _thread = OwnedHandle(process.hThread);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_startup_does_not_launch_a_replacement() {
        let token = open_token(unsafe { GetCurrentProcess() }, false).unwrap();
        if !is_elevated(token.0).unwrap() {
            assert!(!relaunch_if_elevated().unwrap());
        }
    }

    #[test]
    fn invalid_token_is_an_error_instead_of_assuming_ordinary_privileges() {
        assert!(is_elevated(HANDLE::default()).is_err());
    }
}
