//! Dedicated Windows Shell STA lifecycle and message pump.

use std::{
    collections::{HashMap, VecDeque},
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use explorer_common::{
    ErrorSeverity, ExplorerError, ExplorerErrorKind, RequestId, panic_payload_message,
    record_process_error, record_process_error_message,
};
use explorer_model::{
    BreadcrumbIconHint, BreadcrumbSegment, BreadcrumbSegmentId, BreadcrumbTerminal,
    CancellationToken, ClipboardMode, DataTransferRequest, ExplorerCommand, ExplorerEvent,
    LocationDescriptor, OpenDisposition, OperationTerminal,
};
use thiserror::Error;
use windows::Win32::{
    System::{
        Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
        Ole::{OleInitialize, OleUninitialize},
    },
    UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_QUIT,
    },
};

const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_JOIN_TIMEOUT: Duration = Duration::from_secs(5);
const MESSAGE_PUMP_INTERVAL: Duration = Duration::from_millis(8);
const BREADCRUMB_PROVIDER_TIMEOUT: Duration = Duration::from_secs(5);
// One initial Explorer viewport can legitimately request 256 Shell icons in addition to
// navigation/ancestry work. Keep that burst bounded without starving correlation-critical
// navigation and breadcrumb commands behind an artificially tiny queue.
const COMMAND_QUEUE_CAPACITY: usize = 512;
const FOREGROUND_COMMAND_QUEUE_CAPACITY: usize = 64;
const EVENT_QUEUE_CAPACITY: usize = 4_096;
const NAVIGATION_EVENT_QUEUE_CAPACITY: usize = 4_096;
const SEARCH_EVENT_QUEUE_CAPACITY: usize = 4_096;
const OPERATION_PROGRESS_QUEUE_CAPACITY: usize = 256;
const OPERATION_TERMINAL_QUEUE_CAPACITY: usize = 256;
// A foreground command can leave the control queue before its terminal is consumed. Retaining
// one terminal per foreground slot keeps terminal delivery bounded without making the STA wait.
const NAVIGATION_TERMINAL_RETAIN_CAPACITY: usize = FOREGROUND_COMMAND_QUEUE_CAPACITY;
const OPERATION_TERMINAL_RETAIN_CAPACITY: usize = FOREGROUND_COMMAND_QUEUE_CAPACITY + 4;
const TYPED_TERMINAL_RETAIN_CAPACITY: usize =
    COMMAND_QUEUE_CAPACITY + FOREGROUND_COMMAND_QUEUE_CAPACITY;
const FILE_OPERATION_WORKER_CAPACITY: usize = 4;
const THUMBNAIL_WORKER_CAPACITY: usize = 2;
const SEARCH_WORKER_CAPACITY: usize = 8;
const BREADCRUMB_WORKER_CAPACITY: usize = 4;

static ACTIVE_STA_THREADS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_CONTROL_CHANNELS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_JOIN_HANDLES: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_BREADCRUMB_WORKERS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_FILE_OPERATION_WORKERS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_THUMBNAIL_WORKERS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_SEARCH_WORKERS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_SEARCH_EXECUTORS: AtomicUsize = AtomicUsize::new(0);
static SATURATED_FILE_OPERATION_WORKERS: AtomicUsize = AtomicUsize::new(0);
static SATURATED_THUMBNAIL_WORKERS: AtomicUsize = AtomicUsize::new(0);
static SATURATED_SEARCH_WORKERS: AtomicUsize = AtomicUsize::new(0);
static SATURATED_BREADCRUMB_WORKERS: AtomicUsize = AtomicUsize::new(0);
static STALE_CANCELLED_COMPLETIONS: AtomicUsize = AtomicUsize::new(0);

struct TerminalLaneCounters {
    current_depth: AtomicUsize,
    high_water_depth: AtomicUsize,
    failed_publications: AtomicUsize,
    retained_publications: AtomicUsize,
}

impl TerminalLaneCounters {
    const fn new() -> Self {
        Self {
            current_depth: AtomicUsize::new(0),
            high_water_depth: AtomicUsize::new(0),
            failed_publications: AtomicUsize::new(0),
            retained_publications: AtomicUsize::new(0),
        }
    }

    fn published(&self) {
        let depth = self.current_depth.fetch_add(1, Ordering::AcqRel) + 1;
        let mut high_water = self.high_water_depth.load(Ordering::Acquire);
        while depth > high_water {
            match self.high_water_depth.compare_exchange_weak(
                high_water,
                depth,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => high_water = observed,
            }
        }
    }

    fn received(&self) {
        let _ = self
            .current_depth
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |depth| {
                depth.checked_sub(1)
            });
    }
}

static NAVIGATION_TERMINAL_COUNTERS: TerminalLaneCounters = TerminalLaneCounters::new();
static OPERATION_TERMINAL_COUNTERS: TerminalLaneCounters = TerminalLaneCounters::new();
static TYPED_TERMINAL_COUNTERS: TerminalLaneCounters = TerminalLaneCounters::new();

/// A bounded terminal lane that never waits for a UI receiver. A full primary channel retains
/// the exactly-once terminal in a separately bounded queue sized for all queued foreground work.
#[derive(Clone)]
pub(crate) struct ReliableTerminalPublisher {
    primary: SyncSender<ExplorerEvent>,
    retained: Arc<Mutex<VecDeque<ExplorerEvent>>>,
    retained_capacity: usize,
    counters: &'static TerminalLaneCounters,
    ordered: bool,
}

struct ReliableTerminalReceiver {
    primary: Receiver<ExplorerEvent>,
    retained: Arc<Mutex<VecDeque<ExplorerEvent>>>,
    counters: &'static TerminalLaneCounters,
    retained_first: bool,
}

impl ReliableTerminalPublisher {
    fn channel(
        primary_capacity: usize,
        retained_capacity: usize,
        counters: &'static TerminalLaneCounters,
    ) -> (Self, ReliableTerminalReceiver) {
        Self::channel_with_order(primary_capacity, retained_capacity, counters, true)
    }

    /// Creates a lane whose primary FIFO is drained before retained terminals. This is used for
    /// request streams such as breadcrumb/search where batches published before a terminal must
    /// be observed first. The retained queue still guarantees a bounded terminal when the primary
    /// FIFO is full.
    fn ordered_channel(
        primary_capacity: usize,
        retained_capacity: usize,
        counters: &'static TerminalLaneCounters,
    ) -> (Self, ReliableTerminalReceiver) {
        Self::channel_with_order(primary_capacity, retained_capacity, counters, false)
    }

    fn channel_with_order(
        primary_capacity: usize,
        retained_capacity: usize,
        counters: &'static TerminalLaneCounters,
        retained_first: bool,
    ) -> (Self, ReliableTerminalReceiver) {
        let (primary, receiver) = mpsc::sync_channel(primary_capacity);
        let retained = Arc::new(Mutex::new(VecDeque::with_capacity(retained_capacity)));
        (
            Self {
                primary,
                retained: Arc::clone(&retained),
                retained_capacity,
                counters,
                ordered: !retained_first,
            },
            ReliableTerminalReceiver {
                primary: receiver,
                retained,
                counters,
                retained_first,
            },
        )
    }

    fn primary(&self) -> SyncSender<ExplorerEvent> {
        self.primary.clone()
    }

    /// Publishes a required terminal without blocking the calling apartment or worker.
    pub(crate) fn publish(&self, event: ExplorerEvent) {
        if self.ordered {
            let mut retained = self
                .retained
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !retained.is_empty() {
                self.retain_terminal(&mut retained, event);
                return;
            }
        }
        match self.primary.try_send(event) {
            Ok(()) => self.counters.published(),
            Err(TrySendError::Full(event)) => {
                let mut retained = self
                    .retained
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                self.retain_terminal(&mut retained, event);
            }
            Err(TrySendError::Disconnected(_)) => {
                self.counters
                    .failed_publications
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn retain_terminal(&self, retained: &mut VecDeque<ExplorerEvent>, event: ExplorerEvent) {
        if let Some(request_id) = event.context().map(|context| context.request_id)
            && let Some(existing) = retained.iter_mut().find(|existing| {
                existing.is_terminal()
                    && existing.context().map(|context| context.request_id) == Some(request_id)
            })
        {
            // A duplicate terminal for one request is safely superseded; it cannot create a
            // second reducer transition and preserves bounded memory.
            *existing = event;
            return;
        }
        if retained.len() < self.retained_capacity {
            retained.push_back(event);
            self.counters
                .retained_publications
                .fetch_add(1, Ordering::Relaxed);
            self.counters.published();
        } else {
            self.counters
                .failed_publications
                .fetch_add(1, Ordering::Relaxed);
            tracing::error!("required Shell terminal retention capacity was exhausted");
        }
    }

    /// Publishes an earlier batch into the same ordered request stream as its terminal. Once the
    /// primary lane spills, later events remain in the retained FIFO so the terminal cannot pass
    /// the batch that made the visible navigation tree.
    fn try_publish_batch(&self, event: ExplorerEvent) -> Result<(), ()> {
        debug_assert!(!event.is_terminal());
        if self.ordered {
            let mut retained = self
                .retained
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !retained.is_empty() {
                if retained.len() < self.retained_capacity {
                    retained.push_back(event);
                    return Ok(());
                }
                return Err(());
            }
        }
        match self.primary.try_send(event) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(event)) if self.ordered => {
                let mut retained = self
                    .retained
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if retained.len() < self.retained_capacity {
                    retained.push_back(event);
                    Ok(())
                } else {
                    Err(())
                }
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => Err(()),
        }
    }
}

pub(crate) trait RequiredTerminalPublisher: Clone + Send + 'static {
    fn publish_terminal(&self, event: ExplorerEvent);

    fn publish_batch(&self, event: ExplorerEvent) -> Result<(), ()>;

    fn send(&self, event: ExplorerEvent) -> Result<(), ()> {
        self.publish_terminal(event);
        Ok(())
    }
}

impl RequiredTerminalPublisher for ReliableTerminalPublisher {
    fn publish_terminal(&self, event: ExplorerEvent) {
        self.publish(event);
    }

    fn publish_batch(&self, event: ExplorerEvent) -> Result<(), ()> {
        self.try_publish_batch(event)
    }
}

impl RequiredTerminalPublisher for SyncSender<ExplorerEvent> {
    fn publish_terminal(&self, event: ExplorerEvent) {
        let _ = self.try_send(event);
    }

    fn publish_batch(&self, event: ExplorerEvent) -> Result<(), ()> {
        self.try_send(event).map_err(|_| ())
    }
}

impl ReliableTerminalReceiver {
    fn try_recv(&self) -> Result<ExplorerEvent, TryRecvError> {
        if self.retained_first {
            let mut retained = self
                .retained
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(event) = retained.pop_front() {
                self.counters.received();
                return Ok(event);
            }
        }
        match self.primary.try_recv() {
            Ok(event) => {
                if event.is_terminal() {
                    self.counters.received();
                }
                Ok(event)
            }
            Err(error @ (TryRecvError::Empty | TryRecvError::Disconnected)) => {
                let mut retained = self
                    .retained
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(event) = retained.pop_front() {
                    self.counters.received();
                    Ok(event)
                } else {
                    Err(error)
                }
            }
        }
    }
}

fn spawn_sta_thread<F>(job: F) -> Result<JoinHandle<()>, std::io::Error>
where
    F: FnOnce() + Send + 'static,
{
    #[cfg(test)]
    if FAIL_NEXT_STA_THREAD_SPAWN.swap(false, Ordering::AcqRel) {
        return Err(std::io::Error::other("injected Shell STA spawn failure"));
    }
    thread::Builder::new()
        .name("explorer-shell-sta".to_owned())
        .spawn(job)
}

#[cfg(test)]
#[derive(Clone)]
struct FileOperationTestGate {
    request_id: RequestId,
    started: SyncSender<()>,
    release: Arc<AtomicBool>,
}

#[cfg(test)]
static FILE_OPERATION_TEST_GATE: Mutex<Option<FileOperationTestGate>> = Mutex::new(None);

#[cfg(test)]
#[derive(Clone)]
struct BreadcrumbTestGate {
    request_id: RequestId,
    started: SyncSender<()>,
    release: Arc<AtomicBool>,
}

#[cfg(test)]
static BREADCRUMB_TEST_GATE: Mutex<Option<BreadcrumbTestGate>> = Mutex::new(None);

#[cfg(test)]
#[derive(Clone)]
struct SearchTestGate {
    request_id: RequestId,
    started: SyncSender<()>,
    release: Arc<AtomicBool>,
}

#[cfg(test)]
static SEARCH_TEST_GATE: Mutex<Option<SearchTestGate>> = Mutex::new(None);

#[cfg(test)]
static FAIL_NEXT_STA_THREAD_SPAWN: AtomicBool = AtomicBool::new(false);

/// Observable lifecycle of the dedicated Shell apartment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ShellStaState {
    Created = 0,
    Starting = 1,
    Ready = 2,
    Stopping = 3,
    Stopped = 4,
}

impl ShellStaState {
    fn from_atomic(value: u8) -> Self {
        match value {
            0 => Self::Created,
            1 => Self::Starting,
            2 => Self::Ready,
            3 => Self::Stopping,
            _ => Self::Stopped,
        }
    }

    /// Validates and returns the next lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns an error when the transition violates the Shell STA lifecycle.
    pub fn transition(self, next: Self) -> Result<Self, ShellStaError> {
        let valid = matches!(
            (self, next),
            (Self::Created, Self::Starting)
                | (Self::Starting, Self::Ready | Self::Stopped)
                | (Self::Ready, Self::Stopping)
                | (Self::Stopping | Self::Stopped, Self::Stopped)
        );
        if valid {
            Ok(next)
        } else {
            Err(ShellStaError::InvalidTransition {
                from: self,
                to: next,
            })
        }
    }
}

/// Shell STA startup and shutdown failures.
#[derive(Debug, Error)]
pub enum ShellStaError {
    #[error("invalid Shell STA transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: ShellStaState,
        to: ShellStaState,
    },
    #[error("failed to spawn Shell STA thread: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("CoInitializeEx(COINIT_APARTMENTTHREADED) failed with HRESULT {hresult:#010x}")]
    ComInitialization { hresult: i32 },
    #[error("Shell STA startup hook failed: {message}")]
    StartupHook { message: String },
    #[error("Shell STA did not become ready within {timeout:?}")]
    StartupTimeout { timeout: Duration },
    #[error("Shell STA stopped before reporting readiness")]
    StartupChannelClosed,
    #[error("Shell STA did not stop within {timeout:?}")]
    JoinTimeout { timeout: Duration },
    #[error("Shell STA thread panicked")]
    ThreadPanicked,
    #[error("Shell STA synchronization mutex was poisoned")]
    Poisoned,
}

/// Debug-only resource counts owned by the Shell STA implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaResourceSnapshot {
    /// Dedicated STA threads that have started and not yet exited.
    pub active_threads: usize,
    /// Control channel endpoints owned by an STA lifecycle.
    pub active_control_channels: usize,
    /// Rust join handles that still own a native Windows thread handle.
    pub active_join_handles: usize,
    /// Isolated breadcrumb provider workers that have not returned yet.
    pub active_breadcrumb_workers: usize,
    /// Background file-operation apartments that have not returned yet.
    pub active_file_operation_workers: usize,
}

/// Bounded, path-free saturation and completion observations for isolated Shell domains.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellDomainDiagnostics {
    /// Active background file-operation apartments and their fixed capacity.
    pub active_file_operations: usize,
    /// Active thumbnail apartments and their fixed capacity.
    pub active_thumbnails: usize,
    /// Active search workers and their fixed capacity.
    pub active_searches: usize,
    /// Active enrichment-provider workers and their fixed capacity.
    pub active_breadcrumbs: usize,
    /// Rejections caused by the file-operation domain being full.
    pub saturated_file_operations: usize,
    /// Rejections caused by the thumbnail domain being full.
    pub saturated_thumbnails: usize,
    /// Rejections caused by the search domain being full.
    pub saturated_searches: usize,
    /// Rejections caused by the enrichment-provider domain being full.
    pub saturated_breadcrumbs: usize,
    /// Completed workers whose request was cancelled before delivery.
    pub stale_cancelled_completions: usize,
    /// Navigation terminal events currently queued across primary and retained delivery lanes.
    pub navigation_terminal_current_depth: usize,
    /// Largest observed navigation terminal lane depth.
    pub navigation_terminal_high_water_depth: usize,
    /// Navigation terminal publications that could not reach an owned receiver.
    pub failed_navigation_terminal_publications: usize,
    /// Navigation terminals retained after their primary lane was full.
    pub retained_navigation_terminal_publications: usize,
    /// File-operation terminal events currently queued across primary and retained delivery lanes.
    pub operation_terminal_current_depth: usize,
    /// Largest observed file-operation terminal lane depth.
    pub operation_terminal_high_water_depth: usize,
    /// File-operation terminal publications that could not reach an owned receiver.
    pub failed_operation_terminal_publications: usize,
    /// File-operation terminals retained after their primary lane was full.
    pub retained_operation_terminal_publications: usize,
    /// Typed command-recovery terminals queued across primary and retained delivery lanes.
    pub typed_terminal_current_depth: usize,
    /// Largest observed typed command-recovery terminal lane depth.
    pub typed_terminal_high_water_depth: usize,
    /// Typed command-recovery terminal publications that could not reach an owned receiver.
    pub failed_typed_terminal_publications: usize,
    /// Typed command-recovery terminals retained after their primary lane was full.
    pub retained_typed_terminal_publications: usize,
}

impl ShellDomainDiagnostics {
    /// Captures only bounded counters; it deliberately contains no filesystem identity.
    pub fn capture() -> Self {
        Self {
            active_file_operations: ACTIVE_FILE_OPERATION_WORKERS.load(Ordering::Acquire),
            active_thumbnails: ACTIVE_THUMBNAIL_WORKERS.load(Ordering::Acquire),
            active_searches: ACTIVE_SEARCH_WORKERS.load(Ordering::Acquire),
            active_breadcrumbs: ACTIVE_BREADCRUMB_WORKERS.load(Ordering::Acquire),
            saturated_file_operations: SATURATED_FILE_OPERATION_WORKERS.load(Ordering::Acquire),
            saturated_thumbnails: SATURATED_THUMBNAIL_WORKERS.load(Ordering::Acquire),
            saturated_searches: SATURATED_SEARCH_WORKERS.load(Ordering::Acquire),
            saturated_breadcrumbs: SATURATED_BREADCRUMB_WORKERS.load(Ordering::Acquire),
            stale_cancelled_completions: STALE_CANCELLED_COMPLETIONS.load(Ordering::Acquire),
            navigation_terminal_current_depth: NAVIGATION_TERMINAL_COUNTERS
                .current_depth
                .load(Ordering::Acquire),
            navigation_terminal_high_water_depth: NAVIGATION_TERMINAL_COUNTERS
                .high_water_depth
                .load(Ordering::Acquire),
            failed_navigation_terminal_publications: NAVIGATION_TERMINAL_COUNTERS
                .failed_publications
                .load(Ordering::Acquire),
            retained_navigation_terminal_publications: NAVIGATION_TERMINAL_COUNTERS
                .retained_publications
                .load(Ordering::Acquire),
            operation_terminal_current_depth: OPERATION_TERMINAL_COUNTERS
                .current_depth
                .load(Ordering::Acquire),
            operation_terminal_high_water_depth: OPERATION_TERMINAL_COUNTERS
                .high_water_depth
                .load(Ordering::Acquire),
            failed_operation_terminal_publications: OPERATION_TERMINAL_COUNTERS
                .failed_publications
                .load(Ordering::Acquire),
            retained_operation_terminal_publications: OPERATION_TERMINAL_COUNTERS
                .retained_publications
                .load(Ordering::Acquire),
            typed_terminal_current_depth: TYPED_TERMINAL_COUNTERS
                .current_depth
                .load(Ordering::Acquire),
            typed_terminal_high_water_depth: TYPED_TERMINAL_COUNTERS
                .high_water_depth
                .load(Ordering::Acquire),
            failed_typed_terminal_publications: TYPED_TERMINAL_COUNTERS
                .failed_publications
                .load(Ordering::Acquire),
            retained_typed_terminal_publications: TYPED_TERMINAL_COUNTERS
                .retained_publications
                .load(Ordering::Acquire),
        }
    }
}

impl StaResourceSnapshot {
    /// Captures implementation-owned resources without enumerating unrelated process handles.
    pub fn capture() -> Self {
        Self {
            active_threads: ACTIVE_STA_THREADS.load(Ordering::Acquire),
            active_control_channels: ACTIVE_CONTROL_CHANNELS.load(Ordering::Acquire),
            active_join_handles: ACTIVE_JOIN_HANDLES.load(Ordering::Acquire),
            active_breadcrumb_workers: ACTIVE_BREADCRUMB_WORKERS.load(Ordering::Acquire),
            active_file_operation_workers: ACTIVE_FILE_OPERATION_WORKERS.load(Ordering::Acquire),
        }
    }
}

enum ControlMessage {
    Command {
        command: ExplorerCommand,
        queued_at: Instant,
    },
    Shutdown,
}

struct FileOperationCompletion {
    context: explorer_model::RequestContext,
    outcome: OperationTerminal,
    clipboard_paste: bool,
}

