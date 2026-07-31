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
//! Broker lifecycle policy. Trust boundary: app -> broker -> one disposable worker per operation.
//!
//! Threats covered here are hung, crashed, reentrant, oversized, malformed, stale, replayed,
//! child-spawning, unload-failing, and path-disclosing extensions. Requests are bounded and
//! authenticated; workers are disposable; repeated handler failures enter expiring quarantine.

use std::{
    collections::HashMap,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use explorer_extension_protocol::{
    BrokerRequestId, ContextMenuPayload, Frame, FrameDecoder, MAXIMUM_FRAME_BYTES, MessageKind,
    OperationClass as ProtocolOperationClass, PROTOCOL_VERSION, PreviewMessage,
    PreviewStartPayload, SessionNonce, StartPayload, ThumbnailPayload, ThumbnailResultPayload,
    authenticate,
};
use uuid::Uuid;

/// Exactly-once terminal arbitration shared by timeout, cancellation, process
/// disconnect, and successful completion paths.
#[derive(Debug, Default)]
pub struct TerminalGate(std::sync::atomic::AtomicBool);

impl TerminalGate {
    pub fn claim(&self) -> bool {
        !self.0.swap(true, Ordering::AcqRel)
    }
}

/// Privacy-safe broker event. Handler identity is an opaque digest and detail
/// is reduced to a stable category rather than path or document content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrokerDiagnosticEvent {
    pub correlation: u64,
    pub operation: OperationClass,
    pub handler_digest: Option<String>,
    pub category: BrokerDiagnosticCategory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrokerDiagnosticCategory {
    Ready,
    Unavailable,
    VersionMismatch,
    Crash,
    Timeout,
    Quarantined,
    Protocol,
    Cancelled,
}

/// Independent time budgets applied by the app client, supervisor, and handler worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrokerDeadlinePolicy {
    pub request: Duration,
    pub worker: Duration,
    pub handler: Duration,
}

impl BrokerDeadlinePolicy {
    pub fn validate(self) -> bool {
        !self.request.is_zero()
            && !self.worker.is_zero()
            && !self.handler.is_zero()
            && self.handler <= self.worker
            && self.worker <= self.request
    }
}

impl Default for BrokerDeadlinePolicy {
    fn default() -> Self {
        Self {
            request: Duration::from_secs(10),
            worker: Duration::from_secs(8),
            handler: Duration::from_secs(5),
        }
    }
}

/// The sole externally observable terminal classification for one broker request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrokerTerminal {
    Success,
    Error,
    Cancelled,
    Timeout,
    Crash,
    Disconnected,
}

/// Records exactly one terminal reason while allowing every racing path to finish safely.
#[derive(Debug, Default)]
pub struct BrokerTerminalArbiter {
    gate: TerminalGate,
    winner: Mutex<Option<BrokerTerminal>>,
}

impl BrokerTerminalArbiter {
    pub fn claim(&self, terminal: BrokerTerminal) -> bool {
        if !self.gate.claim() {
            return false;
        }
        let mut winner = self
            .winner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *winner = Some(terminal);
        true
    }

    pub fn winner(&self) -> Option<BrokerTerminal> {
        *self
            .winner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Duplicates one explicitly borrowed capability handle into a worker process.
///
/// The source and process handles are borrowed, so this safe wrapper cannot close caller-owned
/// resources. The returned handle has one owner and is closed by `OwnedHandle`.
///
/// # Errors
/// Returns the Windows error from `DuplicateHandle`.
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "DuplicateHandle and conversion of its uniquely owned result require audited Windows FFI"
)]
pub fn duplicate_capability_handle(
    source: std::os::windows::io::BorrowedHandle<'_>,
    target_process: std::os::windows::io::BorrowedHandle<'_>,
    inheritable: bool,
) -> std::io::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _};
    use windows::Win32::{
        Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE},
        System::Threading::GetCurrentProcess,
    };
    let mut duplicated = HANDLE::default();
    // SAFETY: both inputs are live borrowed handles; the output slot is writable and transfers
    // exactly one handle on success.
    unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            HANDLE(source.as_raw_handle()),
            HANDLE(target_process.as_raw_handle()),
            &raw mut duplicated,
            0,
            inheritable,
            DUPLICATE_SAME_ACCESS,
        )
    }
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    // SAFETY: DuplicateHandle transferred unique ownership of this non-null handle.
    Ok(unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(duplicated.0) })
}

