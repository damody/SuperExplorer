//! Startup-only ownership, runtime gates, and bounded draining for native DLLs.
//!
//! The private guarded executor invokes the Rust ABI registrar only after
//! lifecycle admission and durable Safe Mode marker creation.
#![allow(clippy::missing_errors_doc)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, OnceLock},
    time::{Duration, Instant},
};

use thiserror::Error;

use crate::{
    FeatureKeyV1, FeatureRuntimeFactV1, HostRegistrationErrorV1, ResolvedPackageV1,
    dll_loader::{
        ExtensionDllLoaderV1, LoadedExtensionRootV1, LoadedPackageRootsV1, invoke_guarded_registrar,
    },
    plugin_call_guard::{
        self, GuardErrorV1, NativeCallTerminalV1, NativeCallTimingV1, NativeSafeModeIncidentV1,
        PluginCallGuardStoreV1,
    },
};

/// Resolver candidates (128) times Rust entrypoints per manifest (128).
pub const MAX_NATIVE_LEDGER_ENTRIES_V1: usize = 128 * 128;
/// Resolver candidates (128) times manifest features (128); roots share gates.
pub const MAX_NATIVE_FEATURE_GATES_V1: usize = MAX_NATIVE_LEDGER_ENTRIES_V1;
pub const MAX_NATIVE_RESTART_REASONS_PER_FEATURE_V1: usize = 8;
const DEFAULT_NATIVE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Explicit application-owned state required for production native activation.
#[derive(Clone)]
pub struct NativeLifecycleConfigV1 {
    application_state_dir: PathBuf,
    slow_callback_threshold: Duration,
}

impl NativeLifecycleConfigV1 {
    /// Uses a dedicated marker directory below the application-owned state root.
    #[must_use]
    pub fn new(application_state_dir: PathBuf) -> Self {
        Self {
            application_state_dir,
            slow_callback_threshold: Duration::from_millis(250),
        }
    }

    /// Sets the path-free callback timing slow threshold.
    #[must_use]
    pub const fn with_slow_callback_threshold(mut self, threshold: Duration) -> Self {
        self.slow_callback_threshold = threshold;
        self
    }
}

impl fmt::Debug for NativeLifecycleConfigV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeLifecycleConfigV1")
            .field("application_state_dir", &"<redacted>")
            .field("slow_callback_threshold", &self.slow_callback_threshold)
            .finish()
    }
}

/// Feature-scoped runtime authority across all roots in one sealed generation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NativeFeatureIdentityV1 {
    package_id: String,
    sealed_manifest_digest: String,
    feature: FeatureKeyV1,
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
            | LoaderError::DuplicateRustRootModule { .. }
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
    Faulted,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "task 3.5 executor will produce both typed failures"
)]
pub(crate) enum NativeActivationFailureV1 {
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
}

struct GuardedNativeActivationExecutorV1 {
    markers: Arc<PluginCallGuardStoreV1>,
}

impl GuardedNativeActivationExecutorV1 {
    fn new(markers: Arc<PluginCallGuardStoreV1>) -> Self {
        Self { markers }
    }
}

