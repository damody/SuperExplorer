#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! Shared domain primitives that do not depend on GPUI or Win32.

pub mod diagnostics;
pub mod error;
pub mod process;
pub mod roadmap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use diagnostics::{
    DiagnosticsConfig, DiagnosticsError, DiagnosticsRegistry, DiagnosticsSession, ErrorSeverity,
    initialize_diagnostics, install_panic_hook, panic_payload_message, record_process_error,
    record_process_error_message,
};
pub use error::{ExplorerError, ExplorerErrorKind};
pub use process::configure_background_command;
pub use roadmap::{
    ROADMAP_LIMITS_SCHEMA_VERSION, RequestDeadline, RoadmapLimits, RoadmapLimitsError,
    TerminalClaim, TerminalDisposition, TerminalGate,
};

/// Correlates work across the application, service, and diagnostic layers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RequestId(Uuid);

impl RequestId {
    /// Allocates a new opaque request identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

/// Coarse application startup and shutdown phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecyclePhase {
    Created,
    Starting,
    Ready,
    Stopping,
    Stopped,
}

/// Build metadata used by diagnostics and visual evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppBuildInfo {
    pub package_version: &'static str,
    pub git_revision: &'static str,
    pub build_date: &'static str,
    pub author: &'static str,
}

impl AppBuildInfo {
    /// Returns metadata compiled into the current package.
    #[must_use]
    pub const fn current() -> Self {
        let git_revision = match option_env!("EXPLORER_GIT_REVISION") {
            Some(revision) => revision,
            None => "unknown",
        };
        Self {
            package_version: env!("CARGO_PKG_VERSION"),
            git_revision,
            build_date: env!("EXPLORER_BUILD_DATE"),
            author: env!("EXPLORER_BUILD_AUTHOR"),
        }
    }
}
