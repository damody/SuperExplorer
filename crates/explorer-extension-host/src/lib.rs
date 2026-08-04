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
//! Process-resident extension-host composition seam.
//!
//! This crate validates sealed packages and the data-only v1 ABI contract,
//! dispatches registrar and provider callbacks, and owns feature lifecycle,
//! bounded result transport, cache, and UI-ingress composition.

mod bundled_tool;
mod contribution_gate;
mod dll_loader;
mod extension_job_runtime;
mod extension_job_ui_bridge;
mod extension_result_cache;
mod extension_value_router;
mod feature_state;
mod lua_registrar;
mod manifest;
mod native_lifecycle;
mod operation_plan;
mod package_resolver;
mod package_source;
mod package_validation;
mod plugin_call_guard;
mod runtime_authority;
mod sepack_import;
mod ui_invalidation_batcher;

pub use dll_loader::{
    SinglePluginBatchColumnRuntimeV1, SinglePluginSizeMapViewRuntimeV1,
    SinglePluginVisualColumnRuntimeV1, SinglePluginVisualMeasureRuntimeV1,
    SinglePluginVisualRenderRuntimeV1,
};

pub use bundled_tool::mint_attested_tool_handle_v1;
pub use contribution_gate::{
    ContributionGateErrorV1, ContributionGateV1, ContributionJobContractV1, ContributionKindV1,
    ContributionRegistrationV1, MAX_CAPABILITIES_PER_CONTRIBUTION_V1,
    MAX_CONTRIBUTIONS_PER_BATCH_V1, ValidatedContributionSetV1,
};
pub use extension_job_runtime::{
    AcceptedIncrementalResultBatchV1, BatchColumnRuntimeRequestV1, ExtensionJobAuthorityV1,
    ExtensionJobCacheLookupV1, ExtensionJobFinishOutcomeV1, ExtensionJobProducerV1,
    ExtensionJobQuarantineEventV1, ExtensionJobRuntimeErrorV1, ExtensionJobRuntimeRequestV1,
    ExtensionJobRuntimeV1, ExtensionResultBufferConfigV1, HostBatchColumnItemV1,
    HostInputStreamSourceV1, HostLockOwnerQueryServiceV1, MAX_BATCH_COLUMN_INPUT_BYTES_V1,
    MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1, PreparedBatchColumnDispatchTicketV1,
};
pub use extension_job_ui_bridge::{
    ExtensionJobUiInboxV1, ExtensionJobUiIngressV1, ExtensionJobUiPumpErrorV1,
    ExtensionJobUiPumpV1, ExtensionJobUiReadyDrainV1, ExtensionJobUiReadySignalV1,
    ExtensionJobUiSignalOutcomeV1, MAX_EXTENSION_UI_APPLIED_NOTICES_V1,
    MAX_EXTENSION_UI_READY_SIGNALS_V1,
};
pub use extension_result_cache::{
    ExtensionResultCacheAdmissionV1, ExtensionResultCacheConfigV1, ExtensionResultCacheFileFactV1,
    ExtensionResultCacheGenerationV1, ExtensionResultCacheHitV1,
    ExtensionResultCacheInsertOutcomeV1, ExtensionResultCacheKeyV1, ExtensionResultCacheLookupV1,
    ExtensionResultCacheV1,
};
pub use extension_value_router::{
    ExtensionSortDirectionV1, ExtensionValueRowV1, ExtensionValueViewV1, OpaquePayloadBindingV1,
    OpaquePayloadRouteErrorV1, RoutedOpaquePayloadV1, compare_extension_rows_v1,
    project_terminal_outcome_v1,
};
pub use feature_state::{
    DesiredStateV1, EffectiveFeatureReasonV1, EffectiveFeatureResolverErrorV1,
    EffectiveFeatureResolverV1, EffectiveFeatureStateV1, EffectiveFeatureV1,
    FEATURE_STATE_STORE_SCHEMA_VERSION_V1, FeatureCompatibilityFactV1, FeatureCompatibilityIssueV1,
    FeatureDiagnosticFactV1, FeatureKeyV1, FeatureResolutionFactV1, FeatureRuntimeFactV1,
    FeatureStateStoreErrorV1, FeatureStateStoreV1,
};
pub use lua_registrar::{
    LuaContributionV1, LuaRegistrarErrorV1, MAX_LUA_CONTRIBUTIONS_V1,
    run_restricted_lua_registrar_v1,
};
pub use manifest::{
    BundledToolV1, ContactPurposeV1, LocaleResourceV1, LuaEntrypointV1,
    PACKAGE_MANIFEST_VERSION_V1, PackageDependencyV1, PackageFeatureV1, PackageIdentityV1,
    PackageManifestErrorV1, PackageManifestV1, PayloadKindV1, PayloadV1, PublisherContactKindV1,
    PublisherContactV1, PublisherV1, RootContractIdV1, RustEntrypointV1, SdkCompatibilityV1,
    SignatureV1, SkinEntrypointV1, ToolOutputProtocolV1, VerifiedPublisherIdentityV1,
};
pub use native_lifecycle::{
    MAX_NATIVE_FEATURE_GATES_V1, MAX_NATIVE_LEDGER_ENTRIES_V1,
    MAX_NATIVE_RESTART_REASONS_PER_FEATURE_V1, NativeDispatchLeaseV1, NativeExtensionLifecycleV1,
    NativeFeatureIdentityV1, NativeFeatureStateV1, NativeLifecycleConfigV1, NativeLifecycleErrorV1,
    NativeLoaderDiagnosticCodeV1, NativeRestartReasonV1, NativeStartupAdmissionV1, StartupSession,
};
pub use operation_plan::{
    HostOperationPlanEngineV1, OperationCancellationV1, OperationPlanErrorV1,
    identity as operation_file_identity_v1,
};
pub use package_resolver::{
    BlockedPackageV1, PackageResolutionDiagnosticCodeV1, PackageResolutionDiagnosticV1,
    PackageResolutionV1, PackageResolverV1, ResolvedPackageDependencyV1, ResolvedPackageV1,
};
pub use package_source::{
    BuiltInPackageSourceV1, DiscoveredPackageV1, EntitlementDecisionV1, EntitlementErrorV1,
    EntitlementProviderV1, EntitlementRequestV1, LocalDeveloperPackageSourceV1,
    LocalDeveloperPackageStoreErrorV1, LocalDeveloperScratchTelemetryV1, PackageSourceErrorV1,
    PackageSourceKindV1, PackageSourceV1,
};
pub use package_validation::{
    PackageValidationBudgetV1, PackageValidationCancellationV1, PackageValidationErrorV1,
    PackageValidationRequestV1, PackageValidationResultV1, PackageValidatorV1,
    ReleaseTrustRootArtifactErrorV1, SealedPackageActivationGuardV1, SealedPackageStoreV1,
    TrustedPublisherKeyStoreV1, TrustedPublisherKeyV1,
};
pub use plugin_call_guard::{
    NativeCallOperationV1, NativeCallTerminalV1, NativeCallTimingV1, NativeSafeModeIncidentIdV1,
    NativeSafeModeIncidentKindV1, NativeSafeModeIncidentV1,
};
pub use sepack_import::SePackImportErrorV1;
pub use ui_invalidation_batcher::{
    MAX_UI_INVALIDATION_RECORDS_V1, MAX_UI_INVALIDATION_SCOPES_V1, MAX_UI_INVALIDATION_WINDOW_V1,
    MIN_UI_INVALIDATION_WINDOW_V1, UiInvalidationBatchV1, UiInvalidationBatcherConfigErrorV1,
    UiInvalidationBatcherConfigV1, UiInvalidationBatcherV1, UiInvalidationScopeV1,
};

/// Copied, read-only information about one contribution registered by the
/// explicitly selected development DLL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SinglePluginContributionSummaryV1 {
    contribution_id: String,
    kind: explorer_extension_api::RegisteredContributionKindV1,
}

impl SinglePluginContributionSummaryV1 {
    #[must_use]
    pub fn contribution_id(&self) -> &str {
        &self.contribution_id
    }

    #[must_use]
    pub const fn kind(&self) -> explorer_extension_api::RegisteredContributionKindV1 {
        self.kind
    }
}

/// Owned registration result for the one explicitly supplied development DLL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SinglePluginLoadSummaryV1 {
    plugin_id: StableIdV1,
    contributions: Vec<SinglePluginContributionSummaryV1>,
}

impl SinglePluginLoadSummaryV1 {
    #[must_use]
    pub const fn plugin_id(&self) -> StableIdV1 {
        self.plugin_id
    }

    #[must_use]
    pub fn contributions(&self) -> &[SinglePluginContributionSummaryV1] {
        &self.contributions
    }
}

/// Failure while loading the single explicitly selected development DLL.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SinglePluginLoadErrorV1 {
    #[error("plugin DLL path must be absolute")]
    PathMustBeAbsolute,
    #[error("plugin DLL path does not name an existing file")]
    PathDoesNotExist,
    #[error("plugin path must name a .dll file")]
    PathMustBeDll,
    #[error("plugin callback is blocked by recovered Safe Mode incident")]
    BlockedBySafeMode,
    #[error("could not load plugin DLL: {0}")]
    LoadFailed(String),
}

/// A requested direct visual-column contribution is unavailable or has the
/// wrong contribution kind for the selected single-owner runtime.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SinglePluginVisualColumnCallErrorV1 {
    #[error("no retained folder-size column contribution named {0:?}")]
    UnknownMeasureContribution(String),
    #[error("no retained visual renderer contribution named {0:?}")]
    UnknownRenderContribution(String),
}

/// A requested direct bounded batch-column contribution is unavailable.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SinglePluginBatchColumnCallErrorV1 {
    #[error("no retained batch-column contribution named {contribution_id:?}")]
    MissingContribution { contribution_id: String },
    #[error("batch-column runtime rejected the direct invocation: {0}")]
    Runtime(ExtensionJobRuntimeErrorV1),
}

/// A requested direct Size Map view contribution is unavailable.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SinglePluginSizeMapViewCallErrorV1 {
    #[error("no retained Size Map view contribution named {0:?}")]
    UnknownViewContribution(String),
    #[error("Size Map renderer failed with ABI error {0:?}")]
    Plugin(AbiErrorV1),
}

