//! Bounded subscription matching and handler dispatch queues.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use crate::{
    AutomationError, AutomationErrorKind, AutomationEvent, AutomationResult, AutomationTaskId,
    CancellationToken, EventName, HandlerId, ScriptId, TaskContext,
};

/// Handler concurrency and pending-trigger behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchPolicy {
    Queue,
    Parallel,
    Latest,
    Drop,
}

/// Exact, namespace-prefix, or universal event subscription.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventFilter {
    Exact(EventName),
    Prefix(String),
    All,
}

impl EventFilter {
    /// Parses `fs.created`, `fs.*`, or `*`.
    ///
    /// # Errors
    ///
    /// Returns an input error for invalid event-name syntax or unsupported wildcard placement.
    pub fn parse(value: &str) -> AutomationResult<Self> {
        if value == "*" {
            return Ok(Self::All);
        }
        if let Some(prefix) = value.strip_suffix(".*") {
            let validated = EventName::new(prefix).map_err(|_| invalid_filter())?;
            return Ok(Self::Prefix(format!("{}.", validated.as_str())));
        }
        if value.contains('*') {
            return Err(invalid_filter());
        }
        EventName::new(value)
            .map(Self::Exact)
            .map_err(|_| invalid_filter())
    }

    fn matches(&self, name: &EventName) -> bool {
        match self {
            Self::Exact(expected) => expected == name,
            Self::Prefix(prefix) => name.as_str().starts_with(prefix),
            Self::All => true,
        }
    }
}

/// One registered Lua event handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerRegistration {
    pub script_id: ScriptId,
    pub handler_id: HandlerId,
    pub filter: EventFilter,
    pub policy: DispatchPolicy,
    pub queue_capacity: usize,
    pub max_parallel: usize,
}

impl HandlerRegistration {
    /// Creates the default FIFO registration.
    ///
    /// # Errors
    ///
    /// Returns an input error when capacity is zero.
    pub fn queued(
        script_id: ScriptId,
        handler_id: HandlerId,
        filter: EventFilter,
        queue_capacity: usize,
    ) -> AutomationResult<Self> {
        if queue_capacity == 0 {
            return Err(invalid_capacity());
        }
        Ok(Self {
            script_id,
            handler_id,
            filter,
            policy: DispatchPolicy::Queue,
            queue_capacity,
            max_parallel: 1,
        })
    }

    /// Creates a validated registration for an explicit dispatch policy.
    ///
    /// # Errors
    ///
    /// Returns an input error when capacity or parallelism is zero.
    pub fn with_policy(
        script_id: ScriptId,
        handler_id: HandlerId,
        filter: EventFilter,
        policy: DispatchPolicy,
        queue_capacity: usize,
        max_parallel: usize,
    ) -> AutomationResult<Self> {
        if queue_capacity == 0 || max_parallel == 0 {
            return Err(invalid_capacity());
        }
        Ok(Self {
            script_id,
            handler_id,
            filter,
            policy,
            queue_capacity,
            max_parallel,
        })
    }
}

/// Source snapshot captured before routing and queue delay.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskTrigger {
    pub event: AutomationEvent,
    pub cwd: PathBuf,
    pub created_unix_ms: u64,
    pub deadline_unix_ms: Option<u64>,
}

/// Task selected for execution by a handler worker.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutedTask {
    pub context: TaskContext,
}

/// Result of one fan-out attempt.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DispatchSummary {
    pub matched: usize,
    pub enqueued: usize,
    pub overloaded: Vec<HandlerId>,
    pub coalesced: Vec<HandlerId>,
    pub dropped: Vec<HandlerId>,
}

#[derive(Debug)]
struct HandlerState {
    registration: HandlerRegistration,
    pending: VecDeque<RoutedTask>,
    running: usize,
}

/// Deterministic bounded router. It never runs Lua itself.
#[derive(Debug, Default)]
pub struct Router {
    order: Vec<HandlerId>,
    handlers: HashMap<HandlerId, HandlerState>,
}

impl Router {
    /// Adds one unique handler registration.
    ///
    /// # Errors
    ///
    /// Returns a conflict error for a duplicate handler or an input error for invalid limits.
    pub fn register(&mut self, registration: HandlerRegistration) -> AutomationResult<()> {
        if registration.queue_capacity == 0 || registration.max_parallel == 0 {
            return Err(invalid_capacity());
        }
        if self.handlers.contains_key(&registration.handler_id) {
            return Err(AutomationError::new(
                AutomationErrorKind::InvalidInput,
                "router.register",
                false,
                "The automation handler is already registered",
            ));
        }
        self.order.push(registration.handler_id);
        self.handlers.insert(
            registration.handler_id,
            HandlerState {
                registration,
                pending: VecDeque::new(),
                running: 0,
            },
        );
        Ok(())
    }

