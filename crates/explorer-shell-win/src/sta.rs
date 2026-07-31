//! Dedicated Windows Shell STA lifecycle and message pump.

use std::{
    collections::HashMap,
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError},
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
const EVENT_QUEUE_CAPACITY: usize = 4_096;

static ACTIVE_STA_THREADS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_CONTROL_CHANNELS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_JOIN_HANDLES: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_BREADCRUMB_WORKERS: AtomicUsize = AtomicUsize::new(0);

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
}

impl StaResourceSnapshot {
    /// Captures implementation-owned resources without enumerating unrelated process handles.
    pub fn capture() -> Self {
        Self {
            active_threads: ACTIVE_STA_THREADS.load(Ordering::Acquire),
            active_control_channels: ACTIVE_CONTROL_CHANNELS.load(Ordering::Acquire),
            active_join_handles: ACTIVE_JOIN_HANDLES.load(Ordering::Acquire),
            active_breadcrumb_workers: ACTIVE_BREADCRUMB_WORKERS.load(Ordering::Acquire),
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
    control: SyncSender<ControlMessage>,
    events: Mutex<Receiver<ExplorerEvent>>,
    active_requests: Mutex<HashMap<RequestId, CancellationToken>>,
    done: Mutex<Receiver<()>>,
    join: Mutex<Option<JoinHandle<()>>>,
    state: Arc<AtomicU8>,
    pump_cycles: Arc<AtomicUsize>,
    shutdown_requested: AtomicBool,
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
        let (control_tx, control_rx) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let (event_tx, event_rx) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        ACTIVE_CONTROL_CHANNELS.fetch_add(1, Ordering::AcqRel);

        let join = thread::Builder::new()
            .name("explorer-shell-sta".to_owned())
            .spawn(move || {
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

                let mut runtime = StaRuntime::default();
                loop {
                    match control_rx.recv_timeout(MESSAGE_PUMP_INTERVAL) {
                        Ok(ControlMessage::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                        Ok(ControlMessage::Command { command, queued_at }) => {
                            let outcome =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    process_command(&command, queued_at, &event_tx, &mut runtime);
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
                                    let _ = event_tx.try_send(ExplorerEvent::Failed {
                                        context: context.clone(),
                                        error: ExplorerError::new(
                                            ExplorerErrorKind::Internal,
                                            "Shell STA command panic",
                                            true,
                                            "The operation failed, but Explorer can continue.",
                                            message,
                                        ),
                                    });
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
            })
            .map_err(|error| {
                ACTIVE_CONTROL_CHANNELS.fetch_sub(1, Ordering::AcqRel);
                ShellStaError::Spawn(error)
            })?;
        ACTIVE_JOIN_HANDLES.fetch_add(1, Ordering::AcqRel);

        match ready_rx.recv_timeout(timeout) {
            Ok(Ok(())) => {
                tracing::info!(?correlation_id, "Shell STA is ready");
                Ok(Self {
                    correlation_id,
                    control: control_tx,
                    events: Mutex::new(event_rx),
                    active_requests: Mutex::new(HashMap::new()),
                    done: Mutex::new(done_rx),
                    join: Mutex::new(Some(join)),
                    state,
                    pump_cycles,
                    shutdown_requested: AtomicBool::new(false),
                })
            }
            Ok(Err(error)) => {
                let _ = join.join();
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                Err(error)
            }
            Err(RecvTimeoutError::Timeout) => {
                let _ = control_tx.try_send(ControlMessage::Shutdown);
                tracing::error!(?correlation_id, ?timeout, "Shell STA startup timed out");
                drop(join);
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                Err(ShellStaError::StartupTimeout { timeout })
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = join.join();
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
                token.cancel();
            }
        } else if let Some(context) = command.context() {
            self.active_requests
                .lock()
                .map_err(|_| ShellStaEndpointError::Poisoned)?
                .insert(context.request_id, context.cancellation.clone());
        }
        match self.control.try_send(ControlMessage::Command {
            command,
            queued_at: Instant::now(),
        }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(ShellStaEndpointError::CommandQueueFull),
            Err(TrySendError::Disconnected(_)) => {
                Err(ShellStaEndpointError::CommandEndpointDisconnected)
            }
        }
    }

    /// Receives one pending owned event without blocking the caller.
    ///
    /// # Errors
    ///
    /// Returns only synchronization or endpoint disconnect errors; an empty queue is `Ok(None)`.
    pub fn try_recv_event(&self) -> Result<Option<ExplorerEvent>, ShellStaEndpointError> {
        let event = match self
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
        };
        if event.is_terminal()
            && let Some(context) = event.context()
        {
            self.active_requests
                .lock()
                .map_err(|_| ShellStaEndpointError::Poisoned)?
                .remove(&context.request_id);
        }
        Ok(Some(event))
    }

    /// Requests shutdown at most once and never blocks the caller.
    pub fn shutdown(&self) {
        if self.shutdown_requested.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Ok(requests) = self.active_requests.lock() {
            for token in requests.values() {
                token.cancel();
            }
        }
        if matches!(self.state(), ShellStaState::Ready) {
            self.state
                .store(ShellStaState::Stopping as u8, Ordering::Release);
        }
        match self.control.try_send(ControlMessage::Shutdown) {
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
            return Ok(());
        };
        match done.recv_timeout(timeout) {
            Ok(()) => {
                let result = thread.join();
                ACTIVE_JOIN_HANDLES.fetch_sub(1, Ordering::AcqRel);
                result.map_err(|_| ShellStaError::ThreadPanicked)
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
}

struct StaRuntime {
    watchers: HashMap<explorer_model::TabId, crate::watcher::WatcherSession>,
    clipboard: crate::clipboard::ClipboardRuntime,
    icon_cache: crate::icon::ShellIconCache,
}

impl Default for StaRuntime {
    fn default() -> Self {
        Self {
            watchers: HashMap::new(),
            clipboard: crate::clipboard::ClipboardRuntime::new(),
            icon_cache: crate::icon::ShellIconCache::default(),
        }
    }
}

impl StaRuntime {
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
            process_navigation(&context, location, events)
        }
        ExplorerCommand::ResolveAncestry { .. }
        | ExplorerCommand::EnumerateChildContainers { .. } => {
            start_brokered_breadcrumb(command, events);
            Ok(())
        }
        ExplorerCommand::OpenItem {
            item, disposition, ..
        } => match disposition {
            OpenDisposition::CurrentTab | OpenDisposition::NewTab => {
                process_navigation(&context, &item.location, events)
            }
            OpenDisposition::DefaultApplication => crate::navigation::open_default(&item.location)
                .and_then(|()| {
                    events
                        .try_send(ExplorerEvent::OperationFinished {
                            context: context.clone(),
                            outcome: OperationTerminal::Finished,
                        })
                        .map_err(|error| event_send_error(&error))
                }),
        },
        ExplorerCommand::ExecuteFileOperation { request, .. } => {
            let outcome = operation_terminal(
                crate::file_operation::execute(&context, request, events),
                "execute_file_operation",
            );
            events
                .try_send(ExplorerEvent::OperationFinished {
                    context: context.clone(),
                    outcome,
                })
                .map_err(|error| event_send_error(&error))
        }
        ExplorerCommand::DataTransfer { request, .. } => {
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
                DataTransferRequest::Paste {
                    destination,
                    conflict,
                } => runtime
                    .clipboard
                    .paste_request(destination.clone(), *conflict)
                    .and_then(|(operation, data, mode)| {
                        let outcome = crate::file_operation::execute(&context, &operation, events)?;
                        runtime.clipboard.complete_paste(&data, mode, &outcome);
                        let _ = events.try_send(ExplorerEvent::ClipboardChanged {
                            state: runtime.clipboard.state(),
                        });
                        Ok(outcome)
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
                .and_then(|operation| crate::file_operation::execute(&context, &operation, events)),
            };
            let outcome = operation_terminal(result, "data_transfer");
            events
                .try_send(ExplorerEvent::OperationFinished {
                    context: context.clone(),
                    outcome,
                })
                .map_err(|error| event_send_error(&error))
        }
        ExplorerCommand::ShowContextMenu { request, .. } => {
            if request.requested_verb.as_deref().is_some_and(|verb| {
                verb.eq_ignore_ascii_case("properties")
                    || verb.eq_ignore_ascii_case("Windows.Share")
                    || verb.eq_ignore_ascii_case("PinToStartScreen")
            }) {
                crate::context_menu::run_host_owned(&context, request, events);
            } else {
                crate::context_menu::start_brokered(
                    context.clone(),
                    request.clone(),
                    events.clone(),
                );
            }
            Ok(())
        }
        ExplorerCommand::Cancel { .. } => Ok(()),
        ExplorerCommand::StartSearch {
            location, input, ..
        } => crate::search::execute(&context, location, input, events),
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
            events
                .try_send(event)
                .map_err(|error| event_send_error(&error))
        }
        ExplorerCommand::LoadThumbnail {
            key,
            location,
            cache_only,
            ..
        } => {
            let request = explorer_model::ThumbnailRequest::new(
                context.clone(),
                key.clone(),
                explorer_model::ThumbnailPriority::ActiveVisible,
            );
            let outcome = crate::thumbnail::load_shell_thumbnail(
                &request,
                location,
                *cache_only,
                explorer_common::RoadmapLimits::default().thumbnail_memory_bytes,
            );
            let _ = request.claim_terminal(&outcome);
            events
                .try_send(ExplorerEvent::ThumbnailFinished {
                    context: context.clone(),
                    key: key.clone(),
                    outcome,
                })
                .map_err(|error| event_send_error(&error))
        }
        ExplorerCommand::ClearThumbnailCache { .. } => events
            .try_send(ExplorerEvent::ThumbnailCacheCleared {
                context: context.clone(),
                success: crate::thumbnail::clear_thumbnail_disk_cache(),
            })
            .map_err(|error| event_send_error(&error)),
        ExplorerCommand::PreviewHost { command, .. } => events
            .try_send(ExplorerEvent::PreviewHostFinished {
                context: context.clone(),
                outcome: explorer_model::PreviewHostTerminal::Failed {
                    generation: command.generation(),
                    error: explorer_model::PreviewHostError::Unsupported,
                },
            })
            .map_err(|error| event_send_error(&error)),
        ExplorerCommand::DiscoverLockOwners { request, .. } => {
            crate::restart_manager::start_discovery(
                context.clone(),
                request.clone(),
                events.clone(),
            );
            Ok(())
        }
        ExplorerCommand::CloseLockOwners { request, .. } => {
            crate::restart_manager::start_close(context.clone(), request.clone(), events.clone());
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
        let _ = events.try_send(ExplorerEvent::Failed {
            context: context.clone(),
            error,
        });
    }
    tracing::debug!(
        request_id = ?context.request_id,
        tab_id = ?context.tab_id,
        generation = context.generation.value(),
        elapsed_micros = started.elapsed().as_micros(),
        "Shell STA command finished"
    );
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

struct BreadcrumbWorkerGuard;

impl Drop for BreadcrumbWorkerGuard {
    fn drop(&mut self) {
        ACTIVE_BREADCRUMB_WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Runs extension-controlled Shell namespace work outside the application's long-lived STA.
/// The coordinator owns the exactly-once terminal gate; a provider that never returns can leave
/// only its disposable apartment blocked and cannot stall navigation, input, or shutdown.
fn start_brokered_breadcrumb(command: &ExplorerCommand, events: &SyncSender<ExplorerEvent>) {
    start_bounded_breadcrumb_job(
        command,
        events,
        BREADCRUMB_PROVIDER_TIMEOUT,
        |worker_command, worker_events, worker_gate| match ApartmentGuard::initialize() {
            Ok(_apartment) => match &worker_command {
                ExplorerCommand::ResolveAncestry {
                    context, location, ..
                } => {
                    let _ = process_ancestry(context, location, &worker_events, &worker_gate);
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
                        &worker_gate,
                    );
                }
                _ => unreachable!("broker accepts breadcrumb commands only"),
            },
            Err(error) => send_breadcrumb_broker_failure(
                &worker_command,
                &worker_events,
                &worker_gate,
                format!("isolated provider apartment initialization failed: {error}"),
            ),
        },
    );
}

fn start_bounded_breadcrumb_job<F>(
    command: &ExplorerCommand,
    events: &SyncSender<ExplorerEvent>,
    deadline: Duration,
    job: F,
) where
    F: FnOnce(ExplorerCommand, SyncSender<ExplorerEvent>, Arc<AtomicBool>) + Send + 'static,
{
    let Some(context) = command.context().cloned() else {
        return;
    };
    let terminal_sent = Arc::new(AtomicBool::new(false));
    let worker_gate = Arc::clone(&terminal_sent);
    let worker_command = command.clone();
    let worker_events = events.clone();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let worker_name = format!("breadcrumb-provider-{:?}", context.request_id);
    let worker = thread::Builder::new().name(worker_name).spawn(move || {
        ACTIVE_BREADCRUMB_WORKERS.fetch_add(1, Ordering::AcqRel);
        let _worker_guard = BreadcrumbWorkerGuard;
        let panic_command = worker_command.clone();
        let panic_events = worker_events.clone();
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
            &terminal_sent,
            format!("could not start isolated provider worker: {error}"),
        );
        return;
    }

    let timeout_command = command.clone();
    let timeout_events = events.clone();
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
            &terminal_sent,
            format!("could not start provider deadline coordinator: {error}"),
        );
    }
}

fn send_breadcrumb_broker_failure(
    command: &ExplorerCommand,
    events: &SyncSender<ExplorerEvent>,
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
                terminal_sent,
            );
        }
        _ => {}
    }
}

fn process_ancestry(
    context: &explorer_model::RequestContext,
    location: &LocationDescriptor,
    events: &SyncSender<ExplorerEvent>,
    terminal_sent: &AtomicBool,
) -> Result<(), ExplorerError> {
    if context.cancellation.is_cancelled() {
        return send_ancestry_terminal(
            context,
            BreadcrumbTerminal::Cancelled,
            events,
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
                    terminal_sent,
                );
            }
        };
        segments = shell_ancestry_segments(chain);
    }
    events
        .try_send(ExplorerEvent::AncestryBatch {
            context: context.clone(),
            segments: segments.clone(),
        })
        .map_err(|error| event_send_error(&error))?;

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
    events
        .try_send(ExplorerEvent::AncestryBatch {
            context: context.clone(),
            segments: enriched,
        })
        .map_err(|error| event_send_error(&error))?;
    send_ancestry_terminal(context, BreadcrumbTerminal::Finished, events, terminal_sent)
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

fn send_ancestry_terminal(
    context: &explorer_model::RequestContext,
    outcome: BreadcrumbTerminal,
    events: &SyncSender<ExplorerEvent>,
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
    events
        .try_send(ExplorerEvent::AncestryFinished {
            context: context.clone(),
            outcome,
        })
        .map_err(|error| event_send_error(&error))
}

fn process_child_containers(
    context: &explorer_model::RequestContext,
    parent: &LocationDescriptor,
    segment_id: BreadcrumbSegmentId,
    menu_generation: u64,
    events: &SyncSender<ExplorerEvent>,
    terminal_sent: &AtomicBool,
) -> Result<(), ExplorerError> {
    if context.cancellation.is_cancelled() {
        return send_child_terminal(
            context,
            segment_id,
            menu_generation,
            BreadcrumbTerminal::Cancelled,
            events,
            terminal_sent,
        );
    }
    let mut child_count = 0_usize;
    let completed =
        match crate::navigation::enumerate_child_containers(context, parent, |children| {
            child_count = child_count.saturating_add(children.len());
            events
                .try_send(ExplorerEvent::ChildContainersBatch {
                    context: context.clone(),
                    segment_id,
                    menu_generation,
                    children,
                })
                .map_err(|error| event_send_error(&error))
        }) {
            Ok(completed) => completed,
            Err(error) => {
                return send_child_terminal(
                    context,
                    segment_id,
                    menu_generation,
                    BreadcrumbTerminal::Failed(error),
                    events,
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
        terminal_sent,
    )
}

fn send_child_terminal(
    context: &explorer_model::RequestContext,
    segment_id: BreadcrumbSegmentId,
    menu_generation: u64,
    outcome: BreadcrumbTerminal,
    events: &SyncSender<ExplorerEvent>,
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
    events
        .try_send(ExplorerEvent::ChildContainersFinished {
            context: context.clone(),
            segment_id,
            menu_generation,
            outcome,
        })
        .map_err(|error| event_send_error(&error))
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
    events
        .try_send(ExplorerEvent::DirectoryFinished {
            context: context.clone(),
        })
        .map_err(|error| event_send_error(&error))
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
        ShellStaError, ShellStaHandle, ShellStaState, StaResourceSnapshot, filesystem_ancestry,
        send_breadcrumb_broker_failure, shell_ancestry_segments, start_bounded_breadcrumb_job,
        watchable_directory_path,
    };
    use explorer_model::{
        BreadcrumbSegmentId, BreadcrumbTerminal, ClipboardMode, ClipboardState, ConflictDecision,
        DataTransferRequest, ExplorerCommand, ExplorerEvent, ExplorerService, ExplorerWindowState,
        FileOperationFlags, FileOperationKind, FileOperationRequest, Generation, HistoryEntry,
        ItemDescriptor, JournalPreimage, JournalValidation, LocationDescriptor, OperationJournal,
        OperationTerminal, RequestContext, ShellItemId, ShellNewItemRecipe, TabId, ViewAnchor,
    };
    use explorer_test_support::{OwnedTempFixture, validate_breadcrumb_contract};
    use std::{
        fs,
        mem::size_of_val,
        path::PathBuf,
        process::Command,
        sync::{Mutex, mpsc},
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
        let started = Instant::now();
        start_bounded_breadcrumb_job(
            &command,
            &events_tx,
            Duration::from_millis(25),
            |command, events, terminal_gate| {
                thread::sleep(Duration::from_millis(350));
                send_breadcrumb_broker_failure(
                    &command,
                    &events,
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
        assert_eq!(during.active_join_handles, before.active_join_handles + 1);

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
