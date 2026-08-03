//! Typed identities and request correlation shared by pure Explorer state.

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use explorer_common::{RequestDeadline, RequestId};
use serde::{Deserialize, Deserializer, Serialize, de};
use uuid::Uuid;

/// Stable identity of one tab for the lifetime of an application session.
#[derive(Clone, Copy, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TabId(Uuid);

impl TabId {
    /// Allocates a new opaque tab identity.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TabId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for TabId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("TabId").field(&self.0).finish()
    }
}

/// Monotonic request generation owned by one tab.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct Generation(u64);

impl Generation {
    /// Creates a generation from a persisted or test value.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric generation for protocol serialization.
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Advances the generation, returning `None` instead of wrapping stale work into validity.
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Provider-defined stable identity for a Shell item.
#[derive(Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ShellItemId(Vec<u8>);

impl ShellItemId {
    /// Creates an identity from non-empty opaque provider bytes.
    pub fn from_provider_bytes(bytes: impl Into<Vec<u8>>) -> Option<Self> {
        let bytes = bytes.into();
        (!bytes.is_empty()).then_some(Self(bytes))
    }

    /// Returns opaque bytes for the owning provider boundary.
    pub fn provider_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for ShellItemId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShellItemId")
            .field("opaque_byte_count", &self.0.len())
            .finish()
    }
}

/// Maximum encoded payload accepted for one reconstructible location descriptor.
pub const MAX_LOCATION_DESCRIPTOR_BYTES: usize = 64 * 1024;

/// Project-owned roots that are resolved before calling the Windows Shell adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum SyntheticRoot {
    Home,
    QuickAccess,
}

impl SyntheticRoot {
    const HOME_NAME: &'static str = "super-explorer:home";
    const QUICK_ACCESS_NAME: &'static str = "super-explorer:quick-access";

    const fn parsing_name(self) -> &'static str {
        match self {
            Self::Home => Self::HOME_NAME,
            Self::QuickAccess => Self::QUICK_ACCESS_NAME,
        }
    }

    fn from_parsing_name(value: &str) -> Option<Self> {
        match value {
            Self::HOME_NAME => Some(Self::Home),
            Self::QUICK_ACCESS_NAME => Some(Self::QuickAccess),
            _ => None,
        }
    }
}

/// Resolvable location data; it is a descriptor and never an item identity.
#[derive(Clone, Eq, Hash, PartialEq, Serialize)]
pub enum LocationDescriptor {
    /// A local or UNC filesystem path.
    FileSystem(PathBuf),
    /// Opaque provider data for a non-path Shell namespace location.
    ShellNamespace(Vec<u8>),
    /// A canonical Shell parsing name such as `shell:Downloads`.
    ParsingName(String),
    /// A `KNOWNFOLDERID` encoded in its canonical big-endian UUID representation.
    KnownFolder([u8; 16]),
}

impl LocationDescriptor {
    /// Creates a filesystem descriptor without resolving or touching the path.
    pub fn file_system(path: impl Into<PathBuf>) -> Self {
        Self::FileSystem(path.into())
    }

    /// Creates a validated filesystem descriptor for an external or persistence boundary.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is empty or exceeds the descriptor byte limit.
    pub fn try_file_system(
        path: impl Into<PathBuf>,
    ) -> Result<Self, LocationDescriptorValidationError> {
        Self::FileSystem(path.into()).validated()
    }

    /// Creates validated opaque non-path Shell namespace bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the payload is empty or exceeds the descriptor byte limit.
    pub fn try_shell_namespace(
        bytes: impl Into<Vec<u8>>,
    ) -> Result<Self, LocationDescriptorValidationError> {
        Self::ShellNamespace(bytes.into()).validated()
    }

    /// Creates a validated Shell parsing-name descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error when the name is empty or exceeds the descriptor byte limit.
    pub fn try_parsing_name(
        value: impl Into<String>,
    ) -> Result<Self, LocationDescriptorValidationError> {
        Self::ParsingName(value.into()).validated()
    }

    /// Creates a typed project-owned synthetic root descriptor.
    pub fn synthetic(root: SyntheticRoot) -> Self {
        Self::ParsingName(root.parsing_name().to_owned())
    }

