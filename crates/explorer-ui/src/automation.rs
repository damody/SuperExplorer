//! GPUI script manager and non-blocking summary presentation state.

#![allow(clippy::missing_errors_doc)]

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use explorer_automation::{
    AutomationError, AutomationErrorKind, DispatchPolicy, ScriptId, ScriptLifecycle,
    ScriptLifecycleState, ScriptRegistry, WatchRegistration,
};
use gpui::{Context, IntoElement, Render, SharedString, Window, div, prelude::*};

const DEFAULT_HISTORY_LIMIT: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SummaryPresentationMode {
    Docked,
    Popup,
}

/// UI-owned overrides; the Lua source file is never rewritten.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScriptOverrides {
    pub watches: Option<Vec<WatchRegistration>>,
    pub dispatch: Option<DispatchPolicy>,
    pub queue_capacity: Option<usize>,
    pub max_parallel: Option<usize>,
    pub task_timeout_ms: Option<u64>,
    pub summary_mode: Option<SummaryPresentationMode>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskPresentationState {
    Running,
    Completed,
    Failed,
    TimedOut,
    Overloaded,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskHistoryRecord {
    pub script_id: ScriptId,
    pub correlation_id: String,
    pub operation: String,
    pub state: TaskPresentationState,
    pub duration_ms: Option<u64>,
    pub safe_summary: Option<String>,
    pub error_kind: Option<AutomationErrorKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SummaryPanelState {
    Idle,
    Loading {
        correlation_id: String,
    },
    Ready {
        text: String,
    },
    Failed {
        safe_message: String,
        retryable: bool,
    },
    Cancelled,
}

/// Shared state behind the dockable manager and popup summary view.
pub struct AutomationManagerState {
    registry: Arc<Mutex<ScriptRegistry>>,
    scripts: Vec<ScriptLifecycle>,
    overrides: HashMap<ScriptId, ScriptOverrides>,
    history: VecDeque<TaskHistoryRecord>,
    history_limit: usize,
    summary: SummaryPanelState,
    summary_mode: SummaryPresentationMode,
    pending_copy: Option<String>,
    retry_requested: bool,
}

impl AutomationManagerState {
    #[must_use]
    pub fn new(registry: Arc<Mutex<ScriptRegistry>>) -> Self {
        Self {
            registry,
            scripts: Vec::new(),
            overrides: HashMap::new(),
            history: VecDeque::new(),
            history_limit: DEFAULT_HISTORY_LIMIT,
            summary: SummaryPanelState::Idle,
            summary_mode: SummaryPresentationMode::Docked,
            pending_copy: None,
            retry_requested: false,
        }
    }

    pub fn refresh(&mut self) -> Result<(), AutomationError> {
        self.scripts = self.registry.lock().map_err(|_| manager_error())?.list();
        Ok(())
    }

    pub fn enable(&mut self, path: &Path) -> Result<(), AutomationError> {
        self.registry
            .lock()
            .map_err(|_| manager_error())?
            .enable(path)?;
        self.refresh()
    }

    pub fn disable(&mut self, path: &Path) -> Result<(), AutomationError> {
        self.registry
            .lock()
            .map_err(|_| manager_error())?
            .disable(path);
        self.refresh()
    }

    pub fn reload(&mut self, path: &Path) -> Result<(), AutomationError> {
        let result = self
            .registry
            .lock()
            .map_err(|_| manager_error())?
            .reload(path);
        let _ = self.refresh();
        result
    }

    /// Returns the exact source path for the composition root's external-editor launcher.
    #[must_use]
    pub fn external_editor_target(&self, script_id: ScriptId) -> Option<PathBuf> {
        self.scripts
            .iter()
            .find(|script| script.script.id == script_id)
            .map(|script| script.script.path.clone())
    }

    pub fn set_overrides(&mut self, script_id: ScriptId, overrides: ScriptOverrides) {
        self.overrides.insert(script_id, overrides);
    }

    #[must_use]
    pub fn overrides(&self, script_id: ScriptId) -> Option<&ScriptOverrides> {
        self.overrides.get(&script_id)
    }

    pub fn record_task(&mut self, record: TaskHistoryRecord) {
        self.history.push_front(record);
        self.history.truncate(self.history_limit);
    }

    #[must_use]
    pub fn history(&self) -> &VecDeque<TaskHistoryRecord> {
        &self.history
    }

    #[must_use]
    pub fn scripts(&self) -> &[ScriptLifecycle] {
        &self.scripts
    }

    #[must_use]
    pub fn trust_warning(script: &ScriptLifecycle) -> Option<&'static str> {
        (script.state == ScriptLifecycleState::Enabled).then_some(
            "Enabled automation can observe configured events and perform host actions; review its source before trusting it.",
        )
    }

    pub fn begin_summary(&mut self, correlation_id: String, mode: SummaryPresentationMode) {
        self.summary_mode = mode;
        self.summary = SummaryPanelState::Loading { correlation_id };
        self.retry_requested = false;
    }

    pub fn complete_summary(&mut self, text: String) {
        self.summary = SummaryPanelState::Ready { text };
    }

    pub fn fail_summary(&mut self, safe_message: String, retryable: bool) {
        self.summary = SummaryPanelState::Failed {
            safe_message,
            retryable,
        };
    }

    pub fn cancel_summary(&mut self) {
        self.summary = SummaryPanelState::Cancelled;
    }

    pub fn request_copy(&mut self) {
        if let SummaryPanelState::Ready { text } = &self.summary {
            self.pending_copy = Some(text.clone());
        }
    }

    pub fn take_copy_text(&mut self) -> Option<String> {
        self.pending_copy.take()
    }

    pub fn request_retry(&mut self) {
        if matches!(
            self.summary,
            SummaryPanelState::Failed {
                retryable: true,
                ..
            }
        ) {
            self.retry_requested = true;
        }
    }

    pub fn take_retry_request(&mut self) -> bool {
        std::mem::take(&mut self.retry_requested)
    }

    #[must_use]
    pub const fn summary(&self) -> &SummaryPanelState {
        &self.summary
    }
}

impl std::fmt::Debug for AutomationManagerState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AutomationManagerState")
            .field("scripts", &self.scripts)
            .field("history_count", &self.history.len())
            .field("summary", &self.summary)
            .finish_non_exhaustive()
    }
}

