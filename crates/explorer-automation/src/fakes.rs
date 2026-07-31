//! Deterministic host adapters used by contract and integration tests.

#![allow(clippy::missing_errors_doc)]

use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::{
    AutomationError, AutomationErrorKind, AutomationEvent, AutomationFuture, AutomationLogRecord,
    AutomationLogger, AutomationResult, ClipboardHost, CredentialStore, EventSink, FileHost,
    FileWriteMode, HostEffect, ProcessHost, ProcessRequest, ProcessResult, ScriptId, UiHost,
};

/// Mutable deterministic clock expressed as Unix milliseconds.
#[derive(Clone, Debug, Default)]
pub struct FakeClock {
    now_ms: Arc<Mutex<u64>>,
}

impl FakeClock {
    /// Creates a clock at an explicit instant.
    #[must_use]
    pub fn at(now_ms: u64) -> Self {
        Self {
            now_ms: Arc::new(Mutex::new(now_ms)),
        }
    }

    /// Returns the current fake instant.
    pub fn now_ms(&self) -> AutomationResult<u64> {
        self.now_ms
            .lock()
            .map(|value| *value)
            .map_err(|_| poisoned("fake_clock"))
    }

    /// Advances without sleeping.
    pub fn advance(&self, duration_ms: u64) -> AutomationResult<u64> {
        let mut value = self.now_ms.lock().map_err(|_| poisoned("fake_clock"))?;
        *value = value.saturating_add(duration_ms);
        Ok(*value)
    }
}

/// In-memory file host with deterministic operation recording.
#[derive(Clone, Debug, Default)]
pub struct FakeFileHost {
    state: Arc<Mutex<FakeFileState>>,
}

#[derive(Debug, Default)]
struct FakeFileState {
    files: BTreeMap<PathBuf, Vec<u8>>,
    removals: Vec<(ScriptId, PathBuf)>,
}

impl FakeFileHost {
    /// Returns one file snapshot.
    pub fn file(&self, path: &PathBuf) -> AutomationResult<Option<Vec<u8>>> {
        let state = self.state.lock().map_err(|_| poisoned("fake_file"))?;
        Ok(state.files.get(path).cloned())
    }

    /// Returns confirmed removal requests.
    pub fn removals(&self) -> AutomationResult<Vec<(ScriptId, PathBuf)>> {
        let state = self.state.lock().map_err(|_| poisoned("fake_file"))?;
        Ok(state.removals.clone())
    }
}

impl FileHost for FakeFileHost {
    fn read(&self, path: PathBuf) -> AutomationFuture<Vec<u8>> {
        let result = self.state.lock().map_or_else(
            |_| Err(poisoned("fake_file")),
            |state| {
                state.files.get(&path).cloned().ok_or_else(|| {
                    AutomationError::new(
                        AutomationErrorKind::FileSystem,
                        "file.read",
                        false,
                        "The requested fake file does not exist",
                    )
                })
            },
        );
        Box::pin(async move { result })
    }

    fn write(
        &self,
        path: PathBuf,
        bytes: Vec<u8>,
        mode: FileWriteMode,
    ) -> AutomationFuture<PathBuf> {
        let result = self.state.lock().map_or_else(
            |_| Err(poisoned("fake_file")),
            |mut state| match mode {
                FileWriteMode::CreateNew if state.files.contains_key(&path) => {
                    Err(AutomationError::new(
                        AutomationErrorKind::FileSystem,
                        "file.write",
                        false,
                        "The fake destination already exists",
                    ))
                }
                FileWriteMode::Append => {
                    state.files.entry(path.clone()).or_default().extend(bytes);
                    Ok(path)
                }
                FileWriteMode::CreateNew | FileWriteMode::AtomicReplace => {
                    state.files.insert(path.clone(), bytes);
                    Ok(path)
                }
            },
        );
        Box::pin(async move { result })
    }

    fn remove(&self, script_id: ScriptId, path: PathBuf) -> AutomationFuture<()> {
        let result = self.state.lock().map_or_else(
            |_| Err(poisoned("fake_file")),
            |mut state| {
                state.removals.push((script_id, path.clone()));
                state.files.remove(&path);
                Ok(())
            },
        );
        Box::pin(async move { result })
    }
}

/// Process host with queued results and recorded requests.
#[derive(Clone, Debug, Default)]
pub struct FakeProcessHost {
    state: Arc<Mutex<FakeProcessState>>,
}

#[derive(Debug, Default)]
struct FakeProcessState {
    requests: Vec<ProcessRequest>,
    results: VecDeque<AutomationResult<ProcessResult>>,
}

