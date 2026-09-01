//! Windows launch classification for File Explorer-style repeated invocations.

use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE},
        System::Threading::CreateMutexW,
    },
    core::PCWSTR,
};

const SESSION_MUTEX_NAME: &str = r"Local\SuperExplorer.LaunchSession.v1";

/// Whether this invocation should participate in repeated-launch detection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchKind {
    /// A normal user launch.
    Ordinary,
    /// A diagnostic, fixture, auto-close, or plugin-development launch.
    Isolated,
}

impl LaunchKind {
    /// Classifies a launch without mutating process state.
    pub fn classify(diagnostics_console: bool, plugin_dlls_present: bool) -> Self {
        if diagnostics_console
            || plugin_dlls_present
            || std::env::var_os("EXPLORER_VISUAL_FIXTURE").is_some()
            || std::env::var_os("EXPLORER_AUTO_CLOSE_MS").is_some()
            || std::env::var_os("SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION").is_some()
        {
            Self::Isolated
        } else {
            Self::Ordinary
        }
    }
}

/// Holds this process's named-mutex reference for its full UI lifetime.
pub struct LaunchSession {
    handle: HANDLE,
    repeated: bool,
}

impl LaunchSession {
    /// Opens or creates the login-session-scoped launch marker.
    ///
    /// # Errors
    ///
    /// Returns the Windows error reported when the named mutex cannot be opened
    /// or created.
    pub fn acquire() -> windows::core::Result<Self> {
        let name = OsStr::new(SESSION_MUTEX_NAME)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // SAFETY: `name` is a live, NUL-terminated UTF-16 buffer for the duration
        // of the call. The default security descriptor and non-owning mutex mode
        // require no additional pointer lifetimes.
        #[expect(
            unsafe_code,
            reason = "creating the login-session launch marker requires Win32 CreateMutexW"
        )]
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr()))? };
        // SAFETY: GetLastError has no parameters and reads this thread's last-error value.
        #[expect(
            unsafe_code,
            reason = "detecting whether CreateMutexW opened an existing marker requires GetLastError"
        )]
        let repeated = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        Ok(Self { handle, repeated })
    }

    /// Returns whether an earlier ordinary `SuperExplorer` process is alive.
    pub const fn is_repeated(&self) -> bool {
        self.repeated
    }
}

impl Drop for LaunchSession {
    fn drop(&mut self) {
        // SAFETY: `handle` is the valid handle returned by CreateMutexW and is
        // closed exactly once by this guard.
        #[expect(
            unsafe_code,
            reason = "releasing the Win32 launch marker handle requires CloseHandle"
        )]
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_development_launch_is_isolated() {
        assert_eq!(LaunchKind::classify(false, true), LaunchKind::Isolated);
        assert_eq!(LaunchKind::classify(true, false), LaunchKind::Isolated);
    }

    #[test]
    fn ordinary_launch_has_no_plugin_override() {
        if std::env::var_os("EXPLORER_VISUAL_FIXTURE").is_none()
            && std::env::var_os("EXPLORER_AUTO_CLOSE_MS").is_none()
            && std::env::var_os("SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION").is_none()
        {
            assert_eq!(LaunchKind::classify(false, false), LaunchKind::Ordinary);
        }
    }

    #[test]
    fn second_mutex_guard_observes_existing_process_marker() {
        let first = LaunchSession::acquire().expect("first launch marker");
        let second = LaunchSession::acquire().expect("second launch marker");
        assert!(first.is_repeated() || second.is_repeated());
        assert!(second.is_repeated());
    }
}