    /// Returns the typed synthetic root represented by this descriptor.
    pub fn synthetic_root(&self) -> Option<SyntheticRoot> {
        match self {
            Self::ParsingName(value) => SyntheticRoot::from_parsing_name(value),
            Self::FileSystem(_) | Self::ShellNamespace(_) | Self::KnownFolder(_) => None,
        }
    }

    /// Returns the provider payload size used by persistence and IPC bounds.
    pub fn encoded_payload_len(&self) -> usize {
        match self {
            Self::FileSystem(path) => path.as_os_str().len(),
            Self::ShellNamespace(bytes) => bytes.len(),
            Self::ParsingName(value) => value.len(),
            Self::KnownFolder(bytes) => bytes.len(),
        }
    }

    /// Validates non-empty and bounded external payload invariants.
    ///
    /// # Errors
    ///
    /// Returns an error when the descriptor is empty or exceeds the descriptor byte limit.
    pub fn validate(&self) -> Result<(), LocationDescriptorValidationError> {
        let empty = match self {
            Self::FileSystem(path) => path.as_os_str().is_empty(),
            Self::ShellNamespace(bytes) => bytes.is_empty(),
            Self::ParsingName(value) => value.is_empty(),
            Self::KnownFolder(_) => false,
        };
        if empty {
            return Err(LocationDescriptorValidationError::Empty);
        }
        let bytes = self.encoded_payload_len();
        if bytes > MAX_LOCATION_DESCRIPTOR_BYTES {
            return Err(LocationDescriptorValidationError::TooLarge {
                bytes,
                maximum: MAX_LOCATION_DESCRIPTOR_BYTES,
            });
        }
        Ok(())
    }

    fn validated(self) -> Result<Self, LocationDescriptorValidationError> {
        self.validate()?;
        Ok(self)
    }

    /// Borrows the filesystem path when this is a filesystem descriptor.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::FileSystem(path) => Some(path),
            Self::ShellNamespace(_) | Self::ParsingName(_) | Self::KnownFolder(_) => None,
        }
    }
}

#[derive(Deserialize)]
enum LocationDescriptorWire {
    FileSystem(PathBuf),
    ShellNamespace(Vec<u8>),
    ParsingName(String),
    KnownFolder([u8; 16]),
}

impl<'de> Deserialize<'de> for LocationDescriptor {
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        let wire = LocationDescriptorWire::deserialize(deserializer)?;
        let descriptor = match wire {
            LocationDescriptorWire::FileSystem(path) => Self::FileSystem(path),
            LocationDescriptorWire::ShellNamespace(bytes) => Self::ShellNamespace(bytes),
            LocationDescriptorWire::ParsingName(value) => Self::ParsingName(value),
            LocationDescriptorWire::KnownFolder(bytes) => Self::KnownFolder(bytes),
        };
        descriptor.validated().map_err(de::Error::custom)
    }
}

/// Failure to validate an external reconstructible location descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocationDescriptorValidationError {
    Empty,
    TooLarge { bytes: usize, maximum: usize },
}

impl fmt::Display for LocationDescriptorValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("location descriptor payload must not be empty"),
            Self::TooLarge { bytes, maximum } => write!(
                formatter,
                "location descriptor payload is {bytes} bytes; maximum is {maximum}"
            ),
        }
    }
}

impl std::error::Error for LocationDescriptorValidationError {}

impl fmt::Debug for LocationDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileSystem(_) => {
                formatter.write_str("LocationDescriptor::FileSystem(<redacted>)")
            }
            Self::ShellNamespace(bytes) => formatter
                .debug_tuple("LocationDescriptor::ShellNamespace")
                .field(&format_args!("<{} opaque bytes>", bytes.len()))
                .finish(),
            Self::ParsingName(_) => {
                formatter.write_str("LocationDescriptor::ParsingName(<redacted>)")
            }
            Self::KnownFolder(_) => {
                formatter.write_str("LocationDescriptor::KnownFolder(<opaque-guid>)")
            }
        }
    }
}