pub fn diagnostic_category(error: &BrokerClientError) -> BrokerDiagnosticCategory {
    match error {
        BrokerClientError::Unavailable => BrokerDiagnosticCategory::Unavailable,
        BrokerClientError::VersionMismatch => BrokerDiagnosticCategory::VersionMismatch,
        BrokerClientError::Timeout => BrokerDiagnosticCategory::Timeout,
        BrokerClientError::Protocol => BrokerDiagnosticCategory::Protocol,
        BrokerClientError::Start | BrokerClientError::Disconnected => {
            BrokerDiagnosticCategory::Crash
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OperationClass {
    ContextMenu,
    Thumbnail,
    Namespace,
    Preview,
}

/// Shared app-side lifecycle for one persistent broker supervisor generation.
#[derive(Clone)]
pub struct BrokerClient {
    inner: Arc<BrokerClientInner>,
}

struct BrokerClientInner {
    executable: PathBuf,
    policy: BrokerPolicy,
    runtime: Mutex<BrokerRuntime>,
    active_pid: AtomicU32,
    active_worker_pid: AtomicU32,
}

#[derive(Debug, Default)]
struct BrokerRuntime {
    session: Option<BrokerSession>,
    generation: u64,
    next_request_id: u64,
    broker_launches: u64,
    handshakes: u64,
    requests: u64,
    restarts: u64,
    shutdowns: u64,
    last_broker_pid: Option<u32>,
    last_worker_pid: Option<u32>,
}

#[derive(Debug)]
struct BrokerSession {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: mpsc::Receiver<Result<Frame, BrokerClientError>>,
    reader: Option<thread::JoinHandle<()>>,
    nonce: SessionNonce,
}

/// Privacy-safe lifecycle counters used by diagnostics and process-boundary tests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BrokerLifecycleSnapshot {
    pub generation: u64,
    pub broker_pid: Option<u32>,
    pub worker_pid: Option<u32>,
    pub broker_launches: u64,
    pub handshakes: u64,
    pub requests: u64,
    pub restarts: u64,
    pub shutdowns: u64,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum BrokerClientError {
    #[error("broker binary is unavailable")]
    Unavailable,
    #[error("broker binary protocol version is incompatible")]
    VersionMismatch,
    #[error("broker process could not be started")]
    Start,
    #[error("broker request timed out")]
    Timeout,
    #[error("broker disconnected")]
    Disconnected,
    #[error("broker protocol failed")]
    Protocol,
}

fn decode_context_menu_terminal(
    text: &str,
    target: &explorer_model::ShellContextMenuTarget,
) -> Result<explorer_model::ContextMenuOutcome, BrokerClientError> {
    if text == "context-menu-cancelled" {
        return Ok(explorer_model::ContextMenuOutcome::Cancelled);
    }
    if let Some(offset) = text.strip_prefix("context-menu-invoked:") {
        let command_offset = offset.parse().map_err(|_| BrokerClientError::Protocol)?;
        return Ok(explorer_model::ContextMenuOutcome::Invoked { command_offset });
    }
    let payload = text
        .strip_prefix("context-menu-delegated:")
        .ok_or(BrokerClientError::Protocol)?;
    let (offset, command) = payload.split_once(':').ok_or(BrokerClientError::Protocol)?;
    let command_offset = offset.parse().map_err(|_| BrokerClientError::Protocol)?;
    let command = explorer_model::ContextMenuHostCommand::from_wire_name(command)
        .ok_or(BrokerClientError::Protocol)?;
    Ok(explorer_model::ContextMenuOutcome::Delegated {
        command_offset,
        command,
        target: target.clone(),
    })
}

impl BrokerClient {
    pub fn new(executable: impl Into<PathBuf>, policy: BrokerPolicy) -> Self {
        Self {
            inner: Arc::new(BrokerClientInner {
                executable: executable.into(),
                policy,
                runtime: Mutex::new(BrokerRuntime {
                    next_request_id: 1,
                    ..BrokerRuntime::default()
                }),
                active_pid: AtomicU32::new(0),
                active_worker_pid: AtomicU32::new(0),
            }),
        }
    }

    pub fn adjacent_to(application: &Path, policy: BrokerPolicy) -> Self {
        let executable = application
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("explorer-extension-broker.exe");
        Self::new(executable, policy)
    }

    /// Starts or reuses the broker and completes its authenticated compatibility handshake.
    ///
    /// # Errors
    /// Returns a typed unavailable/start/version error.
    pub fn verify(&self) -> Result<(), BrokerClientError> {
        let mut runtime = self
            .inner
            .runtime
            .lock()
            .map_err(|_| BrokerClientError::Protocol)?;
        self.inner.ensure_session(&mut runtime)
    }

    /// Returns whether the configured adjacent executable exists without starting it.
    pub fn is_available(&self) -> bool {
        self.inner.executable.is_file()
    }

    /// Returns a privacy-safe snapshot of the shared supervisor lifecycle.
    pub fn lifecycle_snapshot(&self) -> BrokerLifecycleSnapshot {
        if let Ok(runtime) = self.inner.runtime.try_lock() {
            BrokerLifecycleSnapshot {
                generation: runtime.generation,
                broker_pid: runtime.session.as_ref().map(|session| session.child.id()),
                worker_pid: runtime.last_worker_pid,
                broker_launches: runtime.broker_launches,
                handshakes: runtime.handshakes,
                requests: runtime.requests,
                restarts: runtime.restarts,
                shutdowns: runtime.shutdowns,
                active: runtime.session.is_some(),
            }
        } else {
            let broker_pid = self.inner.active_pid.load(Ordering::Acquire);
            let worker_pid = self.inner.active_worker_pid.load(Ordering::Acquire);
            BrokerLifecycleSnapshot {
                broker_pid: (broker_pid != 0).then_some(broker_pid),
                worker_pid: (worker_pid != 0).then_some(worker_pid),
                active: broker_pid != 0,
                ..BrokerLifecycleSnapshot::default()
            }
        }
    }

    /// Invokes one authenticated request through the shared persistent broker session.
    ///
    /// # Errors
    /// Returns typed process, timeout, disconnect, or protocol failures.
    pub fn invoke(&self, kind: MessageKind, payload: Vec<u8>) -> Result<Frame, BrokerClientError> {
        self.invoke_with_timeout(kind, payload, self.inner.policy.operation_timeout, None)
    }

    fn invoke_with_timeout(
        &self,
        kind: MessageKind,
        payload: Vec<u8>,
        timeout: Duration,
        cancellation: Option<&explorer_model::CancellationToken>,
    ) -> Result<Frame, BrokerClientError> {
        let mut runtime = self
            .inner
            .runtime
            .lock()
            .map_err(|_| BrokerClientError::Protocol)?;
        self.inner.ensure_session(&mut runtime)?;
        let request_id = BrokerRequestId(runtime.next_request_id);
        runtime.next_request_id = runtime.next_request_id.saturating_add(1);
        runtime.requests = runtime.requests.saturating_add(1);
        let response = {
            let session = runtime.session.as_mut().ok_or(BrokerClientError::Start)?;
            exchange(
                session,
                kind,
                request_id,
                payload,
                timeout,
                Some(&self.inner.active_worker_pid),
                cancellation,
            )
        };
        match response {
            Ok(response) => {
                if kind == MessageKind::Start && response.feature_bits != 0 {
                    runtime.last_worker_pid = Some(response.feature_bits);
                }
                self.inner.active_worker_pid.store(0, Ordering::Release);
                Ok(response)
            }
            Err(error) => {
                runtime.restarts = runtime.restarts.saturating_add(1);
                invalidate_session(&mut runtime);
                self.inner.active_pid.store(0, Ordering::Release);
                self.inner.active_worker_pid.store(0, Ordering::Release);
                Err(error)
            }
        }
    }

    /// Terminates only the disposable extension worker currently serving a request.
    /// The persistent broker remains available for the next request.
    pub fn cancel_active_worker(&self) {
        terminate_process_by_id(self.inner.active_worker_pid.load(Ordering::Acquire));
    }

    /// Gracefully closes and reaps the shared broker generation. Repeated calls are harmless.
    pub fn shutdown(&self) {
        match self.inner.runtime.try_lock() {
            Ok(mut runtime) => self.inner.shutdown_locked(&mut runtime),
            Err(std::sync::TryLockError::WouldBlock) => {
                terminate_process_by_id(self.inner.active_pid.load(Ordering::Acquire));
                terminate_process_by_id(self.inner.active_worker_pid.load(Ordering::Acquire));
                let deadline = Instant::now()
                    + self
                        .inner
                        .policy
                        .cancel_grace
                        .max(Duration::from_millis(250));
                loop {
                    match self.inner.runtime.try_lock() {
                        Ok(mut runtime) => {
                            self.inner.shutdown_locked(&mut runtime);
                            break;
                        }
                        Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(
                            std::sync::TryLockError::WouldBlock
                            | std::sync::TryLockError::Poisoned(_),
                        ) => {
                            self.inner.active_pid.store(0, Ordering::Release);
                            self.inner.active_worker_pid.store(0, Ordering::Release);
                            break;
                        }
                    }
                }
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                terminate_process_by_id(self.inner.active_pid.load(Ordering::Acquire));
                terminate_process_by_id(self.inner.active_worker_pid.load(Ordering::Acquire));
            }
        }
    }

    /// Executes a complete Shell context-menu session in a disposable worker.
    /// No Shell interface or provider-owned menu state enters the app process.
    ///
    /// # Errors
    /// Returns an error when encoding, broker transport, authentication, or response decoding fails.
    pub fn show_context_menu(
        &self,
        request: &explorer_model::ContextMenuRequest,
        cancellation: &explorer_model::CancellationToken,
    ) -> Result<explorer_model::ContextMenuOutcome, BrokerClientError> {
        let (background, descriptors) = match &request.target {
            explorer_model::ShellContextMenuTarget::Background { parent } => {
                (true, vec![encode_location_descriptor(parent)])
            }
            explorer_model::ShellContextMenuTarget::Items { items, .. } => (
                false,
                items
                    .iter()
                    .map(|item| encode_location_descriptor(&item.location))
                    .collect(),
            ),
        };
        let payload = ContextMenuPayload {
            version: ContextMenuPayload::VERSION,
            background,
            owner_hwnd: request.owner_window,
            point_x: request.point.x,
            point_y: request.point.y,
            keyboard_invoked: request.keyboard_invoked,
            invocation_profile: u8::from(request.invocation_profile.extended_verbs()),
            item_descriptors: descriptors,
            verb: request.requested_verb.clone(),
        }
        .encode()
        .map_err(|_| BrokerClientError::Protocol)?;
        let start = StartPayload {
            operation: ProtocolOperationClass::ContextMenu,
            flags: 0x8000_0000,
            descriptor: payload,
        }
        .encode()
        .map_err(|_| BrokerClientError::Protocol)?;
        let response = self.invoke_with_timeout(
            MessageKind::Start,
            start,
            self.inner.policy.interactive_timeout,
            Some(cancellation),
        )?;
        let text =
            std::str::from_utf8(&response.payload).map_err(|_| BrokerClientError::Protocol)?;
        if let Ok(outcome) = decode_context_menu_terminal(text, &request.target) {
            return Ok(outcome);
        }
        Err(if text == "timeout" {
            BrokerClientError::Timeout
        } else if text.contains("unavailable") {
            BrokerClientError::Disconnected
        } else {
            BrokerClientError::Protocol
        })
    }

    /// Extracts a thumbnail in a disposable provider process and returns only validated pixels.
    ///
    /// # Errors
    /// Returns an error when encoding, broker transport, or terminal pixel validation fails.
    pub fn load_thumbnail(
        &self,
        key: &explorer_model::ThumbnailRequestKey,
        location: &explorer_model::LocationDescriptor,
        cache_only: bool,
    ) -> Result<explorer_model::ThumbnailTerminal, BrokerClientError> {
        let payload = ThumbnailPayload {
            item_descriptor: encode_location_descriptor(location),
            physical_size: key.physical_size,
            dpi: key.dpi,
            cache_only,
        }
        .encode()
        .map_err(|_| BrokerClientError::Protocol)?;
        let start = StartPayload {
            operation: ProtocolOperationClass::Thumbnail,
            flags: 0x8000_0000,
            descriptor: payload,
        }
        .encode()
        .map_err(|_| BrokerClientError::Protocol)?;
        let response = self.invoke(MessageKind::Start, start)?;
        let terminal = ThumbnailResultPayload::decode(&response.payload)
            .map_err(|_| BrokerClientError::Protocol)?;
        Ok(match terminal {
            ThumbnailResultPayload::Ready {
                source,
                width,
                height,
                stride,
                pixels,
            } => explorer_model::ThumbnailTerminal::Ready {
                source: match source {
                    1 => explorer_model::ThumbnailSource::DiskCache,
                    2 => explorer_model::ThumbnailSource::WindowsCache,
                    3 => explorer_model::ThumbnailSource::Provider,
                    4 => explorer_model::ThumbnailSource::ShellIcon,
                    _ => return Err(BrokerClientError::Protocol),
                },
                pixels: explorer_model::ThumbnailPixels {
                    width,
                    height,
                    stride,
                    bytes: pixels,
                },
            },
            ThumbnailResultPayload::Fallback { reason } => {
                explorer_model::ThumbnailTerminal::Fallback(match reason {
                    1 => explorer_model::ThumbnailFallbackReason::Offline,
                    2 => explorer_model::ThumbnailFallbackReason::Unsupported,
                    3 => explorer_model::ThumbnailFallbackReason::Timeout,
                    4 => explorer_model::ThumbnailFallbackReason::Cancelled,
                    5 => explorer_model::ThumbnailFallbackReason::Corrupt,
                    6 => explorer_model::ThumbnailFallbackReason::ProviderFailure,
                    7 => explorer_model::ThumbnailFallbackReason::ResourceLimit,
                    _ => return Err(BrokerClientError::Protocol),
                })
            }
            ThumbnailResultPayload::Failed => {
                explorer_model::ThumbnailTerminal::Failed("brokered provider failed".to_owned())
            }
        })
    }

    /// Starts one persistent Preview Handler worker attached to an app-owned HWND boundary.
    ///
    /// # Errors
    ///
    /// Returns [`BrokerClientError`] for invalid geometry, encoding or protocol failures,
    /// cancellation, timeout, unavailable handlers, or broker/worker disconnection.
    pub fn start_preview_session(
        &self,
        location: &explorer_model::LocationDescriptor,
        parent_hwnd: isize,
        bounds: explorer_model::PreviewHostBounds,
        cancellation: &explorer_model::CancellationToken,
    ) -> Result<explorer_model::PreviewInitializationMode, BrokerClientError> {
        if !bounds.is_valid() || parent_hwnd <= 0 {
            return Err(BrokerClientError::Protocol);
        }
        let parent_hwnd = u64::try_from(parent_hwnd).map_err(|_| BrokerClientError::Protocol)?;
        let payload = PreviewStartPayload {
            item_descriptor: encode_location_descriptor(location),
            generation: bounds.generation.value(),
            parent_hwnd,
            left: bounds.left_physical,
            top: bounds.top_physical,
            width: bounds.width_physical,
            height: bounds.height_physical,
            dpi: bounds.dpi,
        }
        .encode()
        .map_err(|_| BrokerClientError::Protocol)?;
        let start = StartPayload {
            operation: ProtocolOperationClass::Preview,
            flags: 0x4000_0000,
            descriptor: payload,
        }
        .encode()
        .map_err(|_| BrokerClientError::Protocol)?;
        let response = self.invoke_with_timeout(
            MessageKind::Start,
            start,
            self.inner.policy.interactive_timeout,
            Some(cancellation),
        )?;
        let text =
            std::str::from_utf8(&response.payload).map_err(|_| BrokerClientError::Protocol)?;
        match text.strip_prefix("preview-ready:") {
            Some("File") => Ok(explorer_model::PreviewInitializationMode::File),
            Some("Stream") => Ok(explorer_model::PreviewInitializationMode::Stream),
            Some("ShellItem") => Ok(explorer_model::PreviewInitializationMode::ShellItem),
            _ if text == "timeout" => Err(BrokerClientError::Timeout),
            _ if text == "preview-unavailable" || text == "preview-quarantined" => {
                Err(BrokerClientError::Unavailable)
            }
            _ => Err(BrokerClientError::Protocol),
        }
    }

    /// Sends a generation-bound resize, focus, accelerator, or unload command to the active
    /// preview worker. Lookup/attach are start-only and are rejected by the worker state machine.
    ///
    /// # Errors
    ///
    /// Returns [`BrokerClientError`] when the command is invalid, times out, the worker is no
    /// longer active, or the broker returns an unexpected protocol response.
    pub fn preview_session_command(
        &self,
        message: &PreviewMessage,
    ) -> Result<(), BrokerClientError> {
        let payload = message.encode().map_err(|_| BrokerClientError::Protocol)?;
        let start = StartPayload {
            operation: ProtocolOperationClass::Preview,
            flags: 0x2000_0000,
            descriptor: payload,
        }
        .encode()
        .map_err(|_| BrokerClientError::Protocol)?;
        let response = self.invoke_with_timeout(
            MessageKind::Start,
            start,
            self.inner.policy.operation_timeout,
            None,
        )?;
        match response.payload.as_slice() {
            b"preview-ok" => Ok(()),
            b"timeout" => Err(BrokerClientError::Timeout),
            b"worker-disconnect" | b"preview-not-active" => Err(BrokerClientError::Disconnected),
            _ => Err(BrokerClientError::Protocol),
        }
    }

    /// Converts the platform-neutral model command into the private wire protocol. Session start
    /// remains a separate operation because it also carries the parent window and item identity.
    ///
    /// # Errors
    ///
    /// Returns [`BrokerClientError`] when the command cannot be represented by the active-session
    /// protocol or when the brokered command fails.
    pub fn update_preview_session(
        &self,
        command: &explorer_model::PreviewHostCommand,
    ) -> Result<(), BrokerClientError> {
        let message = match command {
            explorer_model::PreviewHostCommand::SetBounds(bounds) => PreviewMessage::SetBounds {
                generation: bounds.generation.value(),
                left: bounds.left_physical,
                top: bounds.top_physical,
                width: bounds.width_physical,
                height: bounds.height_physical,
                dpi: bounds.dpi,
            },
            explorer_model::PreviewHostCommand::SetFocus { generation } => {
                PreviewMessage::SetFocus {
                    generation: generation.value(),
                }
            }
            explorer_model::PreviewHostCommand::Accelerator {
                generation,
                virtual_key,
                modifiers,
            } => PreviewMessage::Accelerator {
                generation: generation.value(),
                virtual_key: *virtual_key,
                modifiers: *modifiers,
            },
            explorer_model::PreviewHostCommand::Unload { generation } => PreviewMessage::Unload {
                generation: generation.value(),
            },
            explorer_model::PreviewHostCommand::Start { .. } => {
                return Err(BrokerClientError::Protocol);
            }
        };
        self.preview_session_command(&message)
    }

    /// Enumerates an opaque/third-party namespace in a disposable worker. Returned rows are
    /// owned, deserialized with model bounds, and capped before they reach the UI service queue.
    ///
    /// # Errors
    /// Returns an error when encoding, broker transport, deserialization, or item bounds fail.
    pub fn enumerate_namespace(
        &self,
        location: &explorer_model::LocationDescriptor,
        maximum_items: usize,
    ) -> Result<Vec<explorer_model::FileEntry>, BrokerClientError> {
        let maximum = maximum_items.clamp(1, 4_096);
        let start = StartPayload {
            operation: ProtocolOperationClass::Namespace,
            flags: 0x8000_0000 | u32::try_from(maximum).unwrap_or(4_096),
            descriptor: encode_location_descriptor(location),
        }
        .encode()
        .map_err(|_| BrokerClientError::Protocol)?;
        let response = self.invoke(MessageKind::Start, start)?;
        let entries: Vec<explorer_model::FileEntry> =
            serde_json::from_slice(&response.payload).map_err(|_| BrokerClientError::Protocol)?;
        if entries.len() > maximum {
            return Err(BrokerClientError::Protocol);
        }
        Ok(entries)
    }
}

impl std::fmt::Debug for BrokerClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrokerClient")
            .field("executable", &self.inner.executable)
            .field("policy", &self.inner.policy)
            .field("lifecycle", &self.lifecycle_snapshot())
            .finish()
    }
}

impl BrokerClientInner {
    fn ensure_session(&self, runtime: &mut BrokerRuntime) -> Result<(), BrokerClientError> {
        if let Some(session) = runtime.session.as_mut() {
            if let Ok(None) = session.child.try_wait() {
                return Ok(());
            }
            runtime.restarts = runtime.restarts.saturating_add(1);
            invalidate_session(runtime);
            self.active_pid.store(0, Ordering::Release);
        }
        if !self.executable.is_file() {
            return Err(BrokerClientError::Unavailable);
        }
        let nonce = SessionNonce(*Uuid::new_v4().as_bytes());
        let mut command = Command::new(&self.executable);
        command
            .env("EXPLORER_BROKER_NONCE", encode_nonce(nonce))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|_| BrokerClientError::Start)?;
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BrokerClientError::Start);
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BrokerClientError::Start);
        };
        let pid = child.id();
        let (sender, responses) = mpsc::channel();
        let reader = thread::Builder::new()
            .name(format!("extension-broker-reader-{pid}"))
            .spawn(move || read_broker_frames(stdout, &sender))
            .map_err(|_| BrokerClientError::Start)?;
        runtime.generation = runtime.generation.saturating_add(1);
        runtime.broker_launches = runtime.broker_launches.saturating_add(1);
        runtime.last_broker_pid = Some(pid);
        self.active_pid.store(pid, Ordering::Release);
        runtime.session = Some(BrokerSession {
            child,
            stdin: Some(stdin),
            responses,
            reader: Some(reader),
            nonce,
        });
        let handshake = {
            let session = runtime.session.as_mut().ok_or(BrokerClientError::Start)?;
            exchange(
                session,
                MessageKind::Hello,
                BrokerRequestId(0),
                Vec::new(),
                self.policy.ready_timeout,
                None,
                None,
            )
        };
        let compatible = handshake.is_ok_and(|response| {
            response.kind == MessageKind::HelloAck && marker_is_compatible(&response.payload)
        });
        if !compatible {
            invalidate_session(runtime);
            self.active_pid.store(0, Ordering::Release);
            return Err(BrokerClientError::VersionMismatch);
        }
        runtime.handshakes = runtime.handshakes.saturating_add(1);
        Ok(())
    }

    fn shutdown_locked(&self, runtime: &mut BrokerRuntime) {
        shutdown_runtime(runtime, self.policy);
        self.active_pid.store(0, Ordering::Release);
        self.active_worker_pid.store(0, Ordering::Release);
    }
}