struct SearchJob {
    context: explorer_model::RequestContext,
    location: LocationDescriptor,
    input: explorer_model::SearchInput,
    worker_guard: IsolatedWorkerGuard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequiredTerminalLane {
    Navigation,
    Operation,
    Typed,
}

struct ActiveRequest {
    cancellation: CancellationToken,
    required_terminal_lane: Option<RequiredTerminalLane>,
}

/// Non-blocking command/event endpoint failures.
#[derive(Debug, Error)]
pub enum ShellStaEndpointError {
    #[error("Shell command queue is at capacity")]
    CommandQueueFull,
    #[error("Shell STA command endpoint is disconnected")]
    CommandEndpointDisconnected,
    #[error("Shell STA event endpoint is disconnected")]
    EventEndpointDisconnected,
    #[error("Shell STA synchronization mutex was poisoned")]
    Poisoned,
}

/// Owner of the dedicated Shell STA thread.
pub struct ShellStaHandle {
    correlation_id: RequestId,
    foreground_control: SyncSender<ControlMessage>,
    background_control: SyncSender<ControlMessage>,
    navigation_events: Mutex<ReliableTerminalReceiver>,
    operation_terminals: Mutex<ReliableTerminalReceiver>,
    typed_terminals: Mutex<ReliableTerminalReceiver>,
    operation_progress: Mutex<Receiver<ExplorerEvent>>,
    search_events: Mutex<ReliableTerminalReceiver>,
    events: Mutex<ReliableTerminalReceiver>,
    active_requests: Mutex<HashMap<RequestId, ActiveRequest>>,
    done: Mutex<Receiver<()>>,
    join: Mutex<Option<JoinHandle<()>>>,
    search_done: Mutex<Receiver<()>>,
    search_join: Mutex<Option<JoinHandle<()>>>,
    state: Arc<AtomicU8>,
    pump_cycles: Arc<AtomicUsize>,
    shutdown_requested: Arc<AtomicBool>,
}

impl std::fmt::Debug for ShellStaHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShellStaHandle")
            .field("correlation_id", &self.correlation_id)
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl ShellStaHandle {
    /// Starts the dedicated Shell STA and waits for its readiness handshake.
    ///
    /// # Errors
    ///
    /// Returns an error when the thread cannot be created, COM initialization fails, or startup
    /// does not complete before the bounded timeout.
    pub fn start() -> Result<Self, ShellStaError> {
        Self::start_with_hook(|| Ok(()), DEFAULT_STARTUP_TIMEOUT)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "STA startup keeps one linear acquisition and reverse-unwind sequence auditable"
    )]
    fn start_with_hook<F>(startup_hook: F, timeout: Duration) -> Result<Self, ShellStaError>
    where
        F: FnOnce() -> Result<(), ShellStaError> + Send + 'static,
    {
        let correlation_id = RequestId::new();
        let state = Arc::new(AtomicU8::new(ShellStaState::Created as u8));
        let thread_state = Arc::clone(&state);
        let pump_cycles = Arc::new(AtomicUsize::new(0));
        let thread_pump_cycles = Arc::clone(&pump_cycles);
        let (foreground_tx, foreground_rx) = mpsc::sync_channel(FOREGROUND_COMMAND_QUEUE_CAPACITY);
        let (background_tx, background_rx) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let (background_terminals, event_rx) = ReliableTerminalPublisher::ordered_channel(
            EVENT_QUEUE_CAPACITY,
            TYPED_TERMINAL_RETAIN_CAPACITY,
            &TYPED_TERMINAL_COUNTERS,
        );
        let event_tx = background_terminals.primary();
        let (navigation_terminals, navigation_event_rx) = ReliableTerminalPublisher::channel(
            NAVIGATION_EVENT_QUEUE_CAPACITY,
            NAVIGATION_TERMINAL_RETAIN_CAPACITY,
            &NAVIGATION_TERMINAL_COUNTERS,
        );
        let navigation_event_tx = navigation_terminals.primary();
        let (search_terminals, search_event_rx) = ReliableTerminalPublisher::ordered_channel(
            SEARCH_EVENT_QUEUE_CAPACITY,
            TYPED_TERMINAL_RETAIN_CAPACITY,
            &TYPED_TERMINAL_COUNTERS,
        );
        let search_event_tx = search_terminals.primary();
        let (operation_terminals, operation_terminal_rx) = ReliableTerminalPublisher::channel(
            OPERATION_TERMINAL_QUEUE_CAPACITY,
            OPERATION_TERMINAL_RETAIN_CAPACITY,
            &OPERATION_TERMINAL_COUNTERS,
        );
        let (typed_terminals, typed_terminal_rx) = ReliableTerminalPublisher::channel(
            EVENT_QUEUE_CAPACITY,
            TYPED_TERMINAL_RETAIN_CAPACITY,
            &TYPED_TERMINAL_COUNTERS,
        );
        let (operation_progress_tx, operation_progress_rx) =
            mpsc::sync_channel(OPERATION_PROGRESS_QUEUE_CAPACITY);
        let (file_operation_tx, file_operation_rx) = mpsc::channel();
        let (search_tx, search_rx) = mpsc::sync_channel(SEARCH_WORKER_CAPACITY);
        let (search_done_tx, search_done_rx) = mpsc::sync_channel(1);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let thread_shutdown_requested = Arc::clone(&shutdown_requested);
        let search_shutdown = Arc::clone(&shutdown_requested);
        let search_events = search_event_tx;
        let search_terminals = search_terminals.clone();
        let search_join = thread::Builder::new()
            .name("explorer-search-worker".to_owned())
            .spawn(move || {
                search_worker_loop(
                    search_rx,
                    search_events,
                    search_terminals,
                    search_shutdown,
                    search_done_tx,
                );
            })
            .map_err(ShellStaError::Spawn)?;
        ACTIVE_JOIN_HANDLES.fetch_add(1, Ordering::AcqRel);
        ACTIVE_CONTROL_CHANNELS.fetch_add(1, Ordering::AcqRel);

        let sta_spawn = spawn_sta_thread(move || {
            ACTIVE_STA_THREADS.fetch_add(1, Ordering::AcqRel);
            thread_state.store(ShellStaState::Starting as u8, Ordering::Release);

            let startup_result = startup_hook().and_then(|()| ApartmentGuard::initialize());
            let apartment = match startup_result {
                Ok(apartment) => apartment,
                Err(error) => {
                    thread_state.store(ShellStaState::Stopped as u8, Ordering::Release);
                    let _ = ready_tx.send(Err(error));
                    ACTIVE_STA_THREADS.fetch_sub(1, Ordering::AcqRel);
                    ACTIVE_CONTROL_CHANNELS.fetch_sub(1, Ordering::AcqRel);
                    let _ = done_tx.send(());
                    return;
                }
            };

            thread_state.store(ShellStaState::Ready as u8, Ordering::Release);
            if ready_tx.send(Ok(())).is_err() {
                drop(apartment);
                thread_state.store(ShellStaState::Stopped as u8, Ordering::Release);
                ACTIVE_STA_THREADS.fetch_sub(1, Ordering::AcqRel);
                ACTIVE_CONTROL_CHANNELS.fetch_sub(1, Ordering::AcqRel);
                let _ = done_tx.send(());
                return;
            }

            let mut runtime = StaRuntime::new(search_tx);
            loop {
                if thread_shutdown_requested.load(Ordering::Acquire) {
                    break;
                }
                while let Ok(completion) = file_operation_rx.try_recv() {
                    finish_background_file_operation(
                        completion,
                        &event_tx,
                        &operation_terminals,
                        &mut runtime,
                    );
                }
                let command = match foreground_rx.try_recv() {
                    Ok(command) => Ok(command),
                    Err(TryRecvError::Empty) => background_rx.recv_timeout(MESSAGE_PUMP_INTERVAL),
                    Err(TryRecvError::Disconnected) => Err(RecvTimeoutError::Disconnected),
                };
                match command {
                    Ok(ControlMessage::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                    Ok(ControlMessage::Command { command, queued_at }) => {
                        let outcome =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                process_command(
                                    &command,
                                    queued_at,
                                    if is_foreground_command(&command) {
                                        &navigation_event_tx
                                    } else {
                                        &event_tx
                                    },
                                    &navigation_terminals,
                                    &background_terminals,
                                    &typed_terminals,
                                    &file_operation_tx,
                                    &operation_progress_tx,
                                    &operation_terminals,
                                    &mut runtime,
                                );
                            }));
                        if let Err(payload) = outcome {
                            let message = panic_payload_message(payload.as_ref());
                            record_process_error_message(
                                ErrorSeverity::Critical,
                                "shell",
                                "sta_process_command_panic",
                                &message,
                                Some(file!()),
                            );
                            if let Some(context) = command.context() {
                                let event = command_terminal_failure(
                                    &command,
                                    context.clone(),
                                    ExplorerError::new(
                                        ExplorerErrorKind::Internal,
                                        "Shell STA command panic",
                                        true,
                                        "The operation failed, but Explorer can continue.",
                                        message,
                                    ),
                                );
                                publish_command_terminal(
                                    &command,
                                    event,
                                    &navigation_terminals,
                                    &operation_terminals,
                                    &typed_terminals,
                                );
                            }
                        }
                        thread_pump_cycles.fetch_add(1, Ordering::Relaxed);
                        if !pump_pending_messages() {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        thread_pump_cycles.fetch_add(1, Ordering::Relaxed);
                        if !pump_pending_messages() {
                            break;
                        }
                    }
                }
                runtime.poll_clipboard(&event_tx);
            }

            thread_state.store(ShellStaState::Stopping as u8, Ordering::Release);
            drop(runtime);
            drop(apartment);
            thread_state.store(ShellStaState::Stopped as u8, Ordering::Release);
            ACTIVE_STA_THREADS.fetch_sub(1, Ordering::AcqRel);
            ACTIVE_CONTROL_CHANNELS.fetch_sub(1, Ordering::AcqRel);
            let _ = done_tx.send(());
        });
        let join = match sta_spawn {
            Ok(join) => join,
            Err(error) => {
                ACTIVE_CONTROL_CHANNELS.fetch_sub(1, Ordering::AcqRel);
                shutdown_requested.store(true, Ordering::Release);
                let _ = search_join.join();
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                return Err(ShellStaError::Spawn(error));
            }
        };
        ACTIVE_JOIN_HANDLES.fetch_add(1, Ordering::AcqRel);

        match ready_rx.recv_timeout(timeout) {
            Ok(Ok(())) => {
                tracing::info!(?correlation_id, "Shell STA is ready");
                Ok(Self {
                    correlation_id,
                    foreground_control: foreground_tx,
                    background_control: background_tx,
                    navigation_events: Mutex::new(navigation_event_rx),
                    operation_terminals: Mutex::new(operation_terminal_rx),
                    typed_terminals: Mutex::new(typed_terminal_rx),
                    operation_progress: Mutex::new(operation_progress_rx),
                    search_events: Mutex::new(search_event_rx),
                    events: Mutex::new(event_rx),
                    active_requests: Mutex::new(HashMap::new()),
                    done: Mutex::new(done_rx),
                    join: Mutex::new(Some(join)),
                    search_done: Mutex::new(search_done_rx),
                    search_join: Mutex::new(Some(search_join)),
                    state,
                    pump_cycles,
                    shutdown_requested,
                })
            }
            Ok(Err(error)) => {
                let _ = join.join();
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                shutdown_requested.store(true, Ordering::Release);
                let _ = search_join.join();
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                Err(error)
            }
            Err(RecvTimeoutError::Timeout) => {
                shutdown_requested.store(true, Ordering::Release);
                let _ = foreground_tx.try_send(ControlMessage::Shutdown);
                tracing::error!(?correlation_id, ?timeout, "Shell STA startup timed out");
                drop(join);
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                let _ = search_join.join();
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                Err(ShellStaError::StartupTimeout { timeout })
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = join.join();
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                shutdown_requested.store(true, Ordering::Release);
                let _ = search_join.join();
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                Err(ShellStaError::StartupChannelClosed)
            }
        }
    }

    /// Returns the latest observable lifecycle state.
    pub fn state(&self) -> ShellStaState {
        ShellStaState::from_atomic(self.state.load(Ordering::Acquire))
    }

    /// Returns how many bounded waits have completed and pumped pending Windows messages.
    pub fn message_pump_cycles(&self) -> usize {
        self.pump_cycles.load(Ordering::Relaxed)
    }

    /// Submits a typed command without blocking the caller.
    ///
    /// # Errors
    ///
    /// Returns explicit overload or disconnect status. A cancellation command also flips the
    /// shared request token synchronously so long Shell enumerations observe it between children.
    pub fn submit(&self, command: ExplorerCommand) -> Result<(), ShellStaEndpointError> {
        if let ExplorerCommand::Cancel { request_id } = &command {
            if let Some(token) = self
                .active_requests
                .lock()
                .map_err(|_| ShellStaEndpointError::Poisoned)?
                .get(request_id)
            {
                token.cancellation.cancel();
            }
        } else if let Some(context) = command.context() {
            let required_terminal_lane = required_terminal_lane(&command);
            let mut active_requests = self
                .active_requests
                .lock()
                .map_err(|_| ShellStaEndpointError::Poisoned)?;
            if !terminal_submission_capacity_available(&active_requests, required_terminal_lane) {
                return Err(ShellStaEndpointError::CommandQueueFull);
            }
            active_requests.insert(
                context.request_id,
                ActiveRequest {
                    cancellation: context.cancellation.clone(),
                    required_terminal_lane,
                },
            );
        }
        let control = if is_foreground_command(&command) {
            &self.foreground_control
        } else {
            &self.background_control
        };
        let request_id = command.context().map(|context| context.request_id);
        match control.try_send(ControlMessage::Command {
            command,
            queued_at: Instant::now(),
        }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                self.remove_rejected_request(request_id)?;
                Err(ShellStaEndpointError::CommandQueueFull)
            }
            Err(TrySendError::Disconnected(_)) => {
                self.remove_rejected_request(request_id)?;
                Err(ShellStaEndpointError::CommandEndpointDisconnected)
            }
        }
    }

    fn remove_rejected_request(
        &self,
        request_id: Option<RequestId>,
    ) -> Result<(), ShellStaEndpointError> {
        if let Some(request_id) = request_id {
            self.active_requests
                .lock()
                .map_err(|_| ShellStaEndpointError::Poisoned)?
                .remove(&request_id);
        }
        Ok(())
    }

    /// Receives one pending owned event without blocking the caller.
    ///
    /// # Errors
    ///
    /// Returns only synchronization or endpoint disconnect errors; an empty queue is `Ok(None)`.
    pub fn try_recv_event(&self) -> Result<Option<ExplorerEvent>, ShellStaEndpointError> {
        let terminal = self
            .operation_terminals
            .lock()
            .map_err(|_| ShellStaEndpointError::Poisoned)?
            .try_recv();
        let event = match terminal {
            Ok(event) => event,
            Err(TryRecvError::Disconnected) => {
                return Err(ShellStaEndpointError::EventEndpointDisconnected);
            }
            Err(TryRecvError::Empty) => match self
                .navigation_events
                .lock()
                .map_err(|_| ShellStaEndpointError::Poisoned)?
                .try_recv()
            {
                Ok(event) => event,
                Err(TryRecvError::Empty) => match self
                    .typed_terminals
                    .lock()
                    .map_err(|_| ShellStaEndpointError::Poisoned)?
                    .try_recv()
                {
                    Ok(event) => event,
                    Err(TryRecvError::Empty) => match self
                        .operation_progress
                        .lock()
                        .map_err(|_| ShellStaEndpointError::Poisoned)?
                        .try_recv()
                    {
                        Ok(event) => event,
                        Err(TryRecvError::Empty) => match self
                            .search_events
                            .lock()
                            .map_err(|_| ShellStaEndpointError::Poisoned)?
                            .try_recv()
                        {
                            Ok(event) => event,
                            Err(TryRecvError::Empty) => match self
                                .events
                                .lock()
                                .map_err(|_| ShellStaEndpointError::Poisoned)?
                                .try_recv()
                            {
                                Ok(event) => event,
                                Err(TryRecvError::Empty) => return Ok(None),
                                Err(TryRecvError::Disconnected) => {
                                    return Err(ShellStaEndpointError::EventEndpointDisconnected);
                                }
                            },
                            Err(TryRecvError::Disconnected) => {
                                return Err(ShellStaEndpointError::EventEndpointDisconnected);
                            }
                        },
                        Err(TryRecvError::Disconnected) => {
                            return Err(ShellStaEndpointError::EventEndpointDisconnected);
                        }
                    },
                    Err(TryRecvError::Disconnected) => {
                        return Err(ShellStaEndpointError::EventEndpointDisconnected);
                    }
                },
                Err(TryRecvError::Disconnected) => {
                    return Err(ShellStaEndpointError::EventEndpointDisconnected);
                }
            },
        };
        remove_completed_request(&self.active_requests, &event)?;
        Ok(Some(event))
    }

    /// Requests shutdown at most once and never blocks the caller.
    pub fn shutdown(&self) {
        if self.shutdown_requested.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Ok(requests) = self.active_requests.lock() {
            for request in requests.values() {
                request.cancellation.cancel();
            }
        }
        if matches!(self.state(), ShellStaState::Ready) {
            self.state
                .store(ShellStaState::Stopping as u8, Ordering::Release);
        }
        match self.foreground_control.try_send(ControlMessage::Shutdown) {
            Ok(()) | Err(TrySendError::Disconnected(_) | TrySendError::Full(_)) => {}
        }
    }

    /// Requests shutdown and waits for bounded completion.
    ///
    /// # Errors
    ///
    /// Returns an error when synchronization is poisoned, the timeout expires, or the thread
    /// panics while joining.
    pub fn shutdown_and_join(&self, timeout: Duration) -> Result<(), ShellStaError> {
        self.shutdown();
        let done = self.done.lock().map_err(|_| ShellStaError::Poisoned)?;
        let mut join = self.join.lock().map_err(|_| ShellStaError::Poisoned)?;
        let Some(thread) = join.take() else {
            return self.join_search_worker(timeout);
        };
        match done.recv_timeout(timeout) {
            Ok(()) => {
                let result = thread.join();
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                result.map_err(|_| ShellStaError::ThreadPanicked)?;
                self.join_search_worker(timeout)
            }
            Err(RecvTimeoutError::Disconnected) => {
                let result = thread.join();
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                result.map_err(|_| ShellStaError::ThreadPanicked)
            }
            Err(RecvTimeoutError::Timeout) => {
                *join = Some(thread);
                tracing::error!(?timeout, state = ?self.state(), "Shell STA join timed out");
                Err(ShellStaError::JoinTimeout { timeout })
            }
        }
    }

    fn join_search_worker(&self, timeout: Duration) -> Result<(), ShellStaError> {
        let done = self
            .search_done
            .lock()
            .map_err(|_| ShellStaError::Poisoned)?;
        let mut join = self
            .search_join
            .lock()
            .map_err(|_| ShellStaError::Poisoned)?;
        let Some(thread) = join.take() else {
            return Ok(());
        };
        match done.recv_timeout(timeout) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                let result = thread.join().map_err(|_| ShellStaError::ThreadPanicked);
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                result
            }
            Err(RecvTimeoutError::Timeout) => {
                *join = Some(thread);
                Err(ShellStaError::JoinTimeout { timeout })
            }
        }
    }
}

fn terminal_submission_capacity_available(
    active_requests: &HashMap<RequestId, ActiveRequest>,
    required_terminal_lane: Option<RequiredTerminalLane>,
) -> bool {
    // Navigation's primary lane also carries best-effort batches and watcher changes. Stop
    // admitting a new required-terminal command once the retained bound could be consumed by
    // already-accepted foreground work; this preserves nonblocking delivery without assuming
    // that a UI receiver is draining those nonterminal events.
    let Some(required_terminal_lane) = required_terminal_lane else {
        return true;
    };
    let capacity = match required_terminal_lane {
        RequiredTerminalLane::Navigation => NAVIGATION_TERMINAL_RETAIN_CAPACITY,
        RequiredTerminalLane::Operation => OPERATION_TERMINAL_RETAIN_CAPACITY,
        RequiredTerminalLane::Typed => TYPED_TERMINAL_RETAIN_CAPACITY,
    };
    active_requests
        .values()
        .filter(|request| request.required_terminal_lane == Some(required_terminal_lane))
        .count()
        < capacity
}

fn required_terminal_lane(command: &ExplorerCommand) -> Option<RequiredTerminalLane> {
    match command {
        ExplorerCommand::Navigate { .. }
        | ExplorerCommand::Refresh { .. }
        | ExplorerCommand::OpenItem {
            disposition: OpenDisposition::CurrentTab | OpenDisposition::NewTab,
            ..
        } => Some(RequiredTerminalLane::Navigation),
        ExplorerCommand::ExecuteFileOperation { .. }
        | ExplorerCommand::DataTransfer { .. }
        | ExplorerCommand::OpenItem {
            disposition: OpenDisposition::DefaultApplication,
            ..
        } => Some(RequiredTerminalLane::Operation),
        ExplorerCommand::ShowContextMenu { .. }
        | ExplorerCommand::ResolveAncestry { .. }
        | ExplorerCommand::EnumerateChildContainers { .. }
        | ExplorerCommand::StartSearch { .. }
        | ExplorerCommand::LoadShellIcon { .. }
        | ExplorerCommand::LoadThumbnail { .. }
        | ExplorerCommand::ClearThumbnailCache { .. }
        | ExplorerCommand::PreviewHost { .. }
        | ExplorerCommand::DiscoverLockOwners { .. }
        | ExplorerCommand::CloseLockOwners { .. } => Some(RequiredTerminalLane::Typed),
        _ => None,
    }
}

fn remove_completed_request(
    active_requests: &Mutex<HashMap<RequestId, ActiveRequest>>,
    event: &ExplorerEvent,
) -> Result<(), ShellStaEndpointError> {
    if event.is_terminal()
        && let Some(context) = event.context()
    {
        active_requests
            .lock()
            .map_err(|_| ShellStaEndpointError::Poisoned)?
            .remove(&context.request_id);
    }
    Ok(())
}

/// Foreground commands retain capacity even when enrichment, thumbnail, and search queues fill.
fn is_foreground_command(command: &ExplorerCommand) -> bool {
    matches!(
        command,
        ExplorerCommand::Navigate { .. }
            | ExplorerCommand::Refresh { .. }
            | ExplorerCommand::Cancel { .. }
            | ExplorerCommand::ExecuteFileOperation { .. }
            | ExplorerCommand::DataTransfer { .. }
            | ExplorerCommand::ShowContextMenu { .. }
            | ExplorerCommand::StartSearch { .. }
            | ExplorerCommand::OpenItem {
                disposition: OpenDisposition::CurrentTab
                    | OpenDisposition::NewTab
                    | OpenDisposition::DefaultApplication,
                ..
            }
    )
}

struct StaRuntime {
    watchers: HashMap<explorer_model::TabId, crate::watcher::WatcherSession>,
    clipboard: crate::clipboard::ClipboardRuntime,
    icon_cache: crate::icon::ShellIconCache,
    search_jobs: SyncSender<SearchJob>,
}

impl StaRuntime {
    fn new(search_jobs: SyncSender<SearchJob>) -> Self {
        Self {
            watchers: HashMap::new(),
            clipboard: crate::clipboard::ClipboardRuntime::new(),
            icon_cache: crate::icon::ShellIconCache::default(),
            search_jobs,
        }
    }

    fn poll_clipboard(&mut self, events: &SyncSender<ExplorerEvent>) {
        if let Some(state) = self.clipboard.poll_change() {
            let _ = events.try_send(ExplorerEvent::ClipboardChanged { state });
        }
    }
    fn watch_location(
        &mut self,
        context: &explorer_model::RequestContext,
        location: &LocationDescriptor,
        events: &SyncSender<ExplorerEvent>,
    ) {
        self.watchers.remove(&context.tab_id);
        let Some(path) = watchable_directory_path(location) else {
            return;
        };
        match crate::watcher::WatcherSession::start(
            path,
            context.tab_id,
            context.generation,
            events.clone(),
        ) {
            Ok(watcher) => {
                self.watchers.insert(context.tab_id, watcher);
            }
            Err(error) => {
                tracing::warn!(
                    request_id = ?context.request_id,
                    tab_id = ?context.tab_id,
                    generation = context.generation.value(),
                    %error,
                    "directory watcher could not start"
                );
                record_process_error(
                    ErrorSeverity::Error,
                    "shell",
                    "start_directory_watcher",
                    &error,
                    Some(file!()),
                );
            }
        }
    }

    fn enqueue_search(
        &self,
        context: explorer_model::RequestContext,
        location: LocationDescriptor,
        input: explorer_model::SearchInput,
    ) -> Result<(), ExplorerError> {
        let Some(worker_guard) = reserve_worker(
            &ACTIVE_SEARCH_WORKERS,
            SEARCH_WORKER_CAPACITY,
            &SATURATED_SEARCH_WORKERS,
        ) else {
            tracing::warn!(
                request_id = ?context.request_id,
                tab_id = ?context.tab_id,
                generation = context.generation.value(),
                domain = "search",
                "isolated worker domain is saturated"
            );
            return Err(ExplorerError::new(
                ExplorerErrorKind::Availability,
                "start search",
                true,
                "Search is temporarily busy. Try again shortly.",
                "search domain capacity is saturated",
            ));
        };
        self.search_jobs
            .try_send(SearchJob {
                context,
                location,
                input,
                worker_guard,
            })
            .map_err(|error| {
                ExplorerError::new(
                    ExplorerErrorKind::Availability,
                    "start search",
                    true,
                    "Search is temporarily busy. Try again shortly.",
                    error.to_string(),
                )
            })
    }
}

fn watchable_directory_path(location: &LocationDescriptor) -> Option<std::path::PathBuf> {
    let path = location.path()?;
    path.is_dir().then(|| path.to_path_buf())
}

