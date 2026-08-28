//! Startup-only ownership, runtime gates, and bounded draining for native DLLs.
//!
//! The private guarded executor invokes the Rust ABI registrar only after
//! lifecycle admission and durable Safe Mode marker creation.
#![allow(clippy::missing_errors_doc)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};

use thiserror::Error;

use crate::{
    ContributionGateV1, ContributionJobContractV1, ContributionKindV1, ContributionRegistrationV1,
    ExtensionJobAuthorityV1, ExtensionJobFinishOutcomeV1, ExtensionJobRuntimeErrorV1,
    ExtensionJobRuntimeRequestV1, ExtensionJobRuntimeV1, FeatureKeyV1, FeatureRuntimeFactV1,
    HostInputStreamSourceV1, HostRegistrationErrorV1, ResolvedPackageV1,
    ValidatedContributionSetV1,
    bundled_tool::BundledToolAuthorityV1,
    dll_loader::{
        ExtensionDllLoaderV1, LoadedExtensionRootV1, LoadedPackageRootsV1, invoke_guarded_registrar,
    },
    extension_job_runtime::{PreparedProviderDispatchTicketV1, ProviderDispatchControlV1},
    operation_plan::OperationPlanAuthorityV1,
    plugin_call_guard::{
        self, GuardErrorV1, MarkerV1, NativeCallOperationV1, NativeCallTerminalV1,
        NativeCallTimingV1, NativeSafeModeIncidentV1, PluginCallGuardStoreV1, PluginCallGuardV1,
    },
    runtime_authority::{
        AuthorityAdapterV1, AuthorityClaimsV1, AuthorityEnvelopeV1, RuntimeAuthorityV1,
    },
    view_registry::NavigationAuthorityV1,
    virtual_location::VirtualLocationAuthorityV1,
};
use explorer_extension_api::{
    JobHandleV1, JobProviderObjectV1, JobTerminalV1, ROOT_MODULE_CONTRACT_ID_V1,
    RegisteredContributionV1,
};

/// Resolver candidates (128) times Rust entrypoints per manifest (128).
pub const MAX_NATIVE_LEDGER_ENTRIES_V1: usize = 128 * 128;
/// Resolver candidates (128) times manifest features (128); roots share gates.
pub const MAX_NATIVE_FEATURE_GATES_V1: usize = MAX_NATIVE_LEDGER_ENTRIES_V1;
pub const MAX_NATIVE_RESTART_REASONS_PER_FEATURE_V1: usize = 8;
const DEFAULT_NATIVE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Host-owned native provider dispatch transaction.  Preparation happens
/// before ABI entry so the scheduler can obtain its handle/control surface;
/// terminal publication remains impossible until the durable call marker has
/// cleared successfully.
pub struct PreparedNativeJobV1 {
    runtime: Arc<ExtensionJobRuntimeV1>,
    ticket: PreparedProviderDispatchTicketV1,
    provider: Arc<JobProviderObjectV1>,
    markers: Arc<PluginCallGuardStoreV1>,
    marker: MarkerV1,
    permit: Option<PluginCallGuardV1>,
    callback_started: bool,
    callback_elapsed: Option<Duration>,
    runtime_authority: Option<Arc<RuntimeAuthorityV1>>,
    stream_authority: Option<AuthorityEnvelopeV1>,
}

impl PreparedNativeJobV1 {
    #[allow(dead_code)]
    #[must_use]
    pub fn handle(&self) -> JobHandleV1 {
        self.ticket.control().job()
    }

    /// Cloneable host control surface.  It cannot keep the native dispatch
    /// lease alive after this prepared transaction fail-closes.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn control(&self) -> ProviderDispatchControlV1 {
        self.ticket.control()
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn request_control(
        &self,
        control: explorer_extension_api::JobControlStateV1,
    ) -> Result<(), ExtensionJobRuntimeErrorV1> {
        self.ticket.control().request_control(control)
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn update_current_generations(
        &self,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
    ) -> Result<(), ExtensionJobRuntimeErrorV1> {
        self.runtime.update_current_generations(
            self.handle(),
            item_generation,
            location_generation,
            source_generation,
        )
    }

    #[must_use]
    pub fn drain(
        &self,
        current_item_generation: u64,
        current_location_generation: u64,
        current_source_generation: u64,
        maximum_batches: usize,
    ) -> Vec<crate::AcceptedIncrementalResultBatchV1> {
        self.runtime.drain(
            self.handle(),
            current_item_generation,
            current_location_generation,
            current_source_generation,
            maximum_batches,
        )
    }

    /// Final host apply transaction. Host identity data is projected before
    /// the final locked generation check, then rows commit atomically with it.
    pub fn apply(
        &self,
        batch: &crate::AcceptedIncrementalResultBatchV1,
        host_identity: impl FnMut(usize) -> (String, u128),
    ) -> Option<Vec<crate::ExtensionValueRowV1>> {
        self.runtime.apply_accepted_batch(batch, host_identity)
    }

    /// Controlled snapshot of host-owned rows for this exact still-current
    /// batch generation.
    #[must_use]
    pub fn applied_rows_snapshot(
        &self,
        batch: &crate::AcceptedIncrementalResultBatchV1,
    ) -> Option<Vec<crate::ExtensionValueRowV1>> {
        self.runtime.applied_rows_snapshot(batch)
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn retire(&self) -> Result<(), ExtensionJobRuntimeErrorV1> {
        self.ticket.control().retire()
    }

    /// Invokes the provider exactly once without publishing a terminal yet.
    #[allow(clippy::missing_errors_doc)]
    pub fn call_provider(&mut self) -> Result<JobTerminalV1, ExtensionJobRuntimeErrorV1> {
        if self.callback_started {
            return Err(ExtensionJobRuntimeErrorV1::ProviderAlreadyInvoked);
        }
        self.revalidate_stream_authority()?;
        let permit = self
            .markers
            .begin(&self.marker)
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        self.callback_started = true;
        self.permit = Some(permit);
        let started = Instant::now();
        match self.ticket.invoke_once(self.provider.as_ref()) {
            Ok(terminal) => {
                self.callback_elapsed = Some(started.elapsed());
                Ok(terminal)
            }
            Err(error) => {
                let elapsed = started.elapsed();
                self.callback_elapsed = Some(elapsed);
                let terminal = if let Some(permit) = self.permit.take() {
                    if permit.clear().is_err() {
                        self.ticket.fail_marker_clear();
                        NativeCallTerminalV1::MarkerFailure
                    } else {
                        NativeCallTerminalV1::Incompatible
                    }
                } else {
                    NativeCallTerminalV1::Incompatible
                };
                self.ticket.fail_marker_clear();
                self.markers.record_timing(&self.marker, elapsed, terminal);
                Err(error)
            }
        }
    }

    /// Clears the durable marker, then and only then commits the callback
    /// terminal.  Marker-clear failure revokes/purges/retires this generation.
    #[allow(clippy::missing_errors_doc)]
    pub fn publish_terminal_after_marker_clear(
        &mut self,
        terminal: JobTerminalV1,
    ) -> Result<ExtensionJobFinishOutcomeV1, ExtensionJobRuntimeErrorV1> {
        let elapsed = self.callback_elapsed.unwrap_or(Duration::ZERO);
        let Some(permit) = self.permit.take() else {
            return Err(ExtensionJobRuntimeErrorV1::TerminalPublicationDenied);
        };
        if terminal.into_raw() == JobTerminalV1::PANICKED.into_raw() {
            let finish = self.ticket.publish_terminal_after_marker_clear(terminal)?;
            self.markers
                .record_timing(&self.marker, elapsed, NativeCallTerminalV1::Panicked);
            // Intentionally retain the durable marker. The next startup
            // converts it into global Safe Mode and denies every plugin until
            // explicit user confirmation.
            drop(permit);
            return Ok(finish);
        }
        if permit.clear().is_err() {
            self.ticket.fail_marker_clear();
            self.markers
                .record_timing(&self.marker, elapsed, NativeCallTerminalV1::MarkerFailure);
            return Err(ExtensionJobRuntimeErrorV1::MarkerClearFailed);
        }
        if self.revalidate_stream_authority().is_err() {
            self.ticket.fail_marker_clear();
            self.markers
                .record_timing(&self.marker, elapsed, NativeCallTerminalV1::Incompatible);
            return Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority);
        }
        let finish = self.ticket.publish_terminal_after_marker_clear(terminal)?;
        self.markers.record_timing(
            &self.marker,
            elapsed,
            timing_terminal_for_job_terminal(terminal),
        );
        Ok(finish)
    }

    fn revalidate_stream_authority(&self) -> Result<(), ExtensionJobRuntimeErrorV1> {
        match (&self.runtime_authority, &self.stream_authority) {
            (Some(authority), Some(envelope)) => authority
                .revalidate(envelope, AuthorityAdapterV1::Stream)
                .map(|_| ())
                .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority),
            (None, None) => Ok(()),
            _ => Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority),
        }
    }
}

fn timing_terminal_for_job_terminal(terminal: JobTerminalV1) -> NativeCallTerminalV1 {
    match terminal.into_raw() {
        value if value == JobTerminalV1::PANICKED.into_raw() => NativeCallTerminalV1::Panicked,
        value if value == JobTerminalV1::PLUGIN_ERROR.into_raw() => {
            NativeCallTerminalV1::PluginError
        }
        value if value == JobTerminalV1::INCOMPATIBLE.into_raw() => {
            NativeCallTerminalV1::Incompatible
        }
        _ => NativeCallTerminalV1::Accepted,
    }
}

impl Drop for PreparedNativeJobV1 {
    fn drop(&mut self) {
        // A callback that returned but was never marker-cleared/committed must
        // fail closed, yet it is not a process crash: remove the marker when
        // possible and let the ticket revoke/purge/retire its generation.
        if let Some(permit) = self.permit.take() {
            let elapsed = self.callback_elapsed.unwrap_or(Duration::ZERO);
            if permit.clear().is_err() {
                self.ticket.fail_marker_clear();
                self.markers.record_timing(
                    &self.marker,
                    elapsed,
                    NativeCallTerminalV1::MarkerFailure,
                );
            } else {
                self.markers.record_timing(
                    &self.marker,
                    elapsed,
                    NativeCallTerminalV1::Incompatible,
                );
            }
        }
    }
}

/// Explicit application-owned state required for production native activation.
#[derive(Clone)]
pub struct NativeLifecycleConfigV1 {
    application_state_dir: PathBuf,
    slow_callback_threshold: Duration,
    #[cfg(feature = "integration-test-support")]
    integration_test_drain_timeout: Option<Duration>,
}

impl NativeLifecycleConfigV1 {
    /// Uses a dedicated marker directory below the application-owned state root.
    #[must_use]
    pub fn new(application_state_dir: PathBuf) -> Self {
        Self {
            application_state_dir,
            slow_callback_threshold: Duration::from_millis(250),
            #[cfg(feature = "integration-test-support")]
            integration_test_drain_timeout: None,
        }
    }

    /// Sets the path-free callback timing slow threshold.
    #[must_use]
    pub const fn with_slow_callback_threshold(mut self, threshold: Duration) -> Self {
        self.slow_callback_threshold = threshold;
        self
    }

    /// Sets a bounded drain timeout for the isolated native lifecycle contract runner.
    #[cfg(feature = "integration-test-support")]
    #[doc(hidden)]
    #[must_use]
    pub const fn with_integration_test_drain_timeout(mut self, timeout: Duration) -> Self {
        self.integration_test_drain_timeout = Some(timeout);
        self
    }
}

impl fmt::Debug for NativeLifecycleConfigV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("NativeLifecycleConfigV1");
        debug
            .field("application_state_dir", &"<redacted>")
            .field("slow_callback_threshold", &self.slow_callback_threshold);
        #[cfg(feature = "integration-test-support")]
        debug.field(
            "integration_test_drain_timeout",
            &self.integration_test_drain_timeout,
        );
        debug.finish()
    }
}

/// Feature-scoped runtime authority across all roots in one sealed generation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NativeFeatureIdentityV1 {
    package_id: String,
    sealed_manifest_digest: String,
    feature: FeatureKeyV1,
}

impl NativeFeatureIdentityV1 {
    #[must_use]
    pub fn package_id(&self) -> &str {
        &self.package_id
    }
    #[must_use]
    pub fn sealed_manifest_digest(&self) -> &str {
        &self.sealed_manifest_digest
    }
    #[must_use]
    pub fn feature_id(&self) -> &str {
        &self.feature.feature_id
    }
}

/// Exact native feature generation being drained. Runtime cancellation must
/// never spill into sibling features or into a re-enabled epoch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeFeatureDrainScopeV1 {
    identity: NativeFeatureIdentityV1,
    epoch: u64,
}

impl NativeFeatureDrainScopeV1 {
    fn new(identity: NativeFeatureIdentityV1, epoch: u64) -> Self {
        Self { identity, epoch }
    }

    #[must_use]
    pub(crate) fn identity(&self) -> &NativeFeatureIdentityV1 {
        &self.identity
    }

    #[must_use]
    pub(crate) const fn epoch(&self) -> u64 {
        self.epoch
    }
}

/// Backward-compatible name for the feature-scoped runtime authority.
///
/// Root activation authority is private [`EntryKeyV1`]; it never appears on a
/// dispatch gate because multiple validated roots can serve one feature.
pub type NativeRootIdentityV1 = NativeFeatureIdentityV1;

/// Safe startup diagnostic; it never exposes an ABI root or library handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeStartupAdmissionV1 {
    pub package_id: String,
    pub package_version: String,
    pub sealed_manifest_digest: String,
    pub root_count: usize,
    pub activated_feature_count: usize,
}

/// Runtime status held separately from persisted desired state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeFeatureStateV1 {
    Enabled {
        epoch: u64,
    },
    Disabling,
    DisabledResident,
    PendingRestart {
        primary_reason: NativeRestartReasonV1,
    },
    Faulted,
}

/// Why a requested native change cannot take effect in this process.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NativeRestartReasonV1 {
    UnloadedEnable,
    Install,
    Update,
    Replace,
    Remove,
    DrainTimedOut,
    StartupAborted,
}

/// Stable, path-free classification of a rejected native DLL load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLoaderDiagnosticCodeV1 {
    MissingBinaryUiFingerprint,
    BinaryUiFingerprintMismatch,
    GpuiFingerprintMismatch,
    InvalidAbiRoot,
    RootContract,
    IntegrityActivation,
    Mapping,
    UnsupportedPlatform,
    ManifestSdk,
    ResidentState,
    InternalArtifact,
}

