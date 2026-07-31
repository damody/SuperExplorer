//! Versioned automation event envelopes and routing types.

use std::{collections::BTreeMap, fmt, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::task::{AutomationTaskId, CorrelationId, HandlerId, ScriptId};

/// Current schema carried by newly emitted event envelopes.
pub const EVENT_SCHEMA_VERSION: u16 = 1;

/// Complete version-one event catalog. Source adapters must only emit names from this list.
pub const AUTOMATION_EVENT_NAMES: &[&str] = &[
    "app.started",
    "app.stopping",
    "window.opened",
    "window.closed",
    "window.activated",
    "window.deactivated",
    "window.moved",
    "window.resized",
    "window.minimized",
    "window.maximized",
    "theme.changed",
    "navigation.started",
    "navigation.completed",
    "navigation.failed",
    "directory.entered",
    "directory.refreshed",
    "tab.opened",
    "tab.closed",
    "tab.activated",
    "tab.reordered",
    "selection.changed",
    "item.opened",
    "search.started",
    "search.completed",
    "search.cancelled",
    "search.failed",
    "file_operation.started",
    "file_operation.progress",
    "file_operation.completed",
    "file_operation.cancelled",
    "file_operation.failed",
    "file.created",
    "file.renamed",
    "file.copied",
    "file.moved",
    "file.recycled",
    "file.deleted",
    "clipboard.copy",
    "clipboard.cut",
    "clipboard.paste",
    "fs.created",
    "fs.modified",
    "fs.removed",
    "fs.renamed",
    "fs.attributes_changed",
    "fs.security_changed",
    "watch.started",
    "watch.stopped",
    "watch.overflow",
    "watch.error",
    "input.key_down",
    "input.key_up",
    "input.mouse_down",
    "input.mouse_up",
    "input.mouse_move",
    "input.mouse_wheel",
    "input.mouse_hwheel",
    "hotkey.triggered",
    "system.foreground_changed",
    "system.window_created",
    "system.window_destroyed",
    "system.window_shown",
    "system.window_hidden",
    "system.window_location_changed",
    "system.window_title_changed",
    "clipboard.changed",
    "clipboard.text_available",
    "clipboard.files_available",
    "system.session_locked",
    "system.session_unlocked",
    "system.suspend",
    "system.resume",
    "system.display_changed",
    "system.dpi_changed",
    "system.device_arrived",
    "system.device_removed",
    "system.network_changed",
    "task.started",
    "task.completed",
    "task.cancelled",
    "task.failed",
    "process.started",
    "process.stdout",
    "process.stderr",
    "process.exited",
    "process.timed_out",
    "schedule.fired",
    "schedule.missed",
    "ai.started",
    "ai.streaming_delta",
    "ai.completed",
    "ai.cancelled",
    "ai.failed",
    "ai.output_written",
];

/// Validated dotted event name such as `fs.created`.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EventName(String);

impl EventName {
    /// Validates and owns a dotted lowercase event name.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty name, an empty segment, or characters other
    /// than lowercase ASCII letters, digits, and underscores.
    pub fn new(value: impl Into<String>) -> Result<Self, EventNameError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.split('.').all(|segment| {
                !segment.is_empty()
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(EventNameError(value))
        }
    }

    /// Borrows the normalized name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for EventName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("EventName").field(&self.0).finish()
    }
}

impl fmt::Display for EventName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for EventName {
    type Error = EventNameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<EventName> for String {
    fn from(value: EventName) -> Self {
        value.0
    }
}

/// Invalid automation event name.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("invalid automation event name")]
pub struct EventNameError(String);

/// Origin category used for routing, metrics, and privacy policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Application,
    Explorer,
    FileSystem,
    Input,
    Window,
    Clipboard,
    System,
    Task,
    Process,
    Schedule,
    Ai,
}

/// Applicable identifiers and directory snapshot attached to an event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventContext {
    pub script_id: Option<ScriptId>,
    pub handler_id: Option<HandlerId>,
    pub task_id: Option<AutomationTaskId>,
    pub correlation_id: CorrelationId,
    pub window_id: Option<u64>,
    pub tab_id: Option<explorer_model::TabId>,
    pub cwd: Option<PathBuf>,
}

/// Typed data families used by the version-one event catalog.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutomationEventData {
    None,
    Fields {
        values: BTreeMap<String, serde_json::Value>,
    },
    Path {
        path: PathBuf,
        previous_path: Option<PathBuf>,
        watch_root: Option<PathBuf>,
    },
    Key {
        virtual_key: u32,
        scan_code: u32,
        modifiers: u16,
        repeated: bool,
        injected: bool,
    },
    Mouse {
        x: i32,
        y: i32,
        button: Option<u8>,
        wheel_delta: Option<i32>,
        injected: bool,
    },
    Window {
        native_window_id: u64,
        process_id: Option<u32>,
        title_utf8_bytes: Option<usize>,
    },
    Process {
        process_id: Option<u32>,
        exit_code: Option<i32>,
        stream_bytes: Option<usize>,
        truncated: bool,
    },
    Ai {
        provider: String,
        model: String,
        input_bytes: usize,
        output_bytes: Option<usize>,
    },
}

/// Complete owned event passed between source adapters and script handlers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomationEvent {
    pub name: EventName,
    pub version: u16,
    pub sequence: u64,
    pub timestamp_unix_ms: u64,
    pub source: EventSource,
    pub context: EventContext,
    pub data: AutomationEventData,
}

impl AutomationEvent {
    /// Creates a version-one event envelope.
    #[must_use]
    pub const fn version_one(
        name: EventName,
        sequence: u64,
        timestamp_unix_ms: u64,
        source: EventSource,
        context: EventContext,
        data: AutomationEventData,
    ) -> Self {
        Self {
            name,
            version: EVENT_SCHEMA_VERSION,
            sequence,
            timestamp_unix_ms,
            source,
            context,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EventName, EventSource};

    #[test]
    fn dotted_event_names_are_validated() {
        let name = EventName::new("fs.security_changed").expect("valid event name");
        assert_eq!(name.as_str(), "fs.security_changed");
        assert!(EventName::new("FS.Created").is_err());
        assert!(EventName::new("fs..created").is_err());
    }

    #[test]
    fn event_source_serializes_as_stable_snake_case() {
        assert_eq!(
            serde_json::to_string(&EventSource::FileSystem).expect("serialize source"),
            "\"file_system\""
        );
    }
}