impl Drop for BrokerClientInner {
    fn drop(&mut self) {
        let policy = self.policy;
        if let Ok(runtime) = self.runtime.get_mut() {
            shutdown_runtime(runtime, policy);
        }
        self.active_pid.store(0, Ordering::Release);
        self.active_worker_pid.store(0, Ordering::Release);
    }
}

fn shutdown_runtime(runtime: &mut BrokerRuntime, policy: BrokerPolicy) {
    let Some(mut session) = runtime.session.take() else {
        return;
    };
    let request_id = BrokerRequestId(runtime.next_request_id);
    runtime.next_request_id = runtime.next_request_id.saturating_add(1);
    runtime.shutdowns = runtime.shutdowns.saturating_add(1);
    let graceful = exchange(
        &mut session,
        MessageKind::Shutdown,
        request_id,
        Vec::new(),
        policy.cancel_grace.max(Duration::from_millis(250)),
        None,
        None,
    )
    .is_ok_and(|response| {
        response.kind == MessageKind::Terminal && response.payload == b"shutdown"
    });
    reap_session(session, graceful);
}

fn marker_is_compatible(payload: &[u8]) -> bool {
    let Ok(marker) = std::str::from_utf8(payload) else {
        return false;
    };
    marker.contains(&format!("\"protocol\":{PROTOCOL_VERSION}"))
        && marker.contains("\"arch\":\"x64\"")
        && marker.contains("\"role\":\"supervisor\"")
        && marker.contains(&format!("\"build\":\"{}\"", env!("CARGO_PKG_VERSION")))
}