impl NativeLoaderDiagnosticCodeV1 {
    fn from_loader(error: &crate::dll_loader::ExtensionDllLoadErrorV1) -> Self {
        use crate::dll_loader::ExtensionDllLoadErrorV1 as LoaderError;

        match error {
            LoaderError::MissingBinaryUiFingerprint { .. } => Self::MissingBinaryUiFingerprint,
            LoaderError::BinaryUiFingerprintMismatch { .. } => Self::BinaryUiFingerprintMismatch,
            LoaderError::GpuiFingerprintMismatch { .. } => Self::GpuiFingerprintMismatch,
            LoaderError::AbiStable { .. } => Self::InvalidAbiRoot,
            LoaderError::RootValidation { .. }
            | LoaderError::UnexpectedBinaryUiFingerprint { .. } => Self::RootContract,
            LoaderError::SealedPayloadUnavailable { .. }
            | LoaderError::CanonicalManifestDigest { .. }
            | LoaderError::ActivationGuard(_) => Self::IntegrityActivation,
            LoaderError::DynamicLibraryLoad { .. } => Self::Mapping,
            LoaderError::UnsupportedPlatform => Self::UnsupportedPlatform,
            LoaderError::ManifestAbiSchemaMismatch { .. }
            | LoaderError::EntrypointSdkMajorMismatch { .. }
            | LoaderError::DuplicateRustEntrypointPath { .. }
            | LoaderError::RootContractMismatch { .. }
            | LoaderError::ManifestGpuiFingerprintMissing { .. } => Self::ManifestSdk,
            LoaderError::ResidentStatePoisoned
            | LoaderError::AlreadyAttempted { .. }
            | LoaderError::PreviouslyRejected { .. } => Self::ResidentState,
            LoaderError::InvalidHostUiFingerprintArtifact => Self::InternalArtifact,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecyclePhaseV1 {
    New,
    Admitting,
    Running,
    Closed,
    Stopped,
}

#[derive(Clone, Copy)]
struct AdmissionPermitV1 {
    generation: u64,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EntryKeyV1 {
    package_id: String,
    sealed_manifest_digest: String,
    entrypoint_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryStateV1 {
    Prepared,
    Claimed,
    Deferred,
    Activated,
    Rejected,
}

#[derive(Clone, Debug)]
struct FeatureGateV1 {
    state: NativeFeatureStateV1,
    accepting: bool,
    in_flight: usize,
    epoch: u64,
    operation: GateOperationV1,
    members: BTreeSet<EntryKeyV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GateOperationV1 {
    Idle,
    Enabling(u64),
    Disabling(u64),
    Removing(u64),
}

#[derive(Default)]
struct RuntimeStateV1 {
    phase: Option<LifecyclePhaseV1>,
    entries: BTreeMap<EntryKeyV1, EntryStateV1>,
    /// Retains the task-3.3 token for task 3.5's sole guarded registrar claim.
    validated_roots: BTreeMap<EntryKeyV1, LoadedExtensionRootV1>,
    sealed_contributions: BTreeMap<(String, String), ValidatedContributionSetV1>,
    providers: BTreeMap<(String, String, String), Arc<JobProviderObjectV1>>,
    gates: BTreeMap<NativeFeatureIdentityV1, FeatureGateV1>,
    rejected_generations: BTreeSet<(String, String)>,
    restart_reasons: BTreeMap<NativeFeatureIdentityV1, BTreeSet<NativeRestartReasonV1>>,
    next_operation_token: u64,
    startup_generation: u64,
    shutdown_generation: u64,
}

struct SharedRuntimeV1 {
    state: Mutex<RuntimeStateV1>,
    drained: Condvar,
}

static PROCESS_NATIVE_LIFECYCLE_ACQUIRED: OnceLock<Mutex<bool>> = OnceLock::new();

/// Host-private replacement point for task 3.5's marker-guarded registrar call.
///
/// Implementations must only describe already-validated manifest features. They
/// must not make an ABI callback in task 3.4.
pub(crate) trait NativeActivationExecutor: Send + Sync {
    fn claim(
        &self,
        context: NativeActivationContextV1<'_>,
    ) -> Result<NativeActivationClaimV1, NativeActivationFailureV1>;
}

/// Private hand-off of a sealed, layout-validated root to task 3.5.
pub(crate) struct NativeActivationContextV1<'root> {
    #[allow(dead_code, reason = "task 3.5 consumes every bound context field")]
    pub package_id: &'root str,
    #[allow(dead_code, reason = "task 3.5 consumes every bound context field")]
    pub package_version: &'root str,
    #[allow(dead_code, reason = "task 3.5 consumes every bound context field")]
    pub sealed_manifest_digest: &'root str,
    #[allow(dead_code, reason = "task 3.5 consumes every bound context field")]
    pub entrypoint_id: &'root str,
    #[allow(dead_code, reason = "task 3.5 receives the validated root token")]
    pub root: Option<&'root LoadedExtensionRootV1>,
}

#[allow(dead_code, reason = "task 3.5 installs the guarded Activated claim")]
#[allow(clippy::large_enum_variant)]
pub(crate) enum NativeActivationClaimV1 {
    Deferred,
    Activated(NativeActivationBlueprintV1),
}

/// Host-private result of a future guarded registrar claim.
pub(crate) struct NativeActivationBlueprintV1 {
    package_id: String,
    package_version: String,
    sealed_manifest_digest: String,
    features: Vec<FeatureKeyV1>,
    registrations: Option<Vec<ContributionRegistrationV1>>,
    providers: BTreeMap<String, JobProviderObjectV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "task 3.5 executor will produce both typed failures"
)]
pub(crate) enum NativeActivationFailureV1 {
    SafeModeDenied,
    Rejected,
    Faulted,
}

trait NativeDrainPortV1: Send + Sync {
    fn detach(&self, _: &NativeRootIdentityV1) {}
    fn cancel(&self, _: &NativeRootIdentityV1) {}
    fn wait_for_drain(&self, _: &NativeRootIdentityV1, _: Duration) -> bool {
        true
    }
    fn restore(&self, _: &NativeRootIdentityV1) {}
    fn cancel_scope(&self, scope: &NativeFeatureDrainScopeV1) {
        self.cancel(scope.identity());
    }
    fn wait_for_drain_scope(&self, scope: &NativeFeatureDrainScopeV1, timeout: Duration) -> bool {
        self.wait_for_drain(scope.identity(), timeout)
    }
}

struct GuardedNativeActivationExecutorV1 {
    markers: Arc<PluginCallGuardStoreV1>,
}

impl GuardedNativeActivationExecutorV1 {
    fn new(markers: Arc<PluginCallGuardStoreV1>) -> Self {
        Self { markers }
    }
}

fn marker_for_loaded_root(
    context: &NativeActivationContextV1<'_>,
    root: &LoadedExtensionRootV1,
) -> MarkerV1 {
    let metadata = root.metadata();
    plugin_call_guard::marker(
        context.package_id,
        context.sealed_manifest_digest,
        context.entrypoint_id,
        root.root_contract_id(),
        metadata.primary_interface_id.namespace.into_raw(),
        metadata.primary_interface_id.value,
    )
}

impl NativeActivationExecutor for GuardedNativeActivationExecutorV1 {
    fn claim(
        &self,
        context: NativeActivationContextV1<'_>,
    ) -> Result<NativeActivationClaimV1, NativeActivationFailureV1> {
        let Some(root) = context.root else {
            return Err(NativeActivationFailureV1::Faulted);
        };
        let marker = marker_for_loaded_root(&context, root);
        let permit = match self.markers.begin(&marker) {
            Ok(permit) => permit,
            Err(GuardErrorV1::Denied) => {
                self.markers.record_timing(
                    &marker,
                    Duration::ZERO,
                    NativeCallTerminalV1::SafeModeDenied,
                );
                return Err(NativeActivationFailureV1::SafeModeDenied);
            }
            Err(GuardErrorV1::Fault) => {
                self.markers.record_timing(
                    &marker,
                    Duration::ZERO,
                    NativeCallTerminalV1::MarkerFailure,
                );
                return Err(NativeActivationFailureV1::Faulted);
            }
        };
        let started = Instant::now();
        // Keep the durable marker active through descriptor preflight,
        // host-native projection, and every rejected foreign-object drop.
        let claim = match invoke_guarded_registrar(root, &permit) {
            Ok(output) => guarded_blueprint_from_output(&context, output)
                .map(NativeActivationClaimV1::Activated)
                .map_err(|()| NativeActivationFailureV1::Rejected),
            Err(HostRegistrationErrorV1::Panicked(_)) => Err(NativeActivationFailureV1::Faulted),
            Err(_) => Err(NativeActivationFailureV1::Rejected),
        };
        let elapsed = started.elapsed();
        let terminal = match &claim {
            Ok(_) => NativeCallTerminalV1::Accepted,
            Err(NativeActivationFailureV1::SafeModeDenied) => NativeCallTerminalV1::SafeModeDenied,
            Err(NativeActivationFailureV1::Faulted) => NativeCallTerminalV1::Panicked,
            Err(NativeActivationFailureV1::Rejected) => NativeCallTerminalV1::PluginError,
        };
        if terminal == NativeCallTerminalV1::Panicked {
            self.markers.record_timing(&marker, elapsed, terminal);
            drop(permit);
            return claim;
        }
        if permit.clear().is_err() {
            self.markers
                .record_timing(&marker, elapsed, NativeCallTerminalV1::MarkerFailure);
            return Err(NativeActivationFailureV1::Faulted);
        }
        self.markers.record_timing(&marker, elapsed, terminal);
        claim
    }
}

fn guarded_blueprint_from_output(
    context: &NativeActivationContextV1<'_>,
    output: explorer_extension_api::RegistrarOutputV1,
) -> Result<NativeActivationBlueprintV1, ()> {
    if output.outcome.registered_interface_count as usize != output.contributions.len()
        || output.contributions.is_empty()
        || output.contributions.len() > crate::MAX_CONTRIBUTIONS_PER_BATCH_V1
    {
        return Err(());
    }
    let mut aggregate_bytes = 0_usize;
    for descriptor in &output.contributions {
        let nested_renderer_bytes = descriptor
            .renderer_contribution_id
            .as_ref()
            .map_or(0, abi_stable::std_types::RString::len);
        let descriptor_bytes = descriptor
            .feature_id
            .len()
            .checked_add(descriptor.contribution_id.len())
            .and_then(|bytes| bytes.checked_add(nested_renderer_bytes))
            .and_then(|bytes| {
                descriptor
                    .required_capabilities
                    .iter()
                    .try_fold(bytes, |total, capability| {
                        total.checked_add(capability.len())
                    })
            })
            .ok_or(())?;
        aggregate_bytes = aggregate_bytes.checked_add(descriptor_bytes).ok_or(())?;
        if descriptor.feature_id.len() > 64
            || descriptor.contribution_id.len() > 64
            || nested_renderer_bytes > 64
            || descriptor.required_capabilities.len() > crate::MAX_CAPABILITIES_PER_CONTRIBUTION_V1
            || descriptor
                .required_capabilities
                .iter()
                .any(|capability| capability.len() > 64)
            || aggregate_bytes > 64 * crate::MAX_CONTRIBUTIONS_PER_BATCH_V1
        {
            return Err(());
        }
    }
    let mut registrations = Vec::with_capacity(output.contributions.len());
    let mut providers = BTreeMap::new();
    for descriptor in output.contributions {
        let (registration, provider) = contribution_from_abi(descriptor)?;
        if let Some(provider) = provider
            && providers
                .insert(registration.contribution_id.clone(), provider)
                .is_some()
        {
            return Err(());
        }
        registrations.push(registration);
    }
    let features: Vec<_> = registrations
        .iter()
        .map(|registration| FeatureKeyV1::new(context.package_id, &registration.feature_id))
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|_| ())?
        .into_iter()
        .collect();
    if features.is_empty() {
        return Err(());
    }
    Ok(NativeActivationBlueprintV1 {
        package_id: context.package_id.to_owned(),
        package_version: context.package_version.to_owned(),
        sealed_manifest_digest: context.sealed_manifest_digest.to_owned(),
        features,
        registrations: Some(registrations),
        providers,
    })
}

fn contribution_from_abi(
    descriptor: RegisteredContributionV1,
) -> Result<(ContributionRegistrationV1, Option<JobProviderObjectV1>), ()> {
    let kind = match descriptor.kind.into_raw() {
        1 => ContributionKindV1::Column,
        2 => ContributionKindV1::GpuiRenderer,
        3 => ContributionKindV1::Command,
        4 => ContributionKindV1::Form,
        5 => ContributionKindV1::OperationPlan,
        6 => ContributionKindV1::ViewMode,
        7 => ContributionKindV1::Resource,
        _ => return Err(()),
    };
    let folder_admission = descriptor.folder_admission.into_option();
    if folder_admission.is_some()
        && (kind != ContributionKindV1::Column
            || matches!(
                descriptor.batch_column_provider,
                abi_stable::std_types::ROption::RNone
            ))
    {
        return Err(());
    }
    let opaque_schema = descriptor
        .opaque_contract
        .into_option()
        .map(|contract| (contract.schema, contract.schema_version));
    let registration = ContributionRegistrationV1 {
        feature_id: descriptor.feature_id.to_string(),
        contribution_id: descriptor.contribution_id.to_string(),
        kind,
        required_capabilities: descriptor
            .required_capabilities
            .into_iter()
            .map(|capability| capability.to_string())
            .collect(),
        folder_admission,
        job_contract: Some(ContributionJobContractV1 {
            interface_id: descriptor.interface_id,
            expected_sort: descriptor.expected_sort,
            opaque_schema,
            renderer_contribution_id: descriptor
                .renderer_contribution_id
                .into_option()
                .map(|renderer| renderer.to_string()),
        }),
    };
    Ok((registration, descriptor.provider.into_option()))
}

#[cfg(test)]
struct NoopDrainPortV1;
#[cfg(test)]
impl NativeDrainPortV1 for NoopDrainPortV1 {}

/// Production bridge from lifecycle shutdown to the host-owned job registry.
/// It never enters plugin code and only waits for an already-running callback
/// scope after the generation has been synchronously revoked.
struct RuntimeDrainPortV1 {
    runtimes: Arc<Mutex<Vec<Weak<ExtensionJobRuntimeV1>>>>,
}

impl RuntimeDrainPortV1 {
    fn runtimes(&self) -> Vec<Arc<ExtensionJobRuntimeV1>> {
        let Ok(mut runtimes) = self.runtimes.lock() else {
            return Vec::new();
        };
        runtimes.retain(|runtime| runtime.strong_count() != 0);
        runtimes.iter().filter_map(Weak::upgrade).collect()
    }
}

impl NativeDrainPortV1 for RuntimeDrainPortV1 {
    fn cancel_scope(&self, scope: &NativeFeatureDrainScopeV1) {
        for runtime in self.runtimes() {
            runtime.revoke_feature_generation(
                scope.identity().package_id(),
                scope.identity().sealed_manifest_digest(),
                scope.identity().feature_id(),
                scope.epoch(),
            );
        }
    }

    fn wait_for_drain_scope(&self, scope: &NativeFeatureDrainScopeV1, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.runtimes().iter().all(|runtime| {
                runtime.feature_callbacks_drained(
                    scope.identity().package_id(),
                    scope.identity().sealed_manifest_digest(),
                    scope.identity().feature_id(),
                    scope.epoch(),
                )
            }) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// The sole native-DLL startup/load authority for one application process.
///
/// It is intentionally non-`Clone`. Leases retain an `Arc` to the shared gate
/// state, so a late lease drop cannot access a destroyed manager.
pub struct NativeExtensionLifecycleV1 {
    shared: Arc<SharedRuntimeV1>,
    executor: Arc<dyn NativeActivationExecutor>,
    drain_port: Arc<dyn NativeDrainPortV1>,
    drain_timeout: Duration,
    markers: Option<Arc<PluginCallGuardStoreV1>>,
    job_runtimes: Arc<Mutex<Vec<Weak<ExtensionJobRuntimeV1>>>>,
    runtime_authority: Option<Arc<RuntimeAuthorityV1>>,
}

impl fmt::Debug for NativeExtensionLifecycleV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NativeExtensionLifecycleV1 { .. }")
    }
}

impl NativeExtensionLifecycleV1 {
    /// Acquires the one nonrenewable native lifecycle authority for this process.
    pub fn acquire(config: NativeLifecycleConfigV1) -> Result<Self, NativeLifecycleErrorV1> {
        let NativeLifecycleConfigV1 {
            application_state_dir,
            slow_callback_threshold,
            #[cfg(feature = "integration-test-support")]
            integration_test_drain_timeout,
        } = config;
        plugin_call_guard::validate_application_state_dir(&application_state_dir)?;
        let mut acquired = PROCESS_NATIVE_LIFECYCLE_ACQUIRED
            .get_or_init(|| Mutex::new(false))
            .lock()
            .map_err(|_| NativeLifecycleErrorV1::StatePoisoned)?;
        if *acquired {
            return Err(NativeLifecycleErrorV1::AlreadyAcquired);
        }
        let markers = PluginCallGuardStoreV1::open(
            application_state_dir.join("native-call-markers-v1"),
            slow_callback_threshold,
        )?;
        let job_runtimes = Arc::new(Mutex::new(Vec::new()));
        let runtime_authority =
            Arc::new(RuntimeAuthorityV1::new().map_err(|_| NativeLifecycleErrorV1::StatePoisoned)?);
        *acquired = true;
        Ok(Self::with_ports_and_markers(
            Arc::new(GuardedNativeActivationExecutorV1::new(Arc::clone(&markers))),
            Arc::new(RuntimeDrainPortV1 {
                runtimes: Arc::clone(&job_runtimes),
            }),
            {
                #[cfg(feature = "integration-test-support")]
                {
                    integration_test_drain_timeout.unwrap_or(DEFAULT_NATIVE_DRAIN_TIMEOUT)
                }
                #[cfg(not(feature = "integration-test-support"))]
                {
                    DEFAULT_NATIVE_DRAIN_TIMEOUT
                }
            },
            Some(markers),
            job_runtimes,
            Some(runtime_authority),
        ))
    }

    fn with_ports_and_markers(
        executor: Arc<dyn NativeActivationExecutor>,
        drain_port: Arc<dyn NativeDrainPortV1>,
        drain_timeout: Duration,
        markers: Option<Arc<PluginCallGuardStoreV1>>,
        job_runtimes: Arc<Mutex<Vec<Weak<ExtensionJobRuntimeV1>>>>,
        runtime_authority: Option<Arc<RuntimeAuthorityV1>>,
    ) -> Self {
        Self {
            shared: Arc::new(SharedRuntimeV1 {
                state: Mutex::new(RuntimeStateV1 {
                    phase: Some(LifecyclePhaseV1::New),
                    ..RuntimeStateV1::default()
                }),
                drained: Condvar::new(),
            }),
            executor,
            drain_port,
            drain_timeout,
            markers,
            job_runtimes,
            runtime_authority,
        }
    }

    /// Binds the host's canonical job runtime to this lifecycle. The weak
    /// registry is owned by this lifecycle instance, never process-global.
    pub(crate) fn bind_job_runtime(&self, runtime: &Arc<ExtensionJobRuntimeV1>) {
        let Ok(mut runtimes) = self.job_runtimes.lock() else {
            return;
        };
        runtimes.retain(|candidate| candidate.strong_count() != 0);
        if runtimes
            .iter()
            .any(|candidate| candidate.ptr_eq(&Arc::downgrade(runtime)))
        {
            return;
        }
        runtimes.push(Arc::downgrade(runtime));
    }

    fn has_job_runtime(&self, runtime: &Arc<ExtensionJobRuntimeV1>) -> bool {
        self.job_runtimes.lock().is_ok_and(|runtimes| {
            runtimes.iter().any(|candidate| {
                candidate
                    .upgrade()
                    .is_some_and(|candidate| Arc::ptr_eq(&candidate, runtime))
            })
        })
    }

    /// Returns recovered path-free Safe Mode incidents.
    #[must_use]
    pub fn safe_mode_incidents(&self) -> Vec<NativeSafeModeIncidentV1> {
        self.markers
            .as_ref()
            .map_or_else(Vec::new, |markers| markers.incidents())
    }

    /// Whether malformed or overflowed marker residue has globally denied native calls.
    #[must_use]
    pub fn safe_mode_denies_all(&self) -> bool {
        self.markers
            .as_ref()
            .is_some_and(|markers| markers.is_global())
    }

    /// Shares the already-started host's durable marker store with the one
    /// explicit development DLL path. Keeping this crate-private prevents a
    /// second store from hiding recovered incidents from host Safe Mode UI.
    pub(crate) fn direct_callback_marker_store(&self) -> Option<Arc<PluginCallGuardStoreV1>> {
        self.markers.clone()
    }

    /// Exercises the same exact-marker deny branch that native registrar
    /// dispatch uses, without loading a test DLL. This is compiled only for
    /// the isolated Windows integration harness.
    #[cfg(feature = "integration-test-support")]
    #[must_use]
    pub fn integration_test_recovered_callback_is_denied(&self) -> bool {
        let Some(markers) = self.markers.as_ref() else {
            return false;
        };
        self.safe_mode_incidents()
            .into_iter()
            .any(|incident| match incident {
                NativeSafeModeIncidentV1::RegistrarInProgress {
                    package_id,
                    sealed_manifest_digest,
                    entrypoint_id,
                    root_module_id,
                    primary_interface_namespace,
                    primary_interface_value,
                    ..
                } => markers.denies(&plugin_call_guard::marker(
                    &package_id,
                    &sealed_manifest_digest,
                    &entrypoint_id,
                    &root_module_id,
                    primary_interface_namespace,
                    primary_interface_value,
                )),
                NativeSafeModeIncidentV1::UnsafeMarkerState { .. } => markers.is_global(),
            })
    }

    /// Confirms exactly one recovered incident and removes its denial residue.
    pub fn confirm_safe_mode_incident(
        &self,
        incident_id: crate::NativeSafeModeIncidentIdV1,
    ) -> Result<(), NativeLifecycleErrorV1> {
        self.markers
            .as_ref()
            .ok_or(NativeLifecycleErrorV1::SafeModeIncidentUnknown)?
            .confirm(incident_id)
    }

    /// Returns bounded path-free native callback timing diagnostics.
    #[must_use]
    pub fn native_call_timings(&self) -> Vec<NativeCallTimingV1> {
        self.markers
            .as_ref()
            .map_or_else(Vec::new, |markers| markers.timings())
    }

    /// Opens the one linear startup admission session.
    pub fn begin_startup(&mut self) -> Result<StartupSession<'_>, NativeLifecycleErrorV1> {
        let mut state = self.lock()?;
        if state.phase != Some(LifecyclePhaseV1::New) {
            return Err(NativeLifecycleErrorV1::StartupClosed);
        }
        state.startup_generation = state
            .startup_generation
            .checked_add(1)
            .ok_or(NativeLifecycleErrorV1::GenerationOverflow)?;
        state.phase = Some(LifecyclePhaseV1::Admitting);
        drop(state);
        Ok(StartupSession {
            lifecycle: self,
            sealed: false,
        })
    }

    /// Attempts to enter a feature callback/dispatch region.
    pub fn try_enter(
        &self,
        identity: &NativeRootIdentityV1,
    ) -> Result<Option<NativeDispatchLeaseV1>, NativeLifecycleErrorV1> {
        let mut state = self.lock()?;
        if !matches!(state.phase, Some(LifecyclePhaseV1::Running)) {
            return Ok(None);
        }
        let Some(gate) = state.gates.get_mut(identity) else {
            return Ok(None);
        };
        if !gate.accepting {
            return Ok(None);
        }
        let NativeFeatureStateV1::Enabled { epoch } = gate.state else {
            return Ok(None);
        };
        gate.in_flight = gate
            .in_flight
            .checked_add(1)
            .ok_or(NativeLifecycleErrorV1::InFlightOverflow)?;
        Ok(Some(NativeDispatchLeaseV1 {
            shared: Arc::clone(&self.shared),
            identity: identity.clone(),
            epoch,
        }))
    }

    /// Mints job authority solely from a contribution recorded by guarded
    /// activation. Callers cannot provide a descriptor, package digest, or kind.
    pub fn mint_job_authority(
        &self,
        identity: &NativeRootIdentityV1,
        contribution_id: &str,
    ) -> Result<ExtensionJobAuthorityV1, ExtensionJobRuntimeErrorV1> {
        let lease = self
            .try_enter(identity)
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let generation = (
            identity.package_id().to_owned(),
            identity.sealed_manifest_digest().to_owned(),
        );
        let validated = self
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .sealed_contributions
            .get(&generation)
            .cloned()
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        ExtensionJobAuthorityV1::mint_sealed(&validated, contribution_id, lease)
    }

    /// Mints the opaque use-time grant required by one package-attested tool.
    /// The capability and feature identity come only from the sealed
    /// contribution set; callers cannot upgrade a validated registration by
    /// supplying a capability string.
    #[allow(clippy::too_many_arguments)]
    pub fn mint_bundled_tool_authority(
        &self,
        identity: &NativeRootIdentityV1,
        contribution_id: &str,
        job_generation: u64,
        item_generation: u64,
        location_generation: u64,
        refresh_generation: u64,
        container_generation: u64,
    ) -> Result<BundledToolAuthorityV1, ExtensionJobRuntimeErrorV1> {
        let lease = self
            .try_enter(identity)
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let generation = (
            identity.package_id().to_owned(),
            identity.sealed_manifest_digest().to_owned(),
        );
        let validated = self
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .sealed_contributions
            .get(&generation)
            .cloned()
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let contribution = validated
            .contributions()
            .iter()
            .find(|entry| {
                entry.contribution_id == contribution_id
                    && entry.feature_id == identity.feature_id()
                    && entry
                        .required_capabilities
                        .iter()
                        .any(|capability| capability == "tools.execute_bundled")
            })
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let runtime = self
            .runtime_authority
            .as_ref()
            .cloned()
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let envelope = runtime
            .issue(AuthorityClaimsV1 {
                package_id: identity.package_id().to_owned(),
                feature_id: contribution.feature_id.clone(),
                interface_id: contribution.contribution_id.clone(),
                incarnation: lease.epoch(),
                capability: "tools.execute_bundled".to_owned(),
                authorized_root_sha256: identity.sealed_manifest_digest().to_owned(),
                location_generation,
                item_generation,
                refresh_generation,
                container_generation,
                job_generation,
            })
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        runtime
            .revalidate(&envelope, AuthorityAdapterV1::Tool)
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        drop(lease);
        Ok(BundledToolAuthorityV1::from_host(runtime, envelope))
    }

    /// Mints a generation-bound grant for one sealed operation-plan
    /// contribution. Preview alone never becomes commit authority: the engine
    /// revalidates this envelope again before each filesystem mutation.
    #[allow(clippy::too_many_arguments)]
    pub fn mint_operation_plan_authority(
        &self,
        identity: &NativeRootIdentityV1,
        contribution_id: &str,
        job_generation: u64,
        item_generation: u64,
        location_generation: u64,
        refresh_generation: u64,
        container_generation: u64,
    ) -> Result<OperationPlanAuthorityV1, ExtensionJobRuntimeErrorV1> {
        let lease = self
            .try_enter(identity)
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let generation = (
            identity.package_id().to_owned(),
            identity.sealed_manifest_digest().to_owned(),
        );
        let validated = self
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .sealed_contributions
            .get(&generation)
            .cloned()
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let contribution = validated
            .contributions()
            .iter()
            .find(|entry| {
                entry.contribution_id == contribution_id
                    && entry.feature_id == identity.feature_id()
                    && entry.kind == ContributionKindV1::OperationPlan
                    && entry
                        .required_capabilities
                        .iter()
                        .any(|capability| capability == "operations.submit")
            })
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let runtime = self
            .runtime_authority
            .as_ref()
            .cloned()
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let envelope = runtime
            .issue(AuthorityClaimsV1 {
                package_id: identity.package_id().to_owned(),
                feature_id: contribution.feature_id.clone(),
                interface_id: contribution.contribution_id.clone(),
                incarnation: lease.epoch(),
                capability: "operations.submit".to_owned(),
                authorized_root_sha256: identity.sealed_manifest_digest().to_owned(),
                location_generation,
                item_generation,
                refresh_generation,
                container_generation,
                job_generation,
            })
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        runtime
            .revalidate(&envelope, AuthorityAdapterV1::OperationPlan)
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        drop(lease);
        Ok(OperationPlanAuthorityV1::from_host(runtime, envelope))
    }

    /// Mints the use-time navigation grant for one sealed view contribution.
    /// Opaque node authorization remains snapshot-local in the navigation
    /// adapter and is rechecked immediately before model dispatch.
    #[allow(clippy::too_many_arguments)]
    pub fn mint_navigation_authority(
        &self,
        identity: &NativeRootIdentityV1,
        contribution_id: &str,
        job_generation: u64,
        item_generation: u64,
        location_generation: u64,
        refresh_generation: u64,
        container_generation: u64,
    ) -> Result<NavigationAuthorityV1, ExtensionJobRuntimeErrorV1> {
        let lease = self
            .try_enter(identity)
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let generation = (
            identity.package_id().to_owned(),
            identity.sealed_manifest_digest().to_owned(),
        );
        let validated = self
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .sealed_contributions
            .get(&generation)
            .cloned()
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let contribution = validated
            .contributions()
            .iter()
            .find(|entry| {
                entry.contribution_id == contribution_id
                    && entry.feature_id == identity.feature_id()
                    && entry.kind == ContributionKindV1::ViewMode
                    && entry
                        .required_capabilities
                        .iter()
                        .any(|capability| capability == "navigation.request")
            })
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let runtime = self
            .runtime_authority
            .as_ref()
            .cloned()
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let envelope = runtime
            .issue(AuthorityClaimsV1 {
                package_id: identity.package_id().to_owned(),
                feature_id: contribution.feature_id.clone(),
                interface_id: contribution.contribution_id.clone(),
                incarnation: lease.epoch(),
                capability: "navigation.request".to_owned(),
                authorized_root_sha256: identity.sealed_manifest_digest().to_owned(),
                location_generation,
                item_generation,
                refresh_generation,
                container_generation,
                job_generation,
            })
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        runtime
            .revalidate(&envelope, AuthorityAdapterV1::Navigation)
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        drop(lease);
        Ok(NavigationAuthorityV1::from_host(runtime, envelope))
    }

    /// Mints the bounded-read grant for one sealed virtual-folder resource.
    #[allow(clippy::too_many_arguments)]
    pub fn mint_virtual_location_authority(
        &self,
        identity: &NativeRootIdentityV1,
        contribution_id: &str,
        job_generation: u64,
        item_generation: u64,
        location_generation: u64,
        refresh_generation: u64,
        container_generation: u64,
    ) -> Result<VirtualLocationAuthorityV1, ExtensionJobRuntimeErrorV1> {
        let lease = self
            .try_enter(identity)
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let generation = (
            identity.package_id().to_owned(),
            identity.sealed_manifest_digest().to_owned(),
        );
        let validated = self
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .sealed_contributions
            .get(&generation)
            .cloned()
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let contribution = validated
            .contributions()
            .iter()
            .find(|entry| {
                entry.contribution_id == contribution_id
                    && entry.feature_id == identity.feature_id()
                    && entry.kind == ContributionKindV1::Resource
                    && entry
                        .required_capabilities
                        .iter()
                        .any(|capability| capability == "virtual_folder.read")
            })
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let runtime = self
            .runtime_authority
            .as_ref()
            .cloned()
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let envelope = runtime
            .issue(AuthorityClaimsV1 {
                package_id: identity.package_id().to_owned(),
                feature_id: contribution.feature_id.clone(),
                interface_id: contribution.contribution_id.clone(),
                incarnation: lease.epoch(),
                capability: "virtual_folder.read".to_owned(),
                authorized_root_sha256: identity.sealed_manifest_digest().to_owned(),
                location_generation,
                item_generation,
                refresh_generation,
                container_generation,
                job_generation,
            })
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        runtime
            .revalidate(&envelope, AuthorityAdapterV1::VirtualLocation)
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        drop(lease);
        Ok(VirtualLocationAuthorityV1::from_host(runtime, envelope))
    }

    /// Prepares one production provider route before any ABI callback enters.
    /// The returned transaction exposes only host control/result operations and
    /// retains the linear lease until it publishes or fail-closes.
    pub fn prepare_registered_provider(
        &self,
        identity: &NativeRootIdentityV1,
        contribution_id: &str,
        runtime: Arc<ExtensionJobRuntimeV1>,
        job_generation: u64,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
        has_item: bool,
    ) -> Result<PreparedNativeJobV1, ExtensionJobRuntimeErrorV1> {
        self.prepare_registered_provider_with_input(
            identity,
            contribution_id,
            runtime,
            job_generation,
            item_generation,
            location_generation,
            source_generation,
            has_item,
            None,
        )
    }

    /// Production stream-aware route. The source has already been opened and
    /// identity-attested by host code; sealed contribution authorization is
    /// checked again by the runtime before any ABI callback receives it.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_registered_provider_with_input(
        &self,
        identity: &NativeRootIdentityV1,
        contribution_id: &str,
        runtime: Arc<ExtensionJobRuntimeV1>,
        job_generation: u64,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
        has_item: bool,
        input_stream: Option<HostInputStreamSourceV1>,
    ) -> Result<PreparedNativeJobV1, ExtensionJobRuntimeErrorV1> {
        // Test seams can construct a lifecycle directly. Bind once here as
        // well, then verify the transaction uses a lifecycle-local runtime.
        self.bind_job_runtime(&runtime);
        if !self.has_job_runtime(&runtime) {
            return Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority);
        }
        let authority = self.mint_job_authority(identity, contribution_id)?;
        let provider_key = (
            identity.package_id().to_owned(),
            identity.sealed_manifest_digest().to_owned(),
            contribution_id.to_owned(),
        );
        let provider = self
            .lock()
            .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?
            .providers
            .get(&provider_key)
            .cloned()
            .ok_or(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
        let producer = authority.producer().clone();
        let stream_authority = if let Some(runtime_authority) = &self.runtime_authority
            && input_stream.is_some()
        {
            let interface = producer.interface_id();
            let envelope = runtime_authority
                .issue(AuthorityClaimsV1 {
                    package_id: producer.package_id().to_owned(),
                    feature_id: producer.feature_id().to_owned(),
                    interface_id: format!("{}:{}", interface.namespace.into_raw(), interface.value),
                    incarnation: producer.feature_epoch(),
                    capability: "filesystem.read".to_owned(),
                    authorized_root_sha256: producer.sealed_manifest_digest().to_owned(),
                    location_generation,
                    item_generation,
                    refresh_generation: source_generation,
                    container_generation: source_generation,
                    job_generation,
                })
                .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
            runtime_authority
                .revalidate(&envelope, AuthorityAdapterV1::Stream)
                .map_err(|_| ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)?;
            Some(envelope)
        } else {
            None
        };
        let Some(markers) = self.markers.as_ref() else {
            return Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority);
        };
        let marker = plugin_call_guard::marker_with_operation(
            producer.package_id(),
            producer.sealed_manifest_digest(),
            contribution_id,
            "provider",
            producer.interface_id().namespace.into_raw(),
            producer.interface_id().value,
            NativeCallOperationV1::JobProvider,
        );
        let ticket = runtime.prepare_provider_dispatch(ExtensionJobRuntimeRequestV1 {
            authority,
            job_generation,
            item_generation,
            location_generation,
            source_generation,
            has_item,
            input_stream,
        })?;
        Ok(PreparedNativeJobV1 {
            runtime,
            ticket,
            provider,
            markers: Arc::clone(markers),
            marker,
            permit: None,
            callback_started: false,
            callback_elapsed: None,
            runtime_authority: self.runtime_authority.clone(),
            stream_authority,
        })
    }