use explorer_extension_api::{
    ABI_SCHEMA_V1, AbiErrorCodeV1, AbiErrorV1, DESCRIPTOR_CONTRACT_REVISION_V1,
    ExtensionRootModuleV1_Ref, IdNamespaceV1, PluginMetadataV1, ROOT_MODULE_CONTRACT_ID_V1,
    RegistrationOutcomeV1, RegistrationStatusV1, SDK_MAJOR_VERSION_V1, StableIdV1,
};
use std::{
    collections::BTreeMap,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
};
use thiserror::Error;

use package_source::{
    LocalDeveloperPackageStoreV1, extension_host_storage_root_v1, local_developer_storage_root_v1,
};

const MAX_STARTUP_DIAGNOSTICS_V1: usize = 128;

/// Path-free package outcome retained after startup. Package failures are
/// isolated: they never make the file manager unavailable, while the actual
/// reason remains visible to Extension Options/Safe Mode UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionStartupDiagnosticCodeV1 {
    /// Built-in package discovery was deliberately inactive because this build
    /// does not ship immutable product trust roots.
    BuiltInTrustRootsUnavailable,
    /// A candidate failed package validation before any callback.
    ValidationRejected,
    /// Dependency resolution blocked the complete package.
    ResolutionBlocked,
    /// A required dependency failed validation or native startup admission.
    RequiredDependencyAdmissionFailed,
    /// Persisted desired state denied this package before native authority.
    FeatureStateDisabled,
    /// Persisted desired state was corrupt or unreadable, so every extension
    /// was denied while core startup and Safe Mode remained available.
    FeatureStateUnavailable,
    /// Caller cancellation denied all queued local-developer archives before
    /// any imported package could reach resolver or native activation.
    LocalDeveloperImportCancelled,
    /// Safe Mode blocked the package before its registrar callback.
    SafeModeDenied,
    /// The sealed DLL/root/ABI integrity path rejected the package.
    NativeLoaderRejected,
    /// The registrar returned a typed rejection or panic terminal.
    NativeActivationRejected,
    /// The registrar or marker machinery faulted during guarded admission.
    NativeActivationFaulted,
}

/// Bounded path-free startup diagnostic for one package source candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionStartupDiagnosticV1 {
    code: ExtensionStartupDiagnosticCodeV1,
    package_id: Option<String>,
}

impl ExtensionStartupDiagnosticV1 {
    #[must_use]
    pub const fn code(&self) -> ExtensionStartupDiagnosticCodeV1 {
        self.code
    }

    #[must_use]
    pub fn package_id(&self) -> Option<&str> {
        self.package_id.as_deref()
    }
}

fn push_startup_diagnostic(
    diagnostics: &mut Vec<ExtensionStartupDiagnosticV1>,
    code: ExtensionStartupDiagnosticCodeV1,
    package_id: Option<&str>,
) {
    if diagnostics.len() < MAX_STARTUP_DIAGNOSTICS_V1 {
        diagnostics.push(ExtensionStartupDiagnosticV1 {
            code,
            package_id: package_id.map(str::to_owned),
        });
    }
}

/// Runs the actual built-in source path used by production startup. Discovery
/// errors remain startup errors because the executable-adjacent root is host
/// owned; malformed individual packages are isolated as diagnostics.
fn discover_and_validate_built_in_packages_v1(
    root: PathBuf,
    validator: &PackageValidatorV1,
    diagnostics: &mut Vec<ExtensionStartupDiagnosticV1>,
) -> Result<Vec<PackageValidationResultV1>, PackageSourceErrorV1> {
    let source = BuiltInPackageSourceV1::new(root);
    let mut packages = Vec::new();
    for candidate in source.discover()? {
        match candidate.validate(validator) {
            Ok(package) => packages.push(package),
            Err(_) => push_startup_diagnostic(
                diagnostics,
                ExtensionStartupDiagnosticCodeV1::ValidationRejected,
                None,
            ),
        }
    }
    Ok(packages)
}

const FEATURE_STATE_FILE_NAME_V1: &str = "feature-state-v1.json";

/// Loads desired state before any native admission. A missing document is
/// initialized atomically; a corrupt, unsupported, or unreadable document is
/// never silently reset. Instead it yields an all-disabled in-memory state so
/// the core application can continue to Safe Mode without disclosing a path.
struct LoadedFeatureStateV1 {
    store: FeatureStateStoreV1,
    unavailable: bool,
}

fn load_persisted_feature_state_v1(path: &Path) -> LoadedFeatureStateV1 {
    match FeatureStateStoreV1::load(path) {
        Ok(store) => LoadedFeatureStateV1 {
            store,
            unavailable: false,
        },
        Err(FeatureStateStoreErrorV1::Io { source, .. })
            if source.kind() == ErrorKind::NotFound =>
        {
            let store = FeatureStateStoreV1::new();
            if store.save_atomic(path).is_ok() {
                LoadedFeatureStateV1 {
                    store,
                    unavailable: false,
                }
            } else {
                all_extensions_disabled_feature_state_v1()
            }
        }
        Err(_) => all_extensions_disabled_feature_state_v1(),
    }
}

fn all_extensions_disabled_feature_state_v1() -> LoadedFeatureStateV1 {
    let mut store = FeatureStateStoreV1::new();
    store.set_global_desired(DesiredStateV1::Disabled);
    LoadedFeatureStateV1 {
        store,
        unavailable: true,
    }
}