impl fmt::Display for LocationDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileSystem(_) => formatter.write_str("<filesystem-location>"),
            Self::ShellNamespace(_) => formatter.write_str("<shell-namespace-location>"),
            Self::ParsingName(_) => formatter.write_str("<shell-parsing-name>"),
            Self::KnownFolder(_) => formatter.write_str("<known-folder>"),
        }
    }
}

/// Cooperative cancellation state shared by one request and its workers.
#[derive(Default)]
struct CancellationState {
    cancelled: AtomicBool,
    callbacks: Mutex<HashMap<Uuid, Arc<dyn Fn() + Send + Sync>>>,
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<CancellationState>);

/// Aggregate outcome of one cooperative cancellation signal.
///
/// Callback panics are contained per callback so one faulty observer cannot
/// prevent later observers from receiving the cancellation notification.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CancellationSignalReport {
    pub callbacks_invoked: usize,
    pub panicked_callbacks: usize,
    pub already_cancelled: bool,
}

impl CancellationToken {
    /// Creates an active token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation. Repeated calls are idempotent.
    ///
    /// Each registered callback runs outside the token mutex and behind its own
    /// panic boundary. Call [`Self::cancel_with_report`] when aggregate callback
    /// diagnostics are required.
    pub fn cancel(&self) {
        let _ = self.cancel_with_report();
    }

    /// Requests cancellation and reports isolated callback failures.
    ///
    /// Repeated calls are idempotent. Each registered callback runs outside the
    /// token mutex and behind its own panic boundary.
    pub fn cancel_with_report(&self) -> CancellationSignalReport {
        let callbacks = {
            let mut callbacks = self
                .0
                .callbacks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if self.0.cancelled.swap(true, Ordering::AcqRel) {
                return CancellationSignalReport {
                    already_cancelled: true,
                    ..CancellationSignalReport::default()
                };
            }
            std::mem::take(&mut *callbacks)
        };
        let mut report = CancellationSignalReport::default();
        for callback in callbacks.into_values() {
            report.callbacks_invoked += 1;
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback())).is_err() {
                report.panicked_callbacks += 1;
            }
        }
        report
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    /// Registers a notification invoked once if cancellation occurs.
    ///
    /// If cancellation already happened, the callback is invoked synchronously and the returned
    /// registration is inert. Dropping an active registration removes its callback.
    pub fn register(
        &self,
        callback: impl Fn() + Send + Sync + 'static,
    ) -> CancellationRegistration {
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(callback);
        let mut callbacks = self
            .0
            .callbacks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.0.cancelled.load(Ordering::Acquire) {
            drop(callbacks);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback()));
            return CancellationRegistration::inert();
        }
        let id = Uuid::new_v4();
        callbacks.insert(id, callback);
        CancellationRegistration {
            state: Arc::downgrade(&self.0),
            id: Some(id),
        }
    }
}

impl fmt::Debug for CancellationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CancellationToken")
            .field("is_cancelled", &self.is_cancelled())
            .finish()
    }
}

/// Owned cancellation subscription; dropping it unregisters the callback.
pub struct CancellationRegistration {
    state: Weak<CancellationState>,
    id: Option<Uuid>,
}

impl CancellationRegistration {
    const fn inert() -> Self {
        Self {
            state: Weak::new(),
            id: None,
        }
    }
}

impl fmt::Debug for CancellationRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CancellationRegistration")
            .field("active", &self.id.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for CancellationRegistration {
    fn drop(&mut self) {
        let Some(id) = self.id.take() else {
            return;
        };
        let Some(state) = self.state.upgrade() else {
            return;
        };
        state
            .callbacks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id);
    }
}

/// Correlation and cancellation data attached to one asynchronous request.
#[derive(Clone, Debug)]
pub struct RequestContext {
    pub request_id: RequestId,
    pub tab_id: TabId,
    pub generation: Generation,
    pub cancellation: CancellationToken,
    pub deadline: RequestDeadline,
}

impl PartialEq for RequestContext {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
            && self.tab_id == other.tab_id
            && self.generation == other.generation
    }
}

impl Eq for RequestContext {}

