//! Process-wide Windows prerequisites applied before GPUI starts.

use thiserror::Error;
use windows::{
    Win32::{
        Foundation::ERROR_ACCESS_DENIED,
        UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
    },
    core::HRESULT,
};

/// Outcome of requesting per-monitor-v2 DPI awareness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DpiAwarenessOutcome {
    Applied,
    AlreadyConfigured,
}

/// Failure to configure a required Windows process prerequisite.
#[derive(Debug, Error)]
pub enum WindowsPrerequisiteError {
    #[error("SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2) failed with {hresult:#010x}")]
    DpiAwareness { hresult: i32 },
}

/// Requests per-monitor-v2 DPI awareness before any GPUI window is created.
///
/// Windows returns access denied when process DPI awareness was already fixed by the embedded
/// manifest or a host. That result is accepted and recorded separately; all other errors fail
/// startup with the original HRESULT.
///
/// # Errors
///
/// Returns the original HRESULT for an unexpected Windows failure.
#[allow(
    unsafe_code,
    reason = "setting process DPI awareness requires a process-wide Win32 unsafe API"
)]
pub fn initialize_dpi_awareness() -> Result<DpiAwarenessOutcome, WindowsPrerequisiteError> {
    // SAFETY: The supplied awareness context is a Windows-defined constant with no borrowed
    // pointers. The call occurs during single-threaded process startup before GPUI creates HWNDs.
    // The Result checks the BOOL return and preserves its HRESULT; no cleanup handle is produced.
    let result =
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    classify_dpi_result(result)
}

fn classify_dpi_result(
    result: windows::core::Result<()>,
) -> Result<DpiAwarenessOutcome, WindowsPrerequisiteError> {
    match result {
        Ok(()) => Ok(DpiAwarenessOutcome::Applied),
        Err(error) if error.code() == HRESULT::from_win32(ERROR_ACCESS_DENIED.0) => {
            Ok(DpiAwarenessOutcome::AlreadyConfigured)
        }
        Err(error) => Err(WindowsPrerequisiteError::DpiAwareness {
            hresult: error.code().0,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{DpiAwarenessOutcome, WindowsPrerequisiteError, classify_dpi_result};
    use windows::{
        Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER},
        core::{Error, HRESULT},
    };

    #[test]
    fn startup_keeps_the_ui_at_invoker_integrity() {
        let main_source = include_str!("main.rs");

        assert!(!main_source.contains("ensure_administrator"));
        assert!(!main_source.contains("ShellExecuteExW"));
        assert!(!main_source.contains("runas"));
    }

    #[test]
    fn accepts_applied_and_already_configured_results() {
        assert_eq!(
            classify_dpi_result(Ok(())).expect("applied"),
            DpiAwarenessOutcome::Applied
        );
        let already_configured = Error::from_hresult(HRESULT::from_win32(ERROR_ACCESS_DENIED.0));
        assert_eq!(
            classify_dpi_result(Err(already_configured)).expect("manifest or host configured DPI"),
            DpiAwarenessOutcome::AlreadyConfigured
        );
    }

    #[test]
    fn preserves_unexpected_hresult() {
        let hresult = HRESULT::from_win32(ERROR_INVALID_PARAMETER.0);
        let error = classify_dpi_result(Err(Error::from_hresult(hresult)))
            .expect_err("invalid parameter must fail");
        assert!(matches!(
            error,
            WindowsPrerequisiteError::DpiAwareness { hresult: actual }
                if actual == hresult.0
        ));
    }
}