fn read_broker_frames(
    mut stdout: std::process::ChildStdout,
    sender: &mpsc::Sender<Result<Frame, BrokerClientError>>,
) {
    let mut decoder = FrameDecoder::new(MAXIMUM_FRAME_BYTES);
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = match stdout.read(&mut buffer) {
            Ok(0) => {
                let error = if decoder.finish().is_ok() {
                    BrokerClientError::Disconnected
                } else {
                    BrokerClientError::Protocol
                };
                let _ = sender.send(Err(error));
                return;
            }
            Ok(count) => count,
            Err(_) => {
                let _ = sender.send(Err(BrokerClientError::Disconnected));
                return;
            }
        };
        if decoder.push(&buffer[..count]).is_err() {
            let _ = sender.send(Err(BrokerClientError::Protocol));
            return;
        }
        loop {
            match decoder.next_frame() {
                Ok(Some(frame)) => {
                    if sender.send(Ok(frame)).is_err() {
                        return;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    let _ = sender.send(Err(BrokerClientError::Protocol));
                    return;
                }
            }
        }
    }
}

fn exchange(
    session: &mut BrokerSession,
    kind: MessageKind,
    request_id: BrokerRequestId,
    payload: Vec<u8>,
    timeout: Duration,
    active_worker_pid: Option<&AtomicU32>,
    cancellation: Option<&explorer_model::CancellationToken>,
) -> Result<Frame, BrokerClientError> {
    let bytes = Frame::new(kind, 0, session.nonce, request_id, payload)
        .encode(MAXIMUM_FRAME_BYTES)
        .map_err(|_| BrokerClientError::Protocol)?;
    let stdin = session
        .stdin
        .as_mut()
        .ok_or(BrokerClientError::Disconnected)?;
    stdin
        .write_all(&bytes)
        .and_then(|()| stdin.flush())
        .map_err(|_| BrokerClientError::Disconnected)?;
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(BrokerClientError::Timeout);
        }
        if cancellation.is_some_and(explorer_model::CancellationToken::is_cancelled)
            && let Some(active_worker_pid) = active_worker_pid
        {
            terminate_process_by_id(active_worker_pid.load(Ordering::Acquire));
        }
        let poll = remaining.min(Duration::from_millis(25));
        let response = match session.responses.recv_timeout(poll) {
            Ok(result) => result?,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(BrokerClientError::Disconnected);
            }
        };
        authenticate(&response, session.nonce).map_err(|_| BrokerClientError::Protocol)?;
        if response.request_id != request_id {
            return Err(BrokerClientError::Protocol);
        }
        if response.kind == MessageKind::Progress {
            if let Some(active_worker_pid) = active_worker_pid
                && response.feature_bits != 0
            {
                active_worker_pid.store(response.feature_bits, Ordering::Release);
            }
            continue;
        }
        return Ok(response);
    }
}