/// Decides whether a package may enter the native registrar/provider boundary.
///
/// Native registration is package-scoped but authority is feature-scoped. To
/// ensure a disabled feature can never receive registrar/provider authority,
/// the host admits a package only when *all* of its declared features are
/// effectively enabled. This is deliberately conservative for packages that
/// combine independently toggled features in one native registrar.
fn package_may_reach_native_authority_v1(
    desired: &FeatureStateStoreV1,
    manifest: &PackageManifestV1,
) -> Result<bool, EffectiveFeatureResolverErrorV1> {
    if desired.global_desired() == DesiredStateV1::Disabled
        || desired.package_desired(&manifest.package.id) == DesiredStateV1::Disabled
    {
        return Ok(false);
    }
    let facts = manifest
        .features
        .iter()
        .map(|feature| {
            Ok(FeatureResolutionFactV1 {
                feature: FeatureKeyV1::new(&manifest.package.id, &feature.id).map_err(|_| {
                    EffectiveFeatureResolverErrorV1::InvalidIdentifier {
                        field: "feature_id",
                        value: feature.id.clone(),
                    }
                })?,
                dependencies: feature
                    .dependencies
                    .iter()
                    .map(|dependency| {
                        FeatureKeyV1::new(&manifest.package.id, dependency).map_err(|_| {
                            EffectiveFeatureResolverErrorV1::InvalidIdentifier {
                                field: "dependency",
                                value: dependency.clone(),
                            }
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                compatibility: FeatureCompatibilityFactV1::Compatible,
                diagnostic: None,
                runtime: FeatureRuntimeFactV1::Ready,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EffectiveFeatureResolverV1::resolve(desired, &facts)?
        .iter()
        .all(|feature| feature.state == EffectiveFeatureStateV1::Enabled))
}

/// Handles failures sourced from a selected package rather than lifecycle
/// infrastructure. These failures block dependents but cannot make the host
/// process unavailable.
fn record_package_native_admission_failure_v1(
    error: NativeLifecycleErrorV1,
    package_id: &str,
    diagnostics: &mut Vec<ExtensionStartupDiagnosticV1>,
) -> Result<bool, NativeLifecycleErrorV1> {
    let code = match error {
        NativeLifecycleErrorV1::SafeModeDenied => ExtensionStartupDiagnosticCodeV1::SafeModeDenied,
        NativeLifecycleErrorV1::LoaderRejected { .. } => {
            ExtensionStartupDiagnosticCodeV1::NativeLoaderRejected
        }
        NativeLifecycleErrorV1::ActivationRejected
        | NativeLifecycleErrorV1::ActivationAuthorityMismatch
        | NativeLifecycleErrorV1::InvalidFeatureAuthority
        | NativeLifecycleErrorV1::FeatureGateLimitExceeded
        | NativeLifecycleErrorV1::DuplicateFeatureAuthority(_) => {
            ExtensionStartupDiagnosticCodeV1::NativeActivationRejected
        }
        NativeLifecycleErrorV1::ActivationFaulted => {
            ExtensionStartupDiagnosticCodeV1::NativeActivationFaulted
        }
        error => return Err(error),
    };
    push_startup_diagnostic(diagnostics, code, Some(package_id));
    Ok(false)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DependencyAdmissionStateV1 {
    Pending,
    Admitted,
    Failed,
    BlockedByFailedDependency,
}

/// Calls native admission in required-dependency topological order. A selected
/// package whose required dependency failed is never offered to the callback;
/// that block propagates transitively while unrelated packages remain eligible.
fn admit_in_dependency_order_v1<E>(
    resolved: &[ResolvedPackageV1<'_>],
    mut admit: impl FnMut(&ResolvedPackageV1<'_>) -> Result<bool, E>,
) -> Result<Vec<DependencyAdmissionStateV1>, E> {
    let indices = resolved
        .iter()
        .enumerate()
        .map(|(index, package)| (package.manifest().package.id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let dependencies = resolved
        .iter()
        .map(|package| {
            package
                .dependencies()
                .iter()
                .filter(|dependency| !dependency.optional())
                .filter_map(|dependency| indices.get(dependency.package_id()).copied())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut states = vec![DependencyAdmissionStateV1::Pending; resolved.len()];

    while states.contains(&DependencyAdmissionStateV1::Pending) {
        let mut progressed = false;
        for index in 0..resolved.len() {
            if states[index] != DependencyAdmissionStateV1::Pending
                || dependencies[index]
                    .iter()
                    .any(|dependency| states[*dependency] == DependencyAdmissionStateV1::Pending)
            {
                continue;
            }
            states[index] = if dependencies[index].iter().any(|dependency| {
                matches!(
                    states[*dependency],
                    DependencyAdmissionStateV1::Failed
                        | DependencyAdmissionStateV1::BlockedByFailedDependency
                )
            }) {
                DependencyAdmissionStateV1::BlockedByFailedDependency
            } else if admit(&resolved[index])? {
                DependencyAdmissionStateV1::Admitted
            } else {
                DependencyAdmissionStateV1::Failed
            };
            progressed = true;
        }
        if !progressed {
            // PackageResolverV1 guarantees an acyclic required graph. Retain a
            // fail-closed fallback if a future resolver violates that contract.
            for state in &mut states {
                if *state == DependencyAdmissionStateV1::Pending {
                    *state = DependencyAdmissionStateV1::BlockedByFailedDependency;
                }
            }
        }
    }
    Ok(states)
}

/// Host composition policy for unsigned local-development packages.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LocalDeveloperModeV1 {
    /// Production default: unsigned local packages cannot be imported.
    #[default]
    Disabled,
    /// Explicit developer mode using the fixed OS-known application-data root.
    Enabled,
}

/// Process-wide extension-host configuration selected by the application root.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExtensionHostConfigV1 {
    /// Whether the application composition root explicitly permits unsigned
    /// local-development archives at startup.
    pub local_developer_mode: LocalDeveloperModeV1,
    local_developer_archives: Vec<PathBuf>,
    local_developer_import_cancellation: Option<PackageValidationCancellationV1>,
    #[cfg(any(test, feature = "integration-test-support"))]
    test_state_root: Option<PathBuf>,
    #[cfg(any(test, feature = "integration-test-support"))]
    test_local_developer_root: Option<PathBuf>,
}

impl ExtensionHostConfigV1 {
    /// Selects whether this process explicitly permits local developer archive
    /// imports. The default remains disabled for production startup.
    #[must_use]
    pub fn with_local_developer_mode(mut self, mode: LocalDeveloperModeV1) -> Self {
        self.local_developer_mode = mode;
        self
    }

    /// Queues explicit archive files for this process's one startup admission.
    ///
    /// This API deliberately accepts archive paths only. The host derives every
    /// import, sealed-store, marker, and package-source root from the Windows
    /// Known Folder or its own executable; callers cannot select an arbitrary
    /// unsigned-package root or mint local-developer provenance.
    #[must_use]
    pub fn with_local_developer_archives(
        mut self,
        archives: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        self.local_developer_archives = archives.into_iter().collect();
        self
    }

    /// Supplies a caller-owned cancellation token for the bounded local
    /// developer import phase. Cancellation never enables developer mode and
    /// denies every queued local archive before it can reach activation.
    #[must_use]
    pub fn with_local_developer_import_cancellation(
        mut self,
        cancellation: PackageValidationCancellationV1,
    ) -> Self {
        self.local_developer_import_cancellation = Some(cancellation);
        self
    }

    /// Selects an isolated state root for an integration-test process.
    ///
    /// This is deliberately unavailable from normal production builds. The
    /// application UITEST feature uses it to exercise startup recovery without
    /// mutating the operator's Windows Known Folder state.
    #[cfg(any(test, feature = "integration-test-support"))]
    #[must_use]
    pub fn with_integration_test_state_root(mut self, root: PathBuf) -> Self {
        self.test_state_root = Some(root);
        self
    }

    /// Selects an isolated local-developer root for integration tests.
    #[cfg(any(test, feature = "integration-test-support"))]
    #[must_use]
    pub fn with_integration_test_local_developer_root(mut self, root: PathBuf) -> Self {
        self.test_local_developer_root = Some(root);
        self
    }

    #[must_use]
    fn local_developer_archives(&self) -> &[PathBuf] {
        &self.local_developer_archives
    }

    fn local_developer_import_cancellation(&self) -> Option<&PackageValidationCancellationV1> {
        self.local_developer_import_cancellation.as_ref()
    }
}

#[derive(Debug)]
struct LocalDeveloperRuntimeV1 {
    store: LocalDeveloperPackageStoreV1,
}

struct FeatureStateRuntimeV1 {
    store: FeatureStateStoreV1,
    path: PathBuf,
}

impl std::fmt::Debug for FeatureStateRuntimeV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FeatureStateRuntimeV1 { path: <redacted> }")
    }
}

#[derive(Debug)]
struct ExtensionHostRuntimeV1 {
    local_developer: Option<LocalDeveloperRuntimeV1>,
    feature_state: FeatureStateRuntimeV1,
    native_lifecycle: Option<NativeExtensionLifecycleV1>,
    startup_admissions: Vec<NativeStartupAdmissionV1>,
    startup_diagnostics: Vec<ExtensionStartupDiagnosticV1>,
    job_runtime: Arc<ExtensionJobRuntimeV1>,
    job_ui_ingress: ExtensionJobUiIngressV1,
    job_ui_inbox: Option<ExtensionJobUiInboxV1>,
}

/// Inert process-lifetime owner installed by the application composition root.
///
/// Starting and stopping are idempotent because process shutdown can be requested
/// explicitly and again from a drop path. A stopped host cannot be restarted: native
/// plugin loading is a startup-only lifecycle in the platform design.
#[derive(Debug)]
pub struct ExtensionHost {
    state: LifecycleState,
    config: ExtensionHostConfigV1,
    runtime: Option<ExtensionHostRuntimeV1>,
}

#[derive(Debug, Default, Eq, PartialEq)]
enum LifecycleState {
    #[default]
    New,
    Running,
    Stopped,
}

impl Default for ExtensionHost {
    fn default() -> Self {
        Self::new()
    }
}

/// Failure while initializing explicitly enabled extension-host facilities.
#[derive(Debug, Error)]
pub enum ExtensionHostStartErrorV1 {
    /// A caller queued archives without the explicit developer-mode policy.
    #[error("queued local developer archives require explicit developer mode")]
    QueuedDeveloperArchivesRequireEnabledMode,
    /// A configured host-owned package source could not be safely enumerated.
    #[error(transparent)]
    PackageSource(#[from] PackageSourceErrorV1),
    /// A package source candidate failed pre-load validation.
    #[error(transparent)]
    PackageValidation(#[from] PackageValidationErrorV1),
    /// The compiled public-only release trust roots are malformed or bound to a
    /// different SDK bundle. Built-in discovery is denied rather than weakened.
    #[error(transparent)]
    ReleaseTrustRoots(#[from] ReleaseTrustRootArtifactErrorV1),
    /// The fixed host-private developer scratch root was unavailable or unsafe.
    #[error(transparent)]
    DeveloperScratch(#[from] SePackImportErrorV1),
    /// The process-resident native lifecycle could not be acquired or admit a
    /// resolver-selected sealed package.
    #[error(transparent)]
    NativeLifecycle(#[from] NativeLifecycleErrorV1),
}

/// Path-free failure while updating desired feature state for a later startup.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum FeatureStateMutationErrorV1 {
    /// Desired state is unavailable until successful host startup has loaded it.
    #[error("extension host is not running")]
    HostNotRunning,
    /// The requested package or feature identifier was invalid.
    #[error("invalid desired-state identifier")]
    InvalidIdentifier,
    /// The desired-state document could not be atomically persisted.
    #[error("could not persist extension feature state")]
    PersistFailed,
}

impl ExtensionHost {
    /// Loads exactly one explicitly supplied absolute development DLL and
    /// returns copied registration data. This does not start package discovery
    /// or alter the host's normal lifecycle when unused.
    ///
    /// # Errors
    ///
    /// Returns a user-presentable path, loader, ABI, or registration error.
    pub fn load_single_plugin_dll(
        &self,
        path: &Path,
    ) -> Result<SinglePluginLoadSummaryV1, SinglePluginLoadErrorV1> {
        if !path.is_absolute() {
            return Err(SinglePluginLoadErrorV1::PathMustBeAbsolute);
        }
        let markers = self
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.native_lifecycle.as_ref())
            .and_then(NativeExtensionLifecycleV1::direct_callback_marker_store)
            .ok_or_else(|| {
                SinglePluginLoadErrorV1::LoadFailed("extension host is not running".to_owned())
            })?;
        dll_loader::load_single_plugin_dll(path, markers)
    }

    /// Loads one explicit development DLL and retains its visual-column objects
    /// in separate single-owner measure and render runtimes.
    ///
    /// Use [`SinglePluginVisualColumnRuntimeV1::into_parts_with_size_map`] to
    /// move its `Send`, non-`Clone`, non-`Sync` measure, cell-render, and
    /// optional Size Map render owners to their background/GPUI threads.
    pub fn load_single_plugin_visual_column_runtime(
        &self,
        path: &Path,
    ) -> Result<SinglePluginVisualColumnRuntimeV1, SinglePluginLoadErrorV1> {
        if !path.is_absolute() {
            return Err(SinglePluginLoadErrorV1::PathMustBeAbsolute);
        }
        let markers = self
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.native_lifecycle.as_ref())
            .and_then(NativeExtensionLifecycleV1::direct_callback_marker_store)
            .ok_or_else(|| {
                SinglePluginLoadErrorV1::LoadFailed("extension host is not running".to_owned())
            })?;
        dll_loader::load_single_plugin_visual_column_runtime(path, markers)
    }

    /// Creates the inert host seam in its unstarted state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: LifecycleState::New,
            config: ExtensionHostConfigV1 {
                local_developer_mode: LocalDeveloperModeV1::Disabled,
                local_developer_archives: Vec::new(),
                local_developer_import_cancellation: None,
                #[cfg(any(test, feature = "integration-test-support"))]
                test_state_root: None,
                #[cfg(any(test, feature = "integration-test-support"))]
                test_local_developer_root: None,
            },
            runtime: None,
        }
    }

    /// Creates an inert host with an explicit application composition policy.
    #[must_use]
    pub const fn with_config(config: ExtensionHostConfigV1) -> Self {
        Self {
            state: LifecycleState::New,
            config,
            runtime: None,
        }
    }

    /// Starts the host once during application startup.
    ///
    /// # Errors
    ///
    /// Built-in packages are discovered from the application executable's
    /// adjacent host-owned directory. Explicit developer archives, when
    /// enabled, are strictly imported into Known-Folder-owned scratch storage.
    /// Every accepted package is validated, resolved, and any selected Rust
    /// package is admitted through the resident DLL lifecycle before startup is
    /// sealed. Missing optional built-in content is an empty source, preserving
    /// the default no-third-party startup behavior.
    pub fn start(&mut self) -> Result<(), ExtensionHostStartErrorV1> {
        if self.state == LifecycleState::New {
            if !self.config.local_developer_archives().is_empty()
                && self.config.local_developer_mode != LocalDeveloperModeV1::Enabled
            {
                return Err(ExtensionHostStartErrorV1::QueuedDeveloperArchivesRequireEnabledMode);
            }
            self.runtime = self.startup_runtime()?;
            self.state = LifecycleState::Running;
        }
        Ok(())
    }

    fn startup_runtime(&self) -> Result<Option<ExtensionHostRuntimeV1>, ExtensionHostStartErrorV1> {
        let built_in_root = built_in_package_root_v1();
        let developer_enabled = self.config.local_developer_mode == LocalDeveloperModeV1::Enabled;
        #[cfg(any(test, feature = "integration-test-support"))]
        let state_root = self
            .config
            .test_state_root
            .clone()
            .map_or_else(extension_host_storage_root_v1, Ok)?;
        #[cfg(not(any(test, feature = "integration-test-support")))]
        let state_root = extension_host_storage_root_v1()?;
        let sealed_store = SealedPackageStoreV1::new(&state_root.join("sealed"))?;
        let feature_state_path = state_root.join(FEATURE_STATE_FILE_NAME_V1);
        let loaded_feature_state = load_persisted_feature_state_v1(&feature_state_path);
        let feature_state = loaded_feature_state.store;
        let trusted_keys = TrustedPublisherKeyStoreV1::release_bound_v1()?;
        let validator = PackageValidatorV1::new(trusted_keys, sealed_store);
        let mut validated = Vec::new();
        let mut startup_diagnostics = Vec::new();
        if loaded_feature_state.unavailable {
            push_startup_diagnostic(
                &mut startup_diagnostics,
                ExtensionStartupDiagnosticCodeV1::FeatureStateUnavailable,
                None,
            );
        }

        if let Some(root) = built_in_root.filter(|root| root.exists()) {
            validated.extend(discover_and_validate_built_in_packages_v1(
                root,
                &validator,
                &mut startup_diagnostics,
            )?);
        }

        let local_developer = if developer_enabled {
            #[cfg(any(test, feature = "integration-test-support"))]
            let root = self
                .config
                .test_local_developer_root
                .clone()
                .map_or_else(local_developer_storage_root_v1, Ok)?;
            #[cfg(not(any(test, feature = "integration-test-support")))]
            let root = local_developer_storage_root_v1()?;
            let store = LocalDeveloperPackageStoreV1::new(&root.join("scratch"))?;
            let local_validation_start = validated.len();
            let cancellation = self
                .config
                .local_developer_import_cancellation()
                .cloned()
                .unwrap_or_default();
            let mut local_import_cancelled = false;
            for archive in self.config.local_developer_archives() {
                match store.import_and_validate_with_cancellation(
                    archive,
                    &validator,
                    &cancellation,
                ) {
                    Ok(package) => validated.push(package),
                    Err(
                        LocalDeveloperPackageStoreErrorV1::Import(_)
                        | LocalDeveloperPackageStoreErrorV1::Validation(_),
                    ) => {
                        if cancellation.cancelled() {
                            // A caller cancellation invalidates the complete
                            // queued developer-import batch, including any
                            // earlier archive validated in this same startup.
                            validated.truncate(local_validation_start);
                            local_import_cancelled = true;
                            push_startup_diagnostic(
                                &mut startup_diagnostics,
                                ExtensionStartupDiagnosticCodeV1::LocalDeveloperImportCancelled,
                                None,
                            );
                            break;
                        }
                        push_startup_diagnostic(
                            &mut startup_diagnostics,
                            ExtensionStartupDiagnosticCodeV1::ValidationRejected,
                            None,
                        );
                    }
                }
            }
            if cancellation.cancelled() && !local_import_cancelled {
                validated.truncate(local_validation_start);
                push_startup_diagnostic(
                    &mut startup_diagnostics,
                    ExtensionStartupDiagnosticCodeV1::LocalDeveloperImportCancelled,
                    None,
                );
            }
            Some(LocalDeveloperRuntimeV1 { store })
        } else {
            None
        };

        let resolution = PackageResolverV1::resolve(&validated);
        for blocked in resolution.blocked_packages() {
            push_startup_diagnostic(
                &mut startup_diagnostics,
                ExtensionStartupDiagnosticCodeV1::ResolutionBlocked,
                Some(blocked.package_id()),
            );
        }
        let job_runtime = Arc::new(ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::host_default(),
        ));
        let mut lifecycle = NativeExtensionLifecycleV1::acquire(NativeLifecycleConfigV1::new(
            state_root.join("state"),
        ))?;
        lifecycle.bind_job_runtime(&job_runtime);
        let mut session = lifecycle.begin_startup()?;
        let mut admissions = Vec::new();
        let admission_states = admit_in_dependency_order_v1::<NativeLifecycleErrorV1>(
            resolution.resolved_packages(),
            |resolved| {
                match package_may_reach_native_authority_v1(&feature_state, resolved.manifest()) {
                    Ok(true) => {}
                    Ok(false) => {
                        push_startup_diagnostic(
                            &mut startup_diagnostics,
                            ExtensionStartupDiagnosticCodeV1::FeatureStateDisabled,
                            Some(&resolved.manifest().package.id),
                        );
                        return Ok(false);
                    }
                    Err(_) => {
                        push_startup_diagnostic(
                            &mut startup_diagnostics,
                            ExtensionStartupDiagnosticCodeV1::NativeActivationRejected,
                            Some(&resolved.manifest().package.id),
                        );
                        return Ok(false);
                    }
                }
                if resolved.manifest().rust.is_empty() {
                    return Ok(true);
                }
                let admitted = match session.admit_resolved_package(resolved) {
                    Ok(admission) => {
                        admissions.push(admission);
                        true
                    }
                    Err(error) => record_package_native_admission_failure_v1(
                        error,
                        &resolved.manifest().package.id,
                        &mut startup_diagnostics,
                    )?,
                };
                Ok(admitted)
            },
        )?;
        for (resolved, state) in resolution.resolved_packages().iter().zip(admission_states) {
            if state == DependencyAdmissionStateV1::BlockedByFailedDependency {
                push_startup_diagnostic(
                    &mut startup_diagnostics,
                    ExtensionStartupDiagnosticCodeV1::RequiredDependencyAdmissionFailed,
                    Some(&resolved.manifest().package.id),
                );
            }
        }
        session.seal()?;

        let (job_ui_ingress, job_ui_inbox) =
            ExtensionJobUiIngressV1::new_pair(Arc::clone(&job_runtime));
        job_runtime.install_ready_signal_sink(job_ui_ingress.runtime_ready_sink());
        Ok(Some(ExtensionHostRuntimeV1 {
            local_developer,
            feature_state: FeatureStateRuntimeV1 {
                store: feature_state,
                path: feature_state_path,
            },
            native_lifecycle: Some(lifecycle),
            startup_admissions: admissions,
            startup_diagnostics,
            job_runtime,
            job_ui_ingress,
            job_ui_inbox: Some(job_ui_inbox),
        }))
    }

    /// Returns path-free local-developer scratch telemetry when developer mode
    /// is enabled. A successful seal remains successful if later scratch
    /// cleanup fails; callers can use this counter for diagnostics instead.
    #[must_use]
    pub fn local_developer_scratch_telemetry(&self) -> Option<LocalDeveloperScratchTelemetryV1> {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.local_developer.as_ref())
            .map(|runtime| runtime.store.telemetry())
    }

    /// Returns the path-free record of native packages admitted during this
    /// startup. The empty default host has no admissions.
    #[must_use]
    pub fn startup_admissions(&self) -> &[NativeStartupAdmissionV1] {
        self.runtime
            .as_ref()
            .map_or(&[], |runtime| runtime.startup_admissions.as_slice())
    }

    /// Returns bounded path-free package failures observed during this startup.
    #[must_use]
    pub fn startup_diagnostics(&self) -> &[ExtensionStartupDiagnosticV1] {
        self.runtime
            .as_ref()
            .map_or(&[], |runtime| runtime.startup_diagnostics.as_slice())
    }

    /// Returns the persisted desired state loaded for this process. Runtime
    /// changes are saved atomically for the next startup; applying a disable to
    /// already resident native code requires the later drain coordinator.
    #[must_use]
    pub fn feature_state(&self) -> Option<&FeatureStateStoreV1> {
        self.runtime
            .as_ref()
            .map(|runtime| &runtime.feature_state.store)
    }

    /// Persists the global desired state without erasing package or feature
    /// overrides, including overrides for packages not currently installed.
    ///
    /// # Errors
    ///
    /// Returns an error if startup has not loaded desired state or the atomic
    /// persistence operation fails.
    pub fn set_global_feature_desired(
        &mut self,
        desired: DesiredStateV1,
    ) -> Result<(), FeatureStateMutationErrorV1> {
        self.update_feature_state_v1(|state| {
            state.set_global_desired(desired);
            Ok(())
        })
    }

    /// Persists a package desired-state override for a later startup.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid package identifier, unavailable host,
    /// or failed atomic persistence.
    pub fn set_package_feature_desired(
        &mut self,
        package_id: impl Into<String>,
        desired: DesiredStateV1,
    ) -> Result<(), FeatureStateMutationErrorV1> {
        self.update_feature_state_v1(|state| {
            state
                .set_package_desired(package_id, desired)
                .map_err(|_| FeatureStateMutationErrorV1::InvalidIdentifier)
        })
    }

    /// Persists a feature desired-state override for a later startup.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid feature identifier, unavailable host,
    /// or failed atomic persistence.
    pub fn set_individual_feature_desired(
        &mut self,
        feature: FeatureKeyV1,
        desired: DesiredStateV1,
    ) -> Result<(), FeatureStateMutationErrorV1> {
        self.update_feature_state_v1(|state| {
            state
                .set_feature_desired(feature, desired)
                .map_err(|_| FeatureStateMutationErrorV1::InvalidIdentifier)
        })
    }

    fn update_feature_state_v1(
        &mut self,
        update: impl FnOnce(&mut FeatureStateStoreV1) -> Result<(), FeatureStateMutationErrorV1>,
    ) -> Result<(), FeatureStateMutationErrorV1> {
        let runtime = self
            .runtime
            .as_mut()
            .ok_or(FeatureStateMutationErrorV1::HostNotRunning)?;
        let mut updated = runtime.feature_state.store.clone();
        update(&mut updated)?;
        updated
            .save_atomic(&runtime.feature_state.path)
            .map_err(|_| FeatureStateMutationErrorV1::PersistFailed)?;
        runtime.feature_state.store = updated;
        Ok(())
    }

    /// Returns recovered Safe Mode incidents even when no package is admitted.
    #[must_use]
    pub fn safe_mode_incidents(&self) -> Vec<NativeSafeModeIncidentV1> {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.native_lifecycle.as_ref())
            .map_or_else(Vec::new, NativeExtensionLifecycleV1::safe_mode_incidents)
    }

    /// Whether recovered marker residue denies every native callback.
    #[must_use]
    pub fn safe_mode_denies_all(&self) -> bool {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.native_lifecycle.as_ref())
            .is_some_and(NativeExtensionLifecycleV1::safe_mode_denies_all)
    }

    /// Returns whether a recovered marker would deny its exact registrar
    /// callback. Available only in the non-default integration-test build.
    #[cfg(feature = "integration-test-support")]
    #[must_use]
    pub fn integration_test_recovered_callback_is_denied(&self) -> bool {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.native_lifecycle.as_ref())
            .is_some_and(NativeExtensionLifecycleV1::integration_test_recovered_callback_is_denied)
    }

    /// Confirms one recovered Safe Mode incident from host-owned UI policy.
    ///
    /// # Errors
    ///
    /// Returns an error when no active host marker store owns the incident.
    pub fn confirm_safe_mode_incident(
        &self,
        incident_id: NativeSafeModeIncidentIdV1,
    ) -> Result<(), NativeLifecycleErrorV1> {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.native_lifecycle.as_ref())
            .ok_or(NativeLifecycleErrorV1::SafeModeIncidentUnknown)?
            .confirm_safe_mode_incident(incident_id)
    }

    /// Returns bounded path-free native callback timing diagnostics.
    #[must_use]
    pub fn native_call_timings(&self) -> Vec<NativeCallTimingV1> {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.native_lifecycle.as_ref())
            .map_or_else(Vec::new, NativeExtensionLifecycleV1::native_call_timings)
    }

    /// Returns the cloneable producer ingress bound to this host's one
    /// canonical job runtime. It only carries bounded readiness/apply signals;
    /// it cannot consume UI work or close the host mailbox.
    #[must_use]
    pub fn extension_job_ui_ingress(&self) -> Option<ExtensionJobUiIngressV1> {
        (self.state == LifecycleState::Running)
            .then(|| {
                self.runtime
                    .as_ref()
                    .map(|runtime| runtime.job_ui_ingress.clone())
            })
            .flatten()
    }

    /// Returns the host's canonical runtime for model-side generation checks
    /// and draining. Callers must pair it with this host's ingress; the
    /// ingress rejects a different runtime identity.
    #[must_use]
    pub fn extension_job_runtime(&self) -> Option<Arc<ExtensionJobRuntimeV1>> {
        (self.state == LifecycleState::Running)
            .then(|| {
                self.runtime
                    .as_ref()
                    .map(|runtime| Arc::clone(&runtime.job_runtime))
            })
            .flatten()
    }

    /// Transfers the unique UI inbox to the application composition root once.
    /// A second caller receives `None`, preventing nondeterministic competing
    /// consumers from taking accepted-ready signals.
    pub fn take_extension_job_ui_inbox(&mut self) -> Option<ExtensionJobUiInboxV1> {
        (self.state == LifecycleState::Running)
            .then(|| {
                self.runtime
                    .as_mut()
                    .and_then(|runtime| runtime.job_ui_inbox.take())
            })
            .flatten()
    }

    /// Stops the host once during application shutdown.
    pub fn shutdown(&mut self) {
        if self.state == LifecycleState::Running {
            if let Some(runtime) = self.runtime.as_ref() {
                // Stop new signals before lifecycle cancellation/revocation.
                // The UI may still hold the one inbox, but it no longer has a
                // live producer path or publication authority.
                runtime.job_ui_ingress.close();
                runtime.job_runtime.cancel_and_revoke_all();
                if let Some(lifecycle) = runtime.native_lifecycle.as_ref() {
                    lifecycle.shutdown();
                }
            }
            self.state = LifecycleState::Stopped;
        }
    }

    /// Returns whether the process-lifetime host is currently active.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.state == LifecycleState::Running
    }

    /// Validates a root module's required, non-callback data.
    ///
    /// The dynamic loader added in task 3 validates the `abi_stable` layout before
    /// it creates this reference. This method completes semantic validation before
    /// calling a plugin-provided registrar.
    ///
    /// # Errors
    ///
    /// Returns [`HostRegistrationErrorV1::Incompatible`] when a required schema,
    /// SDK major, root contract, plugin ID, or interface ID is malformed or
    /// unsupported.
    pub fn validate_root(
        &self,
        root: ExtensionRootModuleV1_Ref,
    ) -> Result<PluginMetadataV1, HostRegistrationErrorV1> {
        if root.abi_schema() != ABI_SCHEMA_V1 {
            return Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
                AbiErrorCodeV1::SCHEMA_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                root.abi_schema().into_raw(),
            )));
        }

        validate_required_id(root.root_contract_id(), ROOT_MODULE_CONTRACT_ID_V1)?;

        if root.sdk_major() != SDK_MAJOR_VERSION_V1 {
            return Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
                AbiErrorCodeV1::SDK_MAJOR_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                u32::from(root.sdk_major()),
            )));
        }

        if root.reserved() != 0 {
            return Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
                AbiErrorCodeV1::UNSUPPORTED_ID,
                ROOT_MODULE_CONTRACT_ID_V1,
                u32::from(root.reserved()),
            )));
        }

        if root.descriptor_contract_revision() != DESCRIPTOR_CONTRACT_REVISION_V1 {
            return Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
                AbiErrorCodeV1::UNSUPPORTED_ID,
                ROOT_MODULE_CONTRACT_ID_V1,
                root.descriptor_contract_revision(),
            )));
        }

        let metadata = root.metadata();
        validate_id_in_extension_namespace(metadata.plugin_id)?;
        validate_id_in_extension_namespace(metadata.primary_interface_id)?;

        Ok(metadata)
    }

    /// Test-only raw registrar dispatch for ABI boundary coverage.
    ///
    /// A plugin callback returns typed ABI errors. `abi_stable` only supports the
    /// non-unwinding C function ABI here, so plugin callbacks must catch their own
    /// Rust panics and return [`AbiErrorCodeV1::CALLBACK_PANICKED`]. The host maps
    /// that explicit terminal to [`HostRegistrationErrorV1::Panicked`].
    ///
    /// # Errors
    ///
    /// Returns validation failures before invoking the registrar, and translates a
    /// plugin's typed error or translated panic terminal after invocation.
    #[cfg(test)]
    fn register_root_for_test(
        &self,
        root: ExtensionRootModuleV1_Ref,
    ) -> Result<RegistrationOutcomeV1, HostRegistrationErrorV1> {
        self.validate_root(root)?;

        root.create_registrar()
            .create()
            .into_result()
            .map_err(|error| {
                if error.code == AbiErrorCodeV1::CALLBACK_PANICKED {
                    HostRegistrationErrorV1::Panicked(error)
                } else {
                    HostRegistrationErrorV1::Plugin(error)
                }
            })
            .and_then(|registrar| {
                registrar
                    .register(explorer_extension_api::registrar_request_v1())
                    .into_result()
                    .map(|output| output.outcome)
                    .map_err(|error| {
                        if error.code == AbiErrorCodeV1::CALLBACK_PANICKED {
                            HostRegistrationErrorV1::Panicked(error)
                        } else {
                            HostRegistrationErrorV1::Plugin(error)
                        }
                    })
            })
            .and_then(validate_registration_outcome)
    }
}