#[allow(
    clippy::too_many_lines,
    reason = "one dispatch match keeps typed Shell commands and exactly-one terminal emission auditable"
)]
fn process_command(
    command: &ExplorerCommand,
    queued_at: Instant,
    events: &SyncSender<ExplorerEvent>,
    navigation_terminals: &ReliableTerminalPublisher,
    background_terminals: &ReliableTerminalPublisher,
    typed_terminals: &ReliableTerminalPublisher,
    file_operations: &Sender<FileOperationCompletion>,
    operation_progress: &SyncSender<ExplorerEvent>,
    operation_terminals: &ReliableTerminalPublisher,
    runtime: &mut StaRuntime,
) {
    let Some(context) = command.context().cloned() else {
        return;
    };
    tracing::debug!(
        request_id = ?context.request_id,
        tab_id = ?context.tab_id,
        generation = context.generation.value(),
        queue_latency_micros = queued_at.elapsed().as_micros(),
        "Shell STA command started"
    );
    let started = Instant::now();
    let result = match &command {
        ExplorerCommand::Navigate { location, .. } | ExplorerCommand::Refresh { location, .. } => {
            process_navigation(&context, location, events, navigation_terminals)
        }
        ExplorerCommand::ResolveAncestry { .. }
        | ExplorerCommand::EnumerateChildContainers { .. } => {
            start_brokered_breadcrumb(command, events, background_terminals.clone());
            Ok(())
        }
        ExplorerCommand::OpenItem {
            item, disposition, ..
        } => match disposition {
            OpenDisposition::CurrentTab | OpenDisposition::NewTab => {
                process_navigation(&context, &item.location, events, navigation_terminals)
            }
            OpenDisposition::DefaultApplication => crate::navigation::open_default(&item.location)
                .map(|()| {
                    operation_terminals.publish(ExplorerEvent::OperationFinished {
                        context: context.clone(),
                        outcome: OperationTerminal::Finished,
                    });
                }),
        },
        ExplorerCommand::ExecuteFileOperation { request, .. } => start_file_operation_worker(
            context.clone(),
            request.clone(),
            operation_progress.clone(),
            file_operations.clone(),
            false,
        ),
        ExplorerCommand::DataTransfer { request, .. } => match request {
            DataTransferRequest::Paste {
                destination,
                conflict,
            } => runtime
                .clipboard
                .begin_background_paste(context.request_id, destination.clone(), *conflict)
                .and_then(|operation| {
                    start_file_operation_worker(
                        context.clone(),
                        operation,
                        operation_progress.clone(),
                        file_operations.clone(),
                        true,
                    )
                    .inspect_err(|_| {
                        runtime
                            .clipboard
                            .abandon_background_paste(context.request_id);
                    })
                }),
            DataTransferRequest::DropExternal {
                sources,
                destination,
                effect,
                conflict,
            } => crate::drag_drop::external_drop_request(
                sources,
                destination.clone(),
                *effect,
                *conflict,
            )
            .and_then(|operation| {
                start_file_operation_worker(
                    context.clone(),
                    operation,
                    operation_progress.clone(),
                    file_operations.clone(),
                    false,
                )
            }),
            request => {
                let result = match request {
                    DataTransferRequest::Copy { items } => runtime
                        .clipboard
                        .copy_or_cut(items.clone(), ClipboardMode::Copy)
                        .map(|state| {
                            let _ = events.try_send(ExplorerEvent::ClipboardChanged { state });
                            OperationTerminal::Finished
                        }),
                    DataTransferRequest::Cut { items } => runtime
                        .clipboard
                        .copy_or_cut(items.clone(), ClipboardMode::Cut)
                        .map(|state| {
                            let _ = events.try_send(ExplorerEvent::ClipboardChanged { state });
                            OperationTerminal::Finished
                        }),
                    DataTransferRequest::BeginDrag {
                        items,
                        allowed_effects,
                        button,
                    } => crate::drag_drop::begin_native_drag(
                        items,
                        *allowed_effects,
                        *button,
                        context.cancellation.clone(),
                    )
                    .map_err(|_| {
                        ExplorerError::new(
                            ExplorerErrorKind::Availability,
                            "begin OLE drag",
                            true,
                            "拖放服務尚未啟動。",
                            "drag request reached clipboard-only dispatch",
                        )
                    }),
                    DataTransferRequest::Paste { .. }
                    | DataTransferRequest::DropExternal { .. } => {
                        unreachable!("background transfers are handled before synchronous dispatch")
                    }
                };
                let outcome = operation_terminal(result, "data_transfer");
                operation_terminals.publish(ExplorerEvent::OperationFinished {
                    context: context.clone(),
                    outcome,
                });
                Ok(())
            }
        },
        ExplorerCommand::ShowContextMenu { request, .. } => {
            if request.requested_verb.as_deref().is_some_and(|verb| {
                verb.eq_ignore_ascii_case("properties")
                    || verb.eq_ignore_ascii_case("Windows.Share")
                    || verb.eq_ignore_ascii_case("PinToStartScreen")
            }) {
                crate::context_menu::run_host_owned(&context, request, typed_terminals.clone());
            } else {
                crate::context_menu::start_brokered(
                    context.clone(),
                    request.clone(),
                    typed_terminals.clone(),
                );
            }
            Ok(())
        }
        ExplorerCommand::Cancel { .. } => Ok(()),
        ExplorerCommand::StartSearch {
            location, input, ..
        } => runtime.enqueue_search(context.clone(), location.clone(), input.clone()),
        ExplorerCommand::LoadShellIcon { key, .. } => {
            let event = if context.cancellation.is_cancelled() {
                ExplorerEvent::ShellIconFailed {
                    context: context.clone(),
                    key: key.clone(),
                    reason: explorer_model::ShellIconFallbackReason::ShellUnavailable,
                }
            } else {
                match runtime.icon_cache.load(key) {
                    Ok(payload) => ExplorerEvent::ShellIconLoaded {
                        context: context.clone(),
                        payload,
                    },
                    Err(error) => {
                        tracing::debug!(?error, "Shell icon fallback remains active");
                        record_process_error(
                            ErrorSeverity::Warning,
                            "shell",
                            &error.operation,
                            &error,
                            Some(file!()),
                        );
                        ExplorerEvent::ShellIconFailed {
                            context: context.clone(),
                            key: key.clone(),
                            reason: explorer_model::ShellIconFallbackReason::ShellUnavailable,
                        }
                    }
                }
            };
            typed_terminals.publish(event);
            Ok(())
        }
        ExplorerCommand::LoadThumbnail {
            key,
            location,
            cache_only,
            ..
        } => start_thumbnail_worker(
            context.clone(),
            key.clone(),
            location.clone(),
            *cache_only,
            typed_terminals.clone(),
        ),
        ExplorerCommand::ClearThumbnailCache { .. } => {
            typed_terminals.publish(ExplorerEvent::ThumbnailCacheCleared {
                context: context.clone(),
                success: crate::thumbnail::clear_thumbnail_disk_cache(),
            });
            Ok(())
        }
        ExplorerCommand::PreviewHost { command, .. } => {
            typed_terminals.publish(ExplorerEvent::PreviewHostFinished {
                context: context.clone(),
                outcome: explorer_model::PreviewHostTerminal::Failed {
                    generation: command.generation(),
                    error: explorer_model::PreviewHostError::Unsupported,
                },
            });
            Ok(())
        }
        ExplorerCommand::DiscoverLockOwners { request, .. } => {
            crate::restart_manager::start_discovery(
                context.clone(),
                request.clone(),
                typed_terminals.clone(),
            );
            Ok(())
        }
        ExplorerCommand::CloseLockOwners { request, .. } => {
            crate::restart_manager::start_close(
                context.clone(),
                request.clone(),
                typed_terminals.clone(),
            );
            Ok(())
        }
    };
    if result.is_ok() {
        match command {
            ExplorerCommand::Navigate { location, .. }
            | ExplorerCommand::Refresh { location, .. } => {
                runtime.watch_location(&context, location, events);
            }
            ExplorerCommand::OpenItem {
                item,
                disposition: OpenDisposition::CurrentTab | OpenDisposition::NewTab,
                ..
            } => runtime.watch_location(&context, &item.location, events),
            ExplorerCommand::OpenItem {
                disposition: OpenDisposition::DefaultApplication,
                ..
            }
            | ExplorerCommand::ExecuteFileOperation { .. }
            | ExplorerCommand::ResolveAncestry { .. }
            | ExplorerCommand::EnumerateChildContainers { .. }
            | ExplorerCommand::Cancel { .. }
            | ExplorerCommand::ShowContextMenu { .. }
            | ExplorerCommand::StartSearch { .. }
            | ExplorerCommand::DataTransfer { .. }
            | ExplorerCommand::LoadShellIcon { .. }
            | ExplorerCommand::LoadThumbnail { .. }
            | ExplorerCommand::ClearThumbnailCache { .. }
            | ExplorerCommand::PreviewHost { .. }
            | ExplorerCommand::DiscoverLockOwners { .. }
            | ExplorerCommand::CloseLockOwners { .. } => {}
        }
    }
    if let Err(error) = result {
        record_process_error(
            ErrorSeverity::Error,
            "shell",
            &error.operation,
            &error,
            Some(file!()),
        );
        let event = command_terminal_failure(command, context.clone(), error);
        publish_command_terminal(
            command,
            event,
            navigation_terminals,
            operation_terminals,
            typed_terminals,
        );
    }
    tracing::debug!(
        request_id = ?context.request_id,
        tab_id = ?context.tab_id,
        generation = context.generation.value(),
        elapsed_micros = started.elapsed().as_micros(),
        "Shell STA command finished"
    );
}

/// Preserves the reducer's command-specific terminal contract when dispatch itself fails or
/// panics. Generic `Failed` is reserved for commands without a typed terminal.
fn command_terminal_failure(
    command: &ExplorerCommand,
    context: explorer_model::RequestContext,
    error: ExplorerError,
) -> ExplorerEvent {
    match command {
        ExplorerCommand::ExecuteFileOperation { .. }
        | ExplorerCommand::DataTransfer { .. }
        | ExplorerCommand::OpenItem {
            disposition: OpenDisposition::DefaultApplication,
            ..
        } => ExplorerEvent::OperationFinished {
            context,
            outcome: OperationTerminal::Failed(error),
        },
        ExplorerCommand::ShowContextMenu { .. } => ExplorerEvent::ContextMenuFinished {
            context,
            outcome: explorer_model::ContextMenuOutcome::Failed { error },
        },
        ExplorerCommand::PreviewHost { command, .. } => ExplorerEvent::PreviewHostFinished {
            context,
            outcome: explorer_model::PreviewHostTerminal::Failed {
                generation: command.generation(),
                error: explorer_model::PreviewHostError::Crash,
            },
        },
        ExplorerCommand::DiscoverLockOwners { .. } => ExplorerEvent::LockOwnersDiscovered {
            context,
            outcome: explorer_model::LockOwnerDiscoveryTerminal::Failed(error),
        },
        ExplorerCommand::CloseLockOwners { .. } => ExplorerEvent::LockOwnersClosed {
            context,
            outcome: explorer_model::LockOwnerCloseTerminal::Failed(error),
        },
        ExplorerCommand::StartSearch { .. } => ExplorerEvent::SearchFinished {
            context,
            outcome: explorer_model::SearchTerminal::Failed(error),
        },
        ExplorerCommand::ResolveAncestry { .. } => ExplorerEvent::AncestryFinished {
            context,
            outcome: BreadcrumbTerminal::Failed(error),
        },
        ExplorerCommand::EnumerateChildContainers {
            segment_id,
            menu_generation,
            ..
        } => ExplorerEvent::ChildContainersFinished {
            context,
            segment_id: *segment_id,
            menu_generation: *menu_generation,
            outcome: BreadcrumbTerminal::Failed(error),
        },
        ExplorerCommand::LoadThumbnail { key, .. } => ExplorerEvent::ThumbnailFinished {
            context,
            key: key.clone(),
            outcome: explorer_model::ThumbnailTerminal::Failed(error.to_string()),
        },
        ExplorerCommand::LoadShellIcon { key, .. } => ExplorerEvent::ShellIconFailed {
            context,
            key: key.clone(),
            reason: explorer_model::ShellIconFallbackReason::ShellUnavailable,
        },
        ExplorerCommand::ClearThumbnailCache { .. } => ExplorerEvent::ThumbnailCacheCleared {
            context,
            success: false,
        },
        ExplorerCommand::Navigate { .. }
        | ExplorerCommand::Refresh { .. }
        | ExplorerCommand::OpenItem { .. }
        | ExplorerCommand::Cancel { .. } => ExplorerEvent::Failed { context, error },
    }
}

fn publish_command_terminal(
    command: &ExplorerCommand,
    event: ExplorerEvent,
    navigation_terminals: &ReliableTerminalPublisher,
    operation_terminals: &ReliableTerminalPublisher,
    typed_terminals: &ReliableTerminalPublisher,
) {
    match command {
        ExplorerCommand::Navigate { .. }
        | ExplorerCommand::Refresh { .. }
        | ExplorerCommand::OpenItem {
            disposition: OpenDisposition::CurrentTab | OpenDisposition::NewTab,
            ..
        } => navigation_terminals.publish(event),
        ExplorerCommand::ExecuteFileOperation { .. }
        | ExplorerCommand::DataTransfer { .. }
        | ExplorerCommand::OpenItem {
            disposition: OpenDisposition::DefaultApplication,
            ..
        } => operation_terminals.publish(event),
        _ => typed_terminals.publish(event),
    }
}

fn operation_terminal(
    result: Result<OperationTerminal, ExplorerError>,
    operation: &str,
) -> OperationTerminal {
    match result {
        Ok(outcome) => outcome,
        Err(error) => {
            record_process_error(
                ErrorSeverity::Error,
                "shell",
                operation,
                &error,
                Some(file!()),
            );
            OperationTerminal::Failed(error)
        }
    }
}

struct IsolatedWorkerGuard(&'static AtomicUsize);

impl Drop for IsolatedWorkerGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn reserve_worker(
    active: &'static AtomicUsize,
    capacity: usize,
    saturated: &'static AtomicUsize,
) -> Option<IsolatedWorkerGuard> {
    let mut observed = active.load(Ordering::Acquire);
    loop {
        if observed >= capacity {
            saturated.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        match active.compare_exchange_weak(
            observed,
            observed + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return Some(IsolatedWorkerGuard(active)),
            Err(current) => observed = current,
        }
    }
}

fn background_file_operation_error(operation: &'static str, detail: String) -> ExplorerError {
    ExplorerError::new(
        ExplorerErrorKind::Availability,
        operation,
        true,
        "無法啟動背景檔案作業。",
        detail,
    )
}

fn start_file_operation_worker(
    context: explorer_model::RequestContext,
    request: explorer_model::FileOperationRequest,
    events: SyncSender<ExplorerEvent>,
    completions: Sender<FileOperationCompletion>,
    clipboard_paste: bool,
) -> Result<(), ExplorerError> {
    let Some(worker_guard) = reserve_worker(
        &ACTIVE_FILE_OPERATION_WORKERS,
        FILE_OPERATION_WORKER_CAPACITY,
        &SATURATED_FILE_OPERATION_WORKERS,
    ) else {
        tracing::warn!(
            request_id = ?context.request_id,
            tab_id = ?context.tab_id,
            generation = context.generation.value(),
            domain = "file-operation",
            "isolated worker domain is saturated"
        );
        return Err(background_file_operation_error(
            "start background file operation",
            "file-operation domain capacity is saturated".to_owned(),
        ));
    };
    let worker_name = format!("file-operation-{:?}", context.request_id);
    thread::Builder::new()
        .name(worker_name)
        .spawn(move || {
            let _worker_guard = worker_guard;
            #[cfg(test)]
            if let Some(gate) = FILE_OPERATION_TEST_GATE
                .lock()
                .ok()
                .and_then(|gate| gate.clone())
                .filter(|gate| gate.request_id == context.request_id)
            {
                let _ = gate.started.try_send(());
                while !gate.release.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(2));
                }
            }
            let panic_context = context.clone();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ApartmentGuard::initialize()
                    .map_err(|error| {
                        background_file_operation_error(
                            "initialize background file operation apartment",
                            error.to_string(),
                        )
                    })
                    .and_then(|_apartment| {
                        crate::file_operation::execute(&context, &request, &events)
                    })
            }))
            .map_or_else(
                |payload| {
                    let message = panic_payload_message(payload.as_ref());
                    record_process_error_message(
                        ErrorSeverity::Critical,
                        "shell",
                        "file_operation_worker_panic",
                        &message,
                        Some(file!()),
                    );
                    OperationTerminal::Failed(background_file_operation_error(
                        "background file operation panic",
                        message,
                    ))
                },
                |result| operation_terminal(result, "background_file_operation"),
            );
            let _ = completions.send(FileOperationCompletion {
                context: panic_context,
                outcome,
                clipboard_paste,
            });
        })
        .map(|_| ())
        .map_err(|error| {
            background_file_operation_error("start background file operation", error.to_string())
        })
}

fn finish_background_file_operation(
    completion: FileOperationCompletion,
    events: &SyncSender<ExplorerEvent>,
    terminals: &ReliableTerminalPublisher,
    runtime: &mut StaRuntime,
) {
    if completion.context.cancellation.is_cancelled() {
        STALE_CANCELLED_COMPLETIONS.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            request_id = ?completion.context.request_id,
            tab_id = ?completion.context.tab_id,
            generation = completion.context.generation.value(),
            "background file-operation completion was cancelled before delivery"
        );
    }
    if completion.clipboard_paste
        && let Some(state) = runtime
            .clipboard
            .complete_background_paste(completion.context.request_id, &completion.outcome)
    {
        // Clipboard updates are optional progress-like state. The required operation terminal
        // below remains independently reliable when this best-effort notification is full.
        let _ = events.try_send(ExplorerEvent::ClipboardChanged { state });
    }
    terminals.publish(ExplorerEvent::OperationFinished {
        context: completion.context,
        outcome: completion.outcome,
    });
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the managed worker owns its endpoints and shutdown guard for the full thread lifecycle"
)]
fn search_worker_loop(
    jobs: Receiver<SearchJob>,
    events: SyncSender<ExplorerEvent>,
    terminals: ReliableTerminalPublisher,
    shutdown_requested: Arc<AtomicBool>,
    done: SyncSender<()>,
) {
    ACTIVE_SEARCH_EXECUTORS.fetch_add(1, Ordering::AcqRel);
    let _executor_guard = IsolatedWorkerGuard(&ACTIVE_SEARCH_EXECUTORS);
    while !shutdown_requested.load(Ordering::Acquire) {
        let job = match jobs.recv_timeout(MESSAGE_PUMP_INTERVAL) {
            Ok(job) => job,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let SearchJob {
            context,
            location,
            input,
            worker_guard: _worker_guard,
        } = job;
        #[cfg(test)]
        if let Some(gate) = SEARCH_TEST_GATE
            .lock()
            .ok()
            .and_then(|gate| gate.clone())
            .filter(|gate| gate.request_id == context.request_id)
        {
            let _ = gate.started.try_send(());
            while !gate.release.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }
        }
        tracing::debug!(
            request_id = ?context.request_id,
            tab_id = ?context.tab_id,
            generation = context.generation.value(),
            "isolated search worker started"
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ApartmentGuard::initialize()
                .map_err(|error| {
                    ExplorerError::new(
                        ExplorerErrorKind::Availability,
                        "initialize search apartment",
                        true,
                        "Search is unavailable.",
                        error.to_string(),
                    )
                })
                .and_then(|_apartment| {
                    crate::search::execute_with_terminals(
                        &context, &location, &input, &events, &terminals,
                    )
                })
        }))
        .unwrap_or_else(|payload| {
            let message = panic_payload_message(payload.as_ref());
            record_process_error_message(
                ErrorSeverity::Critical,
                "shell",
                "search_worker_panic",
                &message,
                Some(file!()),
            );
            Err(ExplorerError::new(
                ExplorerErrorKind::Internal,
                "search worker panic",
                true,
                "Search failed, but Explorer can continue.",
                message,
            ))
        });
        if let Err(error) = result {
            terminals.publish(ExplorerEvent::SearchFinished {
                context: context.clone(),
                outcome: explorer_model::SearchTerminal::Failed(error),
            });
        }
        if context.cancellation.is_cancelled() {
            STALE_CANCELLED_COMPLETIONS.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(
                request_id = ?context.request_id,
                tab_id = ?context.tab_id,
                generation = context.generation.value(),
                "isolated search completion was cancelled before delivery"
            );
        }
    }
    let _ = done.send(());
}

fn start_thumbnail_worker(
    context: explorer_model::RequestContext,
    key: explorer_model::ThumbnailRequestKey,
    location: LocationDescriptor,
    cache_only: bool,
    terminals: ReliableTerminalPublisher,
) -> Result<(), ExplorerError> {
    let Some(worker_guard) = reserve_worker(
        &ACTIVE_THUMBNAIL_WORKERS,
        THUMBNAIL_WORKER_CAPACITY,
        &SATURATED_THUMBNAIL_WORKERS,
    ) else {
        tracing::warn!(
            request_id = ?context.request_id,
            tab_id = ?context.tab_id,
            generation = context.generation.value(),
            domain = "thumbnail",
            "isolated worker domain is saturated"
        );
        return Err(ExplorerError::new(
            ExplorerErrorKind::Availability,
            "load thumbnail",
            true,
            "Thumbnail loading is temporarily busy.",
            "thumbnail domain capacity is saturated",
        ));
    };
    let worker_name = format!("thumbnail-{:?}", context.request_id);
    thread::Builder::new()
        .name(worker_name)
        .spawn(move || {
            let _worker_guard = worker_guard;
            let request = explorer_model::ThumbnailRequest::new(
                context.clone(),
                key.clone(),
                explorer_model::ThumbnailPriority::ActiveVisible,
            );
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ApartmentGuard::initialize().map_or_else(
                    |error| explorer_model::ThumbnailTerminal::Failed(error.to_string()),
                    |_apartment| {
                        crate::thumbnail::load_shell_thumbnail(
                            &request,
                            &location,
                            cache_only,
                            explorer_common::RoadmapLimits::default().thumbnail_memory_bytes,
                        )
                    },
                )
            }))
            .unwrap_or_else(|payload| {
                let message = panic_payload_message(payload.as_ref());
                record_process_error_message(
                    ErrorSeverity::Critical,
                    "shell",
                    "thumbnail_worker_panic",
                    &message,
                    Some(file!()),
                );
                explorer_model::ThumbnailTerminal::Failed(message)
            });
            let _ = request.claim_terminal(&outcome);
            if context.cancellation.is_cancelled() {
                STALE_CANCELLED_COMPLETIONS.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(
                    request_id = ?context.request_id,
                    tab_id = ?context.tab_id,
                    generation = context.generation.value(),
                    "isolated thumbnail completion was cancelled before delivery"
                );
            }
            terminals.publish(ExplorerEvent::ThumbnailFinished {
                context,
                key,
                outcome,
            });
        })
        .map(|_| ())
        .map_err(|error| {
            ExplorerError::new(
                ExplorerErrorKind::Availability,
                "start thumbnail worker",
                true,
                "Thumbnail loading could not be started.",
                error.to_string(),
            )
        })
}

/// Runs extension-controlled Shell namespace work outside the application's long-lived STA.
/// The coordinator owns the exactly-once terminal gate; a provider that never returns can leave
/// only its disposable apartment blocked and cannot stall navigation, input, or shutdown.
fn start_brokered_breadcrumb(
    command: &ExplorerCommand,
    events: &SyncSender<ExplorerEvent>,
    terminals: ReliableTerminalPublisher,
) {
    start_bounded_breadcrumb_job(
        command,
        events,
        terminals.clone(),
        BREADCRUMB_PROVIDER_TIMEOUT,
        move |worker_command, worker_events, worker_gate| match ApartmentGuard::initialize() {
            Ok(_apartment) => match &worker_command {
                ExplorerCommand::ResolveAncestry {
                    context, location, ..
                } => {
                    let _ = process_ancestry(
                        context,
                        location,
                        &worker_events,
                        &terminals,
                        &worker_gate,
                    );
                }
                ExplorerCommand::EnumerateChildContainers {
                    context,
                    parent,
                    segment_id,
                    menu_generation,
                } => {
                    let _ = process_child_containers(
                        context,
                        parent,
                        *segment_id,
                        *menu_generation,
                        &worker_events,
                        &terminals,
                        &worker_gate,
                    );
                }
                _ => unreachable!("broker accepts breadcrumb commands only"),
            },
            Err(error) => send_breadcrumb_broker_failure(
                &worker_command,
                &worker_events,
                &terminals,
                &worker_gate,
                format!("isolated provider apartment initialization failed: {error}"),
            ),
        },
    );
}

fn start_bounded_breadcrumb_job<F, P>(
    command: &ExplorerCommand,
    events: &SyncSender<ExplorerEvent>,
    terminals: P,
    deadline: Duration,
    job: F,
) where
    F: FnOnce(ExplorerCommand, SyncSender<ExplorerEvent>, Arc<AtomicBool>) + Send + 'static,
    P: RequiredTerminalPublisher,
{
    let Some(context) = command.context().cloned() else {
        return;
    };
    let Some(worker_guard) = reserve_worker(
        &ACTIVE_BREADCRUMB_WORKERS,
        BREADCRUMB_WORKER_CAPACITY,
        &SATURATED_BREADCRUMB_WORKERS,
    ) else {
        tracing::warn!(
            request_id = ?context.request_id,
            tab_id = ?context.tab_id,
            generation = context.generation.value(),
            domain = "breadcrumb-enrichment",
            "isolated worker domain is saturated"
        );
        send_breadcrumb_broker_failure(
            command,
            events,
            &terminals,
            &AtomicBool::new(false),
            "enrichment-provider domain capacity is saturated".to_owned(),
        );
        return;
    };
    let terminal_sent = Arc::new(AtomicBool::new(false));
    let worker_gate = Arc::clone(&terminal_sent);
    let worker_command = command.clone();
    let worker_events = events.clone();
    let worker_terminals = terminals.clone();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let worker_name = format!("breadcrumb-provider-{:?}", context.request_id);
    let worker = thread::Builder::new().name(worker_name).spawn(move || {
        let _worker_guard = worker_guard;
        #[cfg(test)]
        if let Some(gate) = BREADCRUMB_TEST_GATE
            .lock()
            .ok()
            .and_then(|gate| gate.clone())
            .filter(|gate| gate.request_id == context.request_id)
        {
            let _ = gate.started.try_send(());
            while !gate.release.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }
        }
        let panic_command = worker_command.clone();
        let panic_events = worker_events.clone();
        let panic_terminals = worker_terminals.clone();
        let panic_gate = Arc::clone(&worker_gate);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            job(worker_command, worker_events, worker_gate);
        }));
        if let Err(payload) = outcome {
            let message = panic_payload_message(payload.as_ref());
            record_process_error_message(
                ErrorSeverity::Critical,
                "shell",
                "breadcrumb_worker_panic",
                &message,
                Some(file!()),
            );
            send_breadcrumb_broker_failure(
                &panic_command,
                &panic_events,
                &panic_terminals,
                &panic_gate,
                format!("isolated provider worker panicked: {message}"),
            );
        }
        let _ = done_tx.try_send(());
    });
    if let Err(error) = worker {
        send_breadcrumb_broker_failure(
            command,
            events,
            &terminals,
            &terminal_sent,
            format!("could not start isolated provider worker: {error}"),
        );
        return;
    }

    let timeout_command = command.clone();
    let timeout_events = events.clone();
    let timeout_terminals = terminals.clone();
    let timeout_gate = Arc::clone(&terminal_sent);
    if let Err(error) = thread::Builder::new()
        .name("breadcrumb-provider-deadline".to_owned())
        .spawn(move || match done_rx.recv_timeout(deadline) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {}
            Err(RecvTimeoutError::Timeout) => {
                if let Some(context) = timeout_command.context() {
                    context.cancellation.cancel();
                }
                send_breadcrumb_broker_failure(
                    &timeout_command,
                    &timeout_events,
                    &timeout_terminals,
                    &timeout_gate,
                    format!(
                        "provider deadline elapsed after {} ms; worker remains isolated",
                        deadline.as_millis()
                    ),
                );
            }
        })
    {
        context.cancellation.cancel();
        send_breadcrumb_broker_failure(
            command,
            events,
            &terminals,
            &terminal_sent,
            format!("could not start provider deadline coordinator: {error}"),
        );
    }
}