impl RequestContext {
    /// Allocates a request context for a tab generation.
    pub fn new(tab_id: TabId, generation: Generation) -> Self {
        Self {
            request_id: RequestId::new(),
            tab_id,
            generation,
            cancellation: CancellationToken::new(),
            deadline: RequestDeadline::none(),
        }
    }

    /// Replaces the unbounded default with a monotonic request deadline.
    #[must_use]
    pub const fn with_deadline(mut self, deadline: RequestDeadline) -> Self {
        self.deadline = deadline;
        self
    }

    /// Rejects an event unless every correlation field still matches this active request.
    ///
    /// # Errors
    ///
    /// Returns the first cancelled or mismatched correlation dimension.
    pub fn validate_event(&self, event: &Self) -> Result<(), RequestRejection> {
        if self.cancellation.is_cancelled() {
            return Err(RequestRejection::Cancelled);
        }
        if self.deadline.is_elapsed_at(std::time::Instant::now()) {
            return Err(RequestRejection::DeadlineElapsed);
        }
        if self.request_id != event.request_id {
            return Err(RequestRejection::RequestId);
        }
        if self.tab_id != event.tab_id {
            return Err(RequestRejection::TabId);
        }
        if self.generation != event.generation {
            return Err(RequestRejection::Generation);
        }
        Ok(())
    }
}

