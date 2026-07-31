//! Immutable task contexts, dispatch, cancellation, and scheduling types.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::AutomationEvent;

macro_rules! opaque_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        pub struct $name(Uuid);

        impl $name {
            /// Allocates a new opaque identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an already-derived UUID, including stable `UUIDv5` identities.
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            /// Returns the underlying UUID for serialization and diagnostics correlation.
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

opaque_id!(ScriptId);
opaque_id!(HandlerId);
opaque_id!(AutomationTaskId);
opaque_id!(CorrelationId);

/// Cloneable cancellation state shared by a task and its children.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Cancels this token and every clone in the same task scope.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether the task scope was cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl PartialEq for CancellationToken {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cancelled, &other.cancelled)
    }
}

/// Immutable execution snapshot created before handler dispatch.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskContext {
    pub id: AutomationTaskId,
    pub parent_id: Option<AutomationTaskId>,
    pub script_id: ScriptId,
    pub handler_id: HandlerId,
    pub correlation_id: CorrelationId,
    pub cwd: PathBuf,
    pub created_unix_ms: u64,
    pub deadline_unix_ms: Option<u64>,
    pub cancellation: CancellationToken,
    pub event: AutomationEvent,
}

impl TaskContext {
    /// Resolves a task-relative path without consulting mutable UI state.
    #[must_use]
    pub fn resolve_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        }
    }

    /// Creates a child task that inherits the immutable cwd and correlation scope.
    #[must_use]
    pub fn child(
        &self,
        id: AutomationTaskId,
        handler_id: HandlerId,
        created_unix_ms: u64,
        deadline_unix_ms: Option<u64>,
    ) -> Self {
        Self {
            id,
            parent_id: Some(self.id),
            script_id: self.script_id,
            handler_id,
            correlation_id: self.correlation_id,
            cwd: self.cwd.clone(),
            created_unix_ms,
            deadline_unix_ms,
            cancellation: self.cancellation.clone(),
            event: self.event.clone(),
        }
    }

    /// Returns whether the task deadline has elapsed at the supplied deterministic instant.
    #[must_use]
    pub fn is_expired(&self, now_unix_ms: u64) -> bool {
        self.deadline_unix_ms
            .is_some_and(|deadline| now_unix_ms >= deadline)
    }

    /// Validates cancellation and deadline state before starting or resuming host work.
    ///
    /// # Errors
    ///
    /// Returns a cancellation or timeout error with the task correlation identifier.
    pub fn ensure_active(&self, now_unix_ms: u64) -> AutomationResult<()> {
        if self.cancellation.is_cancelled() {
            return Err(AutomationError::new(
                AutomationErrorKind::Cancelled,
                "task.resume",
                false,
                "The automation task was cancelled",
            )
            .with_correlation(self.correlation_id));
        }
        if self.is_expired(now_unix_ms) {
            return Err(AutomationError::new(
                AutomationErrorKind::Timeout,
                "task.resume",
                false,
                "The automation task timed out",
            )
            .with_correlation(self.correlation_id));
        }
        Ok(())
    }
}

/// Stable categories used by Lua, UI, retry policy, and tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationErrorKind {
    InvalidInput,
    Unavailable,
    Authorization,
    Overloaded,
    Cancelled,
    Timeout,
    DeletionDenied,
    Script,
    FileSystem,
    Process,
    Ai,
    Internal,
}

/// Optional Lua source location that contains no source text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub script_id: ScriptId,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// User-safe automation error with separately controlled diagnostic metadata.
#[derive(Clone, Debug, Eq, Error, PartialEq, Serialize, Deserialize)]
#[error("{user_message}")]
pub struct AutomationError {
    pub kind: AutomationErrorKind,
    pub operation: Box<str>,
    pub recoverable: bool,
    pub user_message: Box<str>,
    pub safe_detail: Option<Box<str>>,
    pub correlation_id: Option<CorrelationId>,
    pub source_location: Option<SourceLocation>,
}