impl FakeProcessHost {
    pub fn push_result(&self, result: AutomationResult<ProcessResult>) -> AutomationResult<()> {
        let mut state = self.state.lock().map_err(|_| poisoned("fake_process"))?;
        state.results.push_back(result);
        Ok(())
    }

    pub fn requests(&self) -> AutomationResult<Vec<ProcessRequest>> {
        let state = self.state.lock().map_err(|_| poisoned("fake_process"))?;
        Ok(state.requests.clone())
    }
}

impl ProcessHost for FakeProcessHost {
    fn run(&self, request: ProcessRequest) -> AutomationFuture<ProcessResult> {
        let result = self.state.lock().map_or_else(
            |_| Err(poisoned("fake_process")),
            |mut state| {
                state.requests.push(request);
                state.results.pop_front().unwrap_or_else(|| {
                    Err(AutomationError::new(
                        AutomationErrorKind::Unavailable,
                        "process.run",
                        true,
                        "No deterministic process result was configured",
                    ))
                })
            },
        );
        Box::pin(async move { result })
    }

    fn run_script(&self, request: ProcessRequest) -> AutomationFuture<ProcessResult> {
        self.run(request)
    }
}

/// UI host that records effects and returns queued confirmation answers.
#[derive(Clone, Debug, Default)]
pub struct FakeUiHost {
    effects: Arc<Mutex<Vec<HostEffect>>>,
    answers: Arc<Mutex<VecDeque<bool>>>,
}

impl FakeUiHost {
    pub fn push_answer(&self, answer: bool) -> AutomationResult<()> {
        self.answers
            .lock()
            .map_err(|_| poisoned("fake_ui"))?
            .push_back(answer);
        Ok(())
    }

    pub fn effects(&self) -> AutomationResult<Vec<HostEffect>> {
        self.effects
            .lock()
            .map(|effects| effects.clone())
            .map_err(|_| poisoned("fake_ui"))
    }
}