fn send_breadcrumb_broker_failure<P: RequiredTerminalPublisher>(
    command: &ExplorerCommand,
    events: &SyncSender<ExplorerEvent>,
    terminals: &P,
    terminal_sent: &AtomicBool,
    technical_detail: String,
) {
    let error = ExplorerError::new(
        ExplorerErrorKind::Availability,
        "breadcrumb Shell provider",
        true,
        "無法列舉這個位置。請確認位置可用後再試一次。",
        technical_detail,
    );
    record_process_error(
        ErrorSeverity::Error,
        "shell",
        "breadcrumb_provider",
        &error,
        Some(file!()),
    );
    match command {
        ExplorerCommand::ResolveAncestry { context, .. } => {
            let _ = send_ancestry_terminal(
                context,
                BreadcrumbTerminal::Failed(error),
                events,
                terminals,
                terminal_sent,
            );
        }
        ExplorerCommand::EnumerateChildContainers {
            context,
            segment_id,
            menu_generation,
            ..
        } => {
            let _ = send_child_terminal(
                context,
                *segment_id,
                *menu_generation,
                BreadcrumbTerminal::Failed(error),
                events,
                terminals,
                terminal_sent,
            );
        }
        _ => {}
    }
}

fn process_ancestry<P: RequiredTerminalPublisher>(
    context: &explorer_model::RequestContext,
    location: &LocationDescriptor,
    events: &SyncSender<ExplorerEvent>,
    terminals: &P,
    terminal_sent: &AtomicBool,
) -> Result<(), ExplorerError> {
    if context.cancellation.is_cancelled() {
        return send_ancestry_terminal(
            context,
            BreadcrumbTerminal::Cancelled,
            events,
            terminals,
            terminal_sent,
        );
    }
    let mut segments = filesystem_ancestry(location);
    if segments.is_empty() {
        let chain = match crate::navigation::shell_parent_chain(location) {
            Ok(chain) => chain,
            Err(error) => {
                return send_ancestry_terminal(
                    context,
                    BreadcrumbTerminal::Failed(error),
                    events,
                    terminals,
                    terminal_sent,
                );
            }
        };
        segments = shell_ancestry_segments(chain);
    }
    terminals
        .publish_batch(ExplorerEvent::AncestryBatch {
            context: context.clone(),
            segments: segments.clone(),
        })
        .map_err(|()| {
            ExplorerError::new(
                ExplorerErrorKind::Availability,
                "publish breadcrumb ancestry batch",
                true,
                "Folder navigation details are temporarily busy.",
                "ordered breadcrumb event lane is full",
            )
        })?;

    // Publish Shell display metadata as an identity-preserving update batch.
    let mut enriched = segments;
    for segment in &mut enriched {
        if segment.id == BreadcrumbSegmentId(0) || segment.icon_hint == BreadcrumbIconHint::Drive {
            continue;
        }
        if let Ok(resolved) = crate::navigation::resolve_location(&segment.location) {
            segment.display_name = resolved.metadata().display_title;
        }
    }
    terminals
        .publish_batch(ExplorerEvent::AncestryBatch {
            context: context.clone(),
            segments: enriched,
        })
        .map_err(|()| {
            ExplorerError::new(
                ExplorerErrorKind::Availability,
                "publish breadcrumb ancestry batch",
                true,
                "Folder navigation details are temporarily busy.",
                "ordered breadcrumb event lane is full",
            )
        })?;
    send_ancestry_terminal(
        context,
        BreadcrumbTerminal::Finished,
        events,
        terminals,
        terminal_sent,
    )
}

fn shell_ancestry_segments(chain: Vec<(LocationDescriptor, String)>) -> Vec<BreadcrumbSegment> {
    if let Some(archive_index) = chain.iter().position(|(location, _)| {
        location.path().is_some_and(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        })
    }) {
        let mut segments = filesystem_ancestry(&chain[archive_index].0);
        segments.extend(
            chain
                .into_iter()
                .skip(archive_index.saturating_add(1))
                .map(shell_breadcrumb_segment),
        );
        return segments;
    }
    chain.into_iter().map(shell_breadcrumb_segment).collect()
}

fn shell_breadcrumb_segment(
    (location, display_name): (LocationDescriptor, String),
) -> BreadcrumbSegment {
    let this_pc = matches!(
        &location,
        LocationDescriptor::ParsingName(name)
            if name.eq_ignore_ascii_case("shell:MyComputerFolder")
    );
    BreadcrumbSegment {
        id: if this_pc {
            BreadcrumbSegmentId(0)
        } else {
            breadcrumb_id(&location)
        },
        display_name,
        location,
        icon_hint: if this_pc {
            BreadcrumbIconHint::Computer
        } else {
            BreadcrumbIconHint::Namespace
        },
        is_container: true,
    }
}

fn send_ancestry_terminal<P: RequiredTerminalPublisher>(
    context: &explorer_model::RequestContext,
    outcome: BreadcrumbTerminal,
    _events: &SyncSender<ExplorerEvent>,
    terminals: &P,
    terminal_sent: &AtomicBool,
) -> Result<(), ExplorerError> {
    if terminal_sent
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        tracing::warn!(
            request_id = ?context.request_id,
            "ignored late breadcrumb ancestry terminal"
        );
        return Ok(());
    }
    terminals.publish_terminal(ExplorerEvent::AncestryFinished {
        context: context.clone(),
        outcome,
    });
    Ok(())
}

fn process_child_containers<P: RequiredTerminalPublisher>(
    context: &explorer_model::RequestContext,
    parent: &LocationDescriptor,
    segment_id: BreadcrumbSegmentId,
    menu_generation: u64,
    events: &SyncSender<ExplorerEvent>,
    terminals: &P,
    terminal_sent: &AtomicBool,
) -> Result<(), ExplorerError> {
    if context.cancellation.is_cancelled() {
        return send_child_terminal(
            context,
            segment_id,
            menu_generation,
            BreadcrumbTerminal::Cancelled,
            events,
            terminals,
            terminal_sent,
        );
    }
    let mut child_count = 0_usize;
    let completed =
        match crate::navigation::enumerate_child_containers(context, parent, |children| {
            child_count = child_count.saturating_add(children.len());
            terminals
                .publish_batch(ExplorerEvent::ChildContainersBatch {
                    context: context.clone(),
                    segment_id,
                    menu_generation,
                    children,
                })
                .map_err(|()| {
                    ExplorerError::new(
                        ExplorerErrorKind::Availability,
                        "publish navigation child batch",
                        true,
                        "Folder children are temporarily busy.",
                        "ordered breadcrumb event lane is full",
                    )
                })
        }) {
            Ok(completed) => completed,
            Err(error) => {
                return send_child_terminal(
                    context,
                    segment_id,
                    menu_generation,
                    BreadcrumbTerminal::Failed(error),
                    events,
                    terminals,
                    terminal_sent,
                );
            }
        };
    send_child_terminal(
        context,
        segment_id,
        menu_generation,
        if !completed {
            BreadcrumbTerminal::Cancelled
        } else if child_count == 0 {
            BreadcrumbTerminal::Empty
        } else {
            BreadcrumbTerminal::Finished
        },
        events,
        terminals,
        terminal_sent,
    )
}

fn send_child_terminal<P: RequiredTerminalPublisher>(
    context: &explorer_model::RequestContext,
    segment_id: BreadcrumbSegmentId,
    menu_generation: u64,
    outcome: BreadcrumbTerminal,
    _events: &SyncSender<ExplorerEvent>,
    terminals: &P,
    terminal_sent: &AtomicBool,
) -> Result<(), ExplorerError> {
    if terminal_sent
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        tracing::warn!(
            request_id = ?context.request_id,
            menu_generation,
            "ignored late breadcrumb child-container terminal"
        );
        return Ok(());
    }
    terminals.publish_terminal(ExplorerEvent::ChildContainersFinished {
        context: context.clone(),
        segment_id,
        menu_generation,
        outcome,
    });
    Ok(())
}

fn filesystem_ancestry(location: &LocationDescriptor) -> Vec<BreadcrumbSegment> {
    let Some(path) = location.path() else {
        return match location {
            LocationDescriptor::ParsingName(name)
                if name.eq_ignore_ascii_case("shell:MyComputerFolder") =>
            {
                vec![BreadcrumbSegment {
                    id: BreadcrumbSegmentId(0),
                    display_name: "本機".to_owned(),
                    location: location.clone(),
                    icon_hint: BreadcrumbIconHint::Computer,
                    is_container: true,
                }]
            }
            _ => Vec::new(),
        };
    };
    let mut segments = vec![BreadcrumbSegment {
        id: BreadcrumbSegmentId(0),
        display_name: "本機".to_owned(),
        location: LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned()),
        icon_hint: BreadcrumbIconHint::Computer,
        is_container: true,
    }];
    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        let display_name = match component {
            std::path::Component::RootDir => continue,
            _ => component.as_os_str().to_string_lossy().into_owned(),
        };
        let descriptor = LocationDescriptor::file_system(current.clone());
        let mut segment = BreadcrumbSegment {
            id: breadcrumb_id(&descriptor),
            display_name,
            location: descriptor,
            icon_hint: if segments.len() == 1 {
                BreadcrumbIconHint::Drive
            } else if current.extension().is_some_and(|extension| {
                matches!(
                    extension.to_string_lossy().to_ascii_lowercase().as_str(),
                    "zip" | "rar" | "7z" | "tar" | "gz"
                )
            }) {
                BreadcrumbIconHint::Archive
            } else {
                BreadcrumbIconHint::Folder
            },
            is_container: true,
        };
        segment.stabilize_display_name();
        segments.push(segment);
    }
    segments
}

fn breadcrumb_id(location: &LocationDescriptor) -> BreadcrumbSegmentId {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    location.hash(&mut hasher);
    BreadcrumbSegmentId(hasher.finish())
}

fn process_navigation(
    context: &explorer_model::RequestContext,
    location: &LocationDescriptor,
    events: &SyncSender<ExplorerEvent>,
    terminals: &ReliableTerminalPublisher,
) -> Result<(), ExplorerError> {
    if context.cancellation.is_cancelled() {
        return Err(cancelled_error());
    }
    let resolved = crate::navigation::resolve_location(location)?;
    events
        .try_send(ExplorerEvent::LocationResolved {
            context: context.clone(),
            metadata: resolved.metadata(),
        })
        .map_err(|error| event_send_error(&error))?;
    let mut observed_entries = Vec::new();
    let completed = crate::navigation::enumerate_directory(context, &resolved, |event| {
        if let ExplorerEvent::DirectoryBatch { entries, .. } = &event {
            observed_entries.extend(entries.iter().cloned());
        }
        events.try_send(event).is_ok()
    })?;
    if !completed {
        return Err(cancelled_error());
    }
    if let Some(path) = location.path()
        && let Ok(mut index) = explorer_search::LazyIndex::open_default()
    {
        let _ = index.observe_directory(path, &observed_entries);
    }
    terminals.publish(ExplorerEvent::DirectoryFinished {
        context: context.clone(),
    });
    Ok(())
}

fn event_send_error<T>(error: &TrySendError<T>) -> ExplorerError {
    ExplorerError::new(
        ExplorerErrorKind::Availability,
        "publish Shell event",
        true,
        "資料夾更新速度過快，請重新整理。",
        match error {
            TrySendError::Full(_) => "bounded Shell event queue is full",
            TrySendError::Disconnected(_) => "Shell event receiver is disconnected",
        },
    )
}

fn cancelled_error() -> ExplorerError {
    ExplorerError::new(
        ExplorerErrorKind::Cancellation,
        "navigate",
        true,
        "已取消資料夾載入。",
        "request cancellation token was set",
    )
}

impl Drop for ShellStaHandle {
    fn drop(&mut self) {
        let _ = self.shutdown_and_join(DEFAULT_JOIN_TIMEOUT);
    }
}

pub(crate) struct ApartmentGuard(PhantomData<Rc<()>>);

impl ApartmentGuard {
    #[allow(
        unsafe_code,
        reason = "initializing a Windows COM apartment requires the Win32 unsafe API"
    )]
    pub(crate) fn initialize() -> Result<Self, ShellStaError> {
        // SAFETY: The reserved pointer is null as required. This call and the matching
        // CoUninitialize in Drop both run on the dedicated Shell STA thread. The returned
        // HRESULT is checked and preserved in ShellStaError when initialization fails.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        result
            .ok()
            .map_err(|error| ShellStaError::ComInitialization {
                hresult: error.code().0,
            })?;
        // SAFETY: OLE initialization occurs after COM STA initialization on the same thread.
        if let Err(error) = unsafe { OleInitialize(None) } {
            // SAFETY: balances the successful CoInitializeEx when OLE initialization fails.
            unsafe { CoUninitialize() };
            return Err(ShellStaError::ComInitialization {
                hresult: error.code().0,
            });
        }
        Ok(Self(PhantomData))
    }
}

impl Drop for ApartmentGuard {
    #[allow(
        unsafe_code,
        reason = "balancing a successful CoInitializeEx requires the Win32 unsafe API"
    )]
    fn drop(&mut self) {
        // SAFETY: both calls balance successful initialization on this exact STA, in reverse order.
        unsafe {
            OleUninitialize();
            CoUninitialize();
        }
    }
}