impl AutomationError {
    /// Creates a structured error whose display text is safe for normal UI and logs.
    pub fn new(
        kind: AutomationErrorKind,
        operation: impl Into<String>,
        recoverable: bool,
        user_message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation: operation.into().into_boxed_str(),
            recoverable,
            user_message: user_message.into().into_boxed_str(),
            safe_detail: None,
            correlation_id: None,
            source_location: None,
        }
    }

    /// Attaches already-redacted diagnostic detail.
    #[must_use]
    pub fn with_safe_detail(mut self, detail: impl Into<String>) -> Self {
        self.safe_detail = Some(detail.into().into_boxed_str());
        self
    }

    /// Attaches a correlation identifier.
    #[must_use]
    pub const fn with_correlation(mut self, correlation_id: CorrelationId) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }
}

/// Automation result used by host adapters and runtime coordination.
pub type AutomationResult<T> = Result<T, AutomationError>;

#[cfg(test)]
pub(crate) mod tests_support {
    use std::path::PathBuf;

    use crate::{
        AutomationEvent, AutomationEventData, EVENT_SCHEMA_VERSION, EventContext, EventName,
        EventSource,
    };

    use super::{
        AutomationError, AutomationErrorKind, AutomationTaskId, CorrelationId, HandlerId, ScriptId,
        TaskContext,
    };

    pub(crate) fn task_for_runtime_test(handler_id: HandlerId, cwd: &str) -> TaskContext {
        let correlation_id = CorrelationId::new();
        TaskContext {
            id: AutomationTaskId::new(),
            parent_id: None,
            script_id: ScriptId::new(),
            handler_id,
            correlation_id,
            cwd: PathBuf::from(cwd),
            created_unix_ms: 10,
            deadline_unix_ms: Some(100),
            cancellation: super::CancellationToken::default(),
            event: AutomationEvent {
                name: EventName::new("directory.entered").expect("valid event"),
                version: EVENT_SCHEMA_VERSION,
                sequence: 1,
                timestamp_unix_ms: 10,
                source: EventSource::Explorer,
                context: EventContext {
                    script_id: None,
                    handler_id: None,
                    task_id: None,
                    correlation_id,
                    window_id: None,
                    tab_id: None,
                    cwd: Some(PathBuf::from(cwd)),
                },
                data: AutomationEventData::None,
            },
        }
    }

    #[test]
    fn relative_paths_and_children_keep_captured_cwd() {
        let parent = task_for_runtime_test(HandlerId::new(), r"D:\A");
        assert_eq!(
            parent.resolve_path("summary.txt"),
            PathBuf::from(r"D:\A\summary.txt")
        );
        let child = parent.child(AutomationTaskId::new(), HandlerId::new(), 20, None);
        assert_eq!(child.cwd, PathBuf::from(r"D:\A"));
        assert_eq!(child.parent_id, Some(parent.id));
    }

    #[test]
    fn display_exposes_only_user_safe_error_text() {
        let error = AutomationError::new(
            AutomationErrorKind::Authorization,
            "ai.request",
            false,
            "DeepSeek credential is unavailable",
        )
        .with_safe_detail("provider=deepseek status=401");
        assert_eq!(error.to_string(), "DeepSeek credential is unavailable");
        assert!(!error.to_string().contains("401"));
    }

    #[test]
    fn child_shares_cancellation_and_deadline_checks_are_deterministic() {
        let parent = task_for_runtime_test(HandlerId::new(), r"D:\A");
        let child = parent.child(AutomationTaskId::new(), HandlerId::new(), 20, Some(50));
        assert_eq!(child.ensure_active(49), Ok(()));
        assert_eq!(
            child.ensure_active(50).expect_err("deadline").kind,
            AutomationErrorKind::Timeout
        );
        parent.cancellation.cancel();
        assert!(child.cancellation.is_cancelled());
        assert_eq!(
            child.ensure_active(25).expect_err("cancelled").kind,
            AutomationErrorKind::Cancelled
        );
    }
}
