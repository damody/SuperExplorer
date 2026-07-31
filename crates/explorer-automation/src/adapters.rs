//! Host-effect contracts implemented by production and deterministic adapters.

use std::{collections::BTreeMap, future::Future, path::PathBuf, pin::Pin};

use serde::{Deserialize, Serialize};

use crate::{AutomationEvent, AutomationResult, CorrelationId, ScriptId};

/// Boxed host operation that can be awaited by any executor.
pub type AutomationFuture<T> = Pin<Box<dyn Future<Output = AutomationResult<T>> + Send + 'static>>;

/// File destination conflict behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileWriteMode {
    CreateNew,
    AtomicReplace,
    Append,
}

/// Platform-neutral asynchronous file effects.
pub trait FileHost: Send + Sync {
    fn read(&self, path: PathBuf) -> AutomationFuture<Vec<u8>>;
    fn write(
        &self,
        path: PathBuf,
        bytes: Vec<u8>,
        mode: FileWriteMode,
    ) -> AutomationFuture<PathBuf>;
    fn remove(&self, script_id: ScriptId, path: PathBuf) -> AutomationFuture<()>;
}

/// Direct executable request with no command-string interpretation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProcessRequest {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub cwd: PathBuf,
    pub timeout_ms: u64,
    pub correlation_id: CorrelationId,
}

/// Bounded completed process result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProcessResult {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

/// Platform-neutral process effect.
pub trait ProcessHost: Send + Sync {
    fn run(&self, request: ProcessRequest) -> AutomationFuture<ProcessResult>;

    /// Runs a request already transformed by the trusted fixed-interpreter policy.
    fn run_script(&self, _request: ProcessRequest) -> AutomationFuture<ProcessResult> {
        Box::pin(async {
            Err(crate::AutomationError::new(
                crate::AutomationErrorKind::Unavailable,
                "process.run_script",
                false,
                "Script process execution is unavailable",
            ))
        })
    }
}

/// UI effects represented without GPUI values.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostEffect {
    Notify {
        title: String,
        body: Option<String>,
    },
    ShowSummary {
        text: String,
        popup: bool,
    },
    ConfirmDeletion {
        script_id: ScriptId,
        paths: Vec<PathBuf>,
    },
}

/// Platform-neutral UI presentation boundary.
pub trait UiHost: Send + Sync {
    fn present(&self, effect: HostEffect) -> AutomationFuture<bool>;
}

/// Clipboard reads are explicit host effects and never persisted by the runtime.
pub trait ClipboardHost: Send + Sync {
    fn read_text(&self) -> AutomationFuture<Option<String>>;
}

/// Privacy-safe structured log severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Bounded structured diagnostics. Values must not contain content or credentials.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AutomationLogRecord {
    pub level: AutomationLogLevel,
    pub operation: String,
    pub correlation_id: Option<CorrelationId>,
    pub safe_fields: BTreeMap<String, String>,
}

pub trait AutomationLogger: Send + Sync {
    fn log(&self, record: AutomationLogRecord) -> AutomationFuture<()>;
}

/// Secret storage that never exposes credentials through debug formatting.
pub trait CredentialStore: Send + Sync {
    fn load(&self, key: String) -> AutomationFuture<Option<String>>;
    fn store(&self, key: String, secret: String) -> AutomationFuture<()>;
    fn remove(&self, key: String) -> AutomationFuture<()>;
}

/// Non-blocking event destination used by source adapters.
pub trait EventSink: Send + Sync {
    /// Attempts to publish without waiting for downstream work.
    ///
    /// # Errors
    ///
    /// Returns the event when the sink is full or disconnected.
    fn try_publish(&self, event: AutomationEvent) -> Result<(), Box<AutomationEvent>>;
}

/// Executor-independent timer boundary used by Lua coroutine host functions.
pub trait TimerHost: Send + Sync {
    fn now_ms(&self) -> u64;
    fn sleep(&self, duration_ms: u64) -> AutomationFuture<()>;
}