fn built_in_package_root_v1() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|executable| {
        executable
            .parent()
            .map(|directory| directory.join("extensions").join("built-in-v1"))
    })
}

/// Typed failure from host-side ABI validation or registrar dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostRegistrationErrorV1 {
    /// The root's required ABI data is not compatible with this host.
    Incompatible(AbiErrorV1),
    /// The plugin returned a typed registration error.
    Plugin(AbiErrorV1),
    /// The registrar reported a translated panic terminal.
    Panicked(AbiErrorV1),
}

fn validate_required_id(
    actual: StableIdV1,
    expected: StableIdV1,
) -> Result<(), HostRegistrationErrorV1> {
    if !actual.is_valid() {
        return Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
            AbiErrorCodeV1::INVALID_ID,
            actual,
            0,
        )));
    }
    if actual != expected {
        return Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
            AbiErrorCodeV1::UNSUPPORTED_ID,
            actual,
            0,
        )));
    }
    Ok(())
}

fn validate_id_in_extension_namespace(id: StableIdV1) -> Result<(), HostRegistrationErrorV1> {
    if id.is_in_namespace(extension_id_namespace_v1()) {
        return Ok(());
    }

    let code = if id.is_valid() {
        AbiErrorCodeV1::UNSUPPORTED_ID
    } else {
        AbiErrorCodeV1::INVALID_ID
    };
    Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
        code, id, 0,
    )))
}

