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
//! Platform-neutral Lua automation coordination for Explorer.

pub mod adapters;
pub mod bookmark;
pub mod event;
pub mod event_bridge;
pub mod fakes;
pub mod file_host;
pub mod lifecycle;
pub mod process_host;
pub mod registration;
pub mod router;
pub mod runtime;
pub mod schedule;
pub mod summary;
pub mod task;
pub mod timing;

pub use adapters::{
    AutomationFuture, AutomationLogLevel, AutomationLogRecord, AutomationLogger, ClipboardHost,
    CredentialStore, EventSink, FileHost, FileWriteMode, HostEffect, ProcessHost, ProcessRequest,
    ProcessResult, TimerHost, UiHost,
};
pub use bookmark::{
    BOOKMARK_LUA_TIMEOUT_MS, LuaBookmarkRequest, LuaBookmarkResult, execute_lua_bookmark,
};
pub use event::{
    AUTOMATION_EVENT_NAMES, AutomationEvent, AutomationEventData, EVENT_SCHEMA_VERSION,
    EventContext, EventName, EventNameError, EventSource,
};
pub use event_bridge::{EventBridge, EventBridgeError};
pub use file_host::{ConfirmingFileHost, NativeFileHost, TaskFiles};
pub use lifecycle::{
    DiscoveredScript, ScriptLifecycle, ScriptLifecycleState, ScriptRegistry, discover_lua_scripts,
};
pub use process_host::{NativeProcessHost, ProcessPolicy, ScriptDeletionRisk};
pub use registration::{
    ActivationMode, HandlerDescriptor, HandlerKind, RegisteredScript, ScheduleDeclaration,
    ScriptConfig, WatchRegistration,
};
pub use router::{
    DispatchPolicy, DispatchSummary, EventFilter, HandlerRegistration, RoutedTask, Router,
    TaskTrigger,
};
pub use runtime::{LuaResourceLimits, LuaVm};
pub use schedule::{
    CatchUpDecision, CronSchedule, MissedRunPolicy, SchedulePlan, ScheduledInstant,
};
pub use summary::SummaryService;
pub use task::{
    AutomationError, AutomationErrorKind, AutomationResult, AutomationTaskId, CancellationToken,
    CorrelationId, HandlerId, ScriptId, SourceLocation, TaskContext,
};
pub use timing::{Debouncer, ManualTimer, Sleep, Throttler, Timeout, TimeoutElapsed};

/// Public Lua host API contract implemented by this crate.
pub const AUTOMATION_API_VERSION: &str = "explorer-automation/v1";

#[cfg(test)]
mod docs_contract;