    /// Removes a handler and returns all pending tasks for cancellation reporting.
    #[must_use]
    pub fn unregister(&mut self, handler_id: HandlerId) -> Vec<RoutedTask> {
        self.order.retain(|candidate| *candidate != handler_id);
        self.handlers
            .remove(&handler_id)
            .map_or_else(Vec::new, |state| state.pending.into())
    }

    /// Fans one source snapshot out to matching bounded queues without running handlers.
    #[must_use]
    pub fn dispatch(&mut self, trigger: &TaskTrigger) -> DispatchSummary {
        let mut summary = DispatchSummary::default();
        for handler_id in self.order.clone() {
            let Some(state) = self.handlers.get_mut(&handler_id) else {
                continue;
            };
            if !state.registration.filter.matches(&trigger.event.name) {
                continue;
            }
            summary.matched += 1;
            let mut event = trigger.event.clone();
            let task_id = AutomationTaskId::new();
            event.context.script_id = Some(state.registration.script_id);
            event.context.handler_id = Some(handler_id);
            event.context.task_id = Some(task_id);
            event.context.cwd = Some(trigger.cwd.clone());
            let task = RoutedTask {
                context: TaskContext {
                    id: task_id,
                    parent_id: None,
                    script_id: state.registration.script_id,
                    handler_id,
                    correlation_id: event.context.correlation_id,
                    cwd: trigger.cwd.clone(),
                    created_unix_ms: trigger.created_unix_ms,
                    deadline_unix_ms: trigger.deadline_unix_ms,
                    cancellation: CancellationToken::default(),
                    event,
                },
            };

            match state.registration.policy {
                DispatchPolicy::Queue | DispatchPolicy::Parallel => {
                    if state.pending.len() == state.registration.queue_capacity {
                        summary.overloaded.push(handler_id);
                    } else {
                        state.pending.push_back(task);
                        summary.enqueued += 1;
                    }
                }
                DispatchPolicy::Latest => {
                    if state.pending.pop_back().is_some() {
                        summary.coalesced.push(handler_id);
                    }
                    state.pending.push_back(task);
                    summary.enqueued += 1;
                }
                DispatchPolicy::Drop => {
                    if state.running > 0 || !state.pending.is_empty() {
                        summary.dropped.push(handler_id);
                    } else {
                        state.pending.push_back(task);
                        summary.enqueued += 1;
                    }
                }
            }
        }
        summary
    }

    /// Takes the next runnable FIFO task and records it as running.
    #[must_use]
    pub fn take_ready(&mut self, handler_id: HandlerId) -> Option<RoutedTask> {
        let state = self.handlers.get_mut(&handler_id)?;
        if state.running >= state.registration.max_parallel {
            return None;
        }
        let task = state.pending.pop_front()?;
        state.running += 1;
        Some(task)
    }

    /// Records one terminal handler result.
    ///
    /// # Errors
    ///
    /// Returns an internal error if no task is currently running for the handler.
    pub fn complete(&mut self, handler_id: HandlerId) -> AutomationResult<()> {
        let state = self.handlers.get_mut(&handler_id).ok_or_else(|| {
            AutomationError::new(
                AutomationErrorKind::InvalidInput,
                "router.complete",
                false,
                "The automation handler is not registered",
            )
        })?;
        if state.running == 0 {
            return Err(AutomationError::new(
                AutomationErrorKind::Internal,
                "router.complete",
                false,
                "The automation handler has no running task",
            ));
        }
        state.running -= 1;
        Ok(())
    }

    /// Returns pending and running counts for diagnostics.
    #[must_use]
    pub fn counts(&self, handler_id: HandlerId) -> Option<(usize, usize)> {
        self.handlers
            .get(&handler_id)
            .map(|state| (state.pending.len(), state.running))
    }
}

fn invalid_filter() -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::InvalidInput,
        "event_filter.parse",
        false,
        "The automation event filter is invalid",
    )
}