/// Reason an asynchronous event cannot mutate current tab state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestRejection {
    Cancelled,
    DeadlineElapsed,
    RequestId,
    TabId,
    Generation,
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use super::*;

    #[test]
    fn typed_ids_preserve_equality_hash_and_serialization_contracts() {
        let tab = TabId::new();
        let tab_json = serde_json::to_string(&tab).expect("serialize tab id");
        assert_eq!(
            serde_json::from_str::<TabId>(&tab_json).expect("deserialize tab id"),
            tab
        );
        assert_eq!(HashSet::from([tab]).len(), 1);

        let request = RequestId::new();
        let request_json = serde_json::to_string(&request).expect("serialize request id");
        assert_eq!(
            serde_json::from_str::<RequestId>(&request_json).expect("deserialize request id"),
            request
        );
        assert_eq!(HashSet::from([request]).len(), 1);

        let item = ShellItemId::from_provider_bytes([1, 2, 3]).expect("non-empty identity");
        let item_json = serde_json::to_string(&item).expect("serialize shell item id");
        assert_eq!(
            serde_json::from_str::<ShellItemId>(&item_json).expect("deserialize shell item id"),
            item
        );
        assert_eq!(HashSet::from([item]).len(), 1);
        assert!(ShellItemId::from_provider_bytes([]).is_none());

        let generation = Generation::new(7);
        let generation_json = serde_json::to_string(&generation).expect("serialize generation");
        assert_eq!(
            serde_json::from_str::<Generation>(&generation_json).expect("deserialize generation"),
            generation
        );
        assert_eq!(HashSet::from([generation]).len(), 1);
        assert_eq!(generation.checked_next(), Some(Generation::new(8)));
        assert_eq!(Generation::new(u64::MAX).checked_next(), None);
    }

    #[test]
    fn location_logs_redact_sensitive_path_but_serialization_round_trips() {
        let sensitive = r"C:\Users\Secret Person\private\file.txt";
        let location = LocationDescriptor::file_system(sensitive);
        assert!(!format!("{location}").contains("Secret Person"));
        assert!(!format!("{location:?}").contains("Secret Person"));

        let json = serde_json::to_string(&location).expect("serialize descriptor");
        let decoded: LocationDescriptor =
            serde_json::from_str(&json).expect("deserialize descriptor");
        assert_eq!(decoded, location);
        assert_eq!(decoded.path(), Some(PathBuf::from(sensitive).as_path()));
    }

    #[test]
    fn location_boundaries_validate_synthetic_empty_oversized_and_unknown_data() {
        for root in [SyntheticRoot::Home, SyntheticRoot::QuickAccess] {
            let descriptor = LocationDescriptor::synthetic(root);
            assert_eq!(descriptor.synthetic_root(), Some(root));
            let json = serde_json::to_string(&descriptor).expect("serialize synthetic root");
            let decoded: LocationDescriptor =
                serde_json::from_str(&json).expect("deserialize synthetic root");
            assert_eq!(decoded, descriptor);
        }

        assert_eq!(
            LocationDescriptor::try_shell_namespace([]),
            Err(LocationDescriptorValidationError::Empty)
        );
        assert!(matches!(
            LocationDescriptor::try_parsing_name("x".repeat(MAX_LOCATION_DESCRIPTOR_BYTES + 1)),
            Err(LocationDescriptorValidationError::TooLarge { .. })
        ));
        assert!(serde_json::from_str::<LocationDescriptor>(r#"{"FutureRoot":"opaque"}"#).is_err());
        let oversized = serde_json::to_string(&serde_json::json!({
            "ShellNamespace": vec![1_u8; MAX_LOCATION_DESCRIPTOR_BYTES + 1]
        }))
        .expect("serialize oversized wire value");
        assert!(serde_json::from_str::<LocationDescriptor>(&oversized).is_err());
    }

    #[test]
    fn request_validation_rejects_each_stale_dimension_and_cancellation() {
        let active = RequestContext::new(TabId::new(), Generation::new(4));
        assert_eq!(active.validate_event(&active.clone()), Ok(()));

        let mut event = active.clone();
        event.request_id = RequestId::new();
        assert_eq!(
            active.validate_event(&event),
            Err(RequestRejection::RequestId)
        );

        let mut event = active.clone();
        event.tab_id = TabId::new();
        assert_eq!(active.validate_event(&event), Err(RequestRejection::TabId));

        let mut event = active.clone();
        event.generation = Generation::new(5);
        assert_eq!(
            active.validate_event(&event),
            Err(RequestRejection::Generation)
        );

        active.cancellation.cancel();
        assert_eq!(
            active.validate_event(&active.clone()),
            Err(RequestRejection::Cancelled)
        );
    }

    #[test]
    fn request_validation_rejects_elapsed_deadline() {
        let now = std::time::Instant::now();
        let active = RequestContext::new(TabId::new(), Generation::default()).with_deadline(
            RequestDeadline::after(now, std::time::Duration::from_nanos(1))
                .expect("small deadline"),
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
        assert_eq!(
            active.validate_event(&active.clone()),
            Err(RequestRejection::DeadlineElapsed)
        );
    }

    #[test]
    fn cancellation_handles_before_during_repeat_and_dropped_consumer() {
        let cancelled_before = CancellationToken::new();
        cancelled_before.cancel();
        let immediate_calls = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&immediate_calls);
        let registration = cancelled_before.register(move || {
            calls.fetch_add(1, Ordering::Relaxed);
        });
        assert!(cancelled_before.is_cancelled());
        assert_eq!(immediate_calls.load(Ordering::Relaxed), 1);
        drop(registration);

        let during_work = CancellationToken::new();
        let callback_calls = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&callback_calls);
        let _registration = during_work.register(move || {
            calls.fetch_add(1, Ordering::Relaxed);
        });
        during_work.cancel();
        during_work.cancel();
        assert_eq!(callback_calls.load(Ordering::Relaxed), 1);

        let dropped_consumer = CancellationToken::new();
        let dropped_calls = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&dropped_calls);
        let registration = dropped_consumer.register(move || {
            calls.fetch_add(1, Ordering::Relaxed);
        });
        drop(registration);
        dropped_consumer.cancel();
        assert_eq!(dropped_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn cancellation_isolates_each_callback_panic_and_reports_it() {
        let token = CancellationToken::new();
        let ran_after_panic = Arc::new(AtomicUsize::new(0));
        let _first = token.register(|| panic!("faulty cancellation callback"));
        let ran = Arc::clone(&ran_after_panic);
        let _second = token.register(move || {
            ran.fetch_add(1, Ordering::Relaxed);
        });
        let report = token.cancel_with_report();
        assert_eq!(report.callbacks_invoked, 2);
        assert_eq!(report.panicked_callbacks, 1);
        assert!(!report.already_cancelled);
        assert_eq!(ran_after_panic.load(Ordering::Relaxed), 1);
        assert_eq!(
            token.cancel_with_report(),
            CancellationSignalReport {
                already_cancelled: true,
                ..CancellationSignalReport::default()
            }
        );
    }
}