fn invalidate_session(runtime: &mut BrokerRuntime) {
    if let Some(session) = runtime.session.take() {
        reap_session(session, false);
    }
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "bounded shutdown may need to terminate the owned broker while another client thread waits on IPC"
)]
fn terminate_process_by_id(process_id: u32) {
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    if process_id == 0 {
        return;
    }
    if let Ok(process) = unsafe { OpenProcess(PROCESS_TERMINATE, false, process_id) } {
        let _ = unsafe { TerminateProcess(process, 2) };
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(process) };
    }
}

#[cfg(not(windows))]
fn terminate_process_by_id(_process_id: u32) {}

fn reap_session(mut session: BrokerSession, graceful: bool) {
    session.stdin.take();
    if !graceful {
        let _ = session.child.kill();
    }
    let deadline = Instant::now() + Duration::from_millis(750);
    loop {
        match session.child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) | Err(_) => {
                let _ = session.child.kill();
                let _ = session.child.wait();
                break;
            }
        }
    }
    if let Some(reader) = session.reader.take() {
        let _ = reader.join();
    }
}

fn encode_location_descriptor(location: &explorer_model::LocationDescriptor) -> Vec<u8> {
    let (kind, value) = match location {
        explorer_model::LocationDescriptor::FileSystem(path) => {
            (b'F', path.to_string_lossy().into_owned())
        }
        explorer_model::LocationDescriptor::ParsingName(value) => (b'P', value.clone()),
        explorer_model::LocationDescriptor::KnownFolder(value) => {
            let mut bytes = Vec::with_capacity(17);
            bytes.push(b'K');
            bytes.extend_from_slice(value);
            return bytes;
        }
        explorer_model::LocationDescriptor::ShellNamespace(value) => {
            let mut bytes = Vec::with_capacity(value.len().saturating_add(1));
            bytes.push(b'S');
            bytes.extend_from_slice(value);
            return bytes;
        }
    };
    let mut bytes = Vec::with_capacity(value.len().saturating_add(1));
    bytes.push(kind);
    bytes.extend_from_slice(value.as_bytes());
    bytes
}