    /// Convenience synchronous route for callers that do not need to observe
    /// the prepared handle/control surface. Schedulers should prefer
    /// [`Self::prepare_registered_provider`].
    pub fn dispatch_registered_provider(
        &self,
        identity: &NativeRootIdentityV1,
        contribution_id: &str,
        runtime: Arc<ExtensionJobRuntimeV1>,
        job_generation: u64,
        item_generation: u64,
        location_generation: u64,
        source_generation: u64,
        has_item: bool,
    ) -> Result<ExtensionJobFinishOutcomeV1, ExtensionJobRuntimeErrorV1> {
        let mut prepared = self.prepare_registered_provider(
            identity,
            contribution_id,
            runtime,
            job_generation,
            item_generation,
            location_generation,
            source_generation,
            has_item,
        )?;
        let terminal = prepared.call_provider()?;
        prepared.publish_terminal_after_marker_clear(terminal)
    }

    /// Closes a feature gate, performs ordered drain hooks, then waits boundedly.
    pub fn disable(
        &self,
        identity: &NativeRootIdentityV1,
    ) -> Result<NativeFeatureStateV1, NativeLifecycleErrorV1> {
        let deadline = Instant::now() + self.drain_timeout;
        let token = {
            let mut state = self.lock()?;
            ensure_running(&state)?;
            let token = next_operation_token(&mut state)?;
            let gate = state
                .gates
                .get_mut(identity)
                .ok_or_else(|| NativeLifecycleErrorV1::UnknownRoot(identity.clone()))?;
            if !matches!(gate.operation, GateOperationV1::Idle) {
                return Err(NativeLifecycleErrorV1::OperationInProgress);
            }
            match gate.state {
                NativeFeatureStateV1::Enabled { .. } => {
                    gate.accepting = false;
                    gate.state = NativeFeatureStateV1::Disabling;
                    gate.operation = GateOperationV1::Disabling(token);
                    token
                }
                NativeFeatureStateV1::Disabling => {
                    return Err(NativeLifecycleErrorV1::OperationInProgress);
                }
                other => return Ok(other),
            }
        };
        if let Some(runtime_authority) = &self.runtime_authority {
            runtime_authority
                .revoke_feature(identity.package_id(), identity.feature_id())
                .map_err(|_| NativeLifecycleErrorV1::StatePoisoned)?;
        }
        let drained = self.drain(identity, deadline)?;
        let mut state = self.lock()?;
        let running = state.phase == Some(LifecyclePhaseV1::Running);
        let gate = state
            .gates
            .get_mut(identity)
            .ok_or_else(|| NativeLifecycleErrorV1::UnknownRoot(identity.clone()))?;
        if !running || gate.operation != GateOperationV1::Disabling(token) {
            return Ok(gate.state);
        }
        gate.accepting = false;
        gate.operation = GateOperationV1::Idle;
        if !drained {
            gate.state = NativeFeatureStateV1::PendingRestart {
                primary_reason: NativeRestartReasonV1::DrainTimedOut,
            };
            let _ = gate;
            insert_restart_reason(&mut state, identity, NativeRestartReasonV1::DrainTimedOut);
            return Ok(NativeFeatureStateV1::PendingRestart {
                primary_reason: NativeRestartReasonV1::DrainTimedOut,
            });
        }
        gate.state = NativeFeatureStateV1::DisabledResident;
        Ok(gate.state)
    }

