//! User-safe error contract shared by UI and services.

use thiserror::Error;

/// Stable high-level error categories used to choose UI and retry behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplorerErrorKind {
    Input,
    Availability,
    Authorization,
    Conflict,
    Cancellation,
    Extension,
    Internal,
}

/// Error details that preserve the operation and native code without exposing them by default.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{user_message}")]
pub struct ExplorerError {
    pub kind: ExplorerErrorKind,
    pub operation: String,
    pub location: Option<String>,
    pub native_code: Option<i32>,
    pub recoverable: bool,
    pub user_message: String,
    pub technical_detail: String,
}

impl ExplorerError {
    /// Creates an error with an actionable user message and separately copyable detail.
    pub fn new(
        kind: ExplorerErrorKind,
        operation: impl Into<String>,
        recoverable: bool,
        user_message: impl Into<String>,
        technical_detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation: operation.into(),
            location: None,
            native_code: None,
            recoverable,
            user_message: user_message.into(),
            technical_detail: technical_detail.into(),
        }
    }

    /// Attaches an owned, already-redacted location descriptor.
    #[must_use]
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Attaches a Win32 error or HRESULT represented without platform-specific types.
    #[must_use]
    pub const fn with_native_code(mut self, native_code: i32) -> Self {
        self.native_code = Some(native_code);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{ExplorerError, ExplorerErrorKind};

    #[test]
    fn display_uses_safe_user_message_not_technical_detail() {
        let error = ExplorerError::new(
            ExplorerErrorKind::Authorization,
            "enumerate",
            true,
            "無法存取資料夾，請檢查權限。",
            "HRESULT=0x80070005 path=redacted",
        )
        .with_native_code(i32::from_ne_bytes(0x8007_0005_u32.to_ne_bytes()));

        assert_eq!(error.to_string(), "無法存取資料夾，請檢查權限。");
        assert!(!error.to_string().contains("HRESULT"));
        assert!(error.recoverable);
    }
}