/// Decodes a model descriptor only inside the broker/worker package. The protocol crate remains
/// model-free and treats these as opaque capability bytes.
///
/// # Errors
/// Returns an error when the descriptor kind, UTF-8, or Known Folder payload is invalid.
pub fn decode_location_descriptor(
    bytes: &[u8],
) -> Result<explorer_model::LocationDescriptor, BrokerClientError> {
    let (&kind, value) = bytes.split_first().ok_or(BrokerClientError::Protocol)?;
    match kind {
        b'F' => Ok(explorer_model::LocationDescriptor::file_system(
            std::str::from_utf8(value).map_err(|_| BrokerClientError::Protocol)?,
        )),
        b'P' => Ok(explorer_model::LocationDescriptor::ParsingName(
            std::str::from_utf8(value)
                .map_err(|_| BrokerClientError::Protocol)?
                .to_owned(),
        )),
        b'K' => Ok(explorer_model::LocationDescriptor::KnownFolder(
            value.try_into().map_err(|_| BrokerClientError::Protocol)?,
        )),
        b'S' => Ok(explorer_model::LocationDescriptor::ShellNamespace(
            value.to_vec(),
        )),
        _ => Err(BrokerClientError::Protocol),
    }
}

fn encode_nonce(nonce: SessionNonce) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(32);
    for byte in nonce.0 {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrokerPolicy {
    pub ready_timeout: Duration,
    pub operation_timeout: Duration,
    pub interactive_timeout: Duration,
    pub cancel_grace: Duration,
    pub maximum_workers: usize,
    pub worker_memory_bytes: usize,
}

impl Default for BrokerPolicy {
    fn default() -> Self {
        Self {
            ready_timeout: Duration::from_secs(3),
            operation_timeout: Duration::from_secs(10),
            interactive_timeout: Duration::from_secs(300),
            cancel_grace: Duration::from_millis(500),
            maximum_workers: 4,
            worker_memory_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
struct QuarantineEntry {
    failures: usize,
    until: Option<Instant>,
}

#[derive(Clone, Debug)]
pub struct QuarantineRegistry {
    entries: HashMap<String, QuarantineEntry>,
    threshold: usize,
    duration: Duration,
    capacity: usize,
}

impl QuarantineRegistry {
    pub fn new(threshold: usize, duration: Duration, capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            threshold: threshold.max(1),
            duration,
            capacity: capacity.max(1),
        }
    }

    pub fn record_failure(&mut self, handler_digest: String, now: Instant) -> bool {
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&handler_digest) {
            if let Some(oldest) = self.entries.keys().next().cloned() {
                self.entries.remove(&oldest);
            }
        }
        let entry = self
            .entries
            .entry(handler_digest)
            .or_insert(QuarantineEntry {
                failures: 0,
                until: None,
            });
        entry.failures = entry.failures.saturating_add(1);
        if entry.failures >= self.threshold {
            let exponent = entry.failures.saturating_sub(self.threshold).min(8);
            let multiplier = 1_u32 << u32::try_from(exponent).unwrap_or(8);
            let backoff = self
                .duration
                .checked_mul(multiplier)
                .unwrap_or(Duration::MAX);
            entry.until = now.checked_add(backoff);
        }
        entry.until.is_some_and(|until| until > now)
    }

    pub fn is_quarantined(&mut self, handler_digest: &str, now: Instant) -> bool {
        let Some(entry) = self.entries.get_mut(handler_digest) else {
            return false;
        };
        if entry.until.is_some_and(|until| until > now) {
            return true;
        }
        if entry.until.is_some() {
            entry.failures = 0;
            entry.until = None;
        }
        false
    }

    pub fn reset(&mut self, handler_digest: &str) -> bool {
        self.entries.remove(handler_digest).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_is_bounded_while_an_in_flight_request_owns_the_runtime() {
        let client = BrokerClient::new(
            PathBuf::from("missing-broker.exe"),
            BrokerPolicy {
                cancel_grace: Duration::from_millis(50),
                ..BrokerPolicy::default()
            },
        );
        let inner = Arc::clone(&client.inner);
        let (locked_tx, locked_rx) = mpsc::channel();
        let holder = thread::spawn(move || {
            let _runtime = inner.runtime.lock().expect("runtime lock");
            locked_tx.send(()).expect("lock signal");
            thread::sleep(Duration::from_secs(1));
        });
        locked_rx.recv().expect("runtime held");

        let started = Instant::now();
        client.shutdown();

        assert!(started.elapsed() < Duration::from_millis(750));
        holder.join().expect("runtime holder");
    }

    #[test]
    fn host_context_command_response_is_decoded_without_localized_text() {
        let target = explorer_model::ShellContextMenuTarget::Background {
            parent: explorer_model::LocationDescriptor::file_system(r"C:\fixture"),
        };
        assert_eq!(
            decode_context_menu_terminal("context-menu-delegated:12:rename", &target),
            Ok(explorer_model::ContextMenuOutcome::Delegated {
                command_offset: 12,
                command: explorer_model::ContextMenuHostCommand::Rename,
                target: target.clone(),
            })
        );
        for command in [
            explorer_model::ContextMenuHostCommand::Open,
            explorer_model::ContextMenuHostCommand::Cut,
            explorer_model::ContextMenuHostCommand::Copy,
            explorer_model::ContextMenuHostCommand::CopyPath,
            explorer_model::ContextMenuHostCommand::CreateShortcut,
            explorer_model::ContextMenuHostCommand::Delete,
            explorer_model::ContextMenuHostCommand::Rename,
            explorer_model::ContextMenuHostCommand::Share,
            explorer_model::ContextMenuHostCommand::PinToStart,
            explorer_model::ContextMenuHostCommand::ToggleQuickAccess,
            explorer_model::ContextMenuHostCommand::Properties,
        ] {
            assert_eq!(
                decode_context_menu_terminal(
                    &format!("context-menu-delegated:12:{}", command.wire_name()),
                    &target
                ),
                Ok(explorer_model::ContextMenuOutcome::Delegated {
                    command_offset: 12,
                    command,
                    target: target.clone(),
                })
            );
        }
        assert_eq!(
            decode_context_menu_terminal("context-menu-delegated:12:provider.command", &target),
            Err(BrokerClientError::Protocol)
        );
    }
    #[test]
    fn quarantine_threshold_expiry_capacity_and_manual_reset_are_bounded() {
        let now = Instant::now();
        let mut registry = QuarantineRegistry::new(2, Duration::from_millis(10), 1);
        assert!(!registry.record_failure("a".to_owned(), now));
        assert!(registry.record_failure("a".to_owned(), now));
        assert!(registry.is_quarantined("a", now));
        assert!(!registry.is_quarantined("a", now + Duration::from_secs(1)));
        registry.record_failure("b".to_owned(), now);
        assert!(!registry.is_quarantined("a", now));
        assert!(registry.reset("b"));
    }

    #[test]
    fn repeated_failures_use_bounded_exponential_backoff() {
        let now = Instant::now();
        let mut registry = QuarantineRegistry::new(1, Duration::from_secs(1), 4);
        assert!(registry.record_failure("handler".to_owned(), now));
        assert!(registry.record_failure("handler".to_owned(), now));
        assert!(registry.is_quarantined("handler", now + Duration::from_millis(1_500)));
        assert!(!registry.is_quarantined("handler", now + Duration::from_secs(3)));
    }

    #[test]
    fn concurrent_terminal_paths_have_one_winner() {
        let gate = Arc::new(TerminalGate::default());
        let winners = (0..32)
            .map(|_| {
                let gate = Arc::clone(&gate);
                thread::spawn(move || usize::from(gate.claim()))
            })
            .map(|worker| worker.join().expect("terminal contender"))
            .sum::<usize>();
        assert_eq!(winners, 1);
    }

    #[test]
    fn diagnostics_never_accept_paths_or_content() {
        let event = BrokerDiagnosticEvent {
            correlation: 42,
            operation: OperationClass::Preview,
            handler_digest: Some("sha256:0123".to_owned()),
            category: diagnostic_category(&BrokerClientError::Timeout),
        };
        let rendered = format!("{event:?}");
        assert!(!rendered.contains(r"C:\\"));
        assert!(!rendered.contains("document"));
        assert_eq!(event.category, BrokerDiagnosticCategory::Timeout);
    }

    #[test]
    fn every_terminal_race_has_one_typed_winner_and_independent_deadlines() {
        let policy = BrokerDeadlinePolicy::default();
        assert!(policy.validate());
        assert!(policy.handler < policy.worker && policy.worker < policy.request);
        let arbiter = Arc::new(BrokerTerminalArbiter::default());
        let contenders = [
            BrokerTerminal::Success,
            BrokerTerminal::Error,
            BrokerTerminal::Cancelled,
            BrokerTerminal::Timeout,
            BrokerTerminal::Crash,
            BrokerTerminal::Disconnected,
        ];
        let accepted = contenders
            .into_iter()
            .map(|terminal| {
                let arbiter = Arc::clone(&arbiter);
                thread::spawn(move || arbiter.claim(terminal))
            })
            .map(|worker| worker.join().expect("terminal contender"))
            .filter(|accepted| *accepted)
            .count();
        assert_eq!(accepted, 1);
        assert!(arbiter.winner().is_some());
    }

    #[cfg(windows)]
    #[test]
    #[allow(
        unsafe_code,
        reason = "borrowing the documented current-process pseudo handle is confined to this test"
    )]
    fn duplicated_capability_handle_is_independently_owned_and_readable() {
        use std::{
            io::{Read as _, Seek as _},
            os::windows::io::{AsHandle as _, BorrowedHandle, RawHandle},
        };
        let mut fixture = tempfile::tempfile().expect("capability fixture");
        fixture
            .write_all(b"least-authority")
            .expect("fixture write");
        fixture.rewind().expect("fixture rewind");
        // A real process handle is required as the target; current_process is pseudo and borrowed
        // only for the duration of DuplicateHandle.
        // SAFETY: GetCurrentProcess returns the documented non-owning pseudo handle.
        let pseudo = unsafe { windows::Win32::System::Threading::GetCurrentProcess() };
        // SAFETY: the pseudo current-process handle remains valid for the complete call.
        let process_handle = unsafe { BorrowedHandle::borrow_raw(pseudo.0 as RawHandle) };
        let duplicate = duplicate_capability_handle(fixture.as_handle(), process_handle, false)
            .expect("duplicate file capability");
        let mut duplicate_file = std::fs::File::from(duplicate);
        let mut text = String::new();
        duplicate_file
            .read_to_string(&mut text)
            .expect("read duplicated capability");
        assert_eq!(text, "least-authority");
    }
}
