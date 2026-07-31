//! Non-blocking bridge used by application and platform event adapters.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use crate::{
    AUTOMATION_EVENT_NAMES, AutomationEvent, AutomationEventData, EVENT_SCHEMA_VERSION,
    EventContext, EventName, EventSink, EventSource,
};

/// Immediate source-side rejection without blocking a native callback.
#[derive(Debug)]
pub enum EventBridgeError {
    InvalidName,
    UnknownName,
    Overloaded(Box<AutomationEvent>),
}

/// Stamps a global sequence and forwards owned events without running Lua in callbacks.
pub struct EventBridge {
    sink: Arc<dyn EventSink>,
    sequence: AtomicU64,
}

impl EventBridge {
    #[must_use]
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Self {
            sink,
            sequence: AtomicU64::new(0),
        }
    }

    /// Publishes a catalog event. Returns the complete envelope when downstream is overloaded.
    ///
    /// # Errors
    ///
    /// Returns a name error or the envelope rejected by a full/disconnected sink.
    pub fn emit(
        &self,
        name: &str,
        timestamp_unix_ms: u64,
        source: EventSource,
        context: EventContext,
        data: AutomationEventData,
    ) -> Result<(), EventBridgeError> {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let name = EventName::new(name).map_err(|_| EventBridgeError::InvalidName)?;
        if !AUTOMATION_EVENT_NAMES.contains(&name.as_str()) {
            return Err(EventBridgeError::UnknownName);
        }
        let event = AutomationEvent {
            name,
            version: EVENT_SCHEMA_VERSION,
            sequence,
            timestamp_unix_ms,
            source,
            context,
            data,
        };
        self.sink
            .try_publish(event)
            .map_err(EventBridgeError::Overloaded)
    }

    #[must_use]
    pub fn emitted_count(&self) -> u64 {
        self.sequence.load(Ordering::Relaxed)
    }
}

impl std::fmt::Debug for EventBridge {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventBridge")
            .field("emitted_count", &self.emitted_count())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Instant};

    use crate::{
        AutomationEventData, CorrelationId, EventContext, EventSource,
        event_bridge::{EventBridge, EventBridgeError},
        fakes::FakeEventSink,
    };

    fn context() -> EventContext {
        EventContext {
            script_id: None,
            handler_id: None,
            task_id: None,
            correlation_id: CorrelationId::new(),
            window_id: None,
            tab_id: None,
            cwd: None,
        }
    }

    #[test]
    fn bridge_sequences_catalog_events_and_reports_overload() {
        let sink = Arc::new(FakeEventSink::new(1).expect("sink"));
        let bridge = EventBridge::new(sink.clone());
        bridge
            .emit(
                "app.started",
                10,
                EventSource::Application,
                context(),
                AutomationEventData::None,
            )
            .expect("first");
        let rejected = bridge
            .emit(
                "window.opened",
                11,
                EventSource::Application,
                context(),
                AutomationEventData::None,
            )
            .expect_err("overload");
        let EventBridgeError::Overloaded(rejected) = rejected else {
            panic!("expected overload");
        };
        assert_eq!(rejected.sequence, 2);
        assert_eq!(sink.pop().expect("pop").expect("event").sequence, 1);
    }

    #[test]
    fn one_hundred_thousand_source_callbacks_stay_bounded_and_fast() {
        let sink = Arc::new(FakeEventSink::new(100_000).expect("sink"));
        let bridge = EventBridge::new(sink);
        let mut samples = Vec::with_capacity(100_000);
        for _ in 0..100_000 {
            let started = Instant::now();
            bridge
                .emit(
                    "input.mouse_move",
                    10,
                    EventSource::Input,
                    context(),
                    AutomationEventData::Mouse {
                        x: 1,
                        y: 2,
                        button: None,
                        wheel_delta: None,
                        injected: false,
                    },
                )
                .expect("bounded publish");
            samples.push(started.elapsed());
        }
        samples.sort_unstable();
        let p99 = samples[98_999];
        assert!(p99 < std::time::Duration::from_millis(1), "p99={p99:?}");
        assert_eq!(bridge.emitted_count(), 100_000);
    }
}