fn invalid_capacity() -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::InvalidInput,
        "router.register",
        false,
        "Automation handler limits must be non-zero",
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{
        AutomationEvent, AutomationEventData, CorrelationId, EventContext, EventName, EventSource,
        HandlerId, ScriptId,
    };

    use super::{DispatchPolicy, EventFilter, HandlerRegistration, Router, TaskTrigger};

    fn trigger(sequence: u64, cwd: &str) -> TaskTrigger {
        TaskTrigger {
            event: AutomationEvent::version_one(
                EventName::new("fs.created").expect("event"),
                sequence,
                sequence,
                EventSource::FileSystem,
                EventContext {
                    script_id: None,
                    handler_id: None,
                    task_id: None,
                    correlation_id: CorrelationId::new(),
                    window_id: None,
                    tab_id: None,
                    cwd: None,
                },
                AutomationEventData::None,
            ),
            cwd: PathBuf::from(cwd),
            created_unix_ms: sequence,
            deadline_unix_ms: None,
        }
    }

    #[test]
    fn fifo_queue_matches_prefix_and_preserves_captured_context() {
        let handler = HandlerId::new();
        let mut router = Router::default();
        router
            .register(
                HandlerRegistration::queued(
                    ScriptId::new(),
                    handler,
                    EventFilter::parse("fs.*").expect("filter"),
                    2,
                )
                .expect("registration"),
            )
            .expect("register");

        assert_eq!(router.dispatch(&trigger(1, r"D:\A")).enqueued, 1);
        assert_eq!(router.dispatch(&trigger(2, r"D:\B")).enqueued, 1);
        let first = router.take_ready(handler).expect("first task");
        assert_eq!(first.context.event.sequence, 1);
        assert_eq!(first.context.cwd, PathBuf::from(r"D:\A"));
        assert_eq!(router.take_ready(handler), None);
        router.complete(handler).expect("complete");
        let second = router.take_ready(handler).expect("second task");
        assert_eq!(second.context.event.sequence, 2);
        assert_eq!(second.context.cwd, PathBuf::from(r"D:\B"));
    }

    #[test]
    fn full_queue_returns_explicit_overload() {
        let handler = HandlerId::new();
        let mut router = Router::default();
        router
            .register(
                HandlerRegistration::queued(
                    ScriptId::new(),
                    handler,
                    EventFilter::parse("fs.created").expect("filter"),
                    1,
                )
                .expect("registration"),
            )
            .expect("register");
        assert_eq!(router.dispatch(&trigger(1, r"D:\A")).enqueued, 1);
        let summary = router.dispatch(&trigger(2, r"D:\B"));
        assert_eq!(summary.matched, 1);
        assert_eq!(summary.enqueued, 0);
        assert_eq!(summary.overloaded, vec![handler]);
    }

    #[test]
    fn parallel_policy_releases_tasks_up_to_concurrency_limit() {
        let handler = HandlerId::new();
        let mut router = Router::default();
        router
            .register(
                HandlerRegistration::with_policy(
                    ScriptId::new(),
                    handler,
                    EventFilter::All,
                    DispatchPolicy::Parallel,
                    4,
                    2,
                )
                .expect("registration"),
            )
            .expect("register");
        let _ = router.dispatch(&trigger(1, r"D:\A"));
        let _ = router.dispatch(&trigger(2, r"D:\A"));
        let _ = router.dispatch(&trigger(3, r"D:\A"));
        assert!(router.take_ready(handler).is_some());
        assert!(router.take_ready(handler).is_some());
        assert!(router.take_ready(handler).is_none());
        assert_eq!(router.counts(handler), Some((1, 2)));
    }

    #[test]
    fn latest_policy_keeps_only_newest_pending_trigger() {
        let handler = HandlerId::new();
        let mut router = Router::default();
        router
            .register(
                HandlerRegistration::with_policy(
                    ScriptId::new(),
                    handler,
                    EventFilter::All,
                    DispatchPolicy::Latest,
                    1,
                    1,
                )
                .expect("registration"),
            )
            .expect("register");
        let _ = router.dispatch(&trigger(1, r"D:\A"));
        let summary = router.dispatch(&trigger(2, r"D:\B"));
        assert_eq!(summary.coalesced, vec![handler]);
        let task = router.take_ready(handler).expect("latest task");
        assert_eq!(task.context.event.sequence, 2);
        assert_eq!(task.context.cwd, PathBuf::from(r"D:\B"));
    }

    #[test]
    fn drop_policy_reports_busy_trigger() {
        let handler = HandlerId::new();
        let mut router = Router::default();
        router
            .register(
                HandlerRegistration::with_policy(
                    ScriptId::new(),
                    handler,
                    EventFilter::All,
                    DispatchPolicy::Drop,
                    1,
                    1,
                )
                .expect("registration"),
            )
            .expect("register");
        let _ = router.dispatch(&trigger(1, r"D:\A"));
        assert!(router.take_ready(handler).is_some());
        let summary = router.dispatch(&trigger(2, r"D:\B"));
        assert_eq!(summary.enqueued, 0);
        assert_eq!(summary.dropped, vec![handler]);
    }
}