impl NativeActivationExecutor for GuardedNativeActivationExecutorV1 {
    fn claim(
        &self,
        context: NativeActivationContextV1<'_>,
    ) -> Result<NativeActivationClaimV1, NativeActivationFailureV1> {
        let Some(root) = context.root else {
            return Err(NativeActivationFailureV1::Faulted);
        };
        let metadata = root.metadata();
        let marker = plugin_call_guard::marker(
            context.package_id,
            context.sealed_manifest_digest,
            context.entrypoint_id,
            root.root_module(),
            metadata.primary_interface_id.namespace.into_raw(),
            metadata.primary_interface_id.value,
        );
        let permit = match self.markers.begin(&marker) {
            Ok(permit) => permit,
            Err(GuardErrorV1::Denied) => {
                self.markers.record_timing(
                    &marker,
                    Duration::ZERO,
                    NativeCallTerminalV1::SafeModeDenied,
                );
                return Err(NativeActivationFailureV1::Rejected);
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
        let result = invoke_guarded_registrar(root, &permit);
        let elapsed = started.elapsed();
        let terminal = match &result {
            Ok(_) => NativeCallTerminalV1::Accepted,
            Err(HostRegistrationErrorV1::Incompatible(_)) => NativeCallTerminalV1::Incompatible,
            Err(HostRegistrationErrorV1::Plugin(_)) => NativeCallTerminalV1::PluginError,
            Err(HostRegistrationErrorV1::Panicked(_)) => NativeCallTerminalV1::Panicked,
        };
        if permit.clear().is_err() {
            self.markers
                .record_timing(&marker, elapsed, NativeCallTerminalV1::MarkerFailure);
            return Err(NativeActivationFailureV1::Faulted);
        }
        self.markers.record_timing(&marker, elapsed, terminal);
        match result {
            Ok(_) => Ok(NativeActivationClaimV1::Activated(
                NativeActivationBlueprintV1 {
                    package_id: context.package_id.to_owned(),
                    package_version: context.package_version.to_owned(),
                    sealed_manifest_digest: context.sealed_manifest_digest.to_owned(),
                    features: Vec::new(),
                },
            )),
            Err(HostRegistrationErrorV1::Panicked(_)) => Err(NativeActivationFailureV1::Faulted),
            Err(_) => Err(NativeActivationFailureV1::Rejected),
        }
    }
}

struct NoopDrainPortV1;
impl NativeDrainPortV1 for NoopDrainPortV1 {}

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
        *acquired = true;
        Ok(Self::with_ports_and_markers(
            Arc::new(GuardedNativeActivationExecutorV1::new(Arc::clone(&markers))),
            Arc::new(NoopDrainPortV1),
            DEFAULT_NATIVE_DRAIN_TIMEOUT,
            Some(markers),
        ))
    }

    #[cfg(test)]
    fn with_ports(
        executor: Arc<dyn NativeActivationExecutor>,
        drain_port: Arc<dyn NativeDrainPortV1>,
        drain_timeout: Duration,
    ) -> Self {
        Self::with_ports_and_markers(executor, drain_port, drain_timeout, None)
    }

    fn with_ports_and_markers(
        executor: Arc<dyn NativeActivationExecutor>,
        drain_port: Arc<dyn NativeDrainPortV1>,
        drain_timeout: Duration,
        markers: Option<Arc<PluginCallGuardStoreV1>>,
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
        }
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

    /// Returns bounded path-free native registrar timing diagnostics.
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
        state.phase = Some(LifecyclePhaseV1::Admitting);
        state.startup_generation = state.startup_generation.saturating_add(1);
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