#[allow(dead_code, reason = "used only by the task 3.5 guarded registrar path")]
fn validate_registration_outcome(
    outcome: RegistrationOutcomeV1,
) -> Result<RegistrationOutcomeV1, HostRegistrationErrorV1> {
    let raw_status = outcome.status.into_raw();
    if raw_status == RegistrationStatusV1::ACCEPTED.into_raw() {
        return Ok(outcome);
    }

    let code = if raw_status == RegistrationStatusV1::REJECTED.into_raw() {
        AbiErrorCodeV1::REGISTRATION_OUTCOME_REJECTED
    } else if raw_status == 0 {
        AbiErrorCodeV1::MALFORMED_REGISTRATION_OUTCOME
    } else {
        AbiErrorCodeV1::UNKNOWN_REGISTRATION_OUTCOME
    };
    Err(HostRegistrationErrorV1::Plugin(AbiErrorV1::new(
        code,
        ROOT_MODULE_CONTRACT_ID_V1,
        raw_status,
    )))
}

const fn extension_id_namespace_v1() -> IdNamespaceV1 {
    explorer_extension_api::EXTENSION_ID_NAMESPACE_V1
}

#[cfg(test)]
mod tests {
    use std::{
        fmt::Write as _,
        fs,
        path::PathBuf,
        sync::atomic::{AtomicBool, Ordering},
    };