impl UiHost for FakeUiHost {
    fn present(&self, effect: HostEffect) -> AutomationFuture<bool> {
        let effects = Arc::clone(&self.effects);
        let answers = Arc::clone(&self.answers);
        Box::pin(async move {
            effects
                .lock()
                .map_err(|_| poisoned("fake_ui"))?
                .push(effect);
            Ok(answers
                .lock()
                .map_err(|_| poisoned("fake_ui"))?
                .pop_front()
                .unwrap_or(true))
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct FakeClipboardHost {
    text: Arc<Mutex<Option<String>>>,
}

impl FakeClipboardHost {
    pub fn set_text(&self, text: Option<String>) -> AutomationResult<()> {
        *self.text.lock().map_err(|_| poisoned("fake_clipboard"))? = text;
        Ok(())
    }
}

impl ClipboardHost for FakeClipboardHost {
    fn read_text(&self) -> AutomationFuture<Option<String>> {
        let result = self
            .text
            .lock()
            .map(|text| text.clone())
            .map_err(|_| poisoned("fake_clipboard"));
        Box::pin(async move { result })
    }
}

#[derive(Clone, Debug, Default)]
pub struct FakeAutomationLogger {
    records: Arc<Mutex<Vec<AutomationLogRecord>>>,
}

impl FakeAutomationLogger {
    pub fn records(&self) -> AutomationResult<Vec<AutomationLogRecord>> {
        self.records
            .lock()
            .map(|records| records.clone())
            .map_err(|_| poisoned("fake_logger"))
    }
}

impl AutomationLogger for FakeAutomationLogger {
    fn log(&self, record: AutomationLogRecord) -> AutomationFuture<()> {
        let result = self
            .records
            .lock()
            .map(|mut records| records.push(record))
            .map_err(|_| poisoned("fake_logger"));
        Box::pin(async move { result })
    }
}

/// In-memory credential store whose debug output never contains secrets.
#[derive(Clone, Default)]
pub struct FakeCredentialStore {
    values: Arc<Mutex<BTreeMap<String, String>>>,
}

impl std::fmt::Debug for FakeCredentialStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FakeCredentialStore")
            .finish_non_exhaustive()
    }
}

impl CredentialStore for FakeCredentialStore {
    fn load(&self, key: String) -> AutomationFuture<Option<String>> {
        let result = self.values.lock().map_or_else(
            |_| Err(poisoned("fake_credential")),
            |values| Ok(values.get(&key).cloned()),
        );
        Box::pin(async move { result })
    }

    fn store(&self, key: String, secret: String) -> AutomationFuture<()> {
        let result = self.values.lock().map_or_else(
            |_| Err(poisoned("fake_credential")),
            |mut values| {
                values.insert(key, secret);
                Ok(())
            },
        );
        Box::pin(async move { result })
    }

    fn remove(&self, key: String) -> AutomationFuture<()> {
        let result = self.values.lock().map_or_else(
            |_| Err(poisoned("fake_credential")),
            |mut values| {
                values.remove(&key);
                Ok(())
            },
        );
        Box::pin(async move { result })
    }
}

/// Bounded event sink used to verify overload without blocking.
#[derive(Clone, Debug)]
pub struct FakeEventSink {
    capacity: usize,
    events: Arc<Mutex<VecDeque<AutomationEvent>>>,
}

impl FakeEventSink {
    /// Creates a sink with non-zero bounded capacity.
    pub fn new(capacity: usize) -> AutomationResult<Self> {
        if capacity == 0 {
            return Err(AutomationError::new(
                AutomationErrorKind::InvalidInput,
                "event_sink.create",
                false,
                "Event sink capacity must be non-zero",
            ));
        }
        Ok(Self {
            capacity,
            events: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    pub fn pop(&self) -> AutomationResult<Option<AutomationEvent>> {
        self.events
            .lock()
            .map(|mut events| events.pop_front())
            .map_err(|_| poisoned("fake_event_sink"))
    }
}

impl EventSink for FakeEventSink {
    fn try_publish(&self, event: AutomationEvent) -> Result<(), Box<AutomationEvent>> {
        let Ok(mut events) = self.events.try_lock() else {
            return Err(Box::new(event));
        };
        if events.len() == self.capacity {
            return Err(Box::new(event));
        }
        events.push_back(event);
        Ok(())
    }
}

fn poisoned(operation: &str) -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::Internal,
        operation,
        false,
        "A deterministic host adapter is unavailable",
    )
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        task::{Context, Poll, Waker},
    };

    use crate::{
        AutomationEvent, AutomationEventData, AutomationFuture, CorrelationId, CredentialStore,
        EVENT_SCHEMA_VERSION, EventContext, EventName, EventSink, EventSource, FileHost,
        FileWriteMode, ScriptId,
    };

    use super::{FakeClock, FakeCredentialStore, FakeEventSink, FakeFileHost};

    fn ready<T>(mut future: AutomationFuture<T>) -> crate::AutomationResult<T> {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("deterministic host future must be immediately ready"),
        }
    }

    fn event(sequence: u64) -> AutomationEvent {
        AutomationEvent {
            name: EventName::new("task.started").expect("valid event"),
            version: EVENT_SCHEMA_VERSION,
            sequence,
            timestamp_unix_ms: 10,
            source: EventSource::Task,
            context: EventContext {
                script_id: None,
                handler_id: None,
                task_id: None,
                correlation_id: CorrelationId::new(),
                window_id: None,
                tab_id: None,
                cwd: None,
            },
            data: AutomationEventData::None,
        }
    }

    #[test]
    fn fake_clock_advances_without_wall_time() {
        let clock = FakeClock::at(100);
        assert_eq!(clock.advance(25), Ok(125));
        assert_eq!(clock.now_ms(), Ok(125));
    }

    #[test]
    fn fake_file_host_applies_write_modes() {
        let host = FakeFileHost::default();
        let path = PathBuf::from(r"D:\A\summary.txt");
        ready(host.write(path.clone(), b"one".to_vec(), FileWriteMode::AtomicReplace))
            .expect("replace");
        ready(host.write(path.clone(), b"two".to_vec(), FileWriteMode::Append)).expect("append");
        assert_eq!(host.file(&path), Ok(Some(b"onetwo".to_vec())));

        ready(host.remove(ScriptId::new(), path.clone())).expect("remove");
        assert_eq!(host.file(&path), Ok(None));
        assert_eq!(host.removals().expect("removals").len(), 1);
    }

    #[test]
    fn fake_credential_debug_never_contains_secret() {
        let store = FakeCredentialStore::default();
        ready(store.store("deepseek".into(), "secret-value".into())).expect("store");
        assert_eq!(
            ready(store.load("deepseek".into())).expect("load"),
            Some("secret-value".into())
        );
        assert!(!format!("{store:?}").contains("secret-value"));
    }

    #[test]
    fn fake_event_sink_returns_overload_without_blocking() {
        let sink = FakeEventSink::new(1).expect("sink");
        assert_eq!(sink.try_publish(event(1)), Ok(()));
        let rejected = sink.try_publish(event(2)).expect_err("bounded overload");
        assert_eq!(rejected.sequence, 2);
        assert_eq!(sink.pop().expect("pop").expect("event").sequence, 1);
    }
}