    /// Reopens a successfully drained resident blueprint without re-claiming it.
    pub fn enable(
        &self,
        identity: &NativeRootIdentityV1,
    ) -> Result<NativeFeatureStateV1, NativeLifecycleErrorV1> {
        let token = {
            let mut state = self.lock()?;
            ensure_running(&state)?;
            let token = next_operation_token(&mut state)?;
            let Some(gate) = state.gates.get_mut(identity) else {
                insert_restart_reason(&mut state, identity, NativeRestartReasonV1::UnloadedEnable);
                return Err(NativeLifecycleErrorV1::RestartRequired {
                    identity: identity.clone(),
                    reason: NativeRestartReasonV1::UnloadedEnable,
                });
            };
            if !matches!(gate.operation, GateOperationV1::Idle) {
                return Err(NativeLifecycleErrorV1::OperationInProgress);
            }
            match gate.state {
                NativeFeatureStateV1::DisabledResident => {
                    gate.operation = GateOperationV1::Enabling(token);
                    token
                }
                NativeFeatureStateV1::Enabled { .. } => return Ok(gate.state),
                NativeFeatureStateV1::PendingRestart { primary_reason } => {
                    return Err(NativeLifecycleErrorV1::RestartRequired {
                        identity: identity.clone(),
                        reason: primary_reason,
                    });
                }
                other => return Ok(other),
            }
        };
        self.drain_port.restore(identity);
        let mut state = self.lock()?;
        let running = state.phase == Some(LifecyclePhaseV1::Running);
        let gate = state
            .gates
            .get_mut(identity)
            .ok_or_else(|| NativeLifecycleErrorV1::UnknownRoot(identity.clone()))?;
        if !running || gate.operation != GateOperationV1::Enabling(token) {
            let _ = gate;
            drop(state);
            // A late restore must not leave revived contributions after shutdown
            // or a superseding operation invalidated its token.
            self.drain_port.detach(identity);
            self.drain_port.cancel(identity);
            return Err(NativeLifecycleErrorV1::OperationSuperseded);
        }
        let Some(epoch) = gate.epoch.checked_add(1) else {
            gate.accepting = false;
            gate.operation = GateOperationV1::Idle;
            gate.state = NativeFeatureStateV1::DisabledResident;
            let _ = gate;
            drop(state);
            // `restore` already ran before the checked epoch transition. Undo
            // that revival before returning an overflow terminal.
            self.drain_port.detach(identity);
            self.drain_port.cancel(identity);
            return Err(NativeLifecycleErrorV1::GenerationOverflow);
        };
        gate.epoch = epoch;
        gate.state = NativeFeatureStateV1::Enabled { epoch };
        gate.operation = GateOperationV1::Idle;
        // Restore completes before this final accepting transition.
        gate.accepting = true;
        Ok(gate.state)
    }

    /// Records a restart-only change without loading or unloading a DLL.
    pub fn require_restart(
        &self,
        identity: &NativeRootIdentityV1,
        reason: NativeRestartReasonV1,
    ) -> Result<(), NativeLifecycleErrorV1> {
        if matches!(
            reason,
            NativeRestartReasonV1::DrainTimedOut | NativeRestartReasonV1::StartupAborted
        ) {
            return Err(NativeLifecycleErrorV1::InvalidRestartReason);
        }
        if !matches!(reason, NativeRestartReasonV1::Remove) {
            let mut state = self.lock()?;
            ensure_running(&state)?;
            insert_restart_reason(&mut state, identity, reason);
            return Ok(());
        }
        let deadline = Instant::now() + self.drain_timeout;
        let token = {
            let mut state = self.lock()?;
            ensure_running(&state)?;
            // Remove is an orthogonal persisted fact even when a prior drain,
            // fault, or concurrent terminal state makes no new transition safe.
            insert_restart_reason(&mut state, identity, NativeRestartReasonV1::Remove);
            let token = next_operation_token(&mut state)?;
            let Some(gate) = state.gates.get_mut(identity) else {
                return Ok(());
            };
            if !matches!(gate.operation, GateOperationV1::Idle) {
                return Err(NativeLifecycleErrorV1::OperationInProgress);
            }
            match gate.state {
                NativeFeatureStateV1::Enabled { .. } => {
                    gate.accepting = false;
                    gate.state = NativeFeatureStateV1::Disabling;
                    gate.operation = GateOperationV1::Removing(token);
                    Some(token)
                }
                NativeFeatureStateV1::DisabledResident => None,
                _ => return Ok(()),
            }
        };
        let drained = match token {
            Some(token) => Some((token, self.drain(identity, deadline)?)),
            None => None,
        };
        let mut state = self.lock()?;
        ensure_running(&state)?;
        insert_restart_reason(&mut state, identity, reason);
        let running = state.phase == Some(LifecyclePhaseV1::Running);
        if let Some(gate) = state.gates.get_mut(identity) {
            if let Some((token, _)) = drained
                && gate.operation != GateOperationV1::Removing(token)
            {
                return Ok(());
            }
            if !running {
                return Ok(());
            }
            gate.accepting = false;
            gate.operation = GateOperationV1::Idle;
            gate.state = NativeFeatureStateV1::PendingRestart {
                primary_reason: NativeRestartReasonV1::Remove,
            };
            if let Some((_, false)) = drained {
                let _ = gate;
                insert_restart_reason(&mut state, identity, NativeRestartReasonV1::DrainTimedOut);
            }
        }
        Ok(())
    }

    /// Projects lifecycle facts for the pure 3.1 effective-state resolver.
    pub fn runtime_fact(
        &self,
        identity: &NativeRootIdentityV1,
    ) -> Result<FeatureRuntimeFactV1, NativeLifecycleErrorV1> {
        let state = self.lock()?;
        let Some(gate) = state.gates.get(identity) else {
            return Ok(if restart_pending_for_slot(&state, identity) {
                FeatureRuntimeFactV1::PendingRestart
            } else {
                FeatureRuntimeFactV1::Ready
            });
        };
        Ok(match gate.state {
            NativeFeatureStateV1::Disabling => FeatureRuntimeFactV1::Disabling,
            NativeFeatureStateV1::PendingRestart { .. } => FeatureRuntimeFactV1::PendingRestart,
            NativeFeatureStateV1::Faulted => FeatureRuntimeFactV1::Faulted,
            NativeFeatureStateV1::Enabled { .. } | NativeFeatureStateV1::DisabledResident => {
                if restart_pending_for_slot(&state, identity) {
                    FeatureRuntimeFactV1::PendingRestart
                } else {
                    FeatureRuntimeFactV1::Ready
                }
            }
        })
    }

    /// Returns all preserved restart facts for diagnostics without changing state.
    pub fn restart_reasons(
        &self,
        identity: &NativeRootIdentityV1,
    ) -> Result<Vec<NativeRestartReasonV1>, NativeLifecycleErrorV1> {
        let state = self.lock()?;
        Ok(state
            .restart_reasons
            .get(identity)
            .map(|reasons| reasons.iter().copied().collect())
            .unwrap_or_default())
    }

    /// Installs one synthetic dispatch gate over a real, sealed, activated DLL root.
    ///
    /// This is deliberately feature-gated so production code cannot create an
    /// authority that was not declared by the sealed package manifest.
    #[cfg(feature = "integration-test-support")]
    #[doc(hidden)]
    pub fn install_integration_test_dispatch_gate(
        &self,
        admission: &NativeStartupAdmissionV1,
    ) -> Result<NativeFeatureIdentityV1, NativeLifecycleErrorV1> {
        let feature = FeatureKeyV1::new(admission.package_id.clone(), "integration-test-dispatch")
            .map_err(|_| NativeLifecycleErrorV1::InvalidFeatureAuthority)?;
        let identity = NativeFeatureIdentityV1 {
            package_id: admission.package_id.clone(),
            sealed_manifest_digest: admission.sealed_manifest_digest.clone(),
            feature,
        };
        let mut state = self.lock()?;
        ensure_running(&state)?;
        if state.gates.contains_key(&identity) {
            return Ok(identity);
        }
        if state.gates.len() >= MAX_NATIVE_FEATURE_GATES_V1 {
            return Err(NativeLifecycleErrorV1::FeatureGateLimitExceeded);
        }
        let Some(member) = state.entries.iter().find_map(|(entry, entry_state)| {
            (entry.package_id == admission.package_id
                && entry.sealed_manifest_digest == admission.sealed_manifest_digest
                && *entry_state == EntryStateV1::Activated
                && state.validated_roots.contains_key(entry))
            .then(|| entry.clone())
        }) else {
            return Err(NativeLifecycleErrorV1::InvalidFeatureAuthority);
        };
        state.gates.insert(
            identity.clone(),
            FeatureGateV1 {
                state: NativeFeatureStateV1::Enabled { epoch: 1 },
                accepting: true,
                in_flight: 0,
                epoch: 1,
                operation: GateOperationV1::Idle,
                members: BTreeSet::from([member]),
            },
        );
        Ok(identity)
    }

    /// Whether the feature-gated integration dispatch authority still retains
    /// an activated, validated DLL root in this process.
    #[cfg(feature = "integration-test-support")]
    #[doc(hidden)]
    pub fn integration_test_has_resident_validated_root(
        &self,
        identity: &NativeFeatureIdentityV1,
    ) -> Result<bool, NativeLifecycleErrorV1> {
        let state = self.lock()?;
        let gate = state
            .gates
            .get(identity)
            .ok_or_else(|| NativeLifecycleErrorV1::UnknownRoot(identity.clone()))?;
        Ok(gate.members.iter().any(|member| {
            state.entries.get(member) == Some(&EntryStateV1::Activated)
                && state.validated_roots.contains_key(member)
        }))
    }