/// Lightweight GPUI surface suitable for a dock slot or popup window.
pub struct AutomationManagerView {
    pub state: AutomationManagerState,
}

impl Render for AutomationManagerView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let status: SharedString = match self.state.summary() {
            SummaryPanelState::Idle => "No summary".into(),
            SummaryPanelState::Loading { .. } => "Summarizing…".into(),
            SummaryPanelState::Ready { text } => text.clone().into(),
            SummaryPanelState::Failed { safe_message, .. } => safe_message.clone().into(),
            SummaryPanelState::Cancelled => "Summary cancelled".into(),
        };
        div()
            .id("automation-manager")
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(format!("Lua scripts: {}", self.state.scripts().len()))
            .child(status)
    }
}

fn manager_error() -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::Internal,
        "script_manager",
        true,
        "The script manager is temporarily unavailable",
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use explorer_automation::{ScriptId, ScriptRegistry};

    use super::{
        AutomationManagerState, SummaryPanelState, SummaryPresentationMode, TaskHistoryRecord,
        TaskPresentationState,
    };

    #[test]
    fn history_is_bounded_and_summary_actions_are_non_blocking_state_changes() {
        let mut manager =
            AutomationManagerState::new(Arc::new(Mutex::new(ScriptRegistry::default())));
        manager.history_limit = 2;
        for index in 0..3 {
            manager.record_task(TaskHistoryRecord {
                script_id: ScriptId::new(),
                correlation_id: index.to_string(),
                operation: "test".into(),
                state: TaskPresentationState::Completed,
                duration_ms: Some(1),
                safe_summary: None,
                error_kind: None,
            });
        }
        assert_eq!(manager.history().len(), 2);
        manager.begin_summary("id".into(), SummaryPresentationMode::Docked);
        assert!(matches!(
            manager.summary(),
            SummaryPanelState::Loading { .. }
        ));
        manager.complete_summary("summary".into());
        manager.request_copy();
        assert_eq!(manager.take_copy_text().as_deref(), Some("summary"));
    }

    #[test]
    fn retry_is_available_only_for_retryable_failures() {
        let mut manager =
            AutomationManagerState::new(Arc::new(Mutex::new(ScriptRegistry::default())));
        manager.fail_summary("safe".into(), false);
        manager.request_retry();
        assert!(!manager.take_retry_request());
        manager.fail_summary("safe".into(), true);
        manager.request_retry();
        assert!(manager.take_retry_request());
    }
}