    /// Closes a feature gate, performs ordered drain hooks, then waits boundedly.
    pub fn disable(
        &self,
        identity: &NativeRootIdentityV1,
    ) -> Result<NativeFeatureStateV1, NativeLifecycleErrorV1> {
        let deadline = Instant::now() + self.drain_timeout;
        let token = {
            let mut state = self.lock()?;
            ensure_running(&state)?;
            let token = next_operation_token(&mut state);
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
            let token = next_operation_token(&mut state);
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
        let epoch = gate.epoch.saturating_add(1);
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
            let token = next_operation_token(&mut state);
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

    /// Closes every gate, invokes bounded cancellation/drain, and never unloads.
    pub fn shutdown(&self) {
        let deadline = Instant::now() + self.drain_timeout;
        let identities = match self.shared.state.lock() {
            Ok(mut state) => {
                if state.phase == Some(LifecyclePhaseV1::Stopped) {
                    return;
                }
                state.phase = Some(LifecyclePhaseV1::Stopped);
                state.shutdown_generation = state.shutdown_generation.saturating_add(1);
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
        {
            let mut state = self.lock()?;
            ensure_admission_active(&state, permit)?;
            if state.entries.len().saturating_add(entry_keys.len()) > MAX_NATIVE_LEDGER_ENTRIES_V1 {
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

        let mut activated = Vec::new();
        let mut staged_gates = Vec::new();
        for (key, (_, root)) in entry_keys.into_iter().zip(roots) {
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
                    let mut state = self.lock()?;
                    ensure_admission_active(&state, permit)?;
                    *state
                        .entries
                        .get_mut(&key)
                        .ok_or(NativeLifecycleErrorV1::AlreadyClaimed)? = EntryStateV1::Deferred;
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
                            let mut state = self.lock()?;
                            ensure_admission_active(&state, permit)?;
                            *state
                                .entries
                                .get_mut(&key)
                                .ok_or(NativeLifecycleErrorV1::AlreadyClaimed)? =
                                EntryStateV1::Rejected;
                            state.rejected_generations.insert((
                                key.package_id.clone(),
                                key.sealed_manifest_digest.clone(),
                            ));
                            return Err(error);
                        }
                    };
                    let mut state = self.lock()?;
                    ensure_admission_active(&state, permit)?;
                    *state
                        .entries
                        .get_mut(&key)
                        .ok_or(NativeLifecycleErrorV1::AlreadyClaimed)? = EntryStateV1::Activated;
                    drop(state);
                    for identity in identities {
                        staged_gates.push((identity.clone(), key.clone()));
                        activated.push(identity);
                    }
                }
                Err(NativeActivationFailureV1::Rejected) => {
                    let mut state = self.lock()?;
                    ensure_admission_active(&state, permit)?;
                    *state
                        .entries
                        .get_mut(&key)
                        .ok_or(NativeLifecycleErrorV1::AlreadyClaimed)? = EntryStateV1::Rejected;
                    state
                        .rejected_generations
                        .insert((key.package_id.clone(), key.sealed_manifest_digest.clone()));
                    return Err(NativeLifecycleErrorV1::ActivationRejected);
                }
                Err(NativeActivationFailureV1::Faulted) => {
                    let mut state = self.lock()?;
                    ensure_admission_active(&state, permit)?;
                    *state
                        .entries
                        .get_mut(&key)
                        .ok_or(NativeLifecycleErrorV1::AlreadyClaimed)? = EntryStateV1::Faulted;
                    state
                        .rejected_generations
                        .insert((key.package_id.clone(), key.sealed_manifest_digest.clone()));
                    return Err(NativeLifecycleErrorV1::ActivationFaulted);
                }
            }
        }
        let mut state = self.lock()?;
        ensure_admission_active(&state, permit)?;
        let new_gate_count = staged_gates
            .iter()
            .map(|(identity, _)| identity.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|identity| !state.gates.contains_key(identity))
            .count();
        if state.gates.len().saturating_add(new_gate_count) > MAX_NATIVE_FEATURE_GATES_V1 {
            state
                .rejected_generations
                .insert((package_id.to_owned(), digest.to_owned()));
            return Err(NativeLifecycleErrorV1::FeatureGateLimitExceeded);
        }
        for (identity, member) in staged_gates {
            state
                .gates
                .entry(identity)
                .and_modify(|gate| {
                    gate.members.insert(member.clone());
                })
                .or_insert_with(|| FeatureGateV1 {
                    state: NativeFeatureStateV1::DisabledResident,
                    accepting: false,
                    in_flight: 0,
                    epoch: 0,
                    operation: GateOperationV1::Idle,
                    members: BTreeSet::from([member]),
                });
        }
        Ok(activated)
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
        if let Ok(mut state) = self.shared.state.lock()
            && state.phase == Some(LifecyclePhaseV1::Admitting)
        {
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
            state.phase = Some(LifecyclePhaseV1::Closed);
        }
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
        // Foreign hooks and bounded waiting intentionally run without the
        // lifecycle state lock so reentrant hooks cannot deadlock the manager.
        self.drain_port.detach(identity);
        self.drain_port.cancel(identity);
        let local_drained = self.wait_for_local_leases_until(identity, deadline)?;
        let port_drained = self
            .drain_port
            .wait_for_drain(identity, remaining_until(deadline));
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
        Self::with_ports(executor, drain_port, timeout)
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
        let loaded = ExtensionDllLoaderV1
            .load_package(resolved)
            .map_err(|error| NativeLifecycleErrorV1::LoaderRejected {
                diagnostic: NativeLoaderDiagnosticCodeV1::from_loader(&error),
            })?;
        self.lifecycle.admit_loaded(permit, resolved, &loaded)
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
}

impl Drop for NativeDispatchLeaseV1 {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.lock()
            && let Some(gate) = state.gates.get_mut(&self.identity)
        {
            gate.in_flight = gate.in_flight.saturating_sub(1);
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

fn next_operation_token(state: &mut RuntimeStateV1) -> u64 {
    state.next_operation_token = state.next_operation_token.saturating_add(1);
    state.next_operation_token
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