    /// Closes every gate, invokes bounded cancellation/drain, and never unloads.
    pub fn shutdown(&self) {
        let deadline = Instant::now() + self.drain_timeout;
        let identities = match self.shared.state.lock() {
            Ok(mut state) => {
                if state.phase == Some(LifecyclePhaseV1::Stopped) {
                    return;
                }
                state.phase = Some(LifecyclePhaseV1::Stopped);
                // Shutdown is one-way: an exhausted diagnostic generation never
                // reopens admission or reuses a token, so leave it unchanged.
                if let Some(next) = state.shutdown_generation.checked_add(1) {
                    state.shutdown_generation = next;
                }
                state
                    .gates
                    .iter_mut()
                    .filter_map(|(identity, gate)| {
                        let needs_drain = matches!(
                            gate.state,
                            NativeFeatureStateV1::Enabled { .. } | NativeFeatureStateV1::Disabling
                        ) || !matches!(gate.operation, GateOperationV1::Idle);
                        gate.accepting = false;
                        gate.operation = GateOperationV1::Idle;
                        if needs_drain {
                            gate.state = NativeFeatureStateV1::Disabling;
                            Some(identity.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            }
            Err(_) => return,
        };
        for identity in identities {
            self.drain_port.detach(&identity);
            self.drain_port.cancel(&identity);
            let local_drained = self
                .wait_for_local_leases_until(&identity, deadline)
                .unwrap_or(false);
            let port_drained = self
                .drain_port
                .wait_for_drain(&identity, remaining_until(deadline));
            if let Ok(mut state) = self.lock()
                && let Some(gate) = state.gates.get_mut(&identity)
                && matches!(gate.state, NativeFeatureStateV1::Disabling)
            {
                gate.accepting = false;
                if local_drained && port_drained {
                    gate.state = NativeFeatureStateV1::DisabledResident;
                } else {
                    gate.state = NativeFeatureStateV1::PendingRestart {
                        primary_reason: NativeRestartReasonV1::DrainTimedOut,
                    };
                    let _ = gate;
                    insert_restart_reason(
                        &mut state,
                        &identity,
                        NativeRestartReasonV1::DrainTimedOut,
                    );
                }
            }
        }
    }

    fn admit_loaded(
        &self,
        permit: AdmissionPermitV1,
        resolved: &ResolvedPackageV1<'_>,
        loaded: &LoadedPackageRootsV1,
    ) -> Result<NativeStartupAdmissionV1, NativeLifecycleErrorV1> {
        let package_id = loaded.package_id().to_owned();
        let package_version = loaded.package_version().to_owned();
        let digest = loaded.sealed_manifest_digest().to_owned();
        let roots = loaded
            .roots()
            .iter()
            .map(|root| (root.entrypoint_id(), Some(root)))
            .collect::<Vec<_>>();
        let identities = self.admit_entries(
            permit,
            &package_id,
            &package_version,
            &digest,
            &roots,
            &resolved.manifest().features,
            Some(resolved),
        )?;
        Ok(NativeStartupAdmissionV1 {
            package_id,
            package_version,
            sealed_manifest_digest: digest,
            root_count: roots.len(),
            activated_feature_count: identities.into_iter().collect::<BTreeSet<_>>().len(),
        })
    }

    fn admit_entries(
        &self,
        permit: AdmissionPermitV1,
        package_id: &str,
        package_version: &str,
        digest: &str,
        roots: &[(&str, Option<&LoadedExtensionRootV1>)],
        declared_features: &[crate::PackageFeatureV1],
        resolved: Option<&ResolvedPackageV1<'_>>,
    ) -> Result<Vec<NativeRootIdentityV1>, NativeLifecycleErrorV1> {
        if roots.len() > MAX_NATIVE_LEDGER_ENTRIES_V1 {
            return Err(NativeLifecycleErrorV1::LedgerLimitExceeded);
        }
        let entry_keys = roots
            .iter()
            .map(|(entrypoint_id, _)| EntryKeyV1 {
                package_id: package_id.to_owned(),
                sealed_manifest_digest: digest.to_owned(),
                entrypoint_id: (*entrypoint_id).to_owned(),
            })
            .collect::<Vec<_>>();
        if let Some(markers) = self.markers.as_ref() {
            for (entrypoint_id, root) in roots {
                if let Some(root) = root {
                    let context = NativeActivationContextV1 {
                        package_id,
                        package_version,
                        sealed_manifest_digest: digest,
                        entrypoint_id,
                        root: Some(*root),
                    };
                    let marker = marker_for_loaded_root(&context, root);
                    if markers.denies(&marker) {
                        markers.record_timing(
                            &marker,
                            Duration::ZERO,
                            NativeCallTerminalV1::SafeModeDenied,
                        );
                        return Err(NativeLifecycleErrorV1::SafeModeDenied);
                    }
                }
            }
        }
        {
            let mut state = self.lock()?;
            ensure_admission_active(&state, permit)?;
            if state
                .entries
                .len()
                .checked_add(entry_keys.len())
                .is_none_or(|count| count > MAX_NATIVE_LEDGER_ENTRIES_V1)
            {
                return Err(NativeLifecycleErrorV1::LedgerLimitExceeded);
            }
            if entry_keys.iter().any(|key| state.entries.contains_key(key)) {
                return Err(NativeLifecycleErrorV1::AlreadyClaimed);
            }
            for (key, (_, root)) in entry_keys.iter().zip(roots) {
                state.entries.insert(key.clone(), EntryStateV1::Prepared);
                if let Some(root) = root {
                    state.validated_roots.insert(key.clone(), (*root).clone());
                }
            }
        }

        let mut activated_blueprints = Vec::new();
        let mut deferred_entries = Vec::new();
        for (key, (_, root)) in entry_keys.iter().cloned().zip(roots) {
            {
                let mut state = self.lock()?;
                ensure_admission_active(&state, permit)?;
                let entry = state
                    .entries
                    .get_mut(&key)
                    .ok_or(NativeLifecycleErrorV1::AlreadyClaimed)?;
                if *entry != EntryStateV1::Prepared {
                    return Err(NativeLifecycleErrorV1::AlreadyClaimed);
                }
                *entry = EntryStateV1::Claimed;
            }
            let claimed = self.executor.claim(NativeActivationContextV1 {
                package_id: &key.package_id,
                package_version,
                sealed_manifest_digest: &key.sealed_manifest_digest,
                entrypoint_id: &key.entrypoint_id,
                root: *root,
            });
            match claimed {
                Ok(NativeActivationClaimV1::Deferred) => {
                    deferred_entries.push(key);
                }
                Ok(NativeActivationClaimV1::Activated(blueprint)) => {
                    let identities = match validate_blueprint(
                        &key,
                        package_version,
                        &blueprint,
                        declared_features,
                    ) {
                        Ok(identities) => identities,
                        Err(error) => {
                            self.fail_or_rollback_admission(
                                &entry_keys,
                                activated_blueprints.is_empty(),
                            );
                            return Err(error);
                        }
                    };
                    activated_blueprints.push((key, blueprint, identities));
                }
                Err(NativeActivationFailureV1::SafeModeDenied) => {
                    self.fail_or_rollback_admission(&entry_keys, activated_blueprints.is_empty());
                    return Err(NativeLifecycleErrorV1::SafeModeDenied);
                }
                Err(NativeActivationFailureV1::Rejected) => {
                    self.fail_or_rollback_admission(&entry_keys, activated_blueprints.is_empty());
                    return Err(NativeLifecycleErrorV1::ActivationRejected);
                }
                Err(NativeActivationFailureV1::Faulted) => {
                    self.fail_or_rollback_admission(&entry_keys, activated_blueprints.is_empty());
                    return Err(NativeLifecycleErrorV1::ActivationFaulted);
                }
            }
        }
        let mut registrations = Vec::new();
        let mut providers = BTreeMap::new();
        let mut staged_gates = Vec::new();
        let mut activated = Vec::new();
        let mut activated_entries = Vec::new();
        for (key, blueprint, identities) in activated_blueprints {
            if let Some(mut root_registrations) = blueprint.registrations {
                if registrations
                    .len()
                    .checked_add(root_registrations.len())
                    .is_none_or(|count| count > crate::MAX_CONTRIBUTIONS_PER_BATCH_V1)
                {
                    self.rollback_admission_entries(&entry_keys);
                    return Err(NativeLifecycleErrorV1::ActivationRejected);
                }
                registrations.append(&mut root_registrations);
            }
            for (contribution_id, provider) in blueprint.providers {
                if providers.insert(contribution_id, provider).is_some() {
                    self.rollback_admission_entries(&entry_keys);
                    return Err(NativeLifecycleErrorV1::ActivationRejected);
                }
            }
            for identity in identities {
                staged_gates.push((identity.clone(), key.clone()));
                activated.push(identity);
            }
            activated_entries.push(key);
        }
        let validated = if registrations.is_empty() {
            None
        } else {
            let Some(resolved) = resolved else {
                self.rollback_admission_entries(&entry_keys);
                return Err(NativeLifecycleErrorV1::ActivationRejected);
            };
            if let Ok(validated) = ContributionGateV1::validate(resolved, &registrations) {
                Some(validated)
            } else {
                self.rollback_admission_entries(&entry_keys);
                return Err(NativeLifecycleErrorV1::ActivationRejected);
            }
        };
        let generation = (package_id.to_owned(), digest.to_owned());
        let commit = (|| {
            let mut state = self.lock()?;
            ensure_admission_active(&state, permit)?;
            if validated.is_some()
                && (state.sealed_contributions.contains_key(&generation)
                    || providers.keys().any(|contribution_id| {
                        state.providers.contains_key(&(
                            generation.0.clone(),
                            generation.1.clone(),
                            contribution_id.clone(),
                        ))
                    }))
            {
                return Err(NativeLifecycleErrorV1::ActivationRejected);
            }
            let new_gate_count = staged_gates
                .iter()
                .map(|(identity, _)| identity.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter(|identity| !state.gates.contains_key(identity))
                .count();
            if state
                .gates
                .len()
                .checked_add(new_gate_count)
                .is_none_or(|count| count > MAX_NATIVE_FEATURE_GATES_V1)
            {
                return Err(NativeLifecycleErrorV1::FeatureGateLimitExceeded);
            }
            if deferred_entries
                .iter()
                .chain(activated_entries.iter())
                .any(|key| !state.entries.contains_key(key))
            {
                return Err(NativeLifecycleErrorV1::AlreadyClaimed);
            }
            for key in &deferred_entries {
                *state
                    .entries
                    .get_mut(key)
                    .ok_or(NativeLifecycleErrorV1::AlreadyClaimed)? = EntryStateV1::Deferred;
            }
            for key in &activated_entries {
                *state
                    .entries
                    .get_mut(key)
                    .ok_or(NativeLifecycleErrorV1::AlreadyClaimed)? = EntryStateV1::Activated;
            }
            for (identity, member) in &staged_gates {
                state
                    .gates
                    .entry(identity.clone())
                    .and_modify(|gate| {
                        gate.members.insert(member.clone());
                    })
                    .or_insert_with(|| FeatureGateV1 {
                        state: NativeFeatureStateV1::DisabledResident,
                        accepting: false,
                        in_flight: 0,
                        epoch: 0,
                        operation: GateOperationV1::Idle,
                        members: BTreeSet::from([member.clone()]),
                    });
            }
            for (contribution_id, provider) in providers {
                let provider_key = (generation.0.clone(), generation.1.clone(), contribution_id);
                debug_assert!(
                    state
                        .providers
                        .insert(provider_key, Arc::new(provider))
                        .is_none()
                );
            }
            if let Some(validated) = validated {
                state.sealed_contributions.insert(generation, validated);
            }
            Ok(())
        })();
        if let Err(error) = commit {
            self.rollback_admission_entries(&entry_keys);
            return Err(error);
        }
        Ok(activated)
    }

    /// Reverts a package-generation admission before its single atomic commit.
    /// Retained ABI roots are released after the lifecycle mutex so their
    /// destruction can never re-enter host state while it is locked.
    fn rollback_admission_entries(&self, entry_keys: &[EntryKeyV1]) {
        let roots = self.shared.state.lock().map_or_else(
            |_| Vec::new(),
            |mut state| {
                for key in entry_keys {
                    state.entries.remove(key);
                }
                entry_keys
                    .iter()
                    .filter_map(|key| state.validated_roots.remove(key))
                    .collect::<Vec<_>>()
            },
        );
        drop(roots);
    }

    fn fail_or_rollback_admission(&self, entry_keys: &[EntryKeyV1], first_root_failed: bool) {
        if !first_root_failed {
            self.rollback_admission_entries(entry_keys);
            return;
        }
        if let Ok(mut state) = self.shared.state.lock() {
            for key in entry_keys {
                if let Some(entry) = state.entries.get_mut(key) {
                    *entry = EntryStateV1::Rejected;
                }
            }
        }
    }

    fn seal_startup(&self) -> Result<(), NativeLifecycleErrorV1> {
        let mut state = self.lock()?;
        if state.phase != Some(LifecyclePhaseV1::Admitting) {
            return Err(NativeLifecycleErrorV1::StartupClosed);
        }
        let rejected = state.rejected_generations.clone();
        for (identity, gate) in &mut state.gates {
            if rejected.contains(&(
                identity.package_id.clone(),
                identity.sealed_manifest_digest.clone(),
            )) {
                gate.accepting = false;
                continue;
            }
            gate.state = NativeFeatureStateV1::Enabled { epoch: 1 };
            gate.epoch = 1;
            gate.accepting = true;
        }
        state.phase = Some(LifecyclePhaseV1::Running);
        Ok(())
    }

    fn reserve_admission(&self) -> Result<AdmissionPermitV1, NativeLifecycleErrorV1> {
        let state = self.lock()?;
        if state.phase != Some(LifecyclePhaseV1::Admitting) {
            return Err(NativeLifecycleErrorV1::StartupClosed);
        }
        Ok(AdmissionPermitV1 {
            generation: state.startup_generation,
        })
    }

    fn abort_startup(&self) {
        let providers = (|| {
            let Ok(mut state) = self.shared.state.lock() else {
                return None;
            };
            if state.phase != Some(LifecyclePhaseV1::Admitting) {
                return None;
            }
            let identities = state.gates.keys().cloned().collect::<Vec<_>>();
            for gate in state.gates.values_mut() {
                gate.accepting = false;
                gate.state = NativeFeatureStateV1::PendingRestart {
                    primary_reason: NativeRestartReasonV1::StartupAborted,
                };
            }
            for identity in identities {
                insert_restart_reason(&mut state, &identity, NativeRestartReasonV1::StartupAborted);
            }
            state.sealed_contributions.clear();
            let providers = std::mem::take(&mut state.providers);
            state.phase = Some(LifecyclePhaseV1::Closed);
            Some(providers)
        })();
        // ABI-owned provider objects can run arbitrary plugin `Drop`; release
        // them only after the lifecycle mutex is no longer held.
        drop(providers);
    }

    fn wait_for_local_leases_until(
        &self,
        identity: &NativeRootIdentityV1,
        deadline: Instant,
    ) -> Result<bool, NativeLifecycleErrorV1> {
        let mut state = self.lock()?;
        loop {
            let gate = state
                .gates
                .get(identity)
                .ok_or_else(|| NativeLifecycleErrorV1::UnknownRoot(identity.clone()))?;
            if gate.in_flight == 0 {
                return Ok(true);
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }
            let waited = self.shared.drained.wait_timeout(state, deadline - now);
            match waited {
                Ok((next, _)) => state = next,
                Err(_) => return Err(NativeLifecycleErrorV1::StatePoisoned),
            }
        }
    }

    fn drain(
        &self,
        identity: &NativeRootIdentityV1,
        deadline: Instant,
    ) -> Result<bool, NativeLifecycleErrorV1> {
        let scope = {
            let state = self.lock()?;
            let gate = state
                .gates
                .get(identity)
                .ok_or_else(|| NativeLifecycleErrorV1::UnknownRoot(identity.clone()))?;
            NativeFeatureDrainScopeV1::new(identity.clone(), gate.epoch)
        };
        // Foreign hooks and bounded waiting intentionally run without the
        // lifecycle state lock so reentrant hooks cannot deadlock the manager.
        self.drain_port.detach(identity);
        self.drain_port.cancel_scope(&scope);
        let local_drained = self.wait_for_local_leases_until(identity, deadline)?;
        let port_drained = self
            .drain_port
            .wait_for_drain_scope(&scope, remaining_until(deadline));
        Ok(local_drained && port_drained)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, RuntimeStateV1>, NativeLifecycleErrorV1> {
        self.shared
            .state
            .lock()
            .map_err(|_| NativeLifecycleErrorV1::StatePoisoned)
    }

    #[cfg(test)]
    fn with_test_ports(
        executor: Arc<dyn NativeActivationExecutor>,
        drain_port: Arc<dyn NativeDrainPortV1>,
        timeout: Duration,
    ) -> Self {
        Self::with_ports_and_markers(
            executor,
            drain_port,
            timeout,
            None,
            Arc::new(Mutex::new(Vec::new())),
            None,
        )
    }

    #[cfg(test)]
    fn admit_synthetic(
        &self,
        package: &str,
        digest: &str,
        entries: &[&str],
        features: &[&str],
    ) -> Result<Vec<NativeRootIdentityV1>, NativeLifecycleErrorV1> {
        let declared = features
            .iter()
            .map(|id| crate::PackageFeatureV1 {
                id: (*id).to_owned(),
                capabilities: Vec::new(),
                dependencies: Vec::new(),
            })
            .collect::<Vec<_>>();
        let permit = self.reserve_admission()?;
        self.admit_entries(
            permit,
            package,
            "1.0.0",
            digest,
            &entries
                .iter()
                .map(|entry| (*entry, None))
                .collect::<Vec<_>>(),
            &declared,
            None,
        )
    }
}

impl Drop for NativeExtensionLifecycleV1 {
    fn drop(&mut self) {
        // Drop must be prompt: leases retain `shared`, and no foreign hook or wait
        // may run while destructing application resources.
        if let Ok(mut state) = self.shared.state.lock() {
            for gate in state.gates.values_mut() {
                gate.accepting = false;
            }
            state.phase = Some(LifecyclePhaseV1::Stopped);
        }
    }
}

/// A non-cloneable linear capability for startup loading.
pub struct StartupSession<'a> {
    lifecycle: &'a mut NativeExtensionLifecycleV1,
    sealed: bool,
}

impl StartupSession<'_> {
    /// Loads only a resolver-selected, sealed package through the private loader.
    pub fn admit_resolved_package(
        &mut self,
        resolved: &ResolvedPackageV1<'_>,
    ) -> Result<NativeStartupAdmissionV1, NativeLifecycleErrorV1> {
        // Reserve while still in admission before any private loader side effect.
        let permit = self.lifecycle.reserve_admission()?;
        let mut load_marker = if resolved.manifest().rust.is_empty() {
            None
        } else if let Some(markers) = self.lifecycle.markers.as_ref() {
            let digest =
                crate::package_validation::sealed_manifest_canonical_digest(resolved.manifest())
                    .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
            let entrypoint = resolved
                .manifest()
                .rust
                .iter()
                .map(|entry| entry.id.as_str())
                .min()
                .ok_or(NativeLifecycleErrorV1::MarkerStateUnavailable)?;
            let marker = plugin_call_guard::marker_with_operation(
                &resolved.manifest().package.id,
                &digest,
                entrypoint,
                "root-contract-v1",
                ROOT_MODULE_CONTRACT_ID_V1.namespace.into_raw(),
                ROOT_MODULE_CONTRACT_ID_V1.value,
                NativeCallOperationV1::LoadLibrary,
            );
            Some(match markers.begin(&marker) {
                Ok(guard) => guard,
                Err(GuardErrorV1::Denied) => return Err(NativeLifecycleErrorV1::SafeModeDenied),
                Err(GuardErrorV1::Fault) => {
                    return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
                }
            })
        } else {
            None
        };
        let loaded = match ExtensionDllLoaderV1.load_package(resolved) {
            Ok(loaded) => loaded,
            Err(error) => {
                if let Some(marker) = load_marker.as_mut() {
                    marker
                        .transition_operation(NativeCallOperationV1::LoadRejectedResident)
                        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
                }
                return Err(NativeLifecycleErrorV1::LoaderRejected {
                    diagnostic: NativeLoaderDiagnosticCodeV1::from_loader(&error),
                });
            }
        };
        let admission = match self.lifecycle.admit_loaded(permit, resolved, &loaded) {
            Ok(admission) => admission,
            Err(error) => {
                // Loading completed and the guarded registrar returned a typed
                // rejection. This is not an interrupted LoadLibrary call and
                // must not leave a crash marker that disables every plugin on
                // the next launch.
                if let Some(marker) = load_marker {
                    marker
                        .clear()
                        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
                }
                return Err(error);
            }
        };
        if let Some(marker) = load_marker {
            marker
                .clear()
                .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
        }
        Ok(admission)
    }

    /// Permanently closes startup admission and enables successfully prepared gates.
    pub fn seal(mut self) -> Result<(), NativeLifecycleErrorV1> {
        self.lifecycle.seal_startup()?;
        self.sealed = true;
        Ok(())
    }

    #[cfg(test)]
    fn admit_synthetic(
        &mut self,
        package: &str,
        digest: &str,
        entries: &[&str],
        features: &[&str],
    ) -> Result<Vec<NativeRootIdentityV1>, NativeLifecycleErrorV1> {
        self.lifecycle
            .admit_synthetic(package, digest, entries, features)
    }
}

#[cfg(all(test, feature = "integration-test-support"))]
#[doc(hidden)]
#[allow(dead_code, clippy::wildcard_imports)]
pub mod integration_test_support {
    use super::*;
    use std::sync::Arc;

    struct Executor;
    impl NativeActivationExecutor for Executor {
        fn claim(
            &self,
            context: NativeActivationContextV1<'_>,
        ) -> Result<NativeActivationClaimV1, NativeActivationFailureV1> {
            Ok(NativeActivationClaimV1::Activated(
                NativeActivationBlueprintV1 {
                    package_id: context.package_id.to_owned(),
                    package_version: context.package_version.to_owned(),
                    sealed_manifest_digest: context.sealed_manifest_digest.to_owned(),
                    features: vec![
                        FeatureKeyV1::new(context.package_id, "feature")
                            .map_err(|_| NativeActivationFailureV1::Rejected)?,
                    ],
                    registrations: None,
                    providers: BTreeMap::new(),
                },
            ))
        }
    }
    pub struct LiveDispatchFixtureV1 {
        lifecycle: NativeExtensionLifecycleV1,
        identity: NativeFeatureIdentityV1,
    }
    impl LiveDispatchFixtureV1 {
        pub fn enter(&self) -> Result<NativeDispatchLeaseV1, NativeLifecycleErrorV1> {
            self.lifecycle
                .try_enter(&self.identity)?
                .ok_or(NativeLifecycleErrorV1::InvalidFeatureAuthority)
        }
        #[must_use]
        pub fn package_id(&self) -> &str {
            self.identity.package_id()
        }
        #[must_use]
        pub fn digest(&self) -> &str {
            self.identity.sealed_manifest_digest()
        }
        pub fn disable(&self) -> Result<NativeFeatureStateV1, NativeLifecycleErrorV1> {
            self.lifecycle.disable(&self.identity)
        }
    }
    pub fn live_dispatch_fixture(
        package_id: &str,
        sealed_manifest_digest: &str,
    ) -> Result<LiveDispatchFixtureV1, NativeLifecycleErrorV1> {
        let mut lifecycle = NativeExtensionLifecycleV1::with_test_ports(
            Arc::new(Executor),
            Arc::new(NoopDrainPortV1),
            Duration::from_millis(1),
        );
        let mut session = lifecycle.begin_startup()?;
        let identities = session.admit_synthetic(
            package_id,
            sealed_manifest_digest,
            &["native"],
            &["feature"],
        )?;
        session.seal()?;
        Ok(LiveDispatchFixtureV1 {
            lifecycle,
            identity: identities
                .into_iter()
                .next()
                .ok_or(NativeLifecycleErrorV1::InvalidFeatureAuthority)?,
        })
    }
}

impl Drop for StartupSession<'_> {
    fn drop(&mut self) {
        if !self.sealed {
            self.lifecycle.abort_startup();
        }
    }
}

/// RAII proof that a callback entered while the feature gate was open.
pub struct NativeDispatchLeaseV1 {
    shared: Arc<SharedRuntimeV1>,
    identity: NativeRootIdentityV1,
    epoch: u64,
}

impl NativeDispatchLeaseV1 {
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Identity bound to this still-live dispatch lease.
    #[must_use]
    pub fn feature_identity(&self) -> &NativeFeatureIdentityV1 {
        &self.identity
    }
}

impl Drop for NativeDispatchLeaseV1 {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.lock()
            && let Some(gate) = state.gates.get_mut(&self.identity)
        {
            let Some(next) = gate.in_flight.checked_sub(1) else {
                gate.accepting = false;
                gate.state = NativeFeatureStateV1::PendingRestart {
                    primary_reason: NativeRestartReasonV1::DrainTimedOut,
                };
                self.shared.drained.notify_all();
                return;
            };
            gate.in_flight = next;
            self.shared.drained.notify_all();
        }
    }
}

#[derive(Debug, Error)]
pub enum NativeLifecycleErrorV1 {
    #[error("the process native lifecycle authority is already acquired")]
    AlreadyAcquired,
    #[error("native marker state is unavailable or unsafe")]
    MarkerStateUnavailable,
    #[error("the requested Safe Mode incident is not active")]
    SafeModeIncidentUnknown,
    #[error("native startup admission is permanently closed")]
    StartupClosed,
    #[error("native root ledger exceeds its manifest-derived bound")]
    LedgerLimitExceeded,
    #[error("native root was already prepared or claimed")]
    AlreadyClaimed,
    #[error("native activation was rejected")]
    ActivationRejected,
    #[error("Safe Mode denied native activation before any callback")]
    SafeModeDenied,
    #[error("native activation faulted")]
    ActivationFaulted,
    #[error("native loader rejected the sealed package")]
    LoaderRejected {
        diagnostic: NativeLoaderDiagnosticCodeV1,
    },
    #[error("activation returned an undeclared or mismatched feature")]
    InvalidFeatureAuthority,
    #[error("activation blueprint is not bound to this sealed package generation")]
    ActivationAuthorityMismatch,
    #[error("native feature gates exceed their manifest-derived bound")]
    FeatureGateLimitExceeded,
    #[error("duplicate feature authority {0:?}")]
    DuplicateFeatureAuthority(NativeRootIdentityV1),
    #[error("unknown native root {0:?}")]
    UnknownRoot(NativeRootIdentityV1),
    #[error("native dispatch count overflowed")]
    InFlightOverflow,
    #[error("native lifecycle generation or operation token overflowed")]
    GenerationOverflow,
    #[error("native lifecycle state is poisoned")]
    StatePoisoned,
    #[error("native lifecycle is not running and rejects runtime mutation")]
    LifecycleStopped,
    #[error("internal restart reasons cannot be requested externally")]
    InvalidRestartReason,
    #[error("a native feature transition is already in progress")]
    OperationInProgress,
    #[error("a native feature transition was invalidated by shutdown or replacement")]
    OperationSuperseded,
    #[error("native root requires restart ({reason:?}): {identity:?}")]
    RestartRequired {
        identity: NativeRootIdentityV1,
        reason: NativeRestartReasonV1,
    },
}

fn validate_blueprint(
    entry: &EntryKeyV1,
    package_version: &str,
    blueprint: &NativeActivationBlueprintV1,
    declared: &[crate::PackageFeatureV1],
) -> Result<Vec<NativeRootIdentityV1>, NativeLifecycleErrorV1> {
    if blueprint.package_id != entry.package_id
        || blueprint.package_version != package_version
        || blueprint.sealed_manifest_digest != entry.sealed_manifest_digest
    {
        return Err(NativeLifecycleErrorV1::ActivationAuthorityMismatch);
    }
    let mut unique = BTreeSet::new();
    let mut identities = Vec::with_capacity(blueprint.features.len());
    for feature in &blueprint.features {
        if feature.package_id != entry.package_id
            || !declared
                .iter()
                .any(|declared| declared.id == feature.feature_id)
            || !unique.insert(feature.clone())
        {
            return Err(NativeLifecycleErrorV1::InvalidFeatureAuthority);
        }
        identities.push(NativeFeatureIdentityV1 {
            package_id: entry.package_id.clone(),
            sealed_manifest_digest: entry.sealed_manifest_digest.clone(),
            feature: feature.clone(),
        });
    }
    Ok(identities)
}

fn insert_restart_reason(
    state: &mut RuntimeStateV1,
    identity: &NativeRootIdentityV1,
    reason: NativeRestartReasonV1,
) {
    let reasons = state.restart_reasons.entry(identity.clone()).or_default();
    if reasons.len() < MAX_NATIVE_RESTART_REASONS_PER_FEATURE_V1 || reasons.contains(&reason) {
        reasons.insert(reason);
    }
}

fn ensure_running(state: &RuntimeStateV1) -> Result<(), NativeLifecycleErrorV1> {
    if state.phase == Some(LifecyclePhaseV1::Running) {
        Ok(())
    } else {
        Err(NativeLifecycleErrorV1::LifecycleStopped)
    }
}

fn ensure_admission_active(
    state: &RuntimeStateV1,
    permit: AdmissionPermitV1,
) -> Result<(), NativeLifecycleErrorV1> {
    if state.phase == Some(LifecyclePhaseV1::Admitting)
        && state.startup_generation == permit.generation
    {
        Ok(())
    } else {
        Err(NativeLifecycleErrorV1::StartupClosed)
    }
}

fn next_operation_token(state: &mut RuntimeStateV1) -> Result<u64, NativeLifecycleErrorV1> {
    let token = state
        .next_operation_token
        .checked_add(1)
        .ok_or(NativeLifecycleErrorV1::GenerationOverflow)?;
    state.next_operation_token = token;
    Ok(token)
}

fn restart_pending_for_slot(state: &RuntimeStateV1, identity: &NativeRootIdentityV1) -> bool {
    state.restart_reasons.iter().any(|(candidate, reasons)| {
        candidate.package_id == identity.package_id
            && candidate.feature == identity.feature
            && !reasons.is_empty()
    })
}

fn remaining_until(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Barrier, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };

    use explorer_extension_api::{
        IdNamespaceV1, InputStreamStatusV1, JobProviderImplementationV1, StableIdV1,
    };
    use serde_json::json;

    use super::*;

    struct FakeExecutor {
        calls: AtomicUsize,
        features: Vec<FeatureKeyV1>,
        failure: Option<NativeActivationFailureV1>,
    }
    impl NativeActivationExecutor for FakeExecutor {
        fn claim(
            &self,
            context: NativeActivationContextV1<'_>,
        ) -> Result<NativeActivationClaimV1, NativeActivationFailureV1> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.failure.map_or_else(
                || {
                    Ok(NativeActivationClaimV1::Activated(
                        NativeActivationBlueprintV1 {
                            package_id: context.package_id.to_owned(),
                            package_version: context.package_version.to_owned(),
                            sealed_manifest_digest: context.sealed_manifest_digest.to_owned(),
                            features: self.features.clone(),
                            registrations: None,
                            providers: BTreeMap::new(),
                        },
                    ))
                },
                Err,
            )
        }
    }
    #[derive(Default)]
    struct FakeDrain {
        events: Mutex<Vec<&'static str>>,
        allow: bool,
    }
    impl NativeDrainPortV1 for FakeDrain {
        fn detach(&self, _: &NativeRootIdentityV1) {
            self.events.lock().expect("events").push("detach");
        }
        fn cancel(&self, _: &NativeRootIdentityV1) {
            self.events.lock().expect("events").push("cancel");
        }
        fn wait_for_drain(&self, _: &NativeRootIdentityV1, _: Duration) -> bool {
            self.events.lock().expect("events").push("wait");
            self.allow
        }
        fn restore(&self, _: &NativeRootIdentityV1) {
            self.events.lock().expect("events").push("restore");
        }
    }