    use abi_stable::{prefix_type::PrefixTypeTrait, std_types::RResult};
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use explorer_extension_api::{
        ABI_SCHEMA_V1, AbiErrorCodeV1, AbiErrorV1, DESCRIPTOR_CONTRACT_REVISION_V1,
        ExtensionRegistrarImplementationV1, ExtensionRootModuleV1, PluginMetadataV1,
        ROOT_MODULE_CONTRACT_ID_V1, RegistrarFactoryV1, RegistrarOutputV1, RegistrarRequestV1,
        RegistrationOutcomeV1, RegistrationStatusV1, SDK_MAJOR_VERSION_V1, StableIdV1,
    };
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::{
        DesiredStateV1, ExtensionHost, ExtensionHostConfigV1, ExtensionRootModuleV1_Ref,
        ExtensionStartupDiagnosticCodeV1, FeatureKeyV1, FeatureStateStoreV1,
        HostRegistrationErrorV1, LifecycleState, LocalDeveloperModeV1, NativeLifecycleErrorV1,
        PackageManifestV1, PackageResolverV1, PackageValidationCancellationV1,
        PackageValidationResultV1, PackageValidatorV1, SealedPackageStoreV1,
        TrustedPublisherKeyStoreV1,
    };

    const PLUGIN_ID: StableIdV1 = StableIdV1::new(super::extension_id_namespace_v1(), 100);
    const INTERFACE_ID: StableIdV1 = StableIdV1::new(super::extension_id_namespace_v1(), 101);