#[allow(
    unsafe_code,
    reason = "pumping a Windows thread message queue requires Win32 unsafe APIs"
)]
fn pump_pending_messages() -> bool {
    loop {
        let mut message = MSG::default();
        // SAFETY: `message` is valid writable storage for MSG, the optional HWND is None, and
        // PM_REMOVE transfers each queued message to this thread for immediate dispatch. A false
        // BOOL means that the queue is empty; PeekMessageW does not use it as an error code.
        let has_message =
            unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_REMOVE).as_bool() };
        if !has_message {
            return true;
        }
        if message.message == WM_QUIT {
            return false;
        }
        // SAFETY: `message` was initialized by PeekMessageW for this thread and remains alive
        // throughout translation and dispatch. DispatchMessageW does not retain the pointer.
        // TranslateMessage's BOOL only reports whether a character message was posted, so it is
        // intentionally ignored; DispatchMessageW's result is irrelevant to pump continuation.
        unsafe {
            let _ = TranslateMessage(&raw const message);
            DispatchMessageW(&raw const message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActiveRequest, BREADCRUMB_TEST_GATE, BreadcrumbTestGate, FAIL_NEXT_STA_THREAD_SPAWN,
        FILE_OPERATION_TEST_GATE, FileOperationTestGate, NAVIGATION_TERMINAL_COUNTERS,
        OPERATION_TERMINAL_COUNTERS, ReliableTerminalPublisher, RequiredTerminalLane,
        SEARCH_TEST_GATE, SearchTestGate, ShellDomainDiagnostics, ShellStaEndpointError,
        ShellStaError, ShellStaHandle, ShellStaState, StaResourceSnapshot, TYPED_TERMINAL_COUNTERS,
        TYPED_TERMINAL_RETAIN_CAPACITY, filesystem_ancestry, remove_completed_request,
        send_breadcrumb_broker_failure, shell_ancestry_segments, start_bounded_breadcrumb_job,
        watchable_directory_path,
    };
    use explorer_common::{
        ExplorerError as TestExplorerError, ExplorerErrorKind as TestExplorerErrorKind,
    };
    use explorer_model::{
        BreadcrumbSegmentId, BreadcrumbTerminal, ClipboardMode, ClipboardState, ConflictDecision,
        DataTransferRequest, ExplorerCommand, ExplorerEvent, ExplorerService, ExplorerWindowState,
        FileOperationFlags, FileOperationKind, FileOperationRequest, Generation, HistoryEntry,
        ItemDescriptor, JournalPreimage, JournalValidation, LocationDescriptor, OperationJournal,
        OperationTerminal, PreviewHostCommand, RequestContext, ShellItemId, ShellNewItemRecipe,
        TabId, ViewAnchor,
    };
    use explorer_test_support::{OwnedTempFixture, validate_breadcrumb_contract};
    use std::{
        collections::HashMap,
        fs,
        mem::size_of_val,
        path::PathBuf,
        process::Command,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread,
        time::{Duration, Instant},
    };

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn zip_shell_ancestry_rebuilds_one_filesystem_root_before_namespace_children() {
        let chain = vec![
            (
                LocationDescriptor::ParsingName("::{desktop-root}".to_owned()),
                "本機".to_owned(),
            ),
            (
                LocationDescriptor::file_system(r"C:\Users\fixture\Desktop"),
                "桌面".to_owned(),
            ),
            (
                LocationDescriptor::ParsingName("::{this-pc}".to_owned()),
                "本機".to_owned(),
            ),
            (LocationDescriptor::file_system(r"D:\"), "D:".to_owned()),
            (
                LocationDescriptor::file_system(r"D:\OpenCV-4.13.zip"),
                "OpenCV-4.13.zip".to_owned(),
            ),
            (
                LocationDescriptor::ShellNamespace(vec![1, 0, 0]),
                "OpenCV".to_owned(),
            ),
        ];

        let segments = shell_ancestry_segments(chain);
        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["本機", "D:", "OpenCV-4.13.zip", "OpenCV"]
        );
        assert_eq!(
            segments
                .iter()
                .filter(|segment| segment.id == BreadcrumbSegmentId(0))
                .count(),
            1
        );
        assert_eq!(
            segments[2].icon_hint,
            explorer_model::BreadcrumbIconHint::Archive
        );
    }

    #[test]
    fn configured_real_zip_breadcrumb_has_one_this_pc_root() {
        let Some(path) = std::env::var_os("EXPLORER_REAL_ZIP_FIXTURE") else {
            eprintln!("SKIP: EXPLORER_REAL_ZIP_FIXTURE is not configured");
            return;
        };
        let _apartment = super::ApartmentGuard::initialize().expect("STA");
        let resolved = crate::navigation::resolve_location(&LocationDescriptor::file_system(path))
            .expect("resolve configured ZIP");
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let mut entries = Vec::new();
        crate::navigation::enumerate_directory(&context, &resolved, |event| {
            if let ExplorerEvent::DirectoryBatch { entries: batch, .. } = event {
                entries.extend(batch);
            }
            true
        })
        .expect("enumerate configured ZIP");
        let folder = entries
            .into_iter()
            .find(|entry| entry.is_container)
            .expect("configured ZIP contains a folder");
        let chain = crate::navigation::shell_parent_chain(&folder.location)
            .expect("resolve configured ZIP Shell ancestry");
        let segments = shell_ancestry_segments(chain);

        assert_eq!(
            segments.first().map(|segment| segment.id),
            Some(BreadcrumbSegmentId(0))
        );
        assert_eq!(
            segments
                .iter()
                .filter(|segment| segment.id == BreadcrumbSegmentId(0))
                .count(),
            1
        );
        assert!(
            segments
                .iter()
                .all(|segment| segment.display_name != "桌面")
        );
        assert!(segments.iter().any(|segment| {
            segment
                .location
                .path()
                .and_then(std::path::Path::extension)
                .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        }));
    }

    #[test]
    fn watcher_accepts_directories_and_rejects_zip_files_and_namespaces() {
        let fixture = OwnedTempFixture::new().expect("watch location fixture");
        let directory = fixture.create_dir("folder").expect("directory");
        let archive = fixture.create_file("archive.zip", b"PK").expect("ZIP file");

        assert_eq!(
            watchable_directory_path(&LocationDescriptor::file_system(&directory)),
            Some(directory)
        );
        assert_eq!(
            watchable_directory_path(&LocationDescriptor::file_system(archive)),
            None
        );
        assert_eq!(
            watchable_directory_path(&LocationDescriptor::ParsingName(
                "shell:RecycleBinFolder".to_owned()
            )),
            None
        );
    }

    struct DeniedAclGuard {
        path: PathBuf,
        account: String,
    }

    impl Drop for DeniedAclGuard {
        fn drop(&mut self) {
            let _ = Command::new("icacls")
                .arg(&self.path)
                .args(["/remove:d", &self.account, "/T", "/C", "/Q"])
                .output();
            let _ = Command::new("icacls")
                .arg(&self.path)
                .args(["/inheritance:e", "/T", "/C", "/Q"])
                .output();
        }
    }

    #[test]
    fn hanging_breadcrumb_provider_times_out_without_blocking_and_rejects_late_terminal() {
        let _serial = TEST_LOCK.lock().unwrap();
        let before = StaResourceSnapshot::capture();
        let context = RequestContext::new(TabId::new(), Generation::new(41));
        let command = ExplorerCommand::ResolveAncestry {
            context: context.clone(),
            location: LocationDescriptor::ParsingName("shell:fixture-hanging-provider".to_owned()),
        };
        let (events_tx, events_rx) = mpsc::sync_channel(8);
        let terminal_events = events_tx.clone();
        let started = Instant::now();
        start_bounded_breadcrumb_job(
            &command,
            &events_tx,
            events_tx.clone(),
            Duration::from_millis(25),
            move |command, events, terminal_gate| {
                thread::sleep(Duration::from_millis(350));
                send_breadcrumb_broker_failure(
                    &command,
                    &events,
                    &terminal_events,
                    &terminal_gate,
                    "late fixture provider terminal".to_owned(),
                );
            },
        );
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "broker submission blocked the caller"
        );
        let terminal = events_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("bounded recoverable terminal");
        assert!(matches!(
            terminal,
            ExplorerEvent::AncestryFinished {
                outcome: BreadcrumbTerminal::Failed(ref error),
                ..
            } if error.recoverable
        ));
        assert!(context.cancellation.is_cancelled());
        thread::sleep(Duration::from_millis(400));
        assert!(
            events_rx.try_recv().is_err(),
            "late terminal escaped the gate"
        );
        let after = StaResourceSnapshot::capture();
        assert_eq!(
            after.active_breadcrumb_workers,
            before.active_breadcrumb_workers
        );
    }

    #[test]
    fn filesystem_ancestry_is_immediate_for_roots_long_unicode_and_reparse_paths() {
        let root = filesystem_ancestry(&LocationDescriptor::file_system(r"D:\"));
        assert_eq!(
            root.first().map(|segment| segment.display_name.as_str()),
            Some("本機")
        );
        assert_eq!(
            root.last().map(|segment| segment.display_name.as_str()),
            Some("D:")
        );

        let long_leaf = "很長的資料夾名稱".repeat(24);
        let long_path = PathBuf::from(r"D:\資料")
            .join("重新解析點")
            .join(&long_leaf);
        assert!(long_path.as_os_str().len() > 260);
        let long = filesystem_ancestry(&LocationDescriptor::file_system(&long_path));
        assert_eq!(
            long.last().map(|segment| segment.display_name.as_str()),
            Some(long_leaf.as_str())
        );
        assert!(long.iter().all(|segment| segment.is_container));
        assert_eq!(
            long.iter()
                .map(|segment| segment.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            long.len(),
            "every early segment keeps a stable unique identity"
        );

        let unc = filesystem_ancestry(&LocationDescriptor::file_system(
            r"\\server\共享\Unicode 子資料夾",
        ));
        assert_eq!(
            unc.last().map(|segment| segment.display_name.as_str()),
            Some("Unicode 子資料夾")
        );

        let fixture = OwnedTempFixture::new().expect("reparse fixture");
        let target = fixture.create_dir("target").expect("reparse target");
        let link = fixture.root().join("junction-link");
        if std::os::windows::fs::symlink_dir(&target, &link).is_ok() {
            let reparse = filesystem_ancestry(&LocationDescriptor::file_system(&link));
            assert_eq!(
                reparse.last().map(|segment| segment.display_name.as_str()),
                Some("junction-link")
            );
        }
    }

    #[test]
    fn real_breadcrumb_protocol_resolves_owned_ancestry_and_direct_containers() {
        let _serial = TEST_LOCK.lock().unwrap();
        let fixture = OwnedTempFixture::new_in(r"D:\").expect("D fixture");
        let child = fixture.create_dir("子資料夾").expect("child folder");
        let _nested = fixture
            .create_dir(r"子資料夾\nested")
            .expect("nested folder");
        let _file = fixture
            .create_file("plain.txt", b"file")
            .expect("plain file");
        let sta = ShellStaHandle::start().expect("STA starts");
        let tab_id = TabId::new();
        let generation = Generation::new(1);

        let ancestry_context = RequestContext::new(tab_id, generation);
        sta.submit(ExplorerCommand::ResolveAncestry {
            context: ancestry_context.clone(),
            location: LocationDescriptor::file_system(&child),
        })
        .expect("submit ancestry");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut ancestry = Vec::new();
        let mut ancestry_batches = Vec::new();
        let mut ancestry_terminal_count = 0;
        loop {
            if let Some(event) = sta.try_recv().expect("ancestry receive") {
                match event {
                    ExplorerEvent::AncestryBatch { context, segments }
                        if context.request_id == ancestry_context.request_id =>
                    {
                        ancestry = segments.clone();
                        ancestry_batches.push(segments);
                    }
                    ExplorerEvent::AncestryFinished { context, outcome }
                        if context.request_id == ancestry_context.request_id =>
                    {
                        assert_eq!(outcome, BreadcrumbTerminal::Finished);
                        ancestry_terminal_count += 1;
                        break;
                    }
                    _ => {}
                }
            }
            assert!(Instant::now() < deadline, "ancestry timed out");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            ancestry.first().map(|segment| segment.id),
            Some(BreadcrumbSegmentId(0))
        );
        assert_eq!(
            ancestry.last().map(|segment| segment.location.path()),
            Some(Some(child.as_path()))
        );
        for batch in &ancestry_batches {
            let drive = batch
                .iter()
                .find(|segment| segment.icon_hint == explorer_model::BreadcrumbIconHint::Drive)
                .expect("every filesystem ancestry batch retains its drive segment");
            assert_eq!(
                drive.display_name, "D:",
                "Shell enrichment must not replace the stable drive designator with a volume label"
            );
        }

        let menu_context = RequestContext::new(tab_id, generation);
        sta.submit(ExplorerCommand::EnumerateChildContainers {
            context: menu_context.clone(),
            parent: LocationDescriptor::file_system(fixture.root()),
            segment_id: BreadcrumbSegmentId(9),
            menu_generation: 4,
        })
        .expect("submit menu");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut children = Vec::new();
        let mut child_batches = Vec::new();
        let mut child_terminal_count = 0;
        loop {
            if let Some(event) = sta.try_recv().expect("menu receive") {
                match event {
                    ExplorerEvent::ChildContainersBatch {
                        context,
                        children: batch,
                        ..
                    } if context.request_id == menu_context.request_id => {
                        children.extend(batch.clone());
                        child_batches.push(batch);
                    }
                    ExplorerEvent::ChildContainersFinished {
                        context, outcome, ..
                    } if context.request_id == menu_context.request_id => {
                        assert_eq!(outcome, BreadcrumbTerminal::Finished);
                        child_terminal_count += 1;
                        break;
                    }
                    _ => {}
                }
            }
            assert!(Instant::now() < deadline, "menu timed out");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(children.len(), 1, "only the direct folder is returned");
        assert_eq!(children[0].display_name, "子資料夾");
        validate_breadcrumb_contract(
            &ancestry_batches,
            &LocationDescriptor::file_system(fixture.root()),
            &child_batches,
            ancestry_terminal_count,
            child_terminal_count,
            true,
        )
        .expect("real provider satisfies shared breadcrumb contract");

        let drives_context = RequestContext::new(tab_id, generation);
        sta.submit(ExplorerCommand::EnumerateChildContainers {
            context: drives_context.clone(),
            parent: LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned()),
            segment_id: BreadcrumbSegmentId(0),
            menu_generation: 5,
        })
        .expect("submit This PC menu");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut drives = Vec::new();
        loop {
            if let Some(event) = sta.try_recv().expect("drive menu receive") {
                match event {
                    ExplorerEvent::ChildContainersBatch {
                        context,
                        children: batch,
                        ..
                    } if context.request_id == drives_context.request_id => drives.extend(batch),
                    ExplorerEvent::ChildContainersFinished {
                        context, outcome, ..
                    } if context.request_id == drives_context.request_id => {
                        assert_eq!(outcome, BreadcrumbTerminal::Finished);
                        break;
                    }
                    _ => {}
                }
            }
            assert!(Instant::now() < deadline, "This PC menu timed out");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            drives
                .iter()
                .any(|drive| { drive.location.path() == Some(std::path::Path::new(r"D:\")) })
        );
        sta.shutdown_and_join(Duration::from_secs(5))
            .expect("STA stops");
    }

    #[test]
    fn breadcrumb_protocol_emits_typed_cancel_and_failure_terminals_exactly_once() {
        let _serial = TEST_LOCK.lock().unwrap();
        let sta = ShellStaHandle::start().expect("STA starts");
        let tab_id = TabId::new();
        let cancelled = RequestContext::new(tab_id, Generation::new(1));
        cancelled.cancellation.cancel();
        sta.submit(ExplorerCommand::ResolveAncestry {
            context: cancelled.clone(),
            location: LocationDescriptor::file_system(r"D:\test"),
        })
        .expect("submit cancelled ancestry");
        let failed = RequestContext::new(tab_id, Generation::new(2));
        sta.submit(ExplorerCommand::ResolveAncestry {
            context: failed.clone(),
            location: LocationDescriptor::ParsingName(
                "shell:CodexExplorerDefinitelyMissingNamespace".to_owned(),
            ),
        })
        .expect("submit invalid ancestry");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut cancel_terminals = 0;
        let mut failure_terminals = 0;
        while cancel_terminals == 0 || failure_terminals == 0 {
            if let Some(event) = sta.try_recv().expect("receive terminal")
                && let ExplorerEvent::AncestryFinished { context, outcome } = event
            {
                if context.request_id == cancelled.request_id {
                    assert_eq!(outcome, BreadcrumbTerminal::Cancelled);
                    cancel_terminals += 1;
                } else if context.request_id == failed.request_id {
                    assert!(matches!(outcome, BreadcrumbTerminal::Failed(_)));
                    failure_terminals += 1;
                }
            }
            assert!(Instant::now() < deadline, "breadcrumb terminal timed out");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(cancel_terminals, 1);
        assert_eq!(failure_terminals, 1);
        sta.shutdown_and_join(Duration::from_secs(5))
            .expect("STA stops");
    }

    fn operation_item(path: &std::path::Path, id: u8) -> ItemDescriptor {
        ItemDescriptor {
            id: ShellItemId::from_provider_bytes(vec![id]).expect("non-empty test identity"),
            location: LocationDescriptor::file_system(path),
        }
    }

    fn real_operation_item(path: &std::path::Path) -> ItemDescriptor {
        let is_directory = fs::metadata(path).expect("real item metadata").is_dir();
        let identity = crate::navigation::filesystem_identity(path, is_directory)
            .expect("real filesystem identity");
        ItemDescriptor {
            id: ShellItemId::from_provider_bytes(identity).expect("non-empty real identity"),
            location: LocationDescriptor::file_system(path),
        }
    }

    fn explorer_paste_from_clipboard(destination: &std::path::Path, expected: &std::path::Path) {
        let script = r"
$destination=$env:EXPLORER_TEST_DESTINATION; $expected=$env:EXPLORER_TEST_EXPECTED
$shell=New-Object -ComObject Shell.Application
$folder=$shell.Namespace($destination); if($null -eq $folder){ exit 2 }
$deadline=(Get-Date).AddSeconds(12)
do {
  $folder.Self.InvokeVerb('paste')
  $attemptDeadline=(Get-Date).AddMilliseconds(750)
  do { Start-Sleep -Milliseconds 50 } while(-not (Test-Path -LiteralPath $expected) -and (Get-Date) -lt $attemptDeadline)
} while(-not (Test-Path -LiteralPath $expected) -and (Get-Date) -lt $deadline)
if(Test-Path -LiteralPath $expected){ exit 0 } else { exit 4 }
";
        let output = Command::new("powershell.exe")
            .env(
                "EXPLORER_TEST_DESTINATION",
                crate::navigation::shell_path_text(destination),
            )
            .env(
                "EXPLORER_TEST_EXPECTED",
                crate::navigation::shell_path_text(expected),
            )
            .args(["-STA", "-NoProfile", "-Command", script])
            .output()
            .expect("Explorer paste automation");
        assert!(
            output.status.success(),
            "Explorer paste failed: status={:?} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn explorer_copy_to_clipboard(source: &std::path::Path) -> bool {
        explorer_transfer_to_clipboard(std::slice::from_ref(&source), false)
    }

    fn explorer_transfer_to_clipboard(sources: &[&std::path::Path], cut: bool) -> bool {
        let Some(parent) = sources.first().and_then(|source| source.parent()) else {
            return false;
        };
        if sources.iter().any(|source| source.parent() != Some(parent)) {
            return false;
        }
        let names = sources
            .iter()
            .filter_map(|source| source.file_name())
            .map(|name| name.to_string_lossy())
            .collect::<Vec<_>>()
            .join("|");
        let script = r#"
$parent=$env:EXPLORER_TEST_PARENT; $names=$env:EXPLORER_TEST_NAMES -split '\|'; $cut=$env:EXPLORER_TEST_CUT -eq '1'
$shell=New-Object -ComObject Shell.Application
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class ExplorerTransferNative { [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetForegroundWindow(IntPtr hwnd); [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, IntPtr pid); [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId(); [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool AttachThreadInput(uint a,uint b,bool attach); [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool ShowWindow(IntPtr hwnd,int command); [DllImport("user32.dll")] public static extern void keybd_event(byte key,byte scan,uint flags,UIntPtr extra); public static bool ForceForeground(IntPtr hwnd) { var foreground=GetForegroundWindow(); var foregroundThread=GetWindowThreadProcessId(foreground,IntPtr.Zero); var currentThread=GetCurrentThreadId(); var attached=foregroundThread!=0 && foregroundThread!=currentThread && AttachThreadInput(currentThread,foregroundThread,true); try { ShowWindow(hwnd,5); keybd_event(0x12,0,0,UIntPtr.Zero); SetForegroundWindow(hwnd); keybd_event(0x12,0,2,UIntPtr.Zero); return GetForegroundWindow()==hwnd; } finally { if(attached) AttachThreadInput(currentThread,foregroundThread,false); } } }'
$before=@($shell.Windows()) | ForEach-Object { $_.HWND }
Start-Process explorer.exe -ArgumentList "/separate,`"$parent`""
$deadline=(Get-Date).AddSeconds(8); $window=$null
do { Start-Sleep -Milliseconds 100; $window=@($shell.Windows()) | Where-Object { try { $_.HWND -notin $before -and $_.Document.Folder.Self.Path -eq $parent } catch {} } | Select-Object -First 1 } while($null -eq $window -and (Get-Date) -lt $deadline)
if($null -eq $window){ exit 2 }
for($i=0; $i -lt $names.Count; $i++){ $item=$window.Document.Folder.ParseName($names[$i]); if($null -eq $item){ $window.Quit(); exit 3 }; $flags=if($i -eq 0){29}else{9}; $window.Document.SelectItem($item,$flags) }
$keys=New-Object -ComObject WScript.Shell
if(-not [ExplorerTransferNative]::ForceForeground([IntPtr]$window.HWND)){ $window.Quit(); exit 4 }
Start-Sleep -Milliseconds 250
for($i=0; $i -lt $names.Count; $i++){ $item=$window.Document.Folder.ParseName($names[$i]); $flags=if($i -eq 0){29}else{9}; $window.Document.SelectItem($item,$flags) }
Start-Sleep -Milliseconds 250; if($cut){$keys.SendKeys('^x')}else{$keys.SendKeys('^c')}; Start-Sleep -Milliseconds 1000
Add-Type -AssemblyName System.Windows.Forms
$ok=[Windows.Forms.Clipboard]::ContainsFileDropList() -and [Windows.Forms.Clipboard]::GetFileDropList().Count -eq $names.Count; $window.Quit(); if($ok){ exit 0 } else { exit 5 }
"#;
        let output = Command::new("powershell.exe")
            .env(
                "EXPLORER_TEST_PARENT",
                crate::navigation::shell_path_text(parent),
            )
            .env("EXPLORER_TEST_NAMES", names)
            .env("EXPLORER_TEST_CUT", if cut { "1" } else { "0" })
            .args(["-STA", "-NoProfile", "-Command", script])
            .output()
            .expect("Explorer copy automation");
        if !output.status.success() {
            eprintln!(
                "Explorer transfer automation failed: status={:?} stdout={} stderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        output.status.success()
    }

    fn run_file_operation(sta: &ShellStaHandle, kind: FileOperationKind) -> OperationTerminal {
        run_file_operation_with_flags(
            sta,
            kind,
            FileOperationFlags {
                conflict: ConflictDecision::Replace,
                ..FileOperationFlags::default()
            },
        )
    }

    fn run_file_operation_with_flags(
        sta: &ShellStaHandle,
        kind: FileOperationKind,
        flags: FileOperationFlags,
    ) -> OperationTerminal {
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let request_id = context.request_id;
        sta.submit(ExplorerCommand::ExecuteFileOperation {
            context,
            request: FileOperationRequest { kind, flags },
        })
        .expect("submit file operation");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(ExplorerEvent::OperationFinished { context, outcome }) =
                sta.try_recv_event().expect("receive operation event")
                && context.request_id == request_id
            {
                return outcome;
            }
            assert!(Instant::now() < deadline, "file operation timed out");
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn run_data_transfer(
        sta: &ShellStaHandle,
        tab_id: TabId,
        request: DataTransferRequest,
    ) -> (OperationTerminal, Vec<ClipboardState>) {
        let context = RequestContext::new(tab_id, Generation::new(1));
        let request_id = context.request_id;
        sta.submit(ExplorerCommand::DataTransfer { context, request })
            .expect("submit data transfer");
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut clipboard_states = Vec::new();
        loop {
            if let Some(event) = sta.try_recv_event().expect("data transfer event") {
                match event {
                    ExplorerEvent::ClipboardChanged { state } => clipboard_states.push(state),
                    ExplorerEvent::OperationFinished { context, outcome }
                        if context.request_id == request_id =>
                    {
                        return (outcome, clipboard_states);
                    }
                    _ => {}
                }
            }
            assert!(Instant::now() < deadline, "data transfer timed out");
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn drive_window_location(
        sta: &ShellStaHandle,
        window: &mut ExplorerWindowState,
        tab_id: TabId,
        location: LocationDescriptor,
        refresh: bool,
    ) -> RequestContext {
        let tab = window.tab_mut(tab_id).expect("E2E tab");
        let context = if refresh {
            tab.begin_refresh_request().expect("refresh generation")
        } else {
            tab.begin_navigation_request()
                .expect("navigation generation")
        };
        drive_correlated_navigation(sta, window, context, location, refresh)
    }

    fn drive_correlated_navigation(
        sta: &ShellStaHandle,
        window: &mut ExplorerWindowState,
        context: RequestContext,
        location: LocationDescriptor,
        refresh: bool,
    ) -> RequestContext {
        let command = if refresh {
            ExplorerCommand::Refresh {
                context: context.clone(),
                location,
            }
        } else {
            ExplorerCommand::Navigate {
                context: context.clone(),
                location,
            }
        };
        sta.submit(command).expect("submit E2E navigation");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(event) = sta.try_recv_event().expect("E2E navigation event") {
                let terminal = matches!(
                    &event,
                    ExplorerEvent::DirectoryFinished { context: event_context }
                        if event_context.request_id == context.request_id
                );
                let _ = window.apply_event(event);
                if terminal {
                    return context;
                }
            }
            assert!(Instant::now() < deadline, "E2E navigation timed out");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn file_operation_isolation_keeps_navigation_available_before_release() {
        struct TestGateReset(Arc<AtomicBool>);
        impl Drop for TestGateReset {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
                if let Ok(mut gate) = FILE_OPERATION_TEST_GATE.lock() {
                    gate.take();
                }
            }
        }

        let _serial = TEST_LOCK.lock().unwrap();
        let workers_before = StaResourceSnapshot::capture().active_file_operation_workers;
        let fixture = OwnedTempFixture::new().expect("background operation fixture");
        let source = fixture
            .create_file("copy-source.bin", b"background-copy")
            .expect("copy source");
        let destination = fixture.create_dir("destination").expect("destination");
        let expected = destination.join("copy-source.bin");
        let operation_context = RequestContext::new(TabId::new(), Generation::new(1));
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let release = Arc::new(AtomicBool::new(false));
        *FILE_OPERATION_TEST_GATE.lock().unwrap() = Some(FileOperationTestGate {
            request_id: operation_context.request_id,
            started: started_tx,
            release: Arc::clone(&release),
        });
        let _gate_reset = TestGateReset(Arc::clone(&release));

        let sta = ShellStaHandle::start().expect("start STA");
        sta.submit(ExplorerCommand::ExecuteFileOperation {
            context: operation_context.clone(),
            request: FileOperationRequest {
                kind: FileOperationKind::Copy {
                    items: vec![real_operation_item(&source)],
                    destination: LocationDescriptor::file_system(&destination),
                },
                flags: FileOperationFlags {
                    conflict: ConflictDecision::Replace,
                    ..FileOperationFlags::default()
                },
            },
        })
        .expect("submit background copy");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("background worker started");

        let navigation_context = RequestContext::new(TabId::new(), Generation::new(1));
        sta.submit(ExplorerCommand::Navigate {
            context: navigation_context.clone(),
            location: LocationDescriptor::file_system(fixture.root()),
        })
        .expect("submit navigation while copy is pending");
        let navigation_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(event) = sta.try_recv_event().expect("navigation event") {
                match event {
                    ExplorerEvent::DirectoryFinished { context }
                        if context.request_id == navigation_context.request_id =>
                    {
                        break;
                    }
                    ExplorerEvent::OperationFinished { context, .. }
                        if context.request_id == operation_context.request_id =>
                    {
                        panic!("copy completed before the test released its background worker")
                    }
                    _ => {}
                }
            }
            assert!(
                Instant::now() < navigation_deadline,
                "navigation was blocked behind the background copy; diagnostics={:?}",
                ShellDomainDiagnostics::capture()
            );
            thread::sleep(Duration::from_millis(2));
        }

        release.store(true, Ordering::Release);
        let operation_deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(ExplorerEvent::OperationFinished { context, outcome }) =
                sta.try_recv_event().expect("operation event")
                && context.request_id == operation_context.request_id
            {
                assert_eq!(outcome, OperationTerminal::Finished);
                break;
            }
            assert!(
                Instant::now() < operation_deadline,
                "background copy did not finish"
            );
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            fs::read(expected).expect("copied bytes"),
            b"background-copy"
        );
        let worker_deadline = Instant::now() + Duration::from_secs(2);
        while StaResourceSnapshot::capture().active_file_operation_workers != workers_before
            && Instant::now() < worker_deadline
        {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            StaResourceSnapshot::capture().active_file_operation_workers,
            workers_before,
            "background operation worker leaked"
        );
        sta.shutdown_and_join(Duration::from_secs(5))
            .expect("STA stops");
    }

    #[test]
    fn cancelled_file_operation_isolation_records_stale_completion_diagnostics() {
        struct TestGateReset(Arc<AtomicBool>);
        impl Drop for TestGateReset {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
                if let Ok(mut gate) = FILE_OPERATION_TEST_GATE.lock() {
                    gate.take();
                }
            }
        }

        let _serial = TEST_LOCK.lock().unwrap();
        let before = ShellDomainDiagnostics::capture().stale_cancelled_completions;
        let fixture = OwnedTempFixture::new().expect("cancelled operation fixture");
        let source = fixture
            .create_file("copy-source.bin", b"background-copy")
            .expect("copy source");
        let destination = fixture.create_dir("destination").expect("destination");
        let operation_context = RequestContext::new(TabId::new(), Generation::new(1));
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let release = Arc::new(AtomicBool::new(false));
        *FILE_OPERATION_TEST_GATE.lock().unwrap() = Some(FileOperationTestGate {
            request_id: operation_context.request_id,
            started: started_tx,
            release: Arc::clone(&release),
        });
        let _gate_reset = TestGateReset(Arc::clone(&release));

        let sta = ShellStaHandle::start().expect("start STA");
        sta.submit(ExplorerCommand::ExecuteFileOperation {
            context: operation_context.clone(),
            request: FileOperationRequest {
                kind: FileOperationKind::Copy {
                    items: vec![real_operation_item(&source)],
                    destination: LocationDescriptor::file_system(&destination),
                },
                flags: FileOperationFlags::default(),
            },
        })
        .expect("submit background copy");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("background worker started");
        sta.submit(ExplorerCommand::Cancel {
            request_id: operation_context.request_id,
        })
        .expect("cancel held operation");
        assert!(operation_context.cancellation.is_cancelled());

        release.store(true, Ordering::Release);
        let completion_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(ExplorerEvent::OperationFinished { context, .. }) =
                sta.try_recv_event().expect("operation completion")
                && context.request_id == operation_context.request_id
            {
                break;
            }
            assert!(
                Instant::now() < completion_deadline,
                "cancelled operation did not complete; diagnostics={:?}",
                ShellDomainDiagnostics::capture()
            );
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            ShellDomainDiagnostics::capture().stale_cancelled_completions > before,
            "cancelled completion was not diagnosed; diagnostics={:?}",
            ShellDomainDiagnostics::capture()
        );
        sta.shutdown_and_join(Duration::from_secs(5))
            .expect("STA stops");
    }

    #[test]
    fn active_search_worker_shutdown_is_bounded_and_releases_its_lifecycle() {
        struct TestGateReset(Arc<AtomicBool>);
        impl Drop for TestGateReset {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
                if let Ok(mut gate) = SEARCH_TEST_GATE.lock() {
                    gate.take();
                }
            }
        }

        let _serial = TEST_LOCK.lock().unwrap();
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let release = Arc::new(AtomicBool::new(false));
        *SEARCH_TEST_GATE.lock().unwrap() = Some(SearchTestGate {
            request_id: context.request_id,
            started: started_tx,
            release: Arc::clone(&release),
        });
        let _gate_reset = TestGateReset(Arc::clone(&release));
        let sta = ShellStaHandle::start().expect("start STA");
        sta.submit(ExplorerCommand::StartSearch {
            context: context.clone(),
            location: LocationDescriptor::file_system(r"C:\definitely-missing-search-fixture"),
            input: explorer_model::SearchInput::new("held"),
        })
        .expect("submit held search");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("search worker started");
        sta.shutdown();
        assert!(context.cancellation.is_cancelled());
        release.store(true, Ordering::Release);
        sta.shutdown_and_join(Duration::from_secs(5))
            .expect("managed search worker stops");
    }

    #[test]
    fn rejected_submission_removes_its_active_request_tracking() {
        let _serial = TEST_LOCK.lock().unwrap();
        let sta = ShellStaHandle::start().expect("start STA");
        sta.shutdown_and_join(Duration::from_secs(5))
            .expect("stop STA");
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        assert!(matches!(
            sta.submit(ExplorerCommand::Navigate {
                context: context.clone(),
                location: LocationDescriptor::file_system(r"C:\disconnected-request"),
            }),
            Err(ShellStaEndpointError::CommandEndpointDisconnected)
        ));
        assert!(
            !sta.active_requests
                .lock()
                .expect("active request map")
                .contains_key(&context.request_id),
            "rejected request leaked active tracking"
        );
    }

    #[test]
    fn timed_out_search_join_is_retried_after_the_sta_has_stopped() {
        struct TestGateReset(Arc<AtomicBool>);
        impl Drop for TestGateReset {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
                if let Ok(mut gate) = SEARCH_TEST_GATE.lock() {
                    gate.take();
                }
            }
        }

        let _serial = TEST_LOCK.lock().unwrap();
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let release = Arc::new(AtomicBool::new(false));
        *SEARCH_TEST_GATE.lock().unwrap() = Some(SearchTestGate {
            request_id: context.request_id,
            started: started_tx,
            release: Arc::clone(&release),
        });
        let _gate_reset = TestGateReset(Arc::clone(&release));
        let sta = ShellStaHandle::start().expect("start STA");
        sta.submit(ExplorerCommand::StartSearch {
            context,
            location: LocationDescriptor::file_system(r"C:\held-search-join"),
            input: explorer_model::SearchInput::new("held"),
        })
        .expect("submit held search");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("search worker started");
        assert!(matches!(
            sta.shutdown_and_join(Duration::from_millis(10)),
            Err(ShellStaError::JoinTimeout { .. })
        ));
        release.store(true, Ordering::Release);
        sta.shutdown_and_join(Duration::from_secs(5))
            .expect("retry joins the retained search worker");
    }

    #[test]
    fn saturated_operation_progress_lane_preserves_operation_and_navigation_terminals() {
        let operation_context = RequestContext::new(TabId::new(), Generation::new(1));
        let navigation_context = RequestContext::new(TabId::new(), Generation::new(2));
        let (terminal_tx, terminal_rx) = mpsc::sync_channel(1);
        let (navigation_tx, navigation_rx) = mpsc::sync_channel(1);
        let (progress_tx, progress_rx) = mpsc::sync_channel(1);
        let (search_tx, search_rx) = mpsc::sync_channel(1);
        let (enrichment_tx, enrichment_rx) = mpsc::sync_channel(1);

        progress_tx
            .try_send(ExplorerEvent::OperationProgress {
                context: operation_context.clone(),
                progress: explorer_model::OperationProgress {
                    completed_items: 1,
                    total_items: 2,
                    completed_bytes: 1,
                    total_bytes: Some(2),
                },
            })
            .expect("fill progress lane");
        assert!(
            progress_tx
                .try_send(ExplorerEvent::OperationProgress {
                    context: operation_context.clone(),
                    progress: explorer_model::OperationProgress {
                        completed_items: 2,
                        total_items: 2,
                        completed_bytes: 2,
                        total_bytes: Some(2),
                    },
                })
                .is_err(),
            "progress lane must saturate independently"
        );
        navigation_tx
            .try_send(ExplorerEvent::DirectoryFinished {
                context: navigation_context.clone(),
            })
            .expect("navigation terminal retains its lane");
        terminal_tx
            .try_send(ExplorerEvent::OperationFinished {
                context: operation_context.clone(),
                outcome: OperationTerminal::Finished,
            })
            .expect("operation terminal retains its lane");

        let receive = || {
            terminal_rx
                .try_recv()
                .or_else(|_| navigation_rx.try_recv())
                .or_else(|_| progress_rx.try_recv())
                .or_else(|_| search_rx.try_recv())
                .or_else(|_| enrichment_rx.try_recv())
        };
        assert!(matches!(
            receive().expect("operation terminal is highest priority"),
            ExplorerEvent::OperationFinished { context, .. } if context.request_id == operation_context.request_id
        ));
        assert!(matches!(
            receive().expect("navigation terminal remains available"),
            ExplorerEvent::DirectoryFinished { context } if context.request_id == navigation_context.request_id
        ));
        drop(search_tx);
        drop(enrichment_tx);
    }

    #[test]
    fn full_navigation_terminal_lane_eventually_delivers_typed_failure_and_cleans_active_request() {
        let (publisher, receiver) =
            ReliableTerminalPublisher::channel(1, 1, &NAVIGATION_TERMINAL_COUNTERS);
        let filler = RequestContext::new(TabId::new(), Generation::new(1));
        let context = RequestContext::new(TabId::new(), Generation::new(2));
        let active_requests = Mutex::new(HashMap::from([(
            context.request_id,
            ActiveRequest {
                cancellation: context.cancellation.clone(),
                required_terminal_lane: Some(RequiredTerminalLane::Navigation),
            },
        )]));

        publisher.publish(ExplorerEvent::DirectoryFinished { context: filler });
        publisher.publish(ExplorerEvent::Failed {
            context: context.clone(),
            error: TestExplorerError::new(
                TestExplorerErrorKind::Internal,
                "navigation terminal test",
                true,
                "Navigation failed.",
                "primary navigation lane deliberately full",
            ),
        });

        let terminal = receiver
            .try_recv()
            .expect("receive retained navigation terminal");
        assert!(
            matches!(
                &terminal,
                ExplorerEvent::Failed { context: terminal_context, .. }
                    if terminal_context.request_id == context.request_id
            ),
            "retained navigation terminal was not delivered; diagnostics={:?}",
            ShellDomainDiagnostics::capture()
        );
        remove_completed_request(&active_requests, &terminal).expect("clean active navigation");
        assert!(
            !active_requests
                .lock()
                .expect("active navigation map")
                .contains_key(&context.request_id),
            "retained navigation terminal must release active request tracking"
        );
        assert!(matches!(
            receiver
                .try_recv()
                .expect("receive filled primary terminal after retained terminal"),
            ExplorerEvent::DirectoryFinished { .. }
        ));
    }

    #[test]
    fn full_operation_terminal_lane_eventually_delivers_operation_finished_and_cleans_active_request()
     {
        let (publisher, receiver) =
            ReliableTerminalPublisher::channel(1, 1, &OPERATION_TERMINAL_COUNTERS);
        let filler = RequestContext::new(TabId::new(), Generation::new(1));
        let context = RequestContext::new(TabId::new(), Generation::new(2));
        let active_requests = Mutex::new(HashMap::from([(
            context.request_id,
            ActiveRequest {
                cancellation: context.cancellation.clone(),
                required_terminal_lane: Some(RequiredTerminalLane::Operation),
            },
        )]));

        publisher.publish(ExplorerEvent::OperationFinished {
            context: filler,
            outcome: OperationTerminal::Finished,
        });
        publisher.publish(ExplorerEvent::OperationFinished {
            context: context.clone(),
            outcome: OperationTerminal::Failed(TestExplorerError::new(
                TestExplorerErrorKind::Internal,
                "operation terminal test",
                true,
                "The operation failed.",
                "primary operation terminal lane deliberately full",
            )),
        });

        let terminal = receiver
            .try_recv()
            .expect("receive retained operation terminal");
        assert!(
            matches!(
                &terminal,
                ExplorerEvent::OperationFinished { context: terminal_context, outcome: OperationTerminal::Failed(_) }
                    if terminal_context.request_id == context.request_id
            ),
            "retained operation terminal was not delivered; diagnostics={:?}",
            ShellDomainDiagnostics::capture()
        );
        remove_completed_request(&active_requests, &terminal).expect("clean active operation");
        assert!(
            !active_requests
                .lock()
                .expect("active operation map")
                .contains_key(&context.request_id),
            "retained operation terminal must release active request tracking"
        );
        assert!(matches!(
            receiver
                .try_recv()
                .expect("receive filled primary terminal after retained terminal"),
            ExplorerEvent::OperationFinished { .. }
        ));
    }

    #[test]
    fn ordered_request_lane_delivers_batches_before_a_retained_terminal() {
        let (publisher, receiver) =
            ReliableTerminalPublisher::ordered_channel(1, 2, &TYPED_TERMINAL_COUNTERS);
        let events = publisher.primary();
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        events
            .try_send(ExplorerEvent::DirectoryBatch {
                context: RequestContext::new(TabId::new(), Generation::new(1)),
                entries: Vec::new(),
            })
            .expect("fill ordered primary lane");
        publisher
            .try_publish_batch(ExplorerEvent::ChildContainersBatch {
                context: context.clone(),
                segment_id: BreadcrumbSegmentId(42),
                menu_generation: 7,
                children: Vec::new(),
            })
            .expect("retain visible child batch");
        publisher.publish(ExplorerEvent::ChildContainersFinished {
            context: context.clone(),
            segment_id: BreadcrumbSegmentId(42),
            menu_generation: 7,
            outcome: BreadcrumbTerminal::Finished,
        });

        assert!(matches!(
            receiver.try_recv().expect("primary filler"),
            ExplorerEvent::DirectoryBatch { .. }
        ));
        assert!(matches!(
            receiver.try_recv().expect("retained child batch"),
            ExplorerEvent::ChildContainersBatch { context: batch, .. }
                if batch.request_id == context.request_id
        ));
        assert!(matches!(
            receiver.try_recv().expect("terminal after batches"),
            ExplorerEvent::ChildContainersFinished { context: terminal, .. }
                if terminal.request_id == context.request_id
        ));
    }

    #[test]
    fn real_sta_rejects_typed_terminal_work_before_retained_capacity_can_overflow() {
        let _serial = TEST_LOCK.lock().expect("lock STA tests");
        let sta = ShellStaHandle::start().expect("start STA");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let context = RequestContext::new(TabId::new(), Generation::new(1));
            match sta.submit(ExplorerCommand::PreviewHost {
                context,
                command: PreviewHostCommand::Unload {
                    generation: Generation::new(1),
                },
            }) {
                Ok(()) => {}
                Err(ShellStaEndpointError::CommandQueueFull) => {
                    let typed_active = sta
                        .active_requests
                        .lock()
                        .expect("active request map")
                        .values()
                        .filter(|request| {
                            request.required_terminal_lane == Some(RequiredTerminalLane::Typed)
                        })
                        .count();
                    if typed_active == TYPED_TERMINAL_RETAIN_CAPACITY {
                        break;
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("unexpected typed submission error: {error:?}"),
            }
            assert!(
                Instant::now() < deadline,
                "typed terminal admission did not saturate; diagnostics={:?}",
                ShellDomainDiagnostics::capture()
            );
        }
        let rejected = RequestContext::new(TabId::new(), Generation::new(2));
        let started = Instant::now();
        assert!(matches!(
            sta.submit(ExplorerCommand::PreviewHost {
                context: rejected,
                command: PreviewHostCommand::Unload {
                    generation: Generation::new(2),
                },
            }),
            Err(ShellStaEndpointError::CommandQueueFull)
        ));
        assert!(
            started.elapsed() < Duration::from_millis(25),
            "typed terminal overload must reject without waiting"
        );
        sta.shutdown_and_join(Duration::from_secs(5))
            .expect("stop saturated STA");
    }

    #[test]
    fn breadcrumb_enrichment_isolation_keeps_navigation_available_before_release() {
        struct TestGateReset(Arc<AtomicBool>);
        impl Drop for TestGateReset {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
                if let Ok(mut gate) = BREADCRUMB_TEST_GATE.lock() {
                    gate.take();
                }
            }
        }

        let _serial = TEST_LOCK.lock().unwrap();
        let fixture = OwnedTempFixture::new().expect("enrichment fixture");
        let enrichment_context = RequestContext::new(TabId::new(), Generation::new(1));
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let release = Arc::new(AtomicBool::new(false));
        *BREADCRUMB_TEST_GATE.lock().unwrap() = Some(BreadcrumbTestGate {
            request_id: enrichment_context.request_id,
            started: started_tx,
            release: Arc::clone(&release),
        });
        let _gate_reset = TestGateReset(Arc::clone(&release));

        let sta = ShellStaHandle::start().expect("start STA");
        sta.submit(ExplorerCommand::ResolveAncestry {
            context: enrichment_context.clone(),
            location: LocationDescriptor::file_system(fixture.root()),
        })
        .expect("submit stalled enrichment");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("enrichment worker started");

        let navigation_context = RequestContext::new(TabId::new(), Generation::new(1));
        sta.submit(ExplorerCommand::Navigate {
            context: navigation_context.clone(),
            location: LocationDescriptor::file_system(fixture.root()),
        })
        .expect("submit navigation while enrichment is pending");
        let navigation_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(ExplorerEvent::DirectoryFinished { context }) =
                sta.try_recv_event().expect("navigation event")
                && context.request_id == navigation_context.request_id
            {
                break;
            }
            assert!(
                Instant::now() < navigation_deadline,
                "navigation was blocked behind enrichment; diagnostics={:?}",
                ShellDomainDiagnostics::capture()
            );
            thread::sleep(Duration::from_millis(2));
        }

        release.store(true, Ordering::Release);
        let enrichment_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(ExplorerEvent::AncestryFinished { context, .. }) =
                sta.try_recv_event().expect("enrichment event")
                && context.request_id == enrichment_context.request_id
            {
                break;
            }
            assert!(
                Instant::now() < enrichment_deadline,
                "enrichment did not finish after release; diagnostics={:?}",
                ShellDomainDiagnostics::capture()
            );
            thread::sleep(Duration::from_millis(2));
        }
        sta.shutdown_and_join(Duration::from_secs(5))
            .expect("STA stops");
    }

    #[test]
    fn state_machine_accepts_only_documented_transitions() {
        let states = [
            ShellStaState::Created,
            ShellStaState::Starting,
            ShellStaState::Ready,
            ShellStaState::Stopping,
            ShellStaState::Stopped,
        ];

        for from in states {
            for to in states {
                let expected_valid = matches!(
                    (from, to),
                    (ShellStaState::Created, ShellStaState::Starting)
                        | (
                            ShellStaState::Starting,
                            ShellStaState::Ready | ShellStaState::Stopped
                        )
                        | (ShellStaState::Ready, ShellStaState::Stopping)
                        | (
                            ShellStaState::Stopping | ShellStaState::Stopped,
                            ShellStaState::Stopped
                        )
                );
                assert_eq!(
                    from.transition(to).is_ok(),
                    expected_valid,
                    "unexpected transition result for {from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn real_sta_starts_and_stops_without_leaking_owned_resources() {
        let _test_guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = StaResourceSnapshot::capture();
        let sta = ShellStaHandle::start().expect("start real STA");
        assert_eq!(sta.state(), ShellStaState::Ready);
        let pump_deadline = Instant::now() + Duration::from_secs(1);
        while sta.message_pump_cycles() == 0 && Instant::now() < pump_deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(sta.message_pump_cycles() > 0, "message pump did not cycle");
        let during = StaResourceSnapshot::capture();
        assert_eq!(during.active_threads, before.active_threads + 1);
        assert_eq!(
            during.active_control_channels,
            before.active_control_channels + 1
        );
        assert_eq!(during.active_join_handles, before.active_join_handles + 2);

        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("stop real STA");
        assert_eq!(sta.state(), ShellStaState::Stopped);
        assert_eq!(StaResourceSnapshot::capture(), before);
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one linear real-disk oracle preserves the operation sequence and cleanup audit"
    )]
    fn real_file_operations_match_safe_disk_oracle() {
        let _test_guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fixture = OwnedTempFixture::new().expect("owned fixture");
        let source = fixture.root().join("來源 📁");
        let destination = fixture.root().join("目的地");
        fs::create_dir_all(&source).expect("source directory");
        fs::create_dir_all(&destination).expect("destination directory");
        let alpha = source.join("alpha.txt");
        let beta = source.join("beta.txt");
        let gamma = source.join("gamma.txt");
        let delta = source.join("delta.txt");
        fs::write(&alpha, b"alpha").expect("alpha fixture");
        fs::write(&beta, b"beta").expect("beta fixture");
        fs::write(&gamma, b"gamma").expect("gamma fixture");
        fs::write(&delta, b"delta").expect("delta fixture");

        let sta = ShellStaHandle::start().expect("start real STA");
        assert_eq!(
            run_file_operation(
                &sta,
                FileOperationKind::CreateFolder {
                    parent: LocationDescriptor::file_system(fixture.root()),
                    name: "新增資料夾".to_owned(),
                },
            ),
            OperationTerminal::Finished
        );
        assert!(fixture.root().join("新增資料夾").is_dir());

        assert_eq!(
            run_file_operation(
                &sta,
                FileOperationKind::CreateItem {
                    parent: LocationDescriptor::file_system(fixture.root()),
                    name: "New Text Document.txt".to_owned(),
                    recipe: ShellNewItemRecipe::EmptyFile,
                },
            ),
            OperationTerminal::Finished
        );
        assert_eq!(
            fs::read(fixture.root().join("New Text Document.txt")).expect("empty new item"),
            b""
        );
        let initial_zip = vec![
            0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(
            run_file_operation(
                &sta,
                FileOperationKind::CreateItem {
                    parent: LocationDescriptor::file_system(fixture.root()),
                    name: "New Compressed Folder.zip".to_owned(),
                    recipe: ShellNewItemRecipe::Data(initial_zip.clone()),
                },
            ),
            OperationTerminal::Finished
        );
        assert_eq!(
            fs::read(fixture.root().join("New Compressed Folder.zip")).expect("initialized zip"),
            initial_zip
        );

        let template = fixture.root().join("shell-new-template.bin");
        fs::write(&template, b"trusted-template-content").expect("template fixture");
        assert_eq!(
            run_file_operation(
                &sta,
                FileOperationKind::CreateItem {
                    parent: LocationDescriptor::file_system(fixture.root()),
                    name: "New Template Item.bin".to_owned(),
                    recipe: ShellNewItemRecipe::TemplateFile(template),
                },
            ),
            OperationTerminal::Finished
        );
        assert_eq!(
            fs::read(fixture.root().join("New Template Item.bin")).expect("template new item"),
            b"trusted-template-content"
        );

        assert_eq!(
            run_file_operation(
                &sta,
                FileOperationKind::CreateItem {
                    parent: LocationDescriptor::file_system(fixture.root()),
                    name: "New Text Document.txt".to_owned(),
                    recipe: ShellNewItemRecipe::EmptyFile,
                },
            ),
            OperationTerminal::Finished
        );
        let collision_safe_text_files = fs::read_dir(fixture.root())
            .expect("list collision results")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("New Text Document")
            })
            .count();
        assert_eq!(collision_safe_text_files, 2);

        let undo_old = source.join("undo-old.txt");
        fs::write(&undo_old, b"undo-redo").expect("undo fixture");
        let undo_forward = FileOperationRequest {
            kind: FileOperationKind::Rename {
                item: operation_item(&undo_old, 100),
                new_name: "undo-new.txt".to_owned(),
            },
            flags: FileOperationFlags::default(),
        };
        assert_eq!(
            run_file_operation(&sta, undo_forward.kind.clone()),
            OperationTerminal::Finished
        );
        let undo_new = source.join("undo-new.txt");
        assert!(undo_new.is_file());
        let mut journal = OperationJournal::default();
        assert!(journal.record_completed_request(
            explorer_common::RequestId::new(),
            &undo_forward,
            &OperationTerminal::Finished,
            JournalPreimage::Rename {
                prior_name: "undo-old.txt".to_owned(),
            },
        ));
        let inverse = journal
            .undo_candidate()
            .expect("undo candidate")
            .inverse_request();
        assert_eq!(
            run_file_operation(&sta, inverse.kind),
            OperationTerminal::Finished
        );
        assert!(undo_old.is_file());
        journal
            .commit_undo_validated(JournalValidation::Valid)
            .expect("commit undo");
        let redo = journal
            .redo_candidate()
            .expect("redo candidate")
            .forward
            .clone();
        assert_eq!(
            run_file_operation(&sta, redo.kind),
            OperationTerminal::Finished
        );
        assert!(undo_new.is_file());
        journal
            .commit_redo_validated(JournalValidation::Valid)
            .expect("commit redo");

        let renamed = source.join("renamed.txt");
        assert_eq!(
            run_file_operation(
                &sta,
                FileOperationKind::Rename {
                    item: operation_item(&alpha, 1),
                    new_name: "renamed.txt".to_owned(),
                },
            ),
            OperationTerminal::Finished
        );
        assert!(!alpha.exists());
        assert_eq!(fs::read(&renamed).expect("renamed bytes"), b"alpha");

        assert_eq!(
            run_file_operation(
                &sta,
                FileOperationKind::Copy {
                    items: vec![operation_item(&renamed, 2), operation_item(&gamma, 3)],
                    destination: LocationDescriptor::file_system(&destination),
                },
            ),
            OperationTerminal::Finished
        );
        let copied = destination.join("renamed.txt");
        let copied_gamma = destination.join("gamma.txt");
        assert_eq!(fs::read(&copied).expect("copied bytes"), b"alpha");
        assert_eq!(fs::read(&copied_gamma).expect("copied gamma"), b"gamma");

        assert_eq!(
            run_file_operation(
                &sta,
                FileOperationKind::Move {
                    items: vec![operation_item(&beta, 4), operation_item(&delta, 5)],
                    destination: LocationDescriptor::file_system(&destination),
                },
            ),
            OperationTerminal::Finished
        );
        let moved = destination.join("beta.txt");
        let moved_delta = destination.join("delta.txt");
        assert!(!beta.exists());
        assert!(!delta.exists());
        assert_eq!(fs::read(&moved).expect("moved bytes"), b"beta");
        assert_eq!(fs::read(&moved_delta).expect("moved delta"), b"delta");

        let conflict_source = source.join("conflict.txt");
        fs::write(&conflict_source, b"new conflict").expect("conflict source");
        fs::write(destination.join("conflict.txt"), b"existing").expect("conflict target");
        let partial_source = source.join("partial.txt");
        fs::write(&partial_source, b"partial").expect("partial source");
        let partial = run_file_operation_with_flags(
            &sta,
            FileOperationKind::Copy {
                items: vec![
                    operation_item(&conflict_source, 6),
                    operation_item(&partial_source, 7),
                ],
                destination: LocationDescriptor::file_system(&destination),
            },
            FileOperationFlags {
                conflict: ConflictDecision::Skip,
                ..FileOperationFlags::default()
            },
        );
        let OperationTerminal::Partial { outcomes } = partial else {
            panic!("mixed collision operation must be partial");
        };
        assert_eq!(outcomes.len(), 2);
        assert!(
            outcomes.iter().any(|outcome| matches!(
                outcome.result,
                explorer_model::OperationItemResult::Skipped
            ))
        );
        assert!(destination.join("partial.txt").is_file());
        assert_eq!(
            fs::read(destination.join("conflict.txt")).expect("preserved conflict"),
            b"existing"
        );

        let recycle = source.join("recycle-me.txt");
        fs::write(&recycle, b"recyclable").expect("recycle fixture");
        fixture
            .verify_destructive_target(&recycle)
            .expect("recycle target remains inside fixture");
        assert_eq!(
            run_file_operation(
                &sta,
                FileOperationKind::RecycleDelete {
                    items: vec![operation_item(&recycle, 8)],
                },
            ),
            OperationTerminal::Finished
        );
        assert!(!recycle.exists());

        for target in [&copied, &copied_gamma, &moved, &moved_delta, &undo_new] {
            fixture
                .verify_destructive_target(target)
                .expect("permanent-delete target remains inside fixture");
        }
        fixture
            .verify_destructive_target(destination.join("partial.txt"))
            .expect("partial copy target remains inside fixture");

        assert_eq!(
            run_file_operation(
                &sta,
                FileOperationKind::PermanentDelete {
                    items: vec![
                        operation_item(&copied, 9),
                        operation_item(&copied_gamma, 10),
                        operation_item(&moved, 11),
                        operation_item(&moved_delta, 12),
                        operation_item(&destination.join("partial.txt"), 13),
                        operation_item(&undo_new, 14),
                    ],
                    confirmed: true,
                },
            ),
            OperationTerminal::Finished
        );
        assert!(!copied.exists());
        assert!(!copied_gamma.exists());
        assert!(!moved.exists());
        assert!(!moved_delta.exists());
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("stop STA");
    }

    #[test]
    fn large_real_copy_cancellation_has_one_terminal_and_no_late_progress() {
        let _test_guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fixture = OwnedTempFixture::new().expect("owned fixture");
        let source = fixture.create_dir("cancel-source").expect("source");
        let destination = fixture
            .create_dir("cancel-destination")
            .expect("destination");
        let payload = vec![0x5a; 1024 * 1024];
        let mut items = Vec::new();
        for index in 0..128_u8 {
            let path = source.join(format!("large-{index:03}.bin"));
            fs::write(&path, &payload).expect("large copy source");
            items.push(operation_item(&path, index.saturating_add(1)));
        }
        let sta = ShellStaHandle::start().expect("start STA");
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let request_id = context.request_id;
        sta.submit(ExplorerCommand::ExecuteFileOperation {
            context,
            request: FileOperationRequest {
                kind: FileOperationKind::Copy {
                    items,
                    destination: LocationDescriptor::file_system(&destination),
                },
                flags: FileOperationFlags {
                    conflict: ConflictDecision::Replace,
                    ..FileOperationFlags::default()
                },
            },
        })
        .expect("submit large copy");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut cancelled = false;
        let terminal = loop {
            if let Some(event) = sta.try_recv_event().expect("copy event") {
                match event {
                    ExplorerEvent::OperationProgress { context, .. }
                        if context.request_id == request_id && !cancelled =>
                    {
                        sta.submit(ExplorerCommand::Cancel { request_id })
                            .expect("cancel copy");
                        cancelled = true;
                    }
                    ExplorerEvent::OperationFinished { context, outcome }
                        if context.request_id == request_id =>
                    {
                        break outcome;
                    }
                    _ => {}
                }
            }
            assert!(Instant::now() < deadline, "cancelled copy timed out");
            thread::sleep(Duration::from_millis(1));
        };
        assert!(cancelled, "copy never emitted cancellable progress");
        assert_eq!(terminal, OperationTerminal::Cancelled);
        assert!(
            fs::read_dir(&destination)
                .expect("destination oracle")
                .count()
                < 128,
            "cancellation should stop before every item is copied"
        );
        thread::sleep(Duration::from_millis(50));
        while let Some(event) = sta.try_recv_event().expect("late event check") {
            assert!(
                !matches!(event, ExplorerEvent::OperationProgress { context, .. } if context.request_id == request_id),
                "late progress arrived after terminal cancellation"
            );
        }
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("stop STA");
    }

    #[test]
    fn real_move_covers_cross_volume_and_reparse_capability() {
        let _test_guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let source_fixture = OwnedTempFixture::new().expect("source fixture");
        let source_dir = source_fixture
            .create_dir("source")
            .expect("source directory");
        let same_volume_destination = source_fixture
            .create_dir("reparse-destination")
            .expect("reparse destination");
        let sta = ShellStaHandle::start().expect("start STA");

        let target = source_fixture
            .create_file("source/reparse-target.txt", b"target")
            .expect("reparse target");
        let link = source_dir.join("reparse-link.txt");
        match std::os::windows::fs::symlink_file(&target, &link) {
            Ok(()) => {
                assert_eq!(
                    run_file_operation(
                        &sta,
                        FileOperationKind::Move {
                            items: vec![operation_item(&link, 201)],
                            destination: LocationDescriptor::file_system(&same_volume_destination,),
                        },
                    ),
                    OperationTerminal::Finished
                );
                let moved_link = same_volume_destination.join("reparse-link.txt");
                assert!(
                    fs::symlink_metadata(&moved_link)
                        .expect("moved reparse metadata")
                        .file_type()
                        .is_symlink()
                );
                assert_eq!(fs::read(&target).expect("target preserved"), b"target");
            }
            Err(error) => tracing::info!(
                %error,
                "reparse move capability unavailable; adapter returned explicit OS capability"
            ),
        }

        let target_base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace")
            .join("target");
        fs::create_dir_all(&target_base).expect("target base");
        let destination_fixture =
            OwnedTempFixture::new_in(&target_base).expect("destination fixture");
        let source_prefix = source_fixture.root().components().next();
        let destination_prefix = destination_fixture.root().components().next();
        if source_prefix == destination_prefix {
            tracing::info!(
                "cross-volume move fixture unavailable because both roots share a volume"
            );
        } else {
            let cross_volume = source_fixture
                .create_file("source/cross-volume.bin", b"cross-volume")
                .expect("cross-volume source");
            assert_eq!(
                run_file_operation(
                    &sta,
                    FileOperationKind::Move {
                        items: vec![operation_item(&cross_volume, 202)],
                        destination: LocationDescriptor::file_system(destination_fixture.root()),
                    },
                ),
                OperationTerminal::Finished
            );
            assert!(!cross_volume.exists());
            assert_eq!(
                fs::read(destination_fixture.root().join("cross-volume.bin"))
                    .expect("cross-volume destination"),
                b"cross-volume"
            );
        }
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("stop STA");
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one serialized system-clipboard scenario verifies ownership across every interoperability boundary"
    )]
    fn real_ole_clipboard_copy_cut_paste_crosses_tabs_and_matches_disk() {
        let _test_guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _clipboard_guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .expect("clipboard lock");
        let fixture = OwnedTempFixture::new().expect("clipboard fixture");
        let source = fixture.create_dir("clipboard-source").expect("source");
        let destination = fixture
            .create_dir("clipboard-destination")
            .expect("destination");
        let copy_source = source.join("copy.txt");
        let cut_source = source.join("cut.txt");
        fs::write(&copy_source, b"copy bytes").expect("copy source");
        fs::write(&cut_source, b"cut bytes").expect("cut source");
        let source_tab = TabId::new();
        let destination_tab = TabId::new();
        let sta = ShellStaHandle::start().expect("start OLE STA");

        let (copy_outcome, copy_states) = run_data_transfer(
            &sta,
            source_tab,
            DataTransferRequest::Copy {
                items: vec![real_operation_item(&copy_source)],
            },
        );
        assert_eq!(copy_outcome, OperationTerminal::Finished);
        assert!(copy_states.iter().any(|state| matches!(
            state,
            ClipboardState::Owned {
                mode: ClipboardMode::Copy,
                ..
            }
        )));
        let explorer_destination = fixture
            .create_dir("explorer-paste-destination")
            .expect("Explorer destination");
        explorer_paste_from_clipboard(
            &explorer_destination,
            &explorer_destination.join("copy.txt"),
        );
        assert_eq!(
            fs::read(explorer_destination.join("copy.txt")).expect("Explorer pasted bytes"),
            b"copy bytes"
        );
        let (paste_copy, _) = run_data_transfer(
            &sta,
            destination_tab,
            DataTransferRequest::Paste {
                destination: LocationDescriptor::file_system(&destination),
                conflict: ConflictDecision::Prompt,
            },
        );
        assert_eq!(paste_copy, OperationTerminal::Finished);
        assert_eq!(
            fs::read(destination.join("copy.txt")).expect("copied clipboard file"),
            b"copy bytes"
        );

        let (cut_outcome, cut_states) = run_data_transfer(
            &sta,
            source_tab,
            DataTransferRequest::Cut {
                items: vec![real_operation_item(&cut_source)],
            },
        );
        assert_eq!(cut_outcome, OperationTerminal::Finished);
        assert!(cut_states.iter().any(|state| matches!(
            state,
            ClipboardState::Owned {
                mode: ClipboardMode::Cut,
                ..
            }
        )));
        let (paste_cut, paste_states) = run_data_transfer(
            &sta,
            destination_tab,
            DataTransferRequest::Paste {
                destination: LocationDescriptor::file_system(&destination),
                conflict: ConflictDecision::Prompt,
            },
        );
        assert_eq!(paste_cut, OperationTerminal::Finished);
        assert!(!cut_source.exists());
        assert_eq!(
            fs::read(destination.join("cut.txt")).expect("moved clipboard file"),
            b"cut bytes"
        );
        assert!(
            paste_states
                .iter()
                .any(|state| matches!(state, ClipboardState::None { .. }))
        );

        let partial_one = source.join("partial-one.txt");
        let partial_two = source.join("partial-two.txt");
        fs::write(&partial_one, b"source one").expect("partial one");
        fs::write(&partial_two, b"source two").expect("partial two");
        fs::write(destination.join("partial-one.txt"), b"existing one").expect("partial collision");
        let (partial_cut, _) = run_data_transfer(
            &sta,
            source_tab,
            DataTransferRequest::Cut {
                items: vec![
                    real_operation_item(&partial_one),
                    real_operation_item(&partial_two),
                ],
            },
        );
        assert_eq!(partial_cut, OperationTerminal::Finished);
        let (partial_paste, partial_states) = run_data_transfer(
            &sta,
            destination_tab,
            DataTransferRequest::Paste {
                destination: LocationDescriptor::file_system(&destination),
                conflict: ConflictDecision::Skip,
            },
        );
        assert!(
            matches!(partial_paste, OperationTerminal::Partial { .. }),
            "unexpected partial paste outcome: {partial_paste:#?}"
        );
        assert!(partial_one.is_file());
        assert!(!partial_two.exists());
        assert_eq!(
            fs::read(destination.join("partial-two.txt")).expect("partial success"),
            b"source two"
        );
        assert!(
            partial_states.iter().any(|state| matches!(
                state,
                ClipboardState::Owned {
                    mode: ClipboardMode::Cut,
                    items,
                    ..
                } if items.len() == 1 && items[0].location.path() == Some(partial_one.as_path())
            )),
            "unexpected partial clipboard states: {partial_states:#?}"
        );
        let (retry_paste, retry_states) = run_data_transfer(
            &sta,
            destination_tab,
            DataTransferRequest::Paste {
                destination: LocationDescriptor::file_system(&destination),
                conflict: ConflictDecision::Replace,
            },
        );
        assert_eq!(retry_paste, OperationTerminal::Finished);
        assert!(!partial_one.exists());
        assert_eq!(
            fs::read(destination.join("partial-one.txt")).expect("retry replacement"),
            b"source one"
        );
        assert!(
            retry_states
                .iter()
                .any(|state| matches!(state, ClipboardState::None { .. }))
        );

        let external_source = source.join("external-client.txt");
        fs::write(&external_source, b"external client").expect("external source");
        if !explorer_copy_to_clipboard(&external_source) {
            tracing::warn!(
                "Explorer UI copy automation unavailable; using independent STA FileDropList writer"
            );
            let escaped = external_source.to_string_lossy().replace('\'', "''");
            let script = format!(
                "Add-Type -AssemblyName System.Windows.Forms; $c=[Collections.Specialized.StringCollection]::new(); [void]$c.Add('{escaped}'); for($i=0;$i -lt 40;$i++){{try{{[Windows.Forms.Clipboard]::SetFileDropList($c); exit 0}}catch{{Start-Sleep -Milliseconds 50}}}}; exit 1"
            );
            let writer = Command::new("powershell.exe")
                .args(["-STA", "-NoProfile", "-Command", &script])
                .output()
                .expect("external clipboard writer");
            assert!(writer.status.success());
        }
        let poll_deadline = Instant::now() + Duration::from_secs(2);
        let mut saw_external = false;
        while Instant::now() < poll_deadline {
            if let Some(ExplorerEvent::ClipboardChanged {
                state: ClipboardState::External { item_count, .. },
            }) = sta.try_recv_event().expect("external ownership event")
            {
                assert_eq!(item_count, Some(1));
                saw_external = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            saw_external,
            "external clipboard ownership was not observed"
        );
        let external_destination = fixture
            .create_dir("external-destination")
            .expect("external destination");
        let (external_paste, _) = run_data_transfer(
            &sta,
            destination_tab,
            DataTransferRequest::Paste {
                destination: LocationDescriptor::file_system(&external_destination),
                conflict: ConflictDecision::Prompt,
            },
        );
        assert_eq!(external_paste, OperationTerminal::Finished);
        assert_eq!(
            fs::read(external_destination.join("external-client.txt"))
                .expect("external pasted file"),
            b"external client"
        );
        let _ = Command::new("powershell.exe")
            .args([
                "-STA",
                "-NoProfile",
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; [Windows.Forms.Clipboard]::Clear()",
            ])
            .status();
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("stop OLE STA");
    }

    #[test]
    #[ignore = "requires an interactive Windows Explorer desktop"]
    fn real_explorer_single_multi_copy_cut_paste_matrix_matches_disk_effects() {
        let _test_guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _clipboard_guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fixture = OwnedTempFixture::new().expect("Explorer clipboard matrix fixture");
        let source = fixture.create_dir("source").expect("source directory");
        let files = [
            "copy-single.txt",
            "copy-multi-a.txt",
            "copy-multi-b.txt",
            "cut-single.txt",
            "cut-multi-a.txt",
            "cut-multi-b.txt",
        ]
        .map(|name| {
            let path = source.join(name);
            fs::write(&path, name.as_bytes()).expect("write matrix source");
            path
        });
        let sta = ShellStaHandle::start().expect("start OLE STA");
        let tab = TabId::new();

        let verify = |case: &str, indexes: &[usize], cut: bool| {
            let destination = fixture
                .create_dir(format!("destination-{case}"))
                .expect("matrix destination");
            let sources = indexes
                .iter()
                .map(|index| files[*index].as_path())
                .collect::<Vec<_>>();
            assert!(
                explorer_transfer_to_clipboard(&sources, cut),
                "Explorer failed to publish {case} selection"
            );
            let (terminal, _) = run_data_transfer(
                &sta,
                tab,
                DataTransferRequest::Paste {
                    destination: LocationDescriptor::file_system(&destination),
                    conflict: ConflictDecision::Prompt,
                },
            );
            assert_eq!(terminal, OperationTerminal::Finished, "{case} terminal");
            for source_path in sources {
                let name = source_path.file_name().expect("source name");
                assert_eq!(
                    fs::read(destination.join(name)).expect("pasted matrix bytes"),
                    name.to_string_lossy().as_bytes(),
                    "{case} bytes"
                );
                assert_eq!(source_path.exists(), !cut, "{case} source effect");
            }
        };
        verify("copy-single", &[0], false);
        verify("copy-multi", &[1, 2], false);
        verify("cut-single", &[3], true);
        verify("cut-multi", &[4, 5], true);
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("stop OLE STA");
    }

    #[test]
    fn repeated_shutdown_and_join_are_idempotent() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let sta = ShellStaHandle::start().expect("start real STA");
        let context = RequestContext::new(TabId::new(), Generation::default());
        let cancellation = context.cancellation.clone();
        sta.submit(ExplorerCommand::StartSearch {
            context,
            location: LocationDescriptor::file_system(r"C:\definitely-missing-shutdown-fixture"),
            input: explorer_model::SearchInput::new("shutdown"),
        })
        .expect("register active request");
        sta.shutdown();
        assert!(cancellation.is_cancelled());
        sta.shutdown();
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("first join");
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("second join");
    }

    #[test]
    fn startup_hook_failure_is_observable_and_releases_thread() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let before = StaResourceSnapshot::capture();
        let result = ShellStaHandle::start_with_hook(
            || {
                Err(ShellStaError::StartupHook {
                    message: "injected failure".to_owned(),
                })
            },
            Duration::from_secs(1),
        );

        assert!(matches!(result, Err(ShellStaError::StartupHook { .. })));
        assert_eq!(StaResourceSnapshot::capture(), before);
    }

    #[test]
    fn failed_second_sta_spawn_unwinds_the_search_worker_and_accounting() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let before = StaResourceSnapshot::capture();
        FAIL_NEXT_STA_THREAD_SPAWN.store(true, Ordering::Release);

        let result = ShellStaHandle::start();

        assert!(matches!(result, Err(ShellStaError::Spawn(_))));
        assert_eq!(
            StaResourceSnapshot::capture(),
            before,
            "failed main STA spawn must join the already-created search worker"
        );
    }

    #[test]
    fn startup_timeout_is_bounded_and_eventually_releases_resources() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let before = StaResourceSnapshot::capture();
        let timeout = Duration::from_millis(5);
        let started = Instant::now();
        let result = ShellStaHandle::start_with_hook(
            || {
                thread::sleep(Duration::from_millis(50));
                Ok(())
            },
            timeout,
        );

        assert!(matches!(result, Err(ShellStaError::StartupTimeout { .. })));
        assert!(started.elapsed() < Duration::from_millis(100));

        let release_deadline = Instant::now() + Duration::from_secs(1);
        while StaResourceSnapshot::capture() != before && Instant::now() < release_deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(StaResourceSnapshot::capture(), before);
    }

    #[test]
    fn real_folder_navigation_emits_metadata_bounded_batches_and_terminal() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let fixture = tempfile::tempdir().expect("create real folder fixture");
        for index in 0..145 {
            let suffix = if index == 144 {
                "x".repeat(180)
            } else {
                String::new()
            };
            fs::write(
                fixture
                    .path()
                    .join(format!("entry-{index:03}-{suffix}.txt")),
                format!("fixture-{index}"),
            )
            .expect("create fixture file");
        }
        fs::create_dir(fixture.path().join("child-folder")).expect("create fixture folder");

        let sta = ShellStaHandle::start().expect("start real STA");
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        sta.submit(ExplorerCommand::Navigate {
            context: context.clone(),
            location: LocationDescriptor::file_system(fixture.path()),
        })
        .expect("submit navigation");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut events = Vec::new();
        while Instant::now() < deadline {
            if let Some(event) = sta.try_recv_event().expect("receive Shell event") {
                let terminal = event.is_terminal();
                events.push(event);
                if terminal {
                    break;
                }
            } else {
                thread::sleep(Duration::from_millis(2));
            }
        }

        assert!(matches!(
            events.first(),
            Some(ExplorerEvent::LocationResolved { .. })
        ));
        let batches: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ExplorerEvent::DirectoryBatch { entries, .. } => Some(entries),
                _ => None,
            })
            .collect();
        assert!(
            batches.len() >= 3,
            "expected incremental batches: {events:?}"
        );
        assert!(batches.iter().all(|batch| batch.len() <= 64));
        assert_eq!(batches.iter().map(|batch| batch.len()).sum::<usize>(), 146);
        assert!(batches.into_iter().flatten().all(|entry| {
            !entry.id.provider_bytes().is_empty()
                && entry
                    .location
                    .path()
                    .is_some_and(std::path::Path::is_absolute)
        }));
        assert!(matches!(
            events.last(),
            Some(ExplorerEvent::DirectoryFinished { .. })
        ));
    }

    #[test]
    fn cancelled_navigation_has_exactly_one_failed_terminal() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let fixture = tempfile::tempdir().expect("create real folder fixture");
        let sta = ShellStaHandle::start().expect("start real STA");
        let context = RequestContext::new(TabId::new(), Generation::new(7));
        context.cancellation.cancel();
        sta.submit(ExplorerCommand::Navigate {
            context: context.clone(),
            location: LocationDescriptor::file_system(fixture.path()),
        })
        .expect("submit cancelled navigation");

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut terminals = Vec::new();
        while Instant::now() < deadline {
            match sta.try_recv_event().expect("receive Shell event") {
                Some(event) if event.is_terminal() => {
                    terminals.push(event);
                    break;
                }
                Some(_) => {}
                None => thread::sleep(Duration::from_millis(2)),
            }
        }
        assert_eq!(terminals.len(), 1);
        assert!(matches!(terminals[0], ExplorerEvent::Failed { .. }));
    }

    #[test]
    fn real_folder_refresh_converges_without_history_and_preserves_stable_view_state() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let fixture = tempfile::tempdir().expect("create real folder fixture");
        fs::write(fixture.path().join("kept.txt"), "kept").expect("create kept file");
        let location = LocationDescriptor::file_system(fixture.path());
        let mut window = ExplorerWindowState::new(HistoryEntry::new(location.clone(), "fixture"));
        let sta = ShellStaHandle::start().expect("start real STA");

        let initial = window
            .active_tab_mut()
            .begin_navigation_request()
            .expect("initial navigation");
        sta.submit(ExplorerCommand::Navigate {
            context: initial,
            location: location.clone(),
        })
        .expect("submit initial navigation");
        drain_directory_request(&sta, &mut window);
        let kept = window
            .active_tab()
            .directory
            .snapshot()
            .expect("initial snapshot")
            .entries()[0]
            .id
            .clone();
        window.active_tab_mut().selection.select_only(kept.clone());
        window.active_tab_mut().view.anchor = ViewAnchor {
            item: Some(kept.clone()),
            offset_logical_pixels: 19,
        };

        fs::write(fixture.path().join("added.txt"), "added").expect("mutate real folder");
        let refresh = window
            .active_tab_mut()
            .begin_refresh_request()
            .expect("refresh request");
        sta.submit(ExplorerCommand::Refresh {
            context: refresh,
            location,
        })
        .expect("submit refresh");
        drain_directory_request(&sta, &mut window);

        assert_eq!(window.active_presentation().item_count, 2);
        assert!(!window.active_tab().history.can_go_back());
        assert!(window.active_tab().selection.contains(&kept));
        assert_eq!(window.active_tab().view.anchor.offset_logical_pixels, 19);
        assert_eq!(window.active_tab().view.anchor.item.as_ref(), Some(&kept));
    }

    #[test]
    fn real_watcher_detects_rename_storm_and_refresh_matches_disk_oracle() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let fixture = tempfile::tempdir().expect("create watcher fixture");
        fs::write(fixture.path().join("kept.txt"), "kept").expect("create kept file");
        let location = LocationDescriptor::file_system(fixture.path());
        let mut window = ExplorerWindowState::new(HistoryEntry::new(location.clone(), "fixture"));
        let sta = ShellStaHandle::start().expect("start real STA");
        let request = window
            .active_tab_mut()
            .begin_navigation_request()
            .expect("navigation request");
        let watched_generation = request.generation;
        sta.submit(ExplorerCommand::Navigate {
            context: request,
            location: location.clone(),
        })
        .expect("submit navigation");
        drain_directory_request(&sta, &mut window);
        let stable_id = window.active_tab().directory.snapshot().unwrap().entries()[0]
            .id
            .clone();
        window
            .active_tab_mut()
            .selection
            .select_only(stable_id.clone());
        thread::sleep(Duration::from_millis(75));

        fs::rename(
            fixture.path().join("kept.txt"),
            fixture.path().join("renamed-😀.txt"),
        )
        .expect("rename watched file");
        for index in 0..25 {
            let path = fixture.path().join(format!("storm-{index}.tmp"));
            fs::write(&path, "storm").expect("create storm file");
            fs::remove_file(path).expect("delete storm file");
        }

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut observed = false;
        while Instant::now() < deadline {
            match sta.try_recv_event().expect("receive watcher event") {
                Some(ExplorerEvent::DirectoryChanged {
                    generation,
                    changes,
                    ..
                }) if generation == watched_generation => {
                    observed = changes
                        .iter()
                        .any(|change| matches!(change, explorer_model::DirectoryDelta::Overflow));
                    break;
                }
                Some(_) => {}
                None => thread::sleep(Duration::from_millis(5)),
            }
        }
        assert!(
            observed,
            "ReadDirectoryChangesW did not report the mutation storm"
        );

        let refresh = window
            .active_tab_mut()
            .begin_refresh_request()
            .expect("refresh request");
        sta.submit(ExplorerCommand::Refresh {
            context: refresh,
            location,
        })
        .expect("submit refresh");
        drain_directory_request(&sta, &mut window);
        let snapshot = window
            .active_tab()
            .directory
            .snapshot()
            .expect("final snapshot");
        assert_eq!(snapshot.entries().len(), 1);
        assert_eq!(snapshot.entries()[0].display_name, "renamed-😀.txt");
        assert_eq!(snapshot.entries()[0].id, stable_id);
        assert!(window.active_tab().selection.contains(&stable_id));
    }

    #[test]
    fn real_navigation_matrix_covers_child_back_forward_up_and_open_error() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let fixture = tempfile::tempdir().expect("create navigation fixture");
        let child = fixture.path().join("child");
        fs::create_dir(&child).expect("create child folder");
        fs::write(child.join("inside.txt"), "inside").expect("create child file");
        let root_location = LocationDescriptor::file_system(fixture.path());
        let child_location = LocationDescriptor::file_system(&child);
        let mut window =
            ExplorerWindowState::new(HistoryEntry::new(root_location.clone(), "fixture"));
        let sta = ShellStaHandle::start().expect("start real STA");

        navigate_window(&sta, &mut window, root_location.clone());
        assert_eq!(window.active_presentation().item_count, 1);
        navigate_window(&sta, &mut window, child_location.clone());
        assert!(window.active_tab().history.can_go_back());
        assert_eq!(window.active_presentation().item_count, 1);

        let back = window
            .active_tab_mut()
            .history
            .go_back()
            .expect("back destination")
            .location
            .clone();
        navigate_window(&sta, &mut window, back);
        assert!(window.active_tab().history.can_go_forward());
        let forward = window
            .active_tab_mut()
            .history
            .go_forward()
            .expect("forward destination")
            .location
            .clone();
        navigate_window(&sta, &mut window, forward);
        let up = child.parent().expect("child parent").to_path_buf();
        navigate_window(&sta, &mut window, LocationDescriptor::file_system(up));
        assert_eq!(window.active_presentation().item_count, 1);

        let before = window.active_presentation().item_count;
        let context = RequestContext::new(window.active_tab_id(), window.active_tab().generation);
        sta.submit(ExplorerCommand::OpenItem {
            context: context.clone(),
            item: ItemDescriptor {
                id: ShellItemId::from_provider_bytes([0xff]).expect("missing id"),
                location: LocationDescriptor::file_system(fixture.path().join("missing.file")),
            },
            disposition: explorer_model::OpenDisposition::DefaultApplication,
        })
        .expect("submit missing file open");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut failed = false;
        while Instant::now() < deadline {
            match sta.try_recv_event().expect("receive open event") {
                Some(event)
                    if event.context().is_some_and(|event_context| {
                        event_context.request_id == context.request_id
                    }) =>
                {
                    failed = matches!(event, ExplorerEvent::Failed { .. });
                    let _ = window.apply_event(event);
                    break;
                }
                Some(_) => {}
                None => thread::sleep(Duration::from_millis(2)),
            }
        }
        assert!(failed);
        assert_eq!(window.active_presentation().item_count, before);
    }

    #[test]
    fn end_to_end_two_tabs_navigation_history_and_watcher_are_isolated() {
        let _test_guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fixture = OwnedTempFixture::new().expect("multi-tab E2E fixture");
        let folder_a = fixture.create_dir("tab-a").expect("tab A");
        let child_a = fixture.create_dir("tab-a/child").expect("tab A child");
        let folder_b = fixture.create_dir("tab-b").expect("tab B");
        fs::write(folder_a.join("a.txt"), b"A").expect("tab A file");
        fs::write(child_a.join("child.txt"), b"child").expect("child file");
        fs::write(folder_b.join("b.txt"), b"B").expect("tab B file");
        let location_a = LocationDescriptor::file_system(&folder_a);
        let location_child = LocationDescriptor::file_system(&child_a);
        let location_b = LocationDescriptor::file_system(&folder_b);
        let mut window = ExplorerWindowState::new(HistoryEntry::new(location_a.clone(), "tab A"));
        let tab_a = window.active_tab_id();
        let sta = ShellStaHandle::start().expect("start real STA");
        drive_window_location(&sta, &mut window, tab_a, location_a.clone(), false);

        let tab_b = window.new_tab();
        drive_window_location(&sta, &mut window, tab_b, location_b.clone(), false);
        let names = |window: &ExplorerWindowState, tab_id: TabId| {
            let mut names = window
                .tabs()
                .iter()
                .find(|tab| tab.id == tab_id)
                .and_then(explorer_model::TabState::visible_snapshot)
                .expect("tab snapshot")
                .entries()
                .iter()
                .map(|entry| entry.display_name.clone())
                .collect::<Vec<_>>();
            names.sort();
            names
        };
        assert_eq!(names(&window, tab_a), vec!["a.txt", "child"]);
        assert_eq!(names(&window, tab_b), vec!["b.txt"]);

        assert!(window.activate(tab_a));
        drive_window_location(&sta, &mut window, tab_a, location_child.clone(), false);
        let (back_context, back_location) = window
            .active_tab_mut()
            .begin_back_request()
            .expect("Back request");
        drive_correlated_navigation(&sta, &mut window, back_context, back_location, false);
        assert_eq!(
            window
                .active_tab()
                .history
                .current()
                .map(|entry| &entry.location),
            Some(&location_a)
        );
        let (forward_context, forward_location) = window
            .active_tab_mut()
            .begin_forward_request()
            .expect("Forward request");
        drive_correlated_navigation(&sta, &mut window, forward_context, forward_location, false);
        assert_eq!(
            window
                .active_tab()
                .history
                .current()
                .map(|entry| &entry.location),
            Some(&location_child)
        );

        fs::write(folder_b.join("watcher-added.txt"), b"watcher").expect("watcher mutation");
        let watcher_deadline = Instant::now() + Duration::from_secs(5);
        let mut watcher_seen = false;
        while Instant::now() < watcher_deadline {
            if let Some(event) = sta.try_recv_event().expect("watcher event") {
                watcher_seen |= matches!(
                    event,
                    ExplorerEvent::DirectoryChanged { tab_id, .. } if tab_id == tab_b
                );
                if watcher_seen {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(watcher_seen, "tab B watcher must observe its real folder");
        drive_window_location(&sta, &mut window, tab_b, location_b, true);
        assert_eq!(names(&window, tab_b), vec!["b.txt", "watcher-added.txt"]);
        assert_eq!(names(&window, tab_a), vec!["child.txt"]);
        assert!(window.activate(tab_b));
        assert_eq!(window.active_presentation().item_count, 2);
        assert!(window.activate(tab_a));
        assert_eq!(window.active_presentation().item_count, 1);
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("stop real STA");
    }

    #[test]
    #[ignore = "requires the real D: volume and Windows Shell namespace providers"]
    fn real_d_unicode_and_parsing_name_two_tab_state_isolation() {
        let _test_guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fixture = tempfile::Builder::new()
            .prefix("分頁-網址列-隔離-")
            .tempdir_in(r"D:\test\target")
            .expect("Unicode real-folder fixture on D:");
        let nested = fixture.path().join("巢狀-😀").join("第二層");
        fs::create_dir_all(&nested).expect("nested Unicode path");
        fs::write(nested.join("項目.txt"), "真實資料").expect("Unicode item");

        let locations = [
            LocationDescriptor::file_system(r"D:\"),
            LocationDescriptor::file_system(&nested),
            LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned()),
        ];
        let sta = ShellStaHandle::start().expect("start real STA");
        for (case, location) in locations.into_iter().enumerate() {
            let mut window = ExplorerWindowState::new(HistoryEntry::new(
                location.clone(),
                format!("case-{case}"),
            ));
            let first = window.active_tab_id();
            drive_window_location(&sta, &mut window, first, location.clone(), false);
            let second = window.new_tab();
            drive_window_location(&sta, &mut window, second, location.clone(), false);

            for (tab_id, label, offset) in [(first, "第一分頁", 41), (second, "第二分頁", 97)]
            {
                let tab = window.tab_mut(tab_id).expect("tab remains live");
                tab.view.address.enter_editing();
                assert!(tab.view.address.update_draft(format!("{label}-草稿")));
                tab.search = explorer_model::TabSearchState::Editing(format!("{label}-搜尋"));
                tab.view.anchor = ViewAnchor {
                    item: tab
                        .visible_snapshot()
                        .and_then(|snapshot| snapshot.entries().first())
                        .map(|entry| entry.id.clone()),
                    offset_logical_pixels: offset,
                };
            }
            if let Some(id) = window
                .tabs()
                .iter()
                .find(|tab| tab.id == first)
                .and_then(explorer_model::TabState::visible_snapshot)
                .and_then(|snapshot| snapshot.entries().first())
                .map(|entry| entry.id.clone())
            {
                window
                    .tab_mut(first)
                    .expect("first tab")
                    .selection
                    .select_only(id);
            }
            window
                .tab_mut(first)
                .expect("first tab")
                .history
                .commit_navigation(HistoryEntry::new(
                    LocationDescriptor::file_system(r"D:\test"),
                    "first-history-probe",
                ));
            window
                .tab_mut(first)
                .expect("first tab")
                .history
                .commit_navigation(HistoryEntry::new(location.clone(), "first-current"));

            let first_tab = window
                .tabs()
                .iter()
                .find(|tab| tab.id == first)
                .expect("first tab");
            let second_tab = window
                .tabs()
                .iter()
                .find(|tab| tab.id == second)
                .expect("second tab");
            assert_eq!(
                first_tab.history.current().map(|entry| &entry.location),
                second_tab.history.current().map(|entry| &entry.location),
                "both tabs resolve to the same real location"
            );
            assert!(first_tab.history.can_go_back());
            assert_ne!(first_tab.view.address.draft, second_tab.view.address.draft);
            assert_ne!(first_tab.search, second_tab.search);
            assert_ne!(first_tab.view.anchor, second_tab.view.anchor);
            assert_ne!(first_tab.selection.len(), second_tab.selection.len());
        }
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("stop real STA");
    }

    #[test]
    fn real_folder_preserves_unicode_long_hidden_system_and_case_identity() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let fixture = tempfile::tempdir().expect("create Unicode fixture");
        let names = [
            "繁體中文-😀.txt".to_owned(),
            "e\u{301}-combining.txt".to_owned(),
            format!("long-{}.txt", "x".repeat(180)),
            "hidden-system.txt".to_owned(),
            "Case-Sensitive-Probe.txt".to_owned(),
        ];
        for name in &names {
            fs::write(fixture.path().join(name), name).expect("create Unicode fixture item");
        }
        set_hidden_system(&fixture.path().join("hidden-system.txt"));
        fs::write(
            fixture.path().join("case-sensitive-probe.txt"),
            "same case-insensitive file",
        )
        .expect("probe case-insensitive filesystem");

        let location = LocationDescriptor::file_system(fixture.path());
        let mut window = ExplorerWindowState::new(HistoryEntry::new(location.clone(), "fixture"));
        let sta = ShellStaHandle::start().expect("start real STA");
        navigate_window(&sta, &mut window, location);
        let entries = window
            .active_tab()
            .directory
            .snapshot()
            .expect("snapshot")
            .entries();
        assert_eq!(entries.len(), names.len());
        for expected in &names[..4] {
            assert!(entries.iter().any(|entry| entry.display_name == *expected));
        }
        let case_entries = entries
            .iter()
            .filter(|entry| {
                entry
                    .display_name
                    .eq_ignore_ascii_case("Case-Sensitive-Probe.txt")
            })
            .collect::<Vec<_>>();
        assert_eq!(case_entries.len(), 1);
        let unique = entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), entries.len());
    }

    #[test]
    fn real_temporary_acl_denial_returns_authorization_instead_of_empty() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let fixture = tempfile::tempdir().expect("create ACL fixture");
        let protected = fixture.path().join("access-denied");
        fs::create_dir(&protected).expect("create protected directory");
        fs::write(protected.join("must-not-look-empty.txt"), b"protected")
            .expect("create protected child");
        let account_output = Command::new("whoami")
            .output()
            .expect("query current Windows account");
        assert!(account_output.status.success(), "whoami must succeed");
        let account = String::from_utf8(account_output.stdout)
            .expect("whoami output is UTF-8")
            .trim()
            .to_owned();
        assert!(!account.is_empty(), "current account must be non-empty");
        let deny_entry = format!("{account}:(OI)(CI)F");
        let deny = Command::new("icacls")
            .arg(&protected)
            .args(["/inheritance:r", "/deny"])
            .arg(&deny_entry)
            .args(["/T", "/C", "/Q"])
            .output()
            .expect("apply temporary deny ACL");
        assert!(
            deny.status.success(),
            "icacls deny failed: {}",
            String::from_utf8_lossy(&deny.stderr)
        );
        let acl_guard = DeniedAclGuard {
            path: protected.clone(),
            account,
        };
        assert_eq!(
            fs::read_dir(&protected)
                .expect_err("fixture must deny directory enumeration")
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );

        let sta = ShellStaHandle::start().expect("start real STA");
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        sta.submit(ExplorerCommand::Navigate {
            context: context.clone(),
            location: LocationDescriptor::file_system(&protected),
        })
        .expect("submit denied navigation");
        let deadline = Instant::now() + Duration::from_secs(5);
        let terminal = loop {
            assert!(Instant::now() < deadline, "denied navigation timed out");
            match sta.try_recv_event().expect("receive denied event") {
                Some(event)
                    if event
                        .context()
                        .is_some_and(|value| value.request_id == context.request_id)
                        && event.is_terminal() =>
                {
                    break event;
                }
                Some(_) => {}
                None => thread::sleep(Duration::from_millis(2)),
            }
        };
        assert!(
            matches!(
                &terminal,
                ExplorerEvent::Failed { error, .. }
                    if error.kind == explorer_common::ExplorerErrorKind::Authorization
            ),
            "denied navigation returned the wrong terminal: {terminal:?}"
        );
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("stop real STA");
        drop(acl_guard);
        assert!(
            fs::read_dir(&protected).is_ok(),
            "ACL guard must restore fixture access before cleanup"
        );
    }

    #[test]
    fn reparse_fixture_navigates_without_recursive_escape_and_cleans_up() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let fixture = tempfile::tempdir().expect("create reparse fixture");
        let target = fixture.path().join("owned-target");
        fs::create_dir(&target).expect("create owned reparse target");
        fs::write(target.join("inside.txt"), "inside").expect("create target content");
        let link = fixture.path().join("owned-link");
        if let Err(error) = std::os::windows::fs::symlink_dir(&target, &link) {
            eprintln!(
                "reparse navigation fixture skipped: Windows denied symlink creation: {error}"
            );
            return;
        }
        let sta = ShellStaHandle::start().expect("start real STA");
        let location = LocationDescriptor::file_system(&link);
        let mut window = ExplorerWindowState::new(HistoryEntry::new(location.clone(), "link"));
        navigate_window(&sta, &mut window, location);
        let snapshot = window
            .active_tab()
            .directory
            .snapshot()
            .expect("link snapshot");
        assert_eq!(snapshot.entries().len(), 1);
        assert_eq!(snapshot.entries()[0].display_name, "inside.txt");
        assert!(
            snapshot.entries()[0]
                .location
                .path()
                .is_some_and(|path| path.starts_with(&link) || path.starts_with(&target))
        );
    }

    #[test]
    fn fake_and_real_services_pass_the_same_navigation_contract() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let fake = explorer_test_support::ImmediateNavigationService::default();
        assert_navigation_contract(
            &fake,
            LocationDescriptor::file_system(r"C:\deterministic-fixture"),
        );
        assert_cancelled_navigation_contract(
            &fake,
            LocationDescriptor::file_system(r"C:\deterministic-fixture"),
        );

        let fixture = tempfile::tempdir().expect("create real contract fixture");
        fs::write(fixture.path().join("real.txt"), "real").expect("create real item");
        let real = ShellStaHandle::start().expect("start real STA");
        assert_navigation_contract(&real, LocationDescriptor::file_system(fixture.path()));
        assert_cancelled_navigation_contract(
            &real,
            LocationDescriptor::file_system(fixture.path()),
        );
    }

    fn assert_navigation_contract(service: &dyn ExplorerService, location: LocationDescriptor) {
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        service
            .submit(ExplorerCommand::Navigate {
                context: context.clone(),
                location,
            })
            .expect("submit navigation contract");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while Instant::now() < deadline {
            match service.try_recv().expect("receive contract event") {
                Some(event) => {
                    let terminal = event.is_terminal();
                    events.push(event);
                    if terminal {
                        break;
                    }
                }
                None => thread::sleep(Duration::from_millis(2)),
            }
        }
        assert!(matches!(
            events.first(),
            Some(ExplorerEvent::LocationResolved { .. })
        ));
        assert_eq!(events.iter().filter(|event| event.is_terminal()).count(), 1);
        assert!(events.last().is_some_and(ExplorerEvent::is_terminal));
        assert!(events.iter().all(|event| {
            event
                .context()
                .is_none_or(|event_context| event_context.request_id == context.request_id)
        }));
    }

    fn assert_cancelled_navigation_contract(
        service: &dyn ExplorerService,
        location: LocationDescriptor,
    ) {
        let context = RequestContext::new(TabId::new(), Generation::new(9));
        context.cancellation.cancel();
        service
            .submit(ExplorerCommand::Navigate {
                context: context.clone(),
                location,
            })
            .expect("submit cancelled contract");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut terminals = Vec::new();
        while Instant::now() < deadline {
            match service
                .try_recv()
                .expect("receive cancelled contract event")
            {
                Some(event)
                    if event
                        .context()
                        .is_some_and(|value| value.request_id == context.request_id) =>
                {
                    if event.is_terminal() {
                        terminals.push(event);
                        break;
                    }
                }
                Some(_) => {}
                None => thread::sleep(Duration::from_millis(2)),
            }
        }
        assert_eq!(terminals.len(), 1);
        assert!(matches!(
            &terminals[0],
            ExplorerEvent::Failed { error, .. }
                if error.kind == explorer_common::ExplorerErrorKind::Cancellation
        ));
    }

    #[test]
    #[ignore = "creates and enumerates 100,000 real files; run explicitly for performance evidence"]
    fn real_100k_dataset_reports_latency_memory_count_and_batch_depth() {
        let _test_guard = TEST_LOCK.lock().expect("lock STA tests");
        let fixture = OwnedTempFixture::new().expect("create 100k fixture");
        let generated = Instant::now();
        let oracle = fixture
            .generate_large_dataset(100_000)
            .expect("generate 100k dataset");
        let generation_elapsed = generated.elapsed();
        let memory_before = process_working_set();
        let location = LocationDescriptor::file_system(&oracle.root);
        let mut window = ExplorerWindowState::new(HistoryEntry::new(location.clone(), "100k"));
        let context = window
            .active_tab_mut()
            .begin_navigation_request()
            .expect("100k request");
        let sta = ShellStaHandle::start().expect("start real STA");
        let started = Instant::now();
        sta.submit(ExplorerCommand::Navigate { context, location })
            .expect("submit 100k navigation");
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut first_item = None;
        let mut max_batch = 0_usize;
        let mut event_count = 0_usize;
        let mut received_items = 0_usize;
        let mut terminal_debug = String::new();
        while Instant::now() < deadline {
            match sta.try_recv_event().expect("receive 100k event") {
                Some(event) => {
                    event_count += 1;
                    if let ExplorerEvent::DirectoryBatch { entries, .. } = &event {
                        first_item.get_or_insert_with(|| started.elapsed());
                        max_batch = max_batch.max(entries.len());
                        received_items += entries.len();
                    }
                    let terminal = event.is_terminal();
                    if terminal {
                        terminal_debug = format!("{event:?}");
                    }
                    let _ = window.apply_event(event);
                    if terminal {
                        break;
                    }
                }
                None => thread::sleep(Duration::from_millis(1)),
            }
        }
        let terminal = started.elapsed();
        let memory_after = process_working_set();
        eprintln!(
            "100k generation={generation_elapsed:?} first_item={first_item:?} first_viewport={first_item:?} terminal={terminal:?} memory_before={} memory_after={} delta={} events={event_count} received_items={received_items} max_batch={max_batch} terminal_event={terminal_debug}",
            memory_before,
            memory_after,
            memory_after.saturating_sub(memory_before),
        );
        assert_eq!(window.active_presentation().item_count, oracle.item_count);
        assert!(first_item.is_some());
        assert!(max_batch <= crate::DIRECTORY_BATCH_ITEM_CAP);
    }

    #[allow(
        unsafe_code,
        reason = "performance evidence reads the current process memory counter into sized storage"
    )]
    fn process_working_set() -> usize {
        let mut counters =
            windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS::default();
        // SAFETY: GetCurrentProcess returns a non-owned pseudo-handle and counters is correctly
        // sized writable storage for the duration of GetProcessMemoryInfo.
        unsafe {
            windows::Win32::System::ProcessStatus::GetProcessMemoryInfo(
                windows::Win32::System::Threading::GetCurrentProcess(),
                &raw mut counters,
                u32::try_from(size_of_val(&counters)).expect("counter size fits u32"),
            )
        }
        .expect("read process memory counters");
        counters.WorkingSetSize
    }

    #[test]
    fn real_search_uses_typed_query_fallback_and_rejects_fast_replacement() {
        let _guard = crate::clipboard::CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fixture = OwnedTempFixture::new().expect("fixture");
        fs::write(
            fixture.root().join("專案 quarter four.txt"),
            vec![b'x'; 12_000],
        )
        .expect("write unicode fixture");
        fs::write(fixture.root().join("old.bin"), [1, 2, 3]).expect("write binary fixture");

        let sta = ShellStaHandle::start().expect("start STA");
        let location = LocationDescriptor::file_system(fixture.root());
        let mut window = ExplorerWindowState::new(HistoryEntry::new(location.clone(), "oracle"));
        let first = window
            .active_tab_mut()
            .begin_search_request("name:never".to_owned())
            .expect("first search");
        sta.submit(ExplorerCommand::StartSearch {
            context: first,
            location: location.clone(),
            input: explorer_model::SearchInput::new("name:never"),
        })
        .expect("submit first search");
        let second = window
            .active_tab_mut()
            .begin_search_request("專案 type:txt size:>10KB".to_owned())
            .expect("replacement search");
        sta.submit(ExplorerCommand::StartSearch {
            context: second.clone(),
            location,
            input: explorer_model::SearchInput::new("專案 type:txt size:>10KB"),
        })
        .expect("submit replacement search");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut replacement_finished = false;
        while Instant::now() < deadline {
            match sta.try_recv_event().expect("receive search event") {
                Some(event) => {
                    replacement_finished |= matches!(
                        &event,
                        ExplorerEvent::SearchFinished { context, .. }
                            if context.request_id == second.request_id
                    );
                    let _ = window.apply_event(event);
                    if replacement_finished {
                        break;
                    }
                }
                None => thread::sleep(Duration::from_millis(2)),
            }
        }
        assert!(replacement_finished, "replacement search must terminate");
        let names: Vec<_> = window
            .active_tab()
            .visible_snapshot()
            .expect("search snapshot")
            .entries()
            .iter()
            .map(|entry| entry.display_name.as_str())
            .collect();
        assert_eq!(names, ["專案 quarter four.txt"]);
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one real two-tab scenario preserves the timing and correlation of replacement, navigation cancellation, and partial fallback"
    )]
    fn end_to_end_two_tab_search_replacement_navigation_cancel_and_partial_fallback() {
        let _test_guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fixture = OwnedTempFixture::new().expect("search E2E fixture");
        let folder_a = fixture.create_dir("search-a").expect("search A");
        let folder_b = fixture.create_dir("search-b").expect("search B");
        let partial = fixture.create_dir("search-partial").expect("partial root");
        fs::write(folder_a.join("alpha.txt"), b"alpha").expect("alpha file");
        fs::write(folder_b.join("beta.txt"), b"beta").expect("beta file");
        for index in 0..4_100_u32 {
            fs::create_dir(partial.join(format!("directory-{index:04}")))
                .expect("partial queue directory");
        }
        let location_a = LocationDescriptor::file_system(&folder_a);
        let location_b = LocationDescriptor::file_system(&folder_b);
        let location_partial = LocationDescriptor::file_system(&partial);
        let mut window =
            ExplorerWindowState::new(HistoryEntry::new(location_a.clone(), "search A"));
        let tab_a = window.active_tab_id();
        let sta = ShellStaHandle::start().expect("start search STA");
        drive_window_location(&sta, &mut window, tab_a, location_a.clone(), false);
        let tab_b = window.new_tab();
        drive_window_location(&sta, &mut window, tab_b, location_b.clone(), false);

        let search_a = window
            .tab_mut(tab_a)
            .expect("tab A")
            .begin_search_request("alpha".to_owned())
            .expect("search A request");
        let search_b = window
            .tab_mut(tab_b)
            .expect("tab B")
            .begin_search_request("beta".to_owned())
            .expect("search B request");
        for (context, location, input) in [
            (search_a.clone(), location_a.clone(), "alpha"),
            (search_b.clone(), location_b.clone(), "beta"),
        ] {
            sta.submit(ExplorerCommand::StartSearch {
                context,
                location,
                input: explorer_model::SearchInput::new(input),
            })
            .expect("submit per-tab search");
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut terminals = std::collections::HashSet::new();
        while terminals.len() < 2 {
            if let Some(event) = sta.try_recv_event().expect("per-tab search event") {
                if let ExplorerEvent::SearchFinished { context, .. } = &event {
                    terminals.insert(context.request_id);
                }
                let _ = window.apply_event(event);
            }
            assert!(Instant::now() < deadline, "two-tab search timed out");
            thread::sleep(Duration::from_millis(2));
        }
        let result_names = |window: &ExplorerWindowState, tab_id: TabId| {
            window
                .tabs()
                .iter()
                .find(|tab| tab.id == tab_id)
                .and_then(explorer_model::TabState::search_results)
                .expect("search results")
                .entries()
                .iter()
                .map(|entry| entry.display_name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(result_names(&window, tab_a), vec!["alpha.txt"]);
        assert_eq!(result_names(&window, tab_b), vec!["beta.txt"]);

        let replaced = window
            .tab_mut(tab_b)
            .expect("tab B")
            .begin_search_request("name:never".to_owned())
            .expect("replaced request");
        sta.submit(ExplorerCommand::StartSearch {
            context: replaced.clone(),
            location: location_b.clone(),
            input: explorer_model::SearchInput::new("name:never"),
        })
        .expect("submit replaced search");
        let replacement = window
            .tab_mut(tab_b)
            .expect("tab B")
            .begin_search_request("beta".to_owned())
            .expect("replacement request");
        assert!(replaced.cancellation.is_cancelled());
        sta.submit(ExplorerCommand::StartSearch {
            context: replacement.clone(),
            location: location_b.clone(),
            input: explorer_model::SearchInput::new("beta"),
        })
        .expect("submit replacement search");
        let replacement_deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(event) = sta.try_recv_event().expect("replacement event") {
                let terminal = matches!(
                    &event,
                    ExplorerEvent::SearchFinished { context, .. }
                        if context.request_id == replacement.request_id
                );
                let _ = window.apply_event(event);
                if terminal {
                    break;
                }
            }
            assert!(
                Instant::now() < replacement_deadline,
                "replacement search timed out"
            );
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(result_names(&window, tab_b), vec!["beta.txt"]);

        let cancelled = window
            .tab_mut(tab_b)
            .expect("tab B")
            .begin_search_request("beta".to_owned())
            .expect("navigation-cancelled search");
        sta.submit(ExplorerCommand::StartSearch {
            context: cancelled.clone(),
            location: location_b.clone(),
            input: explorer_model::SearchInput::new("beta"),
        })
        .expect("submit navigation-cancelled search");
        drive_window_location(&sta, &mut window, tab_b, location_partial.clone(), false);
        assert!(cancelled.cancellation.is_cancelled());

        let partial_context = window
            .tab_mut(tab_b)
            .expect("tab B")
            .begin_search_request("name:never".to_owned())
            .expect("partial search");
        sta.submit(ExplorerCommand::StartSearch {
            context: partial_context.clone(),
            location: location_partial,
            input: explorer_model::SearchInput::new("name:never"),
        })
        .expect("submit partial search");
        let partial_deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(event) = sta.try_recv_event().expect("partial event") {
                let terminal = matches!(
                    &event,
                    ExplorerEvent::SearchFinished { context, .. }
                        if context.request_id == partial_context.request_id
                );
                let _ = window.apply_event(event);
                if terminal {
                    break;
                }
            }
            assert!(
                Instant::now() < partial_deadline,
                "partial search timed out"
            );
            thread::sleep(Duration::from_millis(2));
        }
        let partial_tab = window
            .tabs()
            .iter()
            .find(|tab| tab.id == tab_b)
            .expect("partial tab");
        assert!(matches!(
            partial_tab.search,
            explorer_model::TabSearchState::Partial { .. }
        ));
        assert!(
            partial_tab
                .search_sources
                .iter()
                .any(|status| { status.backend == explorer_model::SearchBackend::LocalIndex })
        );
        sta.shutdown_and_join(Duration::from_secs(2))
            .expect("stop search STA");
    }

    #[allow(
        unsafe_code,
        reason = "test fixture applies documented Windows hidden/system attributes"
    )]
    fn set_hidden_system(path: &std::path::Path) {
        let path = windows::core::HSTRING::from(path.as_os_str().to_string_lossy().as_ref());
        // SAFETY: path is a live NUL-terminated HSTRING and the fixture owns this file.
        unsafe {
            windows::Win32::Storage::FileSystem::SetFileAttributesW(
                &path,
                windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_HIDDEN
                    | windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_SYSTEM,
            )
        }
        .expect("set hidden/system attributes");
    }

    fn navigate_window(
        sta: &ShellStaHandle,
        window: &mut ExplorerWindowState,
        location: LocationDescriptor,
    ) {
        let context = window
            .active_tab_mut()
            .begin_navigation_request()
            .expect("navigation request");
        sta.submit(ExplorerCommand::Navigate { context, location })
            .expect("submit navigation");
        drain_directory_request(sta, window);
    }

    fn drain_directory_request(sta: &ShellStaHandle, window: &mut ExplorerWindowState) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match sta.try_recv_event().expect("receive Shell event") {
                Some(event) => {
                    let terminal = event.is_terminal();
                    let _ = window.apply_event(event);
                    if terminal {
                        return;
                    }
                }
                None => thread::sleep(Duration::from_millis(2)),
            }
        }
        panic!("directory request did not terminate before deadline");
    }
}