    struct BlockingDrain {
        events: Mutex<Vec<&'static str>>,
        entered: Barrier,
        release: Barrier,
        allow: bool,
    }
    impl NativeDrainPortV1 for BlockingDrain {
        fn detach(&self, _: &NativeRootIdentityV1) {
            self.events.lock().expect("events").push("detach");
        }
        fn cancel(&self, _: &NativeRootIdentityV1) {
            self.events.lock().expect("events").push("cancel");
        }
        fn wait_for_drain(&self, _: &NativeRootIdentityV1, _: Duration) -> bool {
            self.events.lock().expect("events").push("wait");
            self.entered.wait();
            self.release.wait();
            self.allow
        }
        fn restore(&self, _: &NativeRootIdentityV1) {
            self.events.lock().expect("events").push("restore");
        }
    }

    struct BlockingRestoreDrain {
        events: Mutex<Vec<&'static str>>,
        restore_entered: Barrier,
        restore_release: Barrier,
        shutdown_detached: Barrier,
        detach_count: AtomicUsize,
    }
    impl NativeDrainPortV1 for BlockingRestoreDrain {
        fn detach(&self, _: &NativeRootIdentityV1) {
            self.events.lock().expect("events").push("detach");
            if self.detach_count.fetch_add(1, Ordering::SeqCst) == 1 {
                self.shutdown_detached.wait();
            }
        }
        fn cancel(&self, _: &NativeRootIdentityV1) {
            self.events.lock().expect("events").push("cancel");
        }
        fn wait_for_drain(&self, _: &NativeRootIdentityV1, _: Duration) -> bool {
            self.events.lock().expect("events").push("wait");
            true
        }
        fn restore(&self, _: &NativeRootIdentityV1) {
            self.events.lock().expect("events").push("restore");
            self.restore_entered.wait();
            self.restore_release.wait();
        }
    }

    struct BlockingExecutor {
        entered: Barrier,
        release: Barrier,
    }
    impl NativeActivationExecutor for BlockingExecutor {
        fn claim(
            &self,
            context: NativeActivationContextV1<'_>,
        ) -> Result<NativeActivationClaimV1, NativeActivationFailureV1> {
            self.entered.wait();
            self.release.wait();
            Ok(NativeActivationClaimV1::Activated(
                NativeActivationBlueprintV1 {
                    package_id: context.package_id.to_owned(),
                    package_version: context.package_version.to_owned(),
                    sealed_manifest_digest: context.sealed_manifest_digest.to_owned(),
                    features: vec![feature()],
                    registrations: None,
                    providers: BTreeMap::new(),
                },
            ))
        }
    }

    struct DeadlineDrain {
        waits: Mutex<Vec<Duration>>,
    }
    impl NativeDrainPortV1 for DeadlineDrain {
        fn wait_for_drain(&self, _: &NativeRootIdentityV1, remaining: Duration) -> bool {
            self.waits.lock().expect("waits").push(remaining);
            true
        }
    }

    struct ReentrantDrain {
        lifecycle: Mutex<Option<&'static NativeExtensionLifecycleV1>>,
        identity: Mutex<Option<NativeRootIdentityV1>>,
        result: Mutex<Option<Result<NativeFeatureStateV1, NativeLifecycleErrorV1>>>,
    }
    impl NativeDrainPortV1 for ReentrantDrain {
        fn detach(&self, _: &NativeRootIdentityV1) {
            let lifecycle = *self.lifecycle.lock().expect("lifecycle");
            let identity = self.identity.lock().expect("identity").clone();
            if let (Some(lifecycle), Some(identity)) = (lifecycle, identity) {
                *self.result.lock().expect("result") = Some(lifecycle.disable(&identity));
            }
        }
    }
    fn feature() -> FeatureKeyV1 {
        FeatureKeyV1::new("pkg", "feature").expect("feature")
    }
    fn lifecycle(executor: Arc<FakeExecutor>, drain: Arc<FakeDrain>) -> NativeExtensionLifecycleV1 {
        NativeExtensionLifecycleV1::with_test_ports(executor, drain, Duration::from_millis(10))
    }
    fn admit(lifecycle: &mut NativeExtensionLifecycleV1) -> NativeRootIdentityV1 {
        let mut session = lifecycle.begin_startup().expect("session");
        let identities = session
            .admit_synthetic("pkg", "digest", &["native"], &["feature"])
            .expect("admit");
        session.seal().expect("seal");
        identities.into_iter().next().expect("identity")
    }

    fn test_lifecycle() -> NativeExtensionLifecycleV1 {
        lifecycle(
            Arc::new(FakeExecutor {
                calls: AtomicUsize::new(0),
                features: vec![feature()],
                failure: None,
            }),
            Arc::new(FakeDrain {
                allow: true,
                ..FakeDrain::default()
            }),
        )
    }

    struct DelayedProvider;

    impl JobProviderImplementationV1 for DelayedProvider {
        fn run(&self, _: explorer_extension_api::JobContextV1) -> JobTerminalV1 {
            thread::sleep(Duration::from_millis(2));
            JobTerminalV1::COMPLETED
        }
    }

    struct InputCapturingProvider {
        observed_status: Arc<Mutex<Option<InputStreamStatusV1>>>,
    }

    impl JobProviderImplementationV1 for InputCapturingProvider {
        fn run(&self, context: explorer_extension_api::JobContextV1) -> JobTerminalV1 {
            let status = context
                .input
                .into_option()
                .map_or(InputStreamStatusV1::CLOSED, |stream| stream.length().status);
            *self.observed_status.lock().expect("observed input status") = Some(status);
            JobTerminalV1::COMPLETED
        }
    }

    struct RegisteredInputExecutor {
        required_capabilities: Vec<String>,
        observed_status: Arc<Mutex<Option<InputStreamStatusV1>>>,
    }

    impl NativeActivationExecutor for RegisteredInputExecutor {
        fn claim(
            &self,
            context: NativeActivationContextV1<'_>,
        ) -> Result<NativeActivationClaimV1, NativeActivationFailureV1> {
            let contribution_id = "input-provider".to_owned();
            let provider = JobProviderObjectV1::new(InputCapturingProvider {
                observed_status: Arc::clone(&self.observed_status),
            });
            Ok(NativeActivationClaimV1::Activated(
                NativeActivationBlueprintV1 {
                    package_id: context.package_id.to_owned(),
                    package_version: context.package_version.to_owned(),
                    sealed_manifest_digest: context.sealed_manifest_digest.to_owned(),
                    features: vec![
                        FeatureKeyV1::new(context.package_id, "feature")
                            .map_err(|_| NativeActivationFailureV1::Rejected)?,
                    ],
                    registrations: Some(vec![ContributionRegistrationV1 {
                        feature_id: "feature".to_owned(),
                        contribution_id: contribution_id.clone(),
                        kind: ContributionKindV1::Column,
                        required_capabilities: self.required_capabilities.clone(),
                        folder_admission: None,
                        job_contract: Some(ContributionJobContractV1 {
                            interface_id: StableIdV1::new(IdNamespaceV1::new(1, 1), 1),
                            expected_sort: abi_stable::std_types::ROption::RNone,
                            opaque_schema: None,
                            renderer_contribution_id: None,
                        }),
                    }]),
                    providers: BTreeMap::from([(contribution_id, provider)]),
                },
            ))
        }
    }