    fn root<T: ExtensionRegistrarImplementationV1 + 'static>(
        abi_schema: explorer_extension_api::AbiSchemaIdV1,
        root_contract_id: StableIdV1,
        sdk_major: u16,
        metadata: PluginMetadataV1,
    ) -> ExtensionRootModuleV1_Ref {
        root_with_layout::<T>(
            abi_schema,
            root_contract_id,
            sdk_major,
            0,
            DESCRIPTOR_CONTRACT_REVISION_V1,
            metadata,
        )
    }

    fn root_with_layout<T: ExtensionRegistrarImplementationV1 + 'static>(
        abi_schema: explorer_extension_api::AbiSchemaIdV1,
        root_contract_id: StableIdV1,
        sdk_major: u16,
        reserved: u16,
        descriptor_contract_revision: u32,
        metadata: PluginMetadataV1,
    ) -> ExtensionRootModuleV1_Ref {
        ExtensionRootModuleV1 {
            abi_schema,
            root_contract_id,
            sdk_major,
            reserved,
            metadata,
            ui_abi_fingerprint_sha256: abi_stable::std_types::ROption::RNone,
            create_registrar: RegistrarFactoryV1::new::<T>(),
            descriptor_contract_revision,
        }
        .leak_into_prefix()
    }

    fn valid_metadata() -> PluginMetadataV1 {
        PluginMetadataV1 {
            plugin_id: PLUGIN_ID,
            primary_interface_id: INTERFACE_ID,
        }
    }

    struct Succeeds;

    impl ExtensionRegistrarImplementationV1 for Succeeds {
        fn create() -> Self {
            Self
        }
        fn register(
            &self,
            _: RegistrarRequestV1,
        ) -> explorer_extension_api::RegistrarOutputResultV1 {
            RResult::ROk(RegistrarOutputV1 {
                outcome: RegistrationOutcomeV1::accepted(2),
                contributions: abi_stable::std_types::RVec::new(),
            })
        }
    }

    struct ReturnsError;

    impl ExtensionRegistrarImplementationV1 for ReturnsError {
        fn create() -> Self {
            Self
        }
        fn register(
            &self,
            _: RegistrarRequestV1,
        ) -> explorer_extension_api::RegistrarOutputResultV1 {
            RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::REGISTRATION_REJECTED,
                INTERFACE_ID,
                7,
            ))
        }
    }

    struct Panics;

    impl ExtensionRegistrarImplementationV1 for Panics {
        fn create() -> Self {
            Self
        }
        fn register(
            &self,
            _: RegistrarRequestV1,
        ) -> explorer_extension_api::RegistrarOutputResultV1 {
            panic!("synthetic registrar panic");
        }
    }

    struct RejectedOutcome;

    impl ExtensionRegistrarImplementationV1 for RejectedOutcome {
        fn create() -> Self {
            Self
        }
        fn register(
            &self,
            _: RegistrarRequestV1,
        ) -> explorer_extension_api::RegistrarOutputResultV1 {
            RResult::ROk(RegistrarOutputV1 {
                outcome: RegistrationOutcomeV1 {
                    status: RegistrationStatusV1::REJECTED,
                    registered_interface_count: 0,
                },
                contributions: abi_stable::std_types::RVec::new(),
            })
        }
    }

    struct MalformedOutcome;

    impl ExtensionRegistrarImplementationV1 for MalformedOutcome {
        fn create() -> Self {
            Self
        }
        fn register(
            &self,
            _: RegistrarRequestV1,
        ) -> explorer_extension_api::RegistrarOutputResultV1 {
            RResult::ROk(RegistrarOutputV1 {
                outcome: RegistrationOutcomeV1 {
                    status: RegistrationStatusV1::from_raw(0),
                    registered_interface_count: 0,
                },
                contributions: abi_stable::std_types::RVec::new(),
            })
        }
    }

    struct UnknownOutcome;

    impl ExtensionRegistrarImplementationV1 for UnknownOutcome {
        fn create() -> Self {
            Self
        }
        fn register(
            &self,
            _: RegistrarRequestV1,
        ) -> explorer_extension_api::RegistrarOutputResultV1 {
            RResult::ROk(RegistrarOutputV1 {
                outcome: RegistrationOutcomeV1 {
                    status: RegistrationStatusV1::from_raw(99),
                    registered_interface_count: 0,
                },
                contributions: abi_stable::std_types::RVec::new(),
            })
        }
    }

    static SCHEMA_CALLBACK_CALLED: AtomicBool = AtomicBool::new(false);

    struct MarksSchemaCallback;

    impl ExtensionRegistrarImplementationV1 for MarksSchemaCallback {
        fn create() -> Self {
            Self
        }
        fn register(
            &self,
            _: RegistrarRequestV1,
        ) -> explorer_extension_api::RegistrarOutputResultV1 {
            SCHEMA_CALLBACK_CALLED.store(true, Ordering::SeqCst);
            RResult::ROk(RegistrarOutputV1 {
                outcome: RegistrationOutcomeV1::accepted(0),
                contributions: abi_stable::std_types::RVec::new(),
            })
        }
    }

    static SDK_CALLBACK_CALLED: AtomicBool = AtomicBool::new(false);

    struct MarksSdkCallback;

    impl ExtensionRegistrarImplementationV1 for MarksSdkCallback {
        fn create() -> Self {
            Self
        }
        fn register(
            &self,
            _: RegistrarRequestV1,
        ) -> explorer_extension_api::RegistrarOutputResultV1 {
            SDK_CALLBACK_CALLED.store(true, Ordering::SeqCst);
            RResult::ROk(RegistrarOutputV1 {
                outcome: RegistrationOutcomeV1::accepted(0),
                contributions: abi_stable::std_types::RVec::new(),
            })
        }
    }

    #[test]
    fn single_plugin_loader_rejects_relative_paths_before_mapping_a_dll() {
        let host = ExtensionHost::new();
        assert!(matches!(
            host.load_single_plugin_dll(std::path::Path::new("p0_consumer.dll")),
            Err(super::SinglePluginLoadErrorV1::PathMustBeAbsolute)
        ));
    }

    #[test]
    fn host_start_and_shutdown_transition_exactly_once() {
        let mut host = ExtensionHost::new();

        if matches!(
            host.start(),
            Err(super::ExtensionHostStartErrorV1::NativeLifecycle(
                NativeLifecycleErrorV1::AlreadyAcquired
            ))
        ) {
            // Native lifecycle unit tests intentionally prove the process-wide
            // owner is nonrenewable. This shared test binary may already have
            // claimed it; production has exactly one ExtensionHost root.
            return;
        }
        host.start().expect("start host idempotently");
        assert_eq!(host.state, LifecycleState::Running);
        assert!(host.startup_admissions().is_empty());
        assert!(host.local_developer_scratch_telemetry().is_none());

        host.shutdown();
        host.shutdown();
        host.start().expect("start host");
        assert_eq!(host.state, LifecycleState::Stopped);
        assert!(!host.is_running());
    }

    #[test]
    fn queued_archives_cannot_silently_enable_local_developer_mode() {
        let archive = PathBuf::from("untrusted.sepack");
        let config = ExtensionHostConfigV1::default().with_local_developer_archives([archive]);
        let mut host = ExtensionHost::with_config(config);

        assert!(matches!(
            host.start(),
            Err(super::ExtensionHostStartErrorV1::QueuedDeveloperArchivesRequireEnabledMode)
        ));
        assert!(!host.is_running());
    }

    fn resolver_candidate<'a>(
        id: &str,
        dependencies: impl IntoIterator<Item = (&'a str, bool)>,
    ) -> PackageValidationResultV1 {
        let manifest = PackageManifestV1::parse_json(
            &json!({
                "manifest_version": 1,
                "package": { "id": id, "version": "1.0.0" },
                "publisher": {
                    "id": "example.publisher", "display_name": "Example Publisher",
                    "contacts": [{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }]
                },
                "sdk": {
                    "bundle_id": "dev.20260802", "target": "x86_64-pc-windows-msvc",
                    "abi_schema": 1, "gpui": false, "ui_abi_fingerprint": null
                },
                "rust": [], "lua": [], "skins": [], "locales": [], "tools": [], "features": [],
                "dependencies": dependencies.into_iter().map(|(package_id, optional)| json!({
                    "package_id": package_id, "version_requirement": "=1.0.0", "optional": optional
                })).collect::<Vec<_>>(),
                "payloads": [], "signature": { "kind": "unsigned" }, "data_version": 1
            })
            .to_string(),
        )
        .expect("resolver fixture manifest");
        PackageValidationResultV1::for_resolver_test(manifest)
    }

    fn manifest_with_features_v1(id: &str, features: &[&str]) -> PackageManifestV1 {
        PackageManifestV1::parse_json(
            &json!({
                "manifest_version": 1,
                "package": { "id": id, "version": "1.0.0" },
                "publisher": {
                    "id": "example.publisher", "display_name": "Example Publisher",
                    "contacts": [{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }]
                },
                "sdk": {
                    "bundle_id": "dev.20260802", "target": "x86_64-pc-windows-msvc",
                    "abi_schema": 1, "gpui": false, "ui_abi_fingerprint": null
                },
                "rust": [], "lua": [], "skins": [], "locales": [], "tools": [],
                "features": features.iter().map(|id| json!({
                    "id": id, "capabilities": [], "dependencies": []
                })).collect::<Vec<_>>(),
                "dependencies": [], "payloads": [],
                "signature": { "kind": "unsigned" }, "data_version": 1
            })
            .to_string(),
        )
        .expect("feature-state fixture manifest")
    }

    #[test]
    fn restart_with_global_disable_preserves_child_override_and_denies_native_authority() {
        let temp = TempDir::new().expect("feature-state fixture");
        let path = temp.path().join("feature-state-v1.json");
        let manifest = manifest_with_features_v1("example.package", &["feature"]);
        let child = FeatureKeyV1::new("example.package", "feature").expect("feature key");
        let mut state = FeatureStateStoreV1::new();
        state
            .set_feature_desired(child.clone(), DesiredStateV1::Enabled)
            .expect("child override");
        state.set_global_desired(DesiredStateV1::Disabled);
        state.save_atomic(&path).expect("persist desired state");

        let mut restarted = super::load_persisted_feature_state_v1(&path).store;
        assert_eq!(restarted.feature_desired(&child), DesiredStateV1::Enabled);
        assert!(
            !super::package_may_reach_native_authority_v1(&restarted, &manifest)
                .expect("global desired resolution")
        );
        restarted.set_global_desired(DesiredStateV1::Enabled);
        assert!(
            super::package_may_reach_native_authority_v1(&restarted, &manifest)
                .expect("restored child desired resolution")
        );
    }

    #[test]
    fn restart_with_package_disable_denies_native_authority() {
        let temp = TempDir::new().expect("feature-state fixture");
        let path = temp.path().join("feature-state-v1.json");
        let manifest = manifest_with_features_v1("example.package", &["feature"]);
        let mut state = FeatureStateStoreV1::new();
        state
            .set_package_desired("example.package", DesiredStateV1::Disabled)
            .expect("package override");
        state.save_atomic(&path).expect("persist desired state");

        let restarted = super::load_persisted_feature_state_v1(&path).store;
        assert!(
            !super::package_may_reach_native_authority_v1(&restarted, &manifest)
                .expect("package desired resolution")
        );
    }

    #[test]
    fn restart_with_feature_disable_denies_native_authority() {
        let temp = TempDir::new().expect("feature-state fixture");
        let path = temp.path().join("feature-state-v1.json");
        let manifest = manifest_with_features_v1("example.package", &["feature"]);
        let mut state = FeatureStateStoreV1::new();
        state
            .set_feature_desired(
                FeatureKeyV1::new("example.package", "feature").expect("feature key"),
                DesiredStateV1::Disabled,
            )
            .expect("feature override");
        state.save_atomic(&path).expect("persist desired state");

        let restarted = super::load_persisted_feature_state_v1(&path).store;
        assert!(
            !super::package_may_reach_native_authority_v1(&restarted, &manifest)
                .expect("feature desired resolution")
        );
    }

    #[test]
    fn corrupt_persisted_feature_state_disables_all_extensions_without_aborting_startup() {
        let temp = TempDir::new().expect("feature-state fixture");
        let path = temp.path().join("feature-state-v1.json");
        fs::write(&path, b"not a feature-state document").expect("corrupt fixture");
        let loaded = super::load_persisted_feature_state_v1(&path);
        assert!(loaded.unavailable);
        assert_eq!(loaded.store.global_desired(), DesiredStateV1::Disabled);
    }

    #[test]
    fn host_start_with_corrupt_feature_state_keeps_safe_mode_lifecycle_available() {
        let temp = TempDir::new().expect("host state fixture");
        let state_root = temp.path().join("extension-state");
        fs::create_dir(&state_root).expect("host state root");
        fs::write(
            state_root.join(super::FEATURE_STATE_FILE_NAME_V1),
            b"corrupt",
        )
        .expect("corrupt state fixture");
        let config = ExtensionHostConfigV1::default().with_integration_test_state_root(state_root);
        let mut host = ExtensionHost::with_config(config);
        match host.start() {
            Ok(()) => {
                assert!(host.startup_admissions().is_empty());
                assert!(host.startup_diagnostics().iter().any(|diagnostic| {
                    diagnostic.code() == ExtensionStartupDiagnosticCodeV1::FeatureStateUnavailable
                }));
                let _incidents = host.safe_mode_incidents();
                host.shutdown();
            }
            Err(super::ExtensionHostStartErrorV1::NativeLifecycle(
                NativeLifecycleErrorV1::AlreadyAcquired,
            )) => {
                // Native lifecycle tests intentionally claim the process-wide
                // owner. Corrupt state still reached lifecycle acquisition.
            }
            Err(error) => panic!("corrupt state must not abort core startup: {error}"),
        }
    }

    #[test]
    fn cancelled_startup_local_developer_imports_never_reach_admission() {
        let temp = TempDir::new().expect("cancelled startup fixture");
        let cancellation = PackageValidationCancellationV1::new();
        cancellation.cancel();
        let config = ExtensionHostConfigV1::default()
            .with_local_developer_mode(LocalDeveloperModeV1::Enabled)
            .with_local_developer_archives([PathBuf::from("cancelled-before-open.sepack")])
            .with_local_developer_import_cancellation(cancellation)
            .with_integration_test_state_root(temp.path().join("state"))
            .with_integration_test_local_developer_root(temp.path().join("developer"));
        let mut host = ExtensionHost::with_config(config);
        match host.start() {
            Ok(()) => {
                assert!(host.startup_admissions().is_empty());
                assert!(host.startup_diagnostics().iter().any(|diagnostic| {
                    diagnostic.code()
                        == ExtensionStartupDiagnosticCodeV1::LocalDeveloperImportCancelled
                }));
                host.shutdown();
            }
            Err(super::ExtensionHostStartErrorV1::NativeLifecycle(
                NativeLifecycleErrorV1::AlreadyAcquired,
            )) => {
                // Import processing completed before the process-wide native
                // lifecycle acquisition, so cancellation was still observed.
            }
            Err(error) => panic!("cancelled developer import must not abort core startup: {error}"),
        }
    }

    #[test]
    fn authority_mismatch_blocks_malicious_package_and_dependents_but_continues_independent_package()
     {
        let candidates = vec![
            resolver_candidate("example.malicious", []),
            resolver_candidate("example.dependent", [("example.malicious", false)]),
            resolver_candidate("example.independent", []),
        ];
        let resolution = PackageResolverV1::resolve(&candidates);
        let mut diagnostics = Vec::new();
        let mut offered = Vec::new();
        let states = super::admit_in_dependency_order_v1(
            resolution.resolved_packages(),
            |resolved| -> Result<bool, NativeLifecycleErrorV1> {
                let id = resolved.manifest().package.id.as_str();
                offered.push(id.to_owned());
                if id == "example.malicious" {
                    super::record_package_native_admission_failure_v1(
                        NativeLifecycleErrorV1::ActivationAuthorityMismatch,
                        id,
                        &mut diagnostics,
                    )
                } else {
                    Ok(true)
                }
            },
        )
        .expect("package-origin failure is isolated");
        assert_eq!(offered, ["example.independent", "example.malicious"]);
        let states = resolution
            .resolved_packages()
            .iter()
            .zip(states)
            .map(|(resolved, state)| (resolved.manifest().package.id.as_str(), state))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            states["example.malicious"],
            super::DependencyAdmissionStateV1::Failed
        );
        assert_eq!(
            states["example.dependent"],
            super::DependencyAdmissionStateV1::BlockedByFailedDependency
        );
        assert_eq!(
            states["example.independent"],
            super::DependencyAdmissionStateV1::Admitted
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code(),
            ExtensionStartupDiagnosticCodeV1::NativeActivationRejected
        );
        assert_eq!(diagnostics[0].package_id(), Some("example.malicious"));
    }

    fn sha256_hex_v1(digest: &[u8; 32]) -> String {
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            write!(hex, "{byte:02x}").expect("writing into String cannot fail");
        }
        hex
    }

    fn write_signed_built_in_package_v1(root: &std::path::Path, key_id: &str) -> Ed25519KeyPair {
        let package_root = root.join("example.builtin");
        fs::create_dir_all(package_root.join("data")).expect("built-in test package directory");
        let payload = b"signed built-in payload";
        fs::write(package_root.join("data/payload.bin"), payload).expect("built-in payload");
        let digest: [u8; 32] = Sha256::digest(payload).into();
        let key_pair =
            Ed25519KeyPair::from_seed_unchecked(&[11_u8; 32]).expect("fixed test Ed25519 seed");
        let mut manifest = json!({
            "manifest_version": 1,
            "package": { "id": "example.builtin", "version": "1.0.0" },
            "publisher": {
                "id": "example.publisher", "display_name": "Example Publisher",
                "contacts": [{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }]
            },
            "sdk": {
                "bundle_id": "dev.20260802", "target": "x86_64-pc-windows-msvc",
                "abi_schema": 1, "gpui": false, "ui_abi_fingerprint": null
            },
            "rust": [], "lua": [], "skins": [], "locales": [], "tools": [], "features": [],
            "dependencies": [],
            "payloads": [{
                "path": "data/payload.bin", "size": payload.len(),
                "sha256": sha256_hex_v1(&digest), "kind": "data"
            }],
            "signature": { "kind": "ed25519", "key_id": key_id, "signature": "" },
            "data_version": 1
        });
        let parsed = PackageManifestV1::parse_json(&manifest.to_string())
            .expect("unsigned built-in manifest shape");
        let signature = STANDARD.encode(
            key_pair
                .sign(
                    &parsed
                        .canonical_ed25519_signing_bytes()
                        .expect("canonical built-in signing bytes"),
                )
                .as_ref(),
        );
        *manifest
            .pointer_mut("/signature/signature")
            .expect("built-in signature field") = json!(signature);
        fs::write(package_root.join("manifest.json"), manifest.to_string())
            .expect("built-in manifest");
        key_pair
    }

    fn release_bound_test_validator_v1(
        key_pair: &Ed25519KeyPair,
        sealed_root: &std::path::Path,
    ) -> PackageValidatorV1 {
        let roots = json!({
            "schema_version": 1,
            "sdk_bundle_id": super::package_validation::RELEASE_TRUST_ROOTS_BUNDLE_ID_V1,
            "keys": [{
                "key_id": "example.signing", "publisher_id": "example.publisher",
                "ed25519_public_key_base64": STANDARD.encode(key_pair.public_key().as_ref())
            }]
        });
        PackageValidatorV1::new(
            TrustedPublisherKeyStoreV1::from_release_artifact_v1(&roots.to_string())
                .expect("release-bound test trust root"),
            SealedPackageStoreV1::new(sealed_root).expect("sealed built-in store"),
        )
    }

    #[test]
    fn production_built_in_composition_accepts_signed_package_and_rejects_unknown_key() {
        let accepted = TempDir::new().expect("accepted built-in fixture");
        let accepted_root = accepted.path().join("built-in-v1");
        fs::create_dir(&accepted_root).expect("accepted source root");
        let key_pair = write_signed_built_in_package_v1(&accepted_root, "example.signing");
        let validator = release_bound_test_validator_v1(&key_pair, &accepted.path().join("sealed"));
        let mut diagnostics = Vec::new();
        let packages = super::discover_and_validate_built_in_packages_v1(
            accepted_root,
            &validator,
            &mut diagnostics,
        )
        .expect("built-in source discovery");
        assert_eq!(packages.len(), 1);
        assert!(diagnostics.is_empty());

        let rejected = TempDir::new().expect("rejected built-in fixture");
        let rejected_root = rejected.path().join("built-in-v1");
        fs::create_dir(&rejected_root).expect("rejected source root");
        let unknown_key = write_signed_built_in_package_v1(&rejected_root, "unknown.signing");
        let validator =
            release_bound_test_validator_v1(&unknown_key, &rejected.path().join("sealed"));
        let mut diagnostics = Vec::new();
        let packages = super::discover_and_validate_built_in_packages_v1(
            rejected_root,
            &validator,
            &mut diagnostics,
        )
        .expect("built-in source discovery remains available");
        assert!(packages.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code(),
            ExtensionStartupDiagnosticCodeV1::ValidationRejected
        );
    }

    #[test]
    fn required_dependency_admission_blocks_transitive_dependents_but_not_unrelated_packages() {
        // A requires B. When B's native admission fails, A is never offered to
        // a registrar; independent C remains eligible.
        let candidates = vec![
            resolver_candidate("example.a", [("example.b", false)]),
            resolver_candidate("example.b", []),
            resolver_candidate("example.c", []),
        ];
        let resolution = PackageResolverV1::resolve(&candidates);
        let mut offered = Vec::new();
        let states = super::admit_in_dependency_order_v1(
            resolution.resolved_packages(),
            |resolved| -> Result<bool, ()> {
                let id = resolved.manifest().package.id.as_str();
                offered.push(id.to_owned());
                Ok(id != "example.b")
            },
        )
        .expect("admission plan");
        let state_by_id = resolution
            .resolved_packages()
            .iter()
            .zip(states)
            .map(|(resolved, state)| (resolved.manifest().package.id.as_str(), state))
            .collect::<std::collections::BTreeMap<_, _>>();

        assert_eq!(
            state_by_id["example.a"],
            super::DependencyAdmissionStateV1::BlockedByFailedDependency
        );
        assert_eq!(
            state_by_id["example.b"],
            super::DependencyAdmissionStateV1::Failed
        );
        assert_eq!(
            state_by_id["example.c"],
            super::DependencyAdmissionStateV1::Admitted
        );
        assert_eq!(offered, ["example.b", "example.c"]);
    }

    #[test]
    fn incompatible_schema_is_rejected_before_registrar_callback() {
        SCHEMA_CALLBACK_CALLED.store(false, Ordering::SeqCst);
        let host = ExtensionHost::new();
        let invalid_schema = explorer_extension_api::AbiSchemaIdV1::new(0x5345, 3);
        let result = host.register_root_for_test(root::<MarksSchemaCallback>(
            invalid_schema,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));

        assert!(matches!(
            result,
            Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1 {
                code: AbiErrorCodeV1::SCHEMA_MISMATCH,
                ..
            }))
        ));
        assert!(!SCHEMA_CALLBACK_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn incompatible_semantic_id_is_rejected_before_registrar_callback() {
        let host = ExtensionHost::new();
        let result = host.register_root_for_test(root::<Succeeds>(
            ABI_SCHEMA_V1,
            StableIdV1::new(super::extension_id_namespace_v1(), 99),
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));

        assert!(matches!(
            result,
            Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1 {
                code: AbiErrorCodeV1::UNSUPPORTED_ID,
                ..
            }))
        ));
    }

    #[test]
    fn incompatible_sdk_major_is_rejected_before_registrar_callback() {
        SDK_CALLBACK_CALLED.store(false, Ordering::SeqCst);
        let host = ExtensionHost::new();
        let result = host.register_root_for_test(root::<MarksSdkCallback>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1 + 1,
            valid_metadata(),
        ));

        assert!(matches!(
            result,
            Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1 {
                code: AbiErrorCodeV1::SDK_MAJOR_MISMATCH,
                ..
            }))
        ));
        assert!(!SDK_CALLBACK_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn nonzero_root_padding_is_rejected_before_registrar_callback() {
        SCHEMA_CALLBACK_CALLED.store(false, Ordering::SeqCst);
        let result =
            ExtensionHost::new().register_root_for_test(root_with_layout::<MarksSchemaCallback>(
                ABI_SCHEMA_V1,
                ROOT_MODULE_CONTRACT_ID_V1,
                SDK_MAJOR_VERSION_V1,
                1,
                DESCRIPTOR_CONTRACT_REVISION_V1,
                valid_metadata(),
            ));
        assert!(matches!(
            result,
            Err(HostRegistrationErrorV1::Incompatible(_))
        ));
        assert!(!SCHEMA_CALLBACK_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn wrong_descriptor_contract_revision_is_rejected_before_registrar_callback() {
        SCHEMA_CALLBACK_CALLED.store(false, Ordering::SeqCst);
        let result =
            ExtensionHost::new().register_root_for_test(root_with_layout::<MarksSchemaCallback>(
                ABI_SCHEMA_V1,
                ROOT_MODULE_CONTRACT_ID_V1,
                SDK_MAJOR_VERSION_V1,
                0,
                DESCRIPTOR_CONTRACT_REVISION_V1 + 1,
                valid_metadata(),
            ));
        assert!(matches!(
            result,
            Err(HostRegistrationErrorV1::Incompatible(_))
        ));
        assert!(!SCHEMA_CALLBACK_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn registrar_typed_error_and_panic_are_translated_at_boundary() {
        let host = ExtensionHost::new();
        let typed_error = host.register_root_for_test(root::<ReturnsError>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));
        let panic_error = host.register_root_for_test(root::<Panics>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));

        assert!(matches!(
            typed_error,
            Err(HostRegistrationErrorV1::Plugin(AbiErrorV1 {
                code: AbiErrorCodeV1::REGISTRATION_REJECTED,
                ..
            }))
        ));
        assert!(matches!(
            panic_error,
            Err(HostRegistrationErrorV1::Panicked(AbiErrorV1 {
                code: AbiErrorCodeV1::CALLBACK_PANICKED,
                ..
            }))
        ));
    }

    #[test]
    fn rejected_malformed_and_unknown_outcomes_are_not_successes() {
        let host = ExtensionHost::new();
        let rejected = host.register_root_for_test(root::<RejectedOutcome>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));
        let malformed = host.register_root_for_test(root::<MalformedOutcome>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));
        let unknown = host.register_root_for_test(root::<UnknownOutcome>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));

        assert!(matches!(
            rejected,
            Err(HostRegistrationErrorV1::Plugin(AbiErrorV1 {
                code: AbiErrorCodeV1::REGISTRATION_OUTCOME_REJECTED,
                detail: 2,
                ..
            }))
        ));
        assert!(matches!(
            malformed,
            Err(HostRegistrationErrorV1::Plugin(AbiErrorV1 {
                code: AbiErrorCodeV1::MALFORMED_REGISTRATION_OUTCOME,
                detail: 0,
                ..
            }))
        ));
        assert!(matches!(
            unknown,
            Err(HostRegistrationErrorV1::Plugin(AbiErrorV1 {
                code: AbiErrorCodeV1::UNKNOWN_REGISTRATION_OUTCOME,
                detail: 99,
                ..
            }))
        ));
    }

    #[test]
    fn valid_root_dispatches_only_after_all_data_validation_passes() {
        let host = ExtensionHost::new();
        let outcome = host.register_root_for_test(root::<Succeeds>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));

        assert_eq!(outcome, Ok(RegistrationOutcomeV1::accepted(2)));
    }
}