    fn with_registered_input_lifecycle(
        required_capabilities: Vec<String>,
        action: impl FnOnce(
            &NativeExtensionLifecycleV1,
            &NativeRootIdentityV1,
            Arc<ExtensionJobRuntimeV1>,
            Arc<Mutex<Option<InputStreamStatusV1>>>,
        ),
    ) {
        let manifest = crate::PackageManifestV1::parse_json(
            &json!({
                "manifest_version": 1,
                "package": { "id": "pkg", "version": "1.0.0" },
                "publisher": {
                    "id": "example.publisher",
                    "display_name": "Example Publisher",
                    "contacts": [{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }]
                },
                "sdk": { "bundle_id": "dev.20260802", "target": "x86_64-pc-windows-msvc", "abi_schema": 1, "gpui": false },
                "rust": [], "lua": [], "skins": [], "locales": [], "tools": [],
                "features": [{ "id": "feature", "capabilities": ["filesystem.read"], "dependencies": [] }],
                "dependencies": [], "payloads": [], "signature": { "kind": "unsigned" }, "data_version": 1
            })
            .to_string(),
        )
        .expect("valid package manifest");
        let candidates = [crate::PackageValidationResultV1::for_resolver_test(
            manifest,
        )];
        let resolution = crate::PackageResolverV1::resolve(&candidates);
        let resolved = &resolution.resolved_packages()[0];
        let digest =
            crate::package_validation::sealed_manifest_canonical_digest(resolved.manifest())
                .expect("canonical sealed manifest digest");
        let observed_status = Arc::new(Mutex::new(None));
        let marker_directory = tempfile::tempdir().expect("marker directory");
        let markers = PluginCallGuardStoreV1::open(
            marker_directory.path().join("markers"),
            Duration::from_millis(10),
        )
        .expect("markers");
        let mut lifecycle = NativeExtensionLifecycleV1::with_ports_and_markers(
            Arc::new(RegisteredInputExecutor {
                required_capabilities,
                observed_status: Arc::clone(&observed_status),
            }),
            Arc::new(FakeDrain {
                allow: true,
                ..FakeDrain::default()
            }),
            Duration::from_millis(10),
            Some(markers),
            Arc::new(Mutex::new(Vec::new())),
            Some(Arc::new(
                RuntimeAuthorityV1::new().expect("runtime authority"),
            )),
        );
        let startup = lifecycle.begin_startup().expect("startup session");
        let permit = startup
            .lifecycle
            .reserve_admission()
            .expect("admission permit");
        let identities = startup
            .lifecycle
            .admit_entries(
                permit,
                "pkg",
                "1.0.0",
                &digest,
                &[("native", None)],
                &resolved.manifest().features,
                Some(resolved),
            )
            .expect("sealed input contribution admission");
        startup.seal().expect("seal lifecycle");
        let runtime = Arc::new(ExtensionJobRuntimeV1::new(
            crate::extension_job_runtime::ExtensionResultBufferConfigV1::try_new(
                4, 4, 8, 8, 8, 64, 64, 64, 4096, 4096, 4096,
            )
            .expect("runtime config"),
        ));
        action(
            &lifecycle,
            &identities[0],
            runtime,
            Arc::clone(&observed_status),
        );
    }

    #[test]
    fn sealed_lifecycle_route_delivers_input_only_to_filesystem_read_contributions() {
        with_registered_input_lifecycle(
            vec!["filesystem.read".to_owned()],
            |lifecycle, identity, runtime, observed_status| {
                let source =
                    HostInputStreamSourceV1::from_host_snapshot(vec![1, 2, 3], 1, true).unwrap();
                let mut prepared = lifecycle
                    .prepare_registered_provider_with_input(
                        identity,
                        "input-provider",
                        runtime,
                        1,
                        1,
                        1,
                        1,
                        true,
                        Some(source),
                    )
                    .expect("sealed filesystem.read provider accepts input");
                let terminal = prepared.call_provider().expect("provider invoked");
                assert_eq!(terminal, JobTerminalV1::COMPLETED);
                assert_eq!(
                    *observed_status.lock().expect("observed input status"),
                    Some(InputStreamStatusV1::OK)
                );
                assert!(matches!(
                    prepared.publish_terminal_after_marker_clear(terminal),
                    Ok(ExtensionJobFinishOutcomeV1::Published(_))
                ));
            },
        );

        with_registered_input_lifecycle(
            Vec::new(),
            |lifecycle, identity, runtime, observed_status| {
                let source = HostInputStreamSourceV1::from_host_snapshot(vec![1], 1, true).unwrap();
                assert!(matches!(
                    lifecycle.prepare_registered_provider_with_input(
                        identity,
                        "input-provider",
                        runtime,
                        1,
                        1,
                        1,
                        1,
                        true,
                        Some(source),
                    ),
                    Err(ExtensionJobRuntimeErrorV1::UnauthorizedInputStream)
                ));
                assert_eq!(
                    *observed_status.lock().expect("observed input status"),
                    None
                );
            },
        );
    }

    #[test]
    fn prepared_stream_revalidates_after_disable_before_callback_and_commit() {
        with_registered_input_lifecycle(
            vec!["filesystem.read".to_owned()],
            |lifecycle, identity, runtime, observed_status| {
                let source =
                    HostInputStreamSourceV1::from_host_snapshot(vec![1, 2, 3], 1, true).unwrap();
                let mut prepared = lifecycle
                    .prepare_registered_provider_with_input(
                        identity,
                        "input-provider",
                        runtime,
                        1,
                        1,
                        1,
                        1,
                        true,
                        Some(source),
                    )
                    .expect("prepared stream");
                assert!(matches!(
                    lifecycle.disable(identity),
                    Ok(NativeFeatureStateV1::PendingRestart { .. })
                ));
                assert_eq!(
                    prepared.call_provider(),
                    Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)
                );
                assert_eq!(*observed_status.lock().unwrap(), None);
            },
        );

        with_registered_input_lifecycle(
            vec!["filesystem.read".to_owned()],
            |lifecycle, identity, runtime, observed_status| {
                let source =
                    HostInputStreamSourceV1::from_host_snapshot(vec![1, 2, 3], 1, true).unwrap();
                let mut prepared = lifecycle
                    .prepare_registered_provider_with_input(
                        identity,
                        "input-provider",
                        runtime,
                        1,
                        1,
                        1,
                        1,
                        true,
                        Some(source),
                    )
                    .expect("prepared stream");
                let terminal = prepared.call_provider().expect("callback terminal");
                assert_eq!(
                    *observed_status.lock().unwrap(),
                    Some(InputStreamStatusV1::OK)
                );
                assert!(matches!(
                    lifecycle.disable(identity),
                    Ok(NativeFeatureStateV1::PendingRestart { .. })
                ));
                assert_eq!(
                    prepared.publish_terminal_after_marker_clear(terminal),
                    Err(ExtensionJobRuntimeErrorV1::UnauthorizedAuthority)
                );
            },
        );
    }

    fn prepared_provider_for_timing(
        state: &std::path::Path,
        threshold: Duration,
    ) -> (PreparedNativeJobV1, Arc<PluginCallGuardStoreV1>) {
        let markers =
            PluginCallGuardStoreV1::open(state.to_path_buf(), threshold).expect("markers");
        let runtime = Arc::new(ExtensionJobRuntimeV1::new(
            crate::extension_job_runtime::ExtensionResultBufferConfigV1::try_new(
                1, 1, 1, 1, 1, 1, 1, 1, 1024, 1024, 1024,
            )
            .expect("runtime config"),
        ));
        let ticket = runtime
            .prepare_provider_dispatch(ExtensionJobRuntimeRequestV1 {
                authority: ExtensionJobAuthorityV1::for_test("provider-package"),
                job_generation: 1,
                item_generation: 1,
                location_generation: 1,
                source_generation: 1,
                has_item: true,
                input_stream: None,
            })
            .expect("dispatch ticket");
        let marker = plugin_call_guard::marker_with_operation(
            "provider-package",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "provider-contribution",
            "provider",
            0x0001_0001,
            1,
            NativeCallOperationV1::JobProvider,
        );
        (
            PreparedNativeJobV1 {
                runtime,
                ticket,
                provider: Arc::new(JobProviderObjectV1::new(DelayedProvider)),
                markers: Arc::clone(&markers),
                marker,
                permit: None,
                callback_started: false,
                callback_elapsed: None,
                runtime_authority: None,
                stream_authority: None,
            },
            markers,
        )
    }

    #[test]
    fn provider_timing_measures_only_the_callback_not_publish_delay() {
        const POST_CALLBACK_DELAY: Duration = Duration::from_millis(40);
        let directory = tempfile::tempdir().expect("state directory");
        let (mut prepared, markers) =
            prepared_provider_for_timing(directory.path(), Duration::ZERO);
        let terminal = prepared.call_provider().expect("provider terminal");
        let callback_elapsed = prepared.callback_elapsed.expect("callback elapsed");
        thread::sleep(POST_CALLBACK_DELAY);
        assert!(matches!(
            prepared.publish_terminal_after_marker_clear(terminal),
            Ok(ExtensionJobFinishOutcomeV1::Published(_))
        ));

        let timing = markers.timings().pop().expect("provider timing");
        assert_eq!(timing.package_id, "provider-package");
        assert_eq!(timing.callback_id, "provider-contribution");
        assert_eq!(timing.primary_interface_namespace, 0x0001_0001);
        assert_eq!(timing.primary_interface_value, 1);
        assert_eq!(timing.operation, NativeCallOperationV1::JobProvider);
        assert_eq!(timing.terminal, NativeCallTerminalV1::Accepted);
        assert!(
            timing.slow,
            "zero threshold classifies the delayed provider as slow"
        );
        assert_eq!(timing.elapsed, callback_elapsed);
    }

    #[test]
    fn provider_dispatch_failure_records_a_bounded_timing_terminal() {
        let directory = tempfile::tempdir().expect("state directory");
        let (mut prepared, markers) =
            prepared_provider_for_timing(directory.path(), Duration::ZERO);
        prepared
            .request_control(explorer_extension_api::JobControlStateV1::CANCELLED)
            .expect("cancel before callback");
        assert_eq!(
            prepared.call_provider(),
            Err(ExtensionJobRuntimeErrorV1::InactiveProviderCall)
        );
        let timing = markers.timings().pop().expect("failure timing");
        assert_eq!(timing.operation, NativeCallOperationV1::JobProvider);
        assert_eq!(timing.terminal, NativeCallTerminalV1::Incompatible);
        assert!(timing.slow);
    }

    #[test]
    fn provider_terminal_diagnostics_preserve_error_classes() {
        let cases = [
            (JobTerminalV1::COMPLETED, NativeCallTerminalV1::Accepted),
            (JobTerminalV1::CANCELLED, NativeCallTerminalV1::Accepted),
            (
                JobTerminalV1::PLUGIN_ERROR,
                NativeCallTerminalV1::PluginError,
            ),
            (
                JobTerminalV1::INCOMPATIBLE,
                NativeCallTerminalV1::Incompatible,
            ),
            (JobTerminalV1::PANICKED, NativeCallTerminalV1::Panicked),
        ];
        for (terminal, expected) in cases {
            assert_eq!(timing_terminal_for_job_terminal(terminal), expected);
        }
    }

    #[test]
    fn counters_fail_closed_at_u64_max_without_reusing_values() {
        let mut startup = test_lifecycle();
        {
            let mut state = startup.shared.state.lock().expect("state");
            state.startup_generation = u64::MAX;
        }
        assert!(matches!(
            startup.begin_startup(),
            Err(NativeLifecycleErrorV1::GenerationOverflow)
        ));
        let state = startup.shared.state.lock().expect("state");
        assert_eq!(state.startup_generation, u64::MAX);
        assert_eq!(state.phase, Some(LifecyclePhaseV1::New));
        drop(state);

        let mut token = test_lifecycle();
        let identity = admit(&mut token);
        token
            .shared
            .state
            .lock()
            .expect("state")
            .next_operation_token = u64::MAX;
        assert!(matches!(
            token.disable(&identity),
            Err(NativeLifecycleErrorV1::GenerationOverflow)
        ));
        let state = token.shared.state.lock().expect("state");
        assert_eq!(state.next_operation_token, u64::MAX);
        assert!(matches!(
            state.gates[&identity].state,
            NativeFeatureStateV1::Enabled { .. }
        ));
        drop(state);

        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut epoch = lifecycle(
            Arc::new(FakeExecutor {
                calls: AtomicUsize::new(0),
                features: vec![feature()],
                failure: None,
            }),
            Arc::clone(&drain),
        );
        let identity = admit(&mut epoch);
        assert!(matches!(
            epoch.disable(&identity),
            Ok(NativeFeatureStateV1::DisabledResident)
        ));
        drain.events.lock().expect("events").clear();
        epoch
            .shared
            .state
            .lock()
            .expect("state")
            .gates
            .get_mut(&identity)
            .expect("gate")
            .epoch = u64::MAX;
        assert!(matches!(
            epoch.enable(&identity),
            Err(NativeLifecycleErrorV1::GenerationOverflow)
        ));
        let state = epoch.shared.state.lock().expect("state");
        let gate = &state.gates[&identity];
        assert_eq!(gate.epoch, u64::MAX);
        assert!(!gate.accepting);
        assert_eq!(gate.state, NativeFeatureStateV1::DisabledResident);
        drop(state);
        assert_eq!(
            *drain.events.lock().expect("events"),
            vec!["restore", "detach", "cancel"]
        );
    }

    #[test]
    fn shutdown_generation_exhaustion_is_one_way_and_stays_closed() {
        let mut lifecycle = test_lifecycle();
        let _ = admit(&mut lifecycle);
        lifecycle
            .shared
            .state
            .lock()
            .expect("state")
            .shutdown_generation = u64::MAX;
        lifecycle.shutdown();
        let state = lifecycle.shared.state.lock().expect("state");
        assert_eq!(state.shutdown_generation, u64::MAX);
        assert_eq!(state.phase, Some(LifecyclePhaseV1::Stopped));
        drop(state);
        lifecycle.shutdown();
        assert_eq!(
            lifecycle
                .shared
                .state
                .lock()
                .expect("state")
                .shutdown_generation,
            u64::MAX
        );
    }

    #[test]
    fn process_owner_is_nonrenewable() {
        let state = tempfile::tempdir().expect("state");
        let config = NativeLifecycleConfigV1::new(state.path().to_path_buf())
            .with_slow_callback_threshold(Duration::ZERO);
        let lifecycle = NativeExtensionLifecycleV1::acquire(config.clone()).expect("first owner");
        assert!(matches!(
            NativeExtensionLifecycleV1::acquire(config),
            Err(NativeLifecycleErrorV1::AlreadyAcquired)
        ));
        assert!(lifecycle.safe_mode_incidents().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn acquire_rejects_a_reparse_application_state_directory_before_ownership() {
        let state = tempfile::tempdir().expect("state");
        let target = tempfile::tempdir().expect("target");
        let reparse_state = state.path().join("reparse-state");
        let output = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&reparse_state)
            .arg(target.path())
            .output()
            .expect("junction command");
        assert!(output.status.success(), "junction creation failed");

        assert!(matches!(
            NativeExtensionLifecycleV1::acquire(NativeLifecycleConfigV1::new(reparse_state)),
            Err(NativeLifecycleErrorV1::MarkerStateUnavailable)
        ));
    }

    #[test]
    fn roots_sharing_a_feature_share_one_gate_and_keep_membership() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(executor, drain);
        let mut session = lifecycle.begin_startup().expect("session");
        let identities = session
            .admit_synthetic("pkg", "digest", &["one", "two"], &["feature"])
            .expect("admit");
        session.seal().expect("seal");
        let state = lifecycle.lock().expect("state");
        assert_eq!(state.gates.len(), 1);
        assert_eq!(
            state.gates.get(&identities[0]).expect("gate").members.len(),
            2
        );
    }

    #[test]
    fn session_drop_closes_startup_and_gates_fail_closed() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(executor, drain);
        let mut session = lifecycle.begin_startup().expect("session");
        let identity = session
            .admit_synthetic("pkg", "digest", &["native"], &["feature"])
            .expect("admit")
            .remove(0);
        drop(session);
        assert!(lifecycle.try_enter(&identity).expect("enter").is_none());
        assert!(matches!(
            lifecycle.begin_startup(),
            Err(NativeLifecycleErrorV1::StartupClosed)
        ));
    }

    #[test]
    fn concurrent_token_claims_execute_once_and_failed_entries_never_retry() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(Arc::clone(&executor), drain);
        let mut session = lifecycle.begin_startup().expect("session");
        assert!(
            session
                .admit_synthetic("pkg", "digest", &["native"], &["feature"])
                .is_ok()
        );
        assert!(matches!(
            session.admit_synthetic("pkg", "digest", &["native"], &["feature"]),
            Err(NativeLifecycleErrorV1::AlreadyClaimed)
        ));
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        session.seal().expect("seal");
    }

    #[test]
    fn gate_close_precedes_ordered_drain_and_timeout_is_sticky() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: false,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(executor, Arc::clone(&drain));
        let identity = admit(&mut lifecycle);
        let lease = lifecycle
            .try_enter(&identity)
            .expect("enter")
            .expect("lease");
        let result = lifecycle.disable(&identity).expect("disable");
        assert_eq!(
            result,
            NativeFeatureStateV1::PendingRestart {
                primary_reason: NativeRestartReasonV1::DrainTimedOut
            }
        );
        drop(lease);
        assert_eq!(
            lifecycle.runtime_fact(&identity).expect("fact"),
            FeatureRuntimeFactV1::PendingRestart
        );
        assert_eq!(
            *drain.events.lock().expect("events"),
            vec!["detach", "cancel", "wait"]
        );
        assert!(matches!(
            lifecycle.enable(&identity),
            Err(NativeLifecycleErrorV1::RestartRequired { .. })
        ));
    }

    #[test]
    fn successful_drain_is_resident_idempotent_and_reenable_advances_epoch() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(Arc::clone(&executor), Arc::clone(&drain));
        let identity = admit(&mut lifecycle);
        assert_eq!(
            lifecycle.disable(&identity).expect("disable"),
            NativeFeatureStateV1::DisabledResident
        );
        assert_eq!(
            lifecycle.disable(&identity).expect("idempotent"),
            NativeFeatureStateV1::DisabledResident
        );
        assert_eq!(
            lifecycle.enable(&identity).expect("enable"),
            NativeFeatureStateV1::Enabled { epoch: 2 }
        );
        let lease = lifecycle
            .try_enter(&identity)
            .expect("enter")
            .expect("lease");
        assert_eq!(lease.epoch(), 2);
        drop(lease);
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            *drain.events.lock().expect("events"),
            vec!["detach", "cancel", "wait", "restore"]
        );
    }

    #[test]
    fn disable_close_wins_against_a_new_dispatch_lease() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(executor, Arc::clone(&drain));
        let identity = admit(&mut lifecycle);
        let lease = lifecycle
            .try_enter(&identity)
            .expect("enter")
            .expect("lease");
        thread::scope(|scope| {
            let disable = scope.spawn(|| lifecycle.disable(&identity));
            while drain.events.lock().expect("events").is_empty() {
                thread::yield_now();
            }
            assert!(lifecycle.try_enter(&identity).expect("enter").is_none());
            drop(lease);
            assert_eq!(
                disable.join().expect("join").expect("disable"),
                NativeFeatureStateV1::DisabledResident
            );
        });
    }

    #[test]
    fn restart_request_never_loads_and_projects_pending_restart() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(Arc::clone(&executor), drain);
        let _current = admit(&mut lifecycle);
        let calls_before = executor.calls.load(Ordering::SeqCst);
        let identity = NativeRootIdentityV1 {
            package_id: "pkg".into(),
            sealed_manifest_digest: "new".into(),
            feature: feature(),
        };
        lifecycle
            .require_restart(&identity, NativeRestartReasonV1::Update)
            .expect("restart");
        assert_eq!(
            lifecycle.runtime_fact(&identity).expect("fact"),
            FeatureRuntimeFactV1::PendingRestart
        );
        assert_eq!(executor.calls.load(Ordering::SeqCst), calls_before);
    }

    #[test]
    fn failed_claim_is_rejected_once_and_never_retried() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: Some(NativeActivationFailureV1::Rejected),
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(Arc::clone(&executor), drain);
        let mut session = lifecycle.begin_startup().expect("session");
        assert!(matches!(
            session.admit_synthetic("pkg", "digest", &["native"], &["feature"]),
            Err(NativeLifecycleErrorV1::ActivationRejected)
        ));
        assert!(matches!(
            session.admit_synthetic("pkg", "digest", &["native"], &["feature"]),
            Err(NativeLifecycleErrorV1::AlreadyClaimed)
        ));
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn second_root_failure_rolls_back_the_entire_package_generation_transaction() {
        struct FailsSecondRoot {
            calls: AtomicUsize,
        }
        impl NativeActivationExecutor for FailsSecondRoot {
            fn claim(
                &self,
                context: NativeActivationContextV1<'_>,
            ) -> Result<NativeActivationClaimV1, NativeActivationFailureV1> {
                if self.calls.fetch_add(1, Ordering::SeqCst) == 1 {
                    return Err(NativeActivationFailureV1::Rejected);
                }
                Ok(NativeActivationClaimV1::Activated(
                    NativeActivationBlueprintV1 {
                        package_id: context.package_id.to_owned(),
                        package_version: context.package_version.to_owned(),
                        sealed_manifest_digest: context.sealed_manifest_digest.to_owned(),
                        features: vec![feature()],
                        registrations: None,
                        providers: BTreeMap::new(),
                    },
                ))
            }
        }
        let executor: Arc<dyn NativeActivationExecutor> = Arc::new(FailsSecondRoot {
            calls: AtomicUsize::new(0),
        });
        let drain: Arc<dyn NativeDrainPortV1> = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle =
            NativeExtensionLifecycleV1::with_test_ports(executor, drain, Duration::from_millis(10));
        let mut session = lifecycle.begin_startup().expect("session");
        assert!(matches!(
            session.admit_synthetic("pkg", "digest", &["one", "two"], &["feature"]),
            Err(NativeLifecycleErrorV1::ActivationRejected)
        ));
        drop(session);
        let state = lifecycle.lock().expect("state");
        assert!(state.entries.is_empty());
        assert!(state.validated_roots.is_empty());
        assert!(state.sealed_contributions.is_empty());
        assert!(state.providers.is_empty());
        assert!(state.gates.is_empty());
    }

    #[test]
    fn restart_reasons_are_orthogonal_and_timeout_cannot_overwrite_them() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: false,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(executor, drain);
        let identity = admit(&mut lifecycle);
        assert!(matches!(
            lifecycle.disable(&identity),
            Ok(NativeFeatureStateV1::PendingRestart { .. })
        ));
        lifecycle
            .require_restart(&identity, NativeRestartReasonV1::Update)
            .expect("update restart");
        assert_eq!(
            lifecycle.restart_reasons(&identity).expect("reasons"),
            vec![
                NativeRestartReasonV1::Update,
                NativeRestartReasonV1::DrainTimedOut,
            ]
        );
    }

    #[test]
    fn remove_after_drain_timeout_preserves_both_restart_reasons() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: false,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(executor, drain);
        let identity = admit(&mut lifecycle);
        assert!(matches!(
            lifecycle.disable(&identity),
            Ok(NativeFeatureStateV1::PendingRestart {
                primary_reason: NativeRestartReasonV1::DrainTimedOut
            })
        ));
        lifecycle
            .require_restart(&identity, NativeRestartReasonV1::Remove)
            .expect("remove");
        assert_eq!(
            lifecycle.restart_reasons(&identity).expect("reasons"),
            vec![
                NativeRestartReasonV1::Remove,
                NativeRestartReasonV1::DrainTimedOut,
            ]
        );
    }

    #[test]
    fn timeout_merges_a_restart_fact_added_while_drain_is_in_progress() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(BlockingDrain {
            events: Mutex::new(Vec::new()),
            entered: Barrier::new(2),
            release: Barrier::new(2),
            allow: false,
        });
        let drain_port: Arc<dyn NativeDrainPortV1> = drain.clone();
        let mut lifecycle = NativeExtensionLifecycleV1::with_test_ports(
            executor,
            drain_port,
            Duration::from_millis(50),
        );
        let identity = admit(&mut lifecycle);
        thread::scope(|scope| {
            let disable = scope.spawn(|| lifecycle.disable(&identity));
            drain.entered.wait();
            {
                let mut state = lifecycle.lock().expect("state");
                insert_restart_reason(&mut state, &identity, NativeRestartReasonV1::Update);
            }
            drain.release.wait();
            assert!(matches!(
                disable.join().expect("join"),
                Ok(NativeFeatureStateV1::PendingRestart { .. })
            ));
        });
        assert_eq!(
            lifecycle.restart_reasons(&identity).expect("reasons"),
            vec![
                NativeRestartReasonV1::Update,
                NativeRestartReasonV1::DrainTimedOut,
            ]
        );
    }

    #[test]
    fn update_and_replace_keep_the_loaded_generation_dispatchable() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(executor, drain);
        let current = admit(&mut lifecycle);
        let target = NativeFeatureIdentityV1 {
            package_id: "pkg".into(),
            sealed_manifest_digest: "next".into(),
            feature: feature(),
        };
        lifecycle
            .require_restart(&target, NativeRestartReasonV1::Update)
            .expect("update");
        lifecycle
            .require_restart(&target, NativeRestartReasonV1::Replace)
            .expect("replace");
        let lease = lifecycle
            .try_enter(&current)
            .expect("enter")
            .expect("current generation remains dispatchable");
        drop(lease);
        assert_eq!(
            lifecycle.runtime_fact(&current).expect("current fact"),
            FeatureRuntimeFactV1::PendingRestart
        );
        assert_eq!(
            lifecycle.runtime_fact(&target).expect("target fact"),
            FeatureRuntimeFactV1::PendingRestart
        );
    }

    #[test]
    fn remove_drains_before_recording_its_restart_fact() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(executor, Arc::clone(&drain));
        let identity = admit(&mut lifecycle);
        lifecycle
            .require_restart(&identity, NativeRestartReasonV1::Remove)
            .expect("remove");
        assert!(lifecycle.try_enter(&identity).expect("enter").is_none());
        assert_eq!(
            lifecycle.restart_reasons(&identity).expect("reasons"),
            vec![NativeRestartReasonV1::Remove]
        );
        assert_eq!(
            *drain.events.lock().expect("events"),
            vec!["detach", "cancel", "wait"]
        );
    }

    #[test]
    fn stale_restore_is_compensated_and_does_not_delay_shutdown() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(BlockingRestoreDrain {
            events: Mutex::new(Vec::new()),
            restore_entered: Barrier::new(2),
            restore_release: Barrier::new(2),
            shutdown_detached: Barrier::new(2),
            detach_count: AtomicUsize::new(0),
        });
        let drain_port: Arc<dyn NativeDrainPortV1> = drain.clone();
        let mut lifecycle = NativeExtensionLifecycleV1::with_test_ports(
            executor,
            drain_port,
            Duration::from_millis(50),
        );
        let identity = admit(&mut lifecycle);
        assert_eq!(
            lifecycle.disable(&identity).expect("disable"),
            NativeFeatureStateV1::DisabledResident
        );
        thread::scope(|scope| {
            let lifecycle_ref: &NativeExtensionLifecycleV1 = &lifecycle;
            let identity_ref = &identity;
            let enable = scope.spawn(move || lifecycle_ref.enable(identity_ref));
            drain.restore_entered.wait();
            let (shutdown_done, shutdown_complete) = std::sync::mpsc::channel();
            let shutdown_lifecycle = lifecycle_ref;
            let shutdown = scope.spawn(move || {
                shutdown_lifecycle.shutdown();
                shutdown_done.send(()).expect("signal shutdown");
            });
            // The second detach is shutdown's hook: its entry occurs after the
            // terminal phase/token invalidation, before restore may return.
            drain.shutdown_detached.wait();
            assert!(
                shutdown_complete
                    .recv_timeout(Duration::from_millis(100))
                    .is_ok()
            );
            drain.restore_release.wait();
            assert!(matches!(
                enable.join().expect("join"),
                Err(NativeLifecycleErrorV1::OperationSuperseded)
            ));
            shutdown.join().expect("join");
        });
        assert!(lifecycle.try_enter(&identity).expect("enter").is_none());
        assert_eq!(
            *drain.events.lock().expect("events"),
            vec![
                "detach", "cancel", "wait", "restore", "detach", "cancel", "wait", "detach",
                "cancel"
            ]
        );
    }

    #[test]
    fn mutating_calls_fail_after_shutdown() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(executor, drain);
        let identity = admit(&mut lifecycle);
        lifecycle.shutdown();
        assert!(matches!(
            lifecycle.disable(&identity),
            Err(NativeLifecycleErrorV1::LifecycleStopped)
        ));
        assert!(matches!(
            lifecycle.enable(&identity),
            Err(NativeLifecycleErrorV1::LifecycleStopped)
        ));
        assert!(matches!(
            lifecycle.require_restart(&identity, NativeRestartReasonV1::Update),
            Err(NativeLifecycleErrorV1::LifecycleStopped)
        ));
    }

    #[test]
    fn admission_guard_rejects_before_any_loader_or_executor_work() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let lifecycle = lifecycle(Arc::clone(&executor), drain);
        assert!(matches!(
            lifecycle.reserve_admission(),
            Err(NativeLifecycleErrorV1::StartupClosed)
        ));
        assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn safe_mode_denial_is_not_reclassified_as_a_plugin_rejection() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: Some(NativeActivationFailureV1::SafeModeDenied),
        });
        let drain = Arc::new(FakeDrain {
            allow: true,
            ..FakeDrain::default()
        });
        let mut lifecycle = lifecycle(Arc::clone(&executor), drain);
        let mut session = lifecycle.begin_startup().expect("session");
        assert!(matches!(
            session.admit_synthetic("pkg", "digest", &["native"], &["feature"]),
            Err(NativeLifecycleErrorV1::SafeModeDenied)
        ));
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn blocked_executor_cannot_commit_after_admission_permit_is_invalidated() {
        let executor = Arc::new(BlockingExecutor {
            entered: Barrier::new(2),
            release: Barrier::new(2),
        });
        let drain: Arc<dyn NativeDrainPortV1> = Arc::new(NoopDrainPortV1);
        let mut lifecycle = NativeExtensionLifecycleV1::with_test_ports(
            executor.clone(),
            drain,
            Duration::from_millis(10),
        );
        let shared = Arc::clone(&lifecycle.shared);
        let mut session = lifecycle.begin_startup().expect("session");
        thread::scope(|scope| {
            let admit =
                scope.spawn(|| session.admit_synthetic("pkg", "digest", &["native"], &["feature"]));
            executor.entered.wait();
            {
                let mut state = shared.state.lock().expect("state");
                state.phase = Some(LifecyclePhaseV1::Closed);
                state.startup_generation = state.startup_generation.saturating_add(1);
            }
            executor.release.wait();
            assert!(matches!(
                admit.join().expect("join"),
                Err(NativeLifecycleErrorV1::StartupClosed)
            ));
        });
        assert!(shared.state.lock().expect("state").gates.is_empty());
    }

    #[test]
    fn reentrant_disable_hook_returns_in_progress_without_deadlock() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let port = Arc::new(ReentrantDrain {
            lifecycle: Mutex::new(None),
            identity: Mutex::new(None),
            result: Mutex::new(None),
        });
        let port_trait: Arc<dyn NativeDrainPortV1> = port.clone();
        let lifecycle = Box::leak(Box::new(NativeExtensionLifecycleV1::with_test_ports(
            executor,
            port_trait,
            Duration::from_millis(10),
        )));
        let identity = admit(lifecycle);
        *port.lifecycle.lock().expect("lifecycle") = Some(&*lifecycle);
        *port.identity.lock().expect("identity") = Some(identity.clone());
        assert_eq!(
            lifecycle.disable(&identity).expect("disable"),
            NativeFeatureStateV1::DisabledResident
        );
        assert!(matches!(
            port.result.lock().expect("result").take(),
            Some(Err(NativeLifecycleErrorV1::OperationInProgress))
        ));
    }

    #[test]
    fn competing_disable_and_remove_return_in_progress_while_one_gate_drains() {
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature()],
            failure: None,
        });
        let drain = Arc::new(BlockingDrain {
            events: Mutex::new(Vec::new()),
            entered: Barrier::new(2),
            release: Barrier::new(2),
            allow: true,
        });
        let drain_port: Arc<dyn NativeDrainPortV1> = drain.clone();
        let mut lifecycle = NativeExtensionLifecycleV1::with_test_ports(
            executor,
            drain_port,
            Duration::from_millis(50),
        );
        let identity = admit(&mut lifecycle);
        thread::scope(|scope| {
            let disable = scope.spawn(|| lifecycle.disable(&identity));
            drain.entered.wait();
            assert!(matches!(
                lifecycle.disable(&identity),
                Err(NativeLifecycleErrorV1::OperationInProgress)
            ));
            assert!(matches!(
                lifecycle.require_restart(&identity, NativeRestartReasonV1::Remove),
                Err(NativeLifecycleErrorV1::OperationInProgress)
            ));
            drain.release.wait();
            assert_eq!(
                disable.join().expect("join").expect("disable"),
                NativeFeatureStateV1::DisabledResident
            );
        });
    }

    #[test]
    fn shutdown_uses_one_deadline_for_multiple_gates() {
        let feature_two = FeatureKeyV1::new("pkg", "second").expect("feature");
        let executor = Arc::new(FakeExecutor {
            calls: AtomicUsize::new(0),
            features: vec![feature(), feature_two],
            failure: None,
        });
        let drain = Arc::new(DeadlineDrain {
            waits: Mutex::new(Vec::new()),
        });
        let drain_port: Arc<dyn NativeDrainPortV1> = drain.clone();
        let timeout = Duration::from_millis(50);
        let mut lifecycle =
            NativeExtensionLifecycleV1::with_test_ports(executor, drain_port, timeout);
        let mut session = lifecycle.begin_startup().expect("session");
        session
            .admit_synthetic("pkg", "digest", &["native"], &["feature", "second"])
            .expect("admit");
        session.seal().expect("seal");
        lifecycle.shutdown();
        let waits = drain.waits.lock().expect("waits");
        assert_eq!(waits.len(), 2);
        assert!(waits[0] <= timeout);
        assert!(waits[1] <= waits[0]);
    }

    #[test]
    fn loader_errors_map_to_stable_sanitized_diagnostics() {
        use crate::dll_loader::ExtensionDllLoadErrorV1 as LoaderError;

        assert_eq!(
            NativeLoaderDiagnosticCodeV1::from_loader(&LoaderError::MissingBinaryUiFingerprint {
                entrypoint_id: "private-path".into(),
            }),
            NativeLoaderDiagnosticCodeV1::MissingBinaryUiFingerprint
        );
        assert_eq!(
            NativeLoaderDiagnosticCodeV1::from_loader(&LoaderError::GpuiFingerprintMismatch {
                host_bundle_id: "host".into(),
                host_fingerprint: "host-private".into(),
                plugin_bundle_id: "plugin".into(),
                plugin_fingerprint: "plugin-private".into(),
            }),
            NativeLoaderDiagnosticCodeV1::GpuiFingerprintMismatch
        );
        assert_eq!(
            NativeLoaderDiagnosticCodeV1::from_loader(&LoaderError::UnsupportedPlatform),
            NativeLoaderDiagnosticCodeV1::UnsupportedPlatform
        );
        assert_eq!(
            NativeLoaderDiagnosticCodeV1::from_loader(&LoaderError::ResidentStatePoisoned),
            NativeLoaderDiagnosticCodeV1::ResidentState
        );
    }
}
