//! Production process composition root.

use std::{
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context as _, Error};
use explorer_common::{DiagnosticsSession, ErrorSeverity, RoadmapLimits};
use explorer_model::SessionStore as _;
use explorer_shell_win::ShellStaHandle;
use explorer_ui::{
    ExplorerRoot, UiTokens, initial_window_options, window_options_with_placement,
    window_options_with_size,
};
use gpui::AppContext as _;

use crate::windows_prerequisites::initialize_dpi_awareness;
use crate::{
    automation_service::AutomationComposition, system_theme::high_contrast_tokens,
    visual_fixture::VisualFixtureConfig,
};

const SHELL_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

const FOLDER_SIZE_CONTRIBUTION_ID_V1: &str = "folder-size";
const FOLDER_SIZE_RENDERER_CONTRIBUTION_ID_V1: &str = "folder-size-renderer";

/// App-owned boundary for projecting runtime-ready extension batches into the
/// current list model. The application, rather than the host transport, owns
/// stable item identities and therefore is the only layer allowed to drain,
/// apply, and acknowledge ready work.
trait ApplicationExtensionReadyProjectorV1 {
    fn project_ready(
        &mut self,
        pump: &mut explorer_extension_host::ExtensionJobUiPumpV1,
        runtime: &Arc<explorer_extension_host::ExtensionJobRuntimeV1>,
        ingress: &explorer_extension_host::ExtensionJobUiIngressV1,
    ) -> Result<usize, explorer_extension_host::ExtensionJobUiPumpErrorV1>;
}

/// Deliberately preserves ready work until the dynamic-column model installs
/// its identity-aware projector. It must not consume a signal merely to make
/// an incomplete composition path appear live.
struct DeferredApplicationExtensionReadyProjectorV1;

impl ApplicationExtensionReadyProjectorV1 for DeferredApplicationExtensionReadyProjectorV1 {
    fn project_ready(
        &mut self,
        _pump: &mut explorer_extension_host::ExtensionJobUiPumpV1,
        _runtime: &Arc<explorer_extension_host::ExtensionJobRuntimeV1>,
        _ingress: &explorer_extension_host::ExtensionJobUiIngressV1,
    ) -> Result<usize, explorer_extension_host::ExtensionJobUiPumpErrorV1> {
        Ok(0)
    }
}

/// GPUI-thread composition of the host's unique UI inbox and its !Send
/// invalidation batcher. The UI crate sees only the host-agnostic poll trait;
/// this app layer additionally owns the ready-projector callback.
struct ApplicationExtensionUiPumpV1 {
    pump: explorer_extension_host::ExtensionJobUiPumpV1,
    runtime: Arc<explorer_extension_host::ExtensionJobRuntimeV1>,
    ingress: explorer_extension_host::ExtensionJobUiIngressV1,
    ready_projector: Box<dyn ApplicationExtensionReadyProjectorV1>,
}

impl ApplicationExtensionUiPumpV1 {
    fn new(
        inbox: explorer_extension_host::ExtensionJobUiInboxV1,
        ingress: explorer_extension_host::ExtensionJobUiIngressV1,
    ) -> Option<Self> {
        Self::with_ready_projector(
            inbox,
            ingress,
            Box::new(DeferredApplicationExtensionReadyProjectorV1),
        )
    }

    fn with_ready_projector(
        inbox: explorer_extension_host::ExtensionJobUiInboxV1,
        ingress: explorer_extension_host::ExtensionJobUiIngressV1,
        ready_projector: Box<dyn ApplicationExtensionReadyProjectorV1>,
    ) -> Option<Self> {
        if !ingress.is_for_runtime(inbox.runtime()) {
            return None;
        }
        let config = explorer_extension_host::UiInvalidationBatcherConfigV1::try_new(
            Duration::from_millis(20),
            explorer_extension_host::MAX_UI_INVALIDATION_SCOPES_V1,
        )
        .ok()?;
        let runtime = Arc::clone(inbox.runtime());
        Some(Self {
            pump: explorer_extension_host::ExtensionJobUiPumpV1::new(inbox, config),
            runtime,
            ingress,
            ready_projector,
        })
    }

    #[cfg(test)]
    fn set_ready_projector(
        &mut self,
        ready_projector: Box<dyn ApplicationExtensionReadyProjectorV1>,
    ) {
        self.ready_projector = ready_projector;
    }
}

impl explorer_ui::ExtensionUiPumpPortV1 for ApplicationExtensionUiPumpV1 {
    fn poll_due(&mut self, now: Instant) -> bool {
        // The current app has no dynamic-column identity model yet. Its
        // deferred projector leaves ready signals intact; task 5 installs the
        // concrete callback that drains, atomically applies, and then notifies
        // through this same app-owned seam.
        let _ = self
            .ready_projector
            .project_ready(&mut self.pump, &self.runtime, &self.ingress);
        let _ = self.pump.poll_applied(1_024);
        let _ = self.pump.next_deadline();
        matches!(self.pump.drain_due(now), Ok(Some(_)))
    }
}

/// One-process bridge for the single P0 folder-size example. The measure
/// object lives exclusively on its worker thread; the renderer is serialized
/// on the GPUI caller thread through this narrow host-owned mutex.
struct ApplicationVisualColumnRuntimeV1 {
    pending: Arc<(Mutex<PendingFolderSizeWorkV1>, Condvar)>,
    request_epoch: Arc<AtomicU64>,
    results: Mutex<mpsc::Receiver<explorer_ui::folder_size_column::FolderSizeResultV1>>,
    renderer: Mutex<explorer_extension_host::SinglePluginVisualRenderRuntimeV1>,
}

#[derive(Default)]
struct PendingFolderSizeWorkV1 {
    requests: Option<Vec<explorer_ui::folder_size_column::FolderSizeRequestV1>>,
    stopped: bool,
}

impl ApplicationVisualColumnRuntimeV1 {
    fn start(
        mut measure: explorer_extension_host::SinglePluginVisualMeasureRuntimeV1,
        renderer: explorer_extension_host::SinglePluginVisualRenderRuntimeV1,
    ) -> Result<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1, Error> {
        let pending = Arc::new((
            Mutex::new(PendingFolderSizeWorkV1::default()),
            Condvar::new(),
        ));
        let worker_pending = pending.clone();
        let request_epoch = Arc::new(AtomicU64::new(0));
        let worker_epoch = request_epoch.clone();
        let (result_tx, result_rx) =
            mpsc::sync_channel::<explorer_ui::folder_size_column::FolderSizeResultV1>(1_024);
        std::thread::Builder::new()
            .name("p0-folder-size".to_owned())
            .spawn(move || {
                loop {
                    let requests = {
                        let (lock, ready) = &*worker_pending;
                        let mut state = lock
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        while state.requests.is_none() && !state.stopped {
                            state = ready
                                .wait(state)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                        if state.stopped {
                            return;
                        }
                        state.requests.take().unwrap_or_default()
                    };
                    let batch_epoch = worker_epoch.load(Ordering::Acquire);
                    let batch_started = Instant::now();
                    for request in requests {
                        if worker_epoch.load(Ordering::Acquire) != batch_epoch {
                            break;
                        }
                        let Some(remaining) = Duration::from_secs(2)
                            .checked_sub(batch_started.elapsed())
                        else {
                            break;
                        };
                        let measured = measure.measure_folder_size(
                            FOLDER_SIZE_CONTRIBUTION_ID_V1,
                            explorer_extension_ui_api::FolderSizeMeasureRequestV1 {
                                filesystem_path: request.path.to_string_lossy().into_owned().into(),
                                max_entries: 100_000,
                                max_depth: 128,
                                deadline_millis: u32::try_from(remaining.as_millis())
                                    .unwrap_or(u32::MAX)
                                    .max(1),
                            },
                        );
                        if worker_epoch.load(Ordering::Acquire) != batch_epoch {
                            break;
                        }
                        let (exact_bytes, partial, error) = match measured {
                            Ok(result) => (
                                (!result.partial).then_some(result.exact_bytes),
                                result.partial,
                                result.error.into_option().map(String::from),
                            ),
                            Err(error) => (None, true, Some(error.to_string())),
                        };
                        if result_tx
                            .try_send(explorer_ui::folder_size_column::FolderSizeResultV1 {
                                context: request.context,
                                item_id: request.item_id,
                                exact_bytes,
                                partial,
                                error,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            })
            .context("failed to start P0 folder-size worker")?;
        Ok(Arc::new(Self {
            pending,
            request_epoch,
            results: Mutex::new(result_rx),
            renderer: Mutex::new(renderer),
        }))
    }
}

impl explorer_ui::folder_size_column::VisualColumnRuntimePortV1
    for ApplicationVisualColumnRuntimeV1
{
    fn config(&self) -> explorer_ui::folder_size_column::VisualColumnConfigV1 {
        explorer_ui::folder_size_column::VisualColumnConfigV1::default()
    }

    fn submit_folder_size_requests(
        &self,
        requests: Vec<explorer_ui::folder_size_column::FolderSizeRequestV1>,
    ) {
        self.request_epoch.fetch_add(1, Ordering::AcqRel);
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.requests = Some(requests);
        ready.notify_one();
    }

    fn drain_folder_size_results(
        &self,
    ) -> Vec<explorer_ui::folder_size_column::FolderSizeResultV1> {
        let Ok(results) = self.results.lock() else {
            return Vec::new();
        };
        results.try_iter().collect()
    }

    fn render_cell(
        &self,
        context: explorer_extension_ui_api::CellRenderContextV1,
    ) -> explorer_extension_ui_api::CellRenderPlanV1 {
        let fallback_theme = context.theme;
        self.renderer
            .lock()
            .ok()
            .and_then(|mut renderer| {
                renderer
                    .render(FOLDER_SIZE_RENDERER_CONTRIBUTION_ID_V1, context)
                    .ok()
            })
            .unwrap_or_else(|| {
                explorer_extension_ui_api::CellRenderPlanV1::text_only(
                    "Folder size unavailable",
                    fallback_theme.muted_foreground,
                )
            })
    }
}

impl Drop for ApplicationVisualColumnRuntimeV1 {
    fn drop(&mut self) {
        self.request_epoch.fetch_add(1, Ordering::AcqRel);
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.stopped = true;
        state.requests = None;
        ready.notify_one();
    }
}

#[cfg(feature = "uitest-support")]
const UITEST_EXTENSION_STATE_ROOT_ENV_V1: &str = "EXPLORER_UITEST_EXTENSION_STATE_ROOT";

#[cfg(feature = "uitest-support")]
const UITEST_SAFE_MODE_PROBE_FILE_V1: &str = "safe-mode-probe-v1.json";

/// Returns the test-owned extension state root only in a binary compiled with
/// the non-default UITEST feature. Production binaries never inspect this
/// environment variable and always use the host's Windows Known Folder root.
#[cfg(feature = "uitest-support")]
fn uitest_extension_state_root_v1() -> Result<Option<PathBuf>, Error> {
    let Some(root) = std::env::var_os(UITEST_EXTENSION_STATE_ROOT_ENV_V1) else {
        return Ok(None);
    };
    let root = PathBuf::from(root);
    if !root.is_dir() {
        anyhow::bail!("UITEST extension state root must be an existing directory");
    }
    root.canonicalize()
        .map(Some)
        .context("failed to canonicalize UITEST extension state root")
}

#[cfg(not(feature = "uitest-support"))]
fn uitest_extension_state_root_v1() -> Result<Option<PathBuf>, Error> {
    Ok(None)
}

#[cfg(feature = "uitest-support")]
fn write_uitest_safe_mode_probe_v1(
    state_root: &std::path::Path,
    recovered_callback_denied: bool,
) -> Result<(), Error> {
    let bytes = if recovered_callback_denied {
        b"{\"schema_version\":1,\"recovered_callback_denied\":true}".as_slice()
    } else {
        b"{\"schema_version\":1,\"recovered_callback_denied\":false}".as_slice()
    };
    let temporary = state_root.join("safe-mode-probe-v1.tmp");
    let destination = state_root.join(UITEST_SAFE_MODE_PROBE_FILE_V1);
    std::fs::write(&temporary, bytes).context("failed to write UITEST Safe Mode probe")?;
    std::fs::rename(&temporary, &destination)
        .context("failed to publish UITEST Safe Mode probe")?;
    Ok(())
}

/// Path-free suspect identity presented by the application Safe Mode offer.
///
/// Every string originates from the host's recovered marker validation, which
/// permits only bounded package/entrypoint/root identity components and a
/// lowercase manifest digest. Filesystem locations and marker paths never
/// reach this application-facing value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeModeSuspectV1 {
    package_id: String,
    sealed_manifest_digest: String,
    entrypoint_id: String,
    root_module_id: String,
    primary_interface_namespace: u32,
    primary_interface_value: u64,
}

impl SafeModeSuspectV1 {
    #[must_use]
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    #[must_use]
    pub fn sealed_manifest_digest(&self) -> &str {
        &self.sealed_manifest_digest
    }

    #[must_use]
    pub fn entrypoint_id(&self) -> &str {
        &self.entrypoint_id
    }

    #[must_use]
    pub fn root_module_id(&self) -> &str {
        &self.root_module_id
    }

    #[must_use]
    pub const fn primary_interface_namespace(&self) -> u32 {
        self.primary_interface_namespace
    }

    #[must_use]
    pub const fn primary_interface_value(&self) -> u64 {
        self.primary_interface_value
    }
}

/// An explicit, path-free Safe Mode confirmation offer owned by application
/// startup. Its opaque ID can only be sent back to the resident extension host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeModeIncidentOfferV1<IncidentId> {
    incident_id: IncidentId,
    presentation_token: u64,
    kind: explorer_extension_host::NativeSafeModeIncidentKindV1,
    suspect: Option<SafeModeSuspectV1>,
}

impl<IncidentId: Copy> SafeModeIncidentOfferV1<IncidentId> {
    #[must_use]
    pub const fn incident_id(&self) -> IncidentId {
        self.incident_id
    }

    /// Returns the lifecycle-local opaque token used by a UI presenter.
    #[must_use]
    pub const fn presentation_token(&self) -> u64 {
        self.presentation_token
    }

    #[must_use]
    pub const fn kind(&self) -> explorer_extension_host::NativeSafeModeIncidentKindV1 {
        self.kind
    }

    #[must_use]
    pub const fn suspect(&self) -> Option<&SafeModeSuspectV1> {
        self.suspect.as_ref()
    }
}

/// Application's concrete Safe Mode offer, keyed by the host-owned opaque ID.
pub type SafeModeIncidentOffer =
    SafeModeIncidentOfferV1<explorer_extension_host::NativeSafeModeIncidentIdV1>;

trait SafeModeIncidentPortV1 {
    type IncidentId: Copy + Eq;
    type Error;

    fn offers(&self) -> Vec<SafeModeIncidentOfferV1<Self::IncidentId>>;
    fn denies_native_callbacks(&self) -> bool;
    fn confirm(&self, incident_id: Self::IncidentId) -> Result<(), Self::Error>;
}

impl SafeModeIncidentPortV1 for explorer_extension_host::ExtensionHost {
    type IncidentId = explorer_extension_host::NativeSafeModeIncidentIdV1;
    type Error = explorer_extension_host::NativeLifecycleErrorV1;

    fn offers(&self) -> Vec<SafeModeIncidentOffer> {
        self.safe_mode_incidents()
            .into_iter()
            .enumerate()
            .map(|(index, incident)| match incident {
                explorer_extension_host::NativeSafeModeIncidentV1::RegistrarInProgress {
                    incident_id,
                    package_id,
                    sealed_manifest_digest,
                    entrypoint_id,
                    root_module_id,
                    primary_interface_namespace,
                    primary_interface_value,
                    ..
                } => SafeModeIncidentOfferV1 {
                    incident_id,
                    presentation_token: index as u64 + 1,
                    kind:
                        explorer_extension_host::NativeSafeModeIncidentKindV1::RegistrarInProgress,
                    suspect: Some(SafeModeSuspectV1 {
                        package_id,
                        sealed_manifest_digest,
                        entrypoint_id,
                        root_module_id,
                        primary_interface_namespace,
                        primary_interface_value,
                    }),
                },
                explorer_extension_host::NativeSafeModeIncidentV1::UnsafeMarkerState {
                    incident_id,
                } => SafeModeIncidentOfferV1 {
                    incident_id,
                    presentation_token: index as u64 + 1,
                    kind: explorer_extension_host::NativeSafeModeIncidentKindV1::UnsafeMarkerState,
                    suspect: None,
                },
            })
            .collect()
    }

    fn denies_native_callbacks(&self) -> bool {
        self.safe_mode_denies_all()
    }

    fn confirm(
        &self,
        incident_id: explorer_extension_host::NativeSafeModeIncidentIdV1,
    ) -> Result<(), explorer_extension_host::NativeLifecycleErrorV1> {
        self.confirm_safe_mode_incident(incident_id)
    }
}

fn confirm_offered_safe_mode_incident_v1<P: SafeModeIncidentPortV1>(
    port: &P,
    offers: &mut Vec<SafeModeIncidentOfferV1<P::IncidentId>>,
    incident_id: P::IncidentId,
) -> Result<bool, P::Error> {
    if !offers.iter().any(|offer| offer.incident_id == incident_id) {
        return Ok(false);
    }
    port.confirm(incident_id)?;
    offers.retain(|offer| offer.incident_id != incident_id);
    Ok(true)
}

fn confirm_presented_safe_mode_incident_v1<P: SafeModeIncidentPortV1>(
    port: &P,
    offers: &mut Vec<SafeModeIncidentOfferV1<P::IncidentId>>,
    presentation_token: u64,
) -> Result<bool, P::Error> {
    let Some(incident_id) = offers
        .iter()
        .find(|offer| offer.presentation_token() == presentation_token)
        .map(SafeModeIncidentOfferV1::incident_id)
    else {
        return Ok(false);
    };
    confirm_offered_safe_mode_incident_v1(port, offers, incident_id)
}

fn emit_post_commit_safe_mode_telemetry_v1<E>(emit: impl FnOnce() -> Result<(), E>) {
    let _ = emit();
}

fn schedule_visual_diagnostics(
    window: &mut gpui::Window,
    fixture: VisualFixtureConfig,
    tokens: UiTokens,
    diagnostics: DiagnosticsSession,
    remaining_frames: u8,
) {
    window.on_next_frame(move |window, cx| {
        if remaining_frames > 1 {
            schedule_visual_diagnostics(window, fixture, tokens, diagnostics, remaining_frames - 1);
            return;
        }
        let actual_scale = window.scale_factor().to_string();
        let regions = cx
            .global::<explorer_ui::diagnostics::RegionDiagnosticsRecorder>()
            .snapshot(window.scale_factor());
        match fixture.write_diagnostics(window, tokens, &regions) {
            Ok(()) => {
                let _ = diagnostics.record_event(
                    "visual_fixture_ready",
                    &[
                        ("theme", fixture.theme.name()),
                        (
                            "expected_dpi_percent",
                            &fixture.expected_dpi_percent.to_string(),
                        ),
                        ("actual_scale_factor", &actual_scale),
                        ("font", &fixture.font),
                        ("state", &fixture.placeholder_state),
                    ],
                );
            }
            Err(error) => {
                tracing::error!(%error, "visual fixture diagnostics failed");
                diagnostics.record_error(
                    ErrorSeverity::Error,
                    "application",
                    "write_visual_fixture_diagnostics",
                    error.as_ref(),
                    Some(file!()),
                );
                let _ = diagnostics
                    .record_event("visual_fixture_failed", &[("error", &error.to_string())]);
            }
        }
    });
}

/// Owns all process-wide resources around the blocking GPUI event loop.
pub struct ApplicationLifecycle {
    resources: Arc<Mutex<ShutdownResources>>,
}

struct ShutdownResources {
    diagnostics: DiagnosticsSession,
    automation: Option<AutomationComposition>,
    extension_host: Option<explorer_extension_host::ExtensionHost>,
    loaded_extension_summary: Option<String>,
    visual_column_runtime: Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>,
    extension_job_ui_inbox: Option<explorer_extension_host::ExtensionJobUiInboxV1>,
    extension_job_ui_ingress: Option<explorer_extension_host::ExtensionJobUiIngressV1>,
    safe_mode_incident_offers: Vec<SafeModeIncidentOffer>,
    broker_warmup: Option<std::thread::JoinHandle<()>>,
    broker: Option<explorer_extension_broker::BrokerClient>,
    shell_sta: Option<Arc<ShellStaHandle>>,
    shutdown: bool,
}

impl ApplicationLifecycle {
    /// Applies Windows prerequisites and starts the sole Shell STA.
    ///
    /// # Errors
    ///
    /// Returns DPI, Shell initialization, or diagnostic write failures without starting GPUI.
    pub fn start(diagnostics: DiagnosticsSession) -> Result<Self, Error> {
        Self::start_with_plugin(diagnostics, None)
    }

    /// Starts the application and, when supplied, loads one development plugin DLL.
    ///
    /// # Errors
    ///
    /// Returns prerequisite, host startup, DLL loading, or diagnostic failures.
    pub fn start_with_plugin(
        diagnostics: DiagnosticsSession,
        plugin_dll: Option<&std::path::Path>,
    ) -> Result<Self, Error> {
        let dpi_outcome = initialize_dpi_awareness()?;
        let dpi_outcome_text = format!("{dpi_outcome:?}");
        diagnostics.record_event("windows_prerequisites_ready", &[("dpi", &dpi_outcome_text)])?;
        let shell_sta = Arc::new(ShellStaHandle::start()?);
        diagnostics.record_event("shell_sta_ready", &[])?;
        let automation = AutomationComposition::start()?;
        let script_count = automation.snapshots()?.len().to_string();
        diagnostics.record_event("automation_ready", &[("scripts", &script_count)])?;
        let _uitest_state_root = uitest_extension_state_root_v1()?;
        #[cfg(feature = "uitest-support")]
        let extension_config = _uitest_state_root.as_ref().map_or_else(
            explorer_extension_host::ExtensionHostConfigV1::default,
            |state_root| {
                explorer_extension_host::ExtensionHostConfigV1::default()
                    .with_integration_test_state_root(state_root.clone())
            },
        );
        #[cfg(not(feature = "uitest-support"))]
        let extension_config = explorer_extension_host::ExtensionHostConfigV1::default();
        let mut extension_host =
            explorer_extension_host::ExtensionHost::with_config(extension_config);
        extension_host.start()?;
        let (loaded_extension_summary, visual_column_runtime) = if let Some(path) = plugin_dll {
            let loaded =
                explorer_extension_host::ExtensionHost::load_single_plugin_visual_column_runtime(
                    path,
                )?;
            let (summary, measure, renderer) = loaded.into_parts();
            let supports_folder_size = summary.contributions().iter().any(|contribution| {
                contribution.contribution_id() == FOLDER_SIZE_CONTRIBUTION_ID_V1
            }) && summary.contributions().iter().any(|contribution| {
                contribution.contribution_id() == FOLDER_SIZE_RENDERER_CONTRIBUTION_ID_V1
            });
            let runtime = if supports_folder_size {
                Some(ApplicationVisualColumnRuntimeV1::start(measure, renderer)?)
            } else {
                None
            };
            (Some(format_single_plugin_summary(path, &summary)), runtime)
        } else {
            (None, None)
        };
        if let Some(summary) = loaded_extension_summary.as_deref() {
            diagnostics.record_event("development_plugin_loaded", &[("summary", summary)])?;
        }
        let extension_job_ui_ingress = extension_host.extension_job_ui_ingress();
        let extension_job_ui_inbox = extension_host.take_extension_job_ui_inbox();
        let safe_mode_incident_offers = extension_host.offers();
        let safe_mode_denies_native_callbacks = extension_host.denies_native_callbacks();
        #[cfg(feature = "uitest-support")]
        if let Some(state_root) = _uitest_state_root.as_deref() {
            write_uitest_safe_mode_probe_v1(
                state_root,
                extension_host.integration_test_recovered_callback_is_denied(),
            )?;
        }
        if !safe_mode_incident_offers.is_empty() || safe_mode_denies_native_callbacks {
            diagnostics.record_event(
                "extension_safe_mode_offer_ready",
                &[
                    ("incidents", &safe_mode_incident_offers.len().to_string()),
                    (
                        "native_callbacks_denied",
                        &safe_mode_denies_native_callbacks.to_string(),
                    ),
                ],
            )?;
        }
        diagnostics.record_event("extension_host_ready", &[])?;
        let broker = std::env::current_exe().ok().map(|application| {
            explorer_extension_broker::BrokerClient::adjacent_to(
                &application,
                explorer_extension_broker::BrokerPolicy::default(),
            )
        });
        match broker.as_ref() {
            Some(client) if client.is_available() => {
                diagnostics.record_event("extension_broker_configured", &[])?;
            }
            Some(_) => diagnostics.record_event(
                "extension_broker_unavailable",
                &[("reason", "adjacent broker executable is unavailable")],
            )?,
            None => diagnostics.record_event(
                "extension_broker_unavailable",
                &[("reason", "application executable location unavailable")],
            )?,
        }
        let broker_warmup = broker
            .as_ref()
            .filter(|client| client.is_available())
            .and_then(|client| {
                let client = client.clone();
                let diagnostics = diagnostics.clone();
                std::thread::Builder::new()
                    .name("extension-broker-warmup".to_owned())
                    .spawn(move || {
                        let verified_health = broker_ui_health(&client);
                        let health = format!("{verified_health:?}");
                        let snapshot = client.lifecycle_snapshot();
                        let generation = snapshot.generation.to_string();
                        let broker_pid = snapshot.broker_pid.unwrap_or_default().to_string();
                        let _ = diagnostics.record_event(
                            "extension_broker_warmup_finished",
                            &[
                                ("health", &health),
                                ("generation", &generation),
                                ("broker_pid", &broker_pid),
                            ],
                        );
                        if verified_health == explorer_ui::state::BrokerUiHealth::Healthy {
                            let _ = diagnostics.record_event(
                                "extension_broker_ready",
                                &[("generation", &generation), ("broker_pid", &broker_pid)],
                            );
                        }
                    })
                    .map_err(|error| {
                        tracing::warn!(%error, "failed to start extension broker warmup");
                        error
                    })
                    .ok()
            });
        Ok(Self {
            resources: Arc::new(Mutex::new(ShutdownResources {
                diagnostics,
                automation: Some(automation),
                extension_host: Some(extension_host),
                loaded_extension_summary,
                visual_column_runtime,
                extension_job_ui_inbox,
                extension_job_ui_ingress,
                safe_mode_incident_offers,
                broker_warmup,
                broker,
                shell_sta: Some(shell_sta),
                shutdown: false,
            })),
        })
    }

    fn take_extension_job_ui_bridge(
        &self,
    ) -> Result<
        Option<(
            explorer_extension_host::ExtensionJobUiInboxV1,
            explorer_extension_host::ExtensionJobUiIngressV1,
        )>,
        Error,
    > {
        self.resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
            .map(|mut resources| {
                resources
                    .extension_job_ui_inbox
                    .take()
                    .zip(resources.extension_job_ui_ingress.take())
            })
    }

    /// Runs GPUI until the final window closes or a test harness requests quit.
    ///
    /// # Errors
    ///
    /// Returns a synchronized launch error if GPUI cannot create the initial window.
    #[allow(
        clippy::too_many_lines,
        reason = "application startup keeps platform, lifecycle, window, fixture, and auto-close ownership visible in one audited path"
    )]
    pub fn run_gpui(&self) -> Result<(), Error> {
        let launch_error = Arc::new(Mutex::new(None::<String>));
        let mut extension_job_ui_bridge = self.take_extension_job_ui_bridge()?;
        let closure_error = Arc::clone(&launch_error);
        let diagnostics = self.diagnostics()?;
        let diagnostics_after_run = diagnostics.clone();
        let shell_sta = self.shell_service()?;
        let broker_client = self.broker_client()?;
        let broker_health = configured_broker_ui_health(&broker_client);
        let retry_client = broker_client.clone();
        let broker_retry: explorer_ui::BrokerRetryObserver =
            Arc::new(move || broker_ui_health(&retry_client));
        let shell_service: Arc<dyn explorer_model::ExplorerService> =
            Arc::new(crate::brokered_service::BrokeredExplorerService::new(
                Arc::clone(&shell_sta),
                broker_client,
            ));
        let shutdown_resources = Arc::clone(&self.resources);
        let folder_scripts = self.automation_handle()?;
        let safe_mode_offers = self.safe_mode_ui_offers()?;
        let loaded_extension_summary = self.loaded_extension_summary()?;
        let visual_column_runtime = self.visual_column_runtime()?;
        let safe_mode_resources = Arc::clone(&self.resources);
        let safe_mode_confirm: explorer_ui::SafeModeConfirmObserverV1 = Arc::new(move |token| {
            ApplicationLifecycle::confirm_safe_mode_incident_for_presentation_token(
                &safe_mode_resources,
                token,
            )
            .map_err(|error| error.to_string())
            .and_then(|confirmed| {
                confirmed
                    .then_some(())
                    .ok_or_else(|| "Safe Mode offer is no longer active".to_owned())
            })
        });
        let auto_close = std::env::var("EXPLORER_AUTO_CLOSE_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis);
        let visual_fixture = VisualFixtureConfig::from_environment()?;
        let show_splash =
            crate::branding::should_show_splash(visual_fixture.is_some(), auto_close.is_some());
        let initial_location = configured_initial_location()?;
        let (restored_tabs, restored_placement) = if visual_fixture.is_none() {
            load_session_restore(&diagnostics, initial_location.clone())
        } else {
            (None, None)
        };
        let (mut persistence, durable_observer, reset_observer, restore_preference, quick_access) =
            if visual_fixture.is_none() {
                create_session_persistence(restored_placement)
            } else {
                (None, None, None, true, Vec::new())
            };

        let platform = gpui_windows::WindowsPlatform::new(false)
            .context("failed to initialize GPUI-CE Windows platform")?;
        gpui::Application::with_platform(Rc::new(platform))
            .with_assets(crate::branding::AppAssets)
            .run(move |cx| {
                if visual_fixture.is_some() {
                    cx.set_global(explorer_ui::diagnostics::RegionDiagnosticsRecorder::default());
                }
                cx.bind_keys(explorer_ui::actions::gpui_text_input_bindings());
                cx.bind_keys(explorer_ui::actions::gpui_key_bindings());

                cx.on_app_quit(move |_| {
                    if let Err(error) = shutdown_shared(&shutdown_resources) {
                        tracing::error!(%error, "application quit cleanup failed");
                        if let Ok(resources) = shutdown_resources.lock() {
                            resources.diagnostics.record_error(
                                ErrorSeverity::Error,
                                "application",
                                "quit_cleanup",
                                error.as_ref(),
                                Some(file!()),
                            );
                        }
                    }
                    std::future::ready(())
                })
                .detach();

                cx.on_window_closed(|cx, _| {
                    if cx.windows().is_empty() {
                        cx.quit();
                    }
                })
                .detach();

                let window_options = visual_fixture.as_ref().map_or_else(
                    || {
                        restored_placement.map_or_else(
                            || initial_window_options(cx),
                            window_options_with_placement,
                        )
                    },
                    |fixture| window_options_with_size(cx, fixture.width, fixture.height),
                );
                let fixture_for_window = visual_fixture.clone();
                let initial_location_for_window = initial_location.clone();
                let restored_tabs_for_window = restored_tabs.clone();
                let durable_observer_for_window = durable_observer.clone();
                let reset_observer_for_window = reset_observer.clone();
                let restore_preference_for_window = restore_preference;
                let quick_access_for_window = quick_access.clone();
                let loaded_extension_summary_for_window = loaded_extension_summary.clone();
                let visual_column_runtime_for_window = visual_column_runtime.clone();
                let fixture_diagnostics = diagnostics.clone();
                let tokens = fixture_tokens(fixture_for_window.as_ref());
                let main_window = match cx.open_window(window_options, move |window, cx| {
                    let drag_threshold = system_drag_threshold(window);
                    let visual_state = fixture_for_window
                        .as_ref()
                        .filter(|fixture| !fixture.real_shell)
                        .map(VisualFixtureConfig::state);
                    if let Some(fixture) = fixture_for_window {
                        let diagnostics = fixture_diagnostics;
                        let frames = if fixture.real_shell { 30 } else { 1 };
                        schedule_visual_diagnostics(window, fixture, tokens, diagnostics, frames);
                    }
                    cx.new(move |cx| {
                        let extension_ui_pump =
                            extension_job_ui_bridge.take().and_then(|(inbox, ingress)| {
                                ApplicationExtensionUiPumpV1::new(inbox, ingress)
                            });
                        create_focused_explorer_root(
                            tokens,
                            shell_service,
                            drag_threshold,
                            visual_state,
                            initial_location_for_window,
                            restored_tabs_for_window,
                            durable_observer_for_window,
                            reset_observer_for_window,
                            restore_preference_for_window,
                            quick_access_for_window,
                            broker_health,
                            broker_retry,
                            folder_scripts,
                            safe_mode_offers,
                            safe_mode_confirm,
                            loaded_extension_summary_for_window,
                            visual_column_runtime_for_window,
                            extension_ui_pump.map(|pump| {
                                Box::new(pump) as Box<dyn explorer_ui::ExtensionUiPumpPortV1>
                            }),
                            window,
                            cx,
                        )
                    })
                }) {
                    Ok(handle) => {
                        let _ = diagnostics.record_event("window_ready", &[]);
                        handle
                    }
                    Err(error) => {
                        let mut launch_error = closure_error
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        *launch_error = Some(error.to_string());
                        cx.quit();
                        return;
                    }
                };

                if show_splash && let Err(error) = crate::branding::open_splash(cx, main_window) {
                    tracing::warn!(%error, "startup splash could not be created");
                    diagnostics.record_error(
                        ErrorSeverity::Warning,
                        "application",
                        "open_startup_splash",
                        error.as_ref(),
                        Some(file!()),
                    );
                }

                if let Some(delay) = auto_close {
                    cx.spawn(async move |cx| {
                        cx.background_executor().timer(delay).await;
                        cx.update(|cx| cx.quit());
                    })
                    .detach();
                }
            });

        let error = launch_error
            .lock()
            .map_err(|_| anyhow::anyhow!("GPUI launch error mutex was poisoned"))?
            .take();
        if let Some(coordinator) = &mut persistence {
            let flushed = coordinator.shutdown(Duration::from_secs(5));
            let health = coordinator.health();
            let _ = diagnostics_after_run.record_event(
                "session_persistence_stopped",
                &[
                    ("flushed", &flushed.to_string()),
                    ("writes", &health.successful_writes.to_string()),
                    ("failures", &health.failed_writes.to_string()),
                ],
            );
        }
        if let Some(error) = error {
            Err(anyhow::anyhow!(error)).context("failed to open initial GPUI window")
        } else {
            Ok(())
        }
    }

    /// Performs idempotent reverse-order process shutdown.
    ///
    /// # Errors
    ///
    /// Returns a bounded Shell join or diagnostics flush failure.
    pub fn shutdown(&mut self) -> Result<(), Error> {
        shutdown_shared(&self.resources)
    }

    /// Returns the startup-recovered, path-free incidents that require explicit
    /// user confirmation. Inspecting this list never clears native denial.
    ///
    /// # Errors
    ///
    /// Returns an error if application lifecycle state is unavailable.
    pub fn safe_mode_incident_offers(&self) -> Result<Vec<SafeModeIncidentOffer>, Error> {
        self.resources
            .lock()
            .map(|resources| resources.safe_mode_incident_offers.clone())
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    /// Explicitly confirms one startup Safe Mode offer through the resident
    /// extension host. No offer is cleared merely by being displayed.
    ///
    /// # Errors
    ///
    /// Returns a host confirmation failure or lifecycle-state error. Unknown or
    /// already-confirmed offers return `Ok(false)` without calling the host.
    pub fn confirm_safe_mode_incident(
        &self,
        incident_id: explorer_extension_host::NativeSafeModeIncidentIdV1,
    ) -> Result<bool, Error> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?;
        let mut offers = std::mem::take(&mut resources.safe_mode_incident_offers);
        let result = resources
            .extension_host
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("extension host is not available"))
            .and_then(|host| {
                confirm_offered_safe_mode_incident_v1(host, &mut offers, incident_id)
                    .map_err(Error::from)
            });
        resources.safe_mode_incident_offers = offers;
        let confirmed = result?;
        if confirmed {
            let remaining = resources.safe_mode_incident_offers.len().to_string();
            emit_post_commit_safe_mode_telemetry_v1(|| {
                resources.diagnostics.record_event(
                    "extension_safe_mode_incident_confirmed",
                    &[("remaining_incidents", &remaining)],
                )
            });
        }
        Ok(confirmed)
    }

    fn confirm_safe_mode_incident_for_presentation_token(
        shared: &Arc<Mutex<ShutdownResources>>,
        token: u64,
    ) -> Result<bool, Error> {
        let mut resources = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?;
        let mut offers = std::mem::take(&mut resources.safe_mode_incident_offers);
        let result = resources
            .extension_host
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("extension host is not available"))
            .and_then(|host| {
                confirm_presented_safe_mode_incident_v1(host, &mut offers, token)
                    .map_err(Error::from)
            });
        resources.safe_mode_incident_offers = offers;
        let confirmed = result?;
        if confirmed {
            let remaining = resources.safe_mode_incident_offers.len().to_string();
            emit_post_commit_safe_mode_telemetry_v1(|| {
                resources.diagnostics.record_event(
                    "extension_safe_mode_incident_confirmed",
                    &[("remaining_incidents", &remaining)],
                )
            });
        }
        Ok(confirmed)
    }

    fn safe_mode_ui_offers(&self) -> Result<Vec<explorer_ui::SafeModeOfferV1>, Error> {
        self.safe_mode_incident_offers().map(|offers| {
            offers
                .into_iter()
                .map(|offer| {
                    let suspect = offer.suspect();
                    explorer_ui::SafeModeOfferV1 {
                        presentation_token: offer.presentation_token(),
                        package_id: suspect.map(|value| value.package_id().to_owned()),
                        primary_interface_namespace: suspect
                            .map(SafeModeSuspectV1::primary_interface_namespace),
                        primary_interface_value: suspect
                            .map(SafeModeSuspectV1::primary_interface_value),
                        operation: format!("{:?}", offer.kind()),
                    }
                })
                .collect()
        })
    }

    fn diagnostics(&self) -> Result<DiagnosticsSession, Error> {
        self.resources
            .lock()
            .map(|resources| resources.diagnostics.clone())
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    fn loaded_extension_summary(&self) -> Result<Option<String>, Error> {
        self.resources
            .lock()
            .map(|resources| resources.loaded_extension_summary.clone())
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    fn visual_column_runtime(
        &self,
    ) -> Result<Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>, Error> {
        self.resources
            .lock()
            .map(|resources| resources.visual_column_runtime.clone())
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    fn shell_service(&self) -> Result<Arc<ShellStaHandle>, Error> {
        self.resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?
            .shell_sta
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Shell STA is not available"))
    }

    fn automation_handle(&self) -> Result<explorer_automation::FolderScriptHandle, Error> {
        self.resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?
            .automation
            .as_ref()
            .map(AutomationComposition::handle)
            .ok_or_else(|| anyhow::anyhow!("automation service is not available"))
    }

    fn broker_client(&self) -> Result<explorer_extension_broker::BrokerClient, Error> {
        self.resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?
            .broker
            .clone()
            .ok_or_else(|| anyhow::anyhow!("extension broker client is not available"))
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the GPUI composition root wires explicit lifecycle-owned services and restore inputs"
)]
fn create_explorer_root(
    tokens: UiTokens,
    shell_service: Arc<dyn explorer_model::ExplorerService>,
    drag_threshold: (f32, f32),
    visual_state: Option<explorer_ui::VisualFixtureState>,
    initial_location: Option<explorer_model::HistoryEntry>,
    restored_tabs: Option<explorer_model::ExplorerWindowState>,
    durable_observer: Option<explorer_ui::DurableStateObserver>,
    reset_observer: Option<explorer_ui::SessionResetObserver>,
    restore_preference: bool,
    quick_access: Vec<explorer_model::PersistedQuickAccessPin>,
    broker_health: explorer_ui::state::BrokerUiHealth,
    broker_retry: explorer_ui::BrokerRetryObserver,
    folder_scripts: explorer_automation::FolderScriptHandle,
    visual_column_runtime: Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>,
    extension_ui_pump: Option<Box<dyn explorer_ui::ExtensionUiPumpPortV1>>,
    window: &gpui::Window,
    cx: &mut gpui::Context<ExplorerRoot>,
) -> ExplorerRoot {
    let mut root = match visual_state {
        Some(state) => {
            let mut root = ExplorerRoot::for_visual_fixture(tokens, state);
            root.attach_service_for_shell_assets(shell_service);
            root
        }
        None => explorer_root(
            tokens,
            shell_service,
            drag_threshold,
            initial_location,
            restored_tabs,
        ),
    };
    root.attach_folder_scripts(folder_scripts);
    root.configure_restore_previous_session(restore_preference);
    root.configure_quick_access(quick_access);
    root.configure_broker_health(broker_health, broker_retry);
    if let Some(runtime) = visual_column_runtime {
        root.attach_visual_column_runtime(runtime);
    }
    if let Some(observer) = durable_observer {
        root.attach_durable_state_observer(observer, window, cx);
    }
    if let Some(observer) = reset_observer {
        root.attach_session_reset_observer(observer);
    }
    if let Some(pump) = extension_ui_pump {
        root.attach_extension_ui_pump(pump);
    }
    root.start_service_pump(window.window_handle(), cx);
    root
}

#[allow(
    clippy::too_many_arguments,
    reason = "the composition root passes independent platform, restore, persistence, and focus adapters"
)]
fn create_focused_explorer_root(
    tokens: UiTokens,
    shell_service: Arc<dyn explorer_model::ExplorerService>,
    drag_threshold: (f32, f32),
    visual_state: Option<explorer_ui::VisualFixtureState>,
    initial_location: Option<explorer_model::HistoryEntry>,
    restored_tabs: Option<explorer_model::ExplorerWindowState>,
    durable_observer: Option<explorer_ui::DurableStateObserver>,
    reset_observer: Option<explorer_ui::SessionResetObserver>,
    restore_preference: bool,
    quick_access: Vec<explorer_model::PersistedQuickAccessPin>,
    broker_health: explorer_ui::state::BrokerUiHealth,
    broker_retry: explorer_ui::BrokerRetryObserver,
    folder_scripts: explorer_automation::FolderScriptHandle,
    safe_mode_offers: Vec<explorer_ui::SafeModeOfferV1>,
    safe_mode_confirm: explorer_ui::SafeModeConfirmObserverV1,
    loaded_extension_summary: Option<String>,
    visual_column_runtime: Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>,
    extension_ui_pump: Option<Box<dyn explorer_ui::ExtensionUiPumpPortV1>>,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<ExplorerRoot>,
) -> ExplorerRoot {
    let focus_handle = cx.focus_handle();
    focus_handle.focus(window, cx);
    let mut root = create_explorer_root(
        tokens,
        shell_service,
        drag_threshold,
        visual_state,
        initial_location,
        restored_tabs,
        durable_observer,
        reset_observer,
        restore_preference,
        quick_access,
        broker_health,
        broker_retry,
        folder_scripts,
        visual_column_runtime,
        extension_ui_pump,
        window,
        cx,
    );
    root.configure_shell_icon_scale(window.scale_factor());
    root.attach_pointer_capture_factory(Arc::new(|hwnd| {
        crate::pointer_capture::NativePointerCapture::acquire(hwnd)
    }));
    root.attach_text_inputs(cx);
    root.attach_focus_handle(focus_handle);
    if !safe_mode_offers.is_empty() {
        root.configure_safe_mode_offers(safe_mode_offers, safe_mode_confirm);
    }
    root.configure_loaded_extension_summary(loaded_extension_summary);
    root
}

fn format_single_plugin_summary(
    path: &std::path::Path,
    summary: &explorer_extension_host::SinglePluginLoadSummaryV1,
) -> String {
    let plugin_id = summary.plugin_id();
    let plugin_name = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("development-plugin")
        .replace('_', "-");
    let contributions = summary
        .contributions()
        .iter()
        .map(|contribution| {
            let kind = match contribution.kind().into_raw() {
                1 => "Column",
                2 => "GPUI Renderer",
                3 => "Command",
                4 => "Form",
                5 => "Operation Plan",
                6 => "View Mode",
                7 => "Resource",
                _ => "Unknown",
            };
            format!("{} ({})", contribution.contribution_id(), kind)
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} — Plugin {}:{}:{} — {}",
        plugin_name,
        plugin_id.namespace.authority(),
        plugin_id.namespace.revision(),
        plugin_id.value,
        contributions
    )
}

fn fixture_tokens(fixture: Option<&VisualFixtureConfig>) -> UiTokens {
    match high_contrast_tokens() {
        Ok(Some(tokens)) => tokens,
        Ok(None) => fixture.map_or_else(UiTokens::default, VisualFixtureConfig::tokens),
        Err(error) => {
            tracing::warn!(%error, "Windows high-contrast query failed; using configured theme");
            fixture.map_or_else(UiTokens::default, VisualFixtureConfig::tokens)
        }
    }
}

fn broker_ui_health(
    client: &explorer_extension_broker::BrokerClient,
) -> explorer_ui::state::BrokerUiHealth {
    match client.verify() {
        Ok(()) => explorer_ui::state::BrokerUiHealth::Healthy,
        Err(explorer_extension_broker::BrokerClientError::Unavailable) => {
            explorer_ui::state::BrokerUiHealth::Unavailable
        }
        Err(explorer_extension_broker::BrokerClientError::VersionMismatch) => {
            explorer_ui::state::BrokerUiHealth::VersionMismatch
        }
        Err(explorer_extension_broker::BrokerClientError::Timeout) => {
            explorer_ui::state::BrokerUiHealth::Timeout
        }
        Err(
            explorer_extension_broker::BrokerClientError::Start
            | explorer_extension_broker::BrokerClientError::Disconnected
            | explorer_extension_broker::BrokerClientError::Protocol,
        ) => explorer_ui::state::BrokerUiHealth::Crash,
    }
}

fn configured_broker_ui_health(
    client: &explorer_extension_broker::BrokerClient,
) -> explorer_ui::state::BrokerUiHealth {
    if client.is_available() {
        explorer_ui::state::BrokerUiHealth::Healthy
    } else {
        explorer_ui::state::BrokerUiHealth::Unavailable
    }
}

fn system_drag_threshold(window: &gpui::Window) -> (f32, f32) {
    explorer_shell_win::SystemDragThreshold::current().logical_at_scale(window.scale_factor())
}

fn configured_initial_location() -> Result<Option<explorer_model::HistoryEntry>, Error> {
    let Some(value) = std::env::var_os("EXPLORER_INITIAL_PATH") else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() || !path.is_dir() {
        anyhow::bail!("EXPLORER_INITIAL_PATH must be an existing absolute directory");
    }
    let title = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map_or_else(|| path.display().to_string(), str::to_owned);
    Ok(Some(explorer_model::HistoryEntry::new(
        explorer_model::LocationDescriptor::file_system(path.display().to_string()),
        title,
    )))
}

fn explorer_root(
    tokens: UiTokens,
    shell_service: Arc<dyn explorer_model::ExplorerService>,
    drag_threshold: (f32, f32),
    initial_location: Option<explorer_model::HistoryEntry>,
    restored_tabs: Option<explorer_model::ExplorerWindowState>,
) -> ExplorerRoot {
    let mut root = if let Some(restored) = restored_tabs {
        ExplorerRoot::with_service_drag_threshold_and_restored_window(
            tokens,
            shell_service,
            drag_threshold,
            restored,
        )
    } else {
        match initial_location {
            Some(initial) => ExplorerRoot::with_service_drag_threshold_and_initial_location(
                tokens,
                Arc::clone(&shell_service),
                drag_threshold,
                initial,
            ),
            None => {
                ExplorerRoot::with_service_and_drag_threshold(tokens, shell_service, drag_threshold)
            }
        }
    };
    root.configure_tortoise_git_available(explorer_shell_win::tortoise_git_is_installed());
    root.configure_new_items(explorer_shell_win::registered_shell_new_items_in_worker());
    root
}

fn load_session_restore(
    diagnostics: &DiagnosticsSession,
    configured: Option<explorer_model::HistoryEntry>,
) -> (
    Option<explorer_model::ExplorerWindowState>,
    Option<explorer_model::PersistedWindowPlacement>,
) {
    let limits = RoadmapLimits::default();
    let Ok(store) = crate::session_store::WindowsSessionStore::from_environment(limits) else {
        let _ = diagnostics.record_event("session_restore_unavailable", &[]);
        return (None, None);
    };
    let Ok(outcome) = store.load() else {
        let _ = diagnostics.record_event("session_restore_failed", &[]);
        return (None, None);
    };
    let Some(envelope) = outcome
        .envelope
        .filter(|value| value.payload.restore_enabled)
    else {
        let _ = diagnostics.record_event(
            "session_restore_defaults",
            &[(
                "rejected_artifacts",
                &outcome.rejected_artifacts.to_string(),
            )],
        );
        return (None, None);
    };
    let Ok(plan) = envelope.restore_plan(limits) else {
        let _ = diagnostics.record_event("session_restore_plan_rejected", &[]);
        return (None, None);
    };
    let placement = crate::session_lifecycle::primary_monitor_work_area().map(|monitor| {
        crate::session_lifecycle::fit_window_placement(plan.window, &[monitor], 640, 480)
    });
    if !should_restore_saved_tabs(configured.as_ref()) {
        let _ = diagnostics.record_event(
            "session_restore_location_overridden",
            &[("tabs", &plan.tabs.len().to_string())],
        );
        return (None, placement);
    }
    let fallback = configured.unwrap_or_else(|| {
        explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\"),
            "This PC",
        )
    });
    let restored = plan.resolve_window(fallback, resolve_saved_location).ok();
    let source = format!("{:?}", outcome.source);
    let _ = diagnostics.record_event(
        "session_restore_ready",
        &[
            ("source", &source),
            ("tabs", &plan.tabs.len().to_string()),
            ("migration", &outcome.migration_performed.to_string()),
        ],
    );
    (restored, placement)
}

const fn should_restore_saved_tabs(configured: Option<&explorer_model::HistoryEntry>) -> bool {
    configured.is_none()
}

fn resolve_saved_location(
    descriptor: &explorer_model::LocationDescriptor,
) -> Option<explorer_model::HistoryEntry> {
    if let Some(path) = descriptor.path() {
        if !path.is_dir() {
            return None;
        }
        let title = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map_or_else(|| path.display().to_string(), str::to_owned);
        return Some(explorer_model::HistoryEntry::new(descriptor.clone(), title));
    }
    Some(explorer_model::HistoryEntry::new(
        descriptor.clone(),
        "Shell location",
    ))
}

fn create_session_persistence(
    _restored_placement: Option<explorer_model::PersistedWindowPlacement>,
) -> (
    Option<crate::session_lifecycle::PersistenceCoordinator>,
    Option<explorer_ui::DurableStateObserver>,
    Option<explorer_ui::SessionResetObserver>,
    bool,
    Vec<explorer_model::PersistedQuickAccessPin>,
) {
    let limits = RoadmapLimits::default();
    let Ok(store) = crate::session_store::WindowsSessionStore::from_environment(limits) else {
        return (None, None, None, true, Vec::new());
    };
    let loaded = store.load().ok().and_then(|outcome| outcome.envelope);
    let generation = loaded
        .as_ref()
        .map_or(1, |envelope| envelope.write_generation.saturating_add(1));
    let quick_access = loaded
        .as_ref()
        .map_or_else(Vec::new, |envelope| envelope.payload.quick_access.clone());
    let restore_enabled = loaded
        .as_ref()
        .is_none_or(|envelope| envelope.payload.restore_enabled);
    let store: Arc<dyn explorer_model::SessionStore> = Arc::new(store);
    let coordinator = crate::session_lifecycle::PersistenceCoordinator::start(
        store,
        Duration::from_millis(limits.preview_debounce_ms.max(250)),
        Duration::from_secs(2),
    );
    let handle = coordinator.handle();
    let generation = Arc::new(AtomicU64::new(generation));
    let reset_handle = handle.clone();
    let reset_observer: explorer_ui::SessionResetObserver =
        Arc::new(move |scope| reset_handle.request_reset(scope));
    let observer: explorer_ui::DurableStateObserver =
        Arc::new(move |window, restore_enabled, quick_access, placement| {
            let write_generation = generation.fetch_add(1, Ordering::AcqRel);
            handle.accepted_runtime(
                crate::session_lifecycle::DurableTransition::ViewSettingsChanged,
                crate::session_lifecycle::RuntimeSessionSnapshot {
                    window,
                    placement,
                    quick_access,
                    restore_enabled,
                    write_generation,
                    provenance: explorer_model::SessionProvenance {
                        app_version: env!("CARGO_PKG_VERSION").to_owned(),
                        app_revision: option_env!("GIT_REVISION").unwrap_or("unknown").to_owned(),
                        windows_build: std::env::var("OS").unwrap_or_else(|_| "Windows".to_owned()),
                    },
                    limits,
                },
            )
        });
    (
        Some(coordinator),
        Some(observer),
        Some(reset_observer),
        restore_enabled,
        quick_access,
    )
}

fn shutdown_shared(resources: &Arc<Mutex<ShutdownResources>>) -> Result<(), Error> {
    let mut resources = resources
        .lock()
        .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?;
    resources.shutdown()
}

impl ShutdownResources {
    fn shutdown(&mut self) -> Result<(), Error> {
        if self.shutdown {
            return Ok(());
        }
        self.shutdown = true;

        let mut failures = Vec::new();
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_started", &[("stage", "automation")]);
        if let Some(mut automation) = self.automation.take() {
            automation.shutdown();
        }
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_finished", &[("stage", "automation")]);
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_started", &[("stage", "extension_host")]);
        if let Some(mut extension_host) = self.extension_host.take() {
            extension_host.shutdown();
        }
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_finished", &[("stage", "extension_host")]);
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_started", &[("stage", "broker_warmup")]);
        if let Some(warmup) = self.broker_warmup.take()
            && warmup.join().is_err()
        {
            failures.push("extension broker warmup thread panicked".to_owned());
        }
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_finished", &[("stage", "broker_warmup")]);
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_started", &[("stage", "broker")]);
        if let Some(broker) = self.broker.take() {
            broker.shutdown();
        }
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_finished", &[("stage", "broker")]);
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_started", &[("stage", "shell_sta")]);
        if let Some(shell_sta) = self.shell_sta.take()
            && let Err(error) = shell_sta.shutdown_and_join(SHELL_JOIN_TIMEOUT)
        {
            failures.push(format!("Shell STA: {error}"));
        }
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_finished", &[("stage", "shell_sta")]);
        if let Err(error) = self.diagnostics.record_event("application_stopped", &[]) {
            failures.push(format!("application_stopped event: {error}"));
        }
        if let Err(error) = self.diagnostics.record_event("clean_shutdown", &[]) {
            failures.push(format!("clean_shutdown event: {error}"));
        }
        if let Err(error) = self.diagnostics.shutdown() {
            failures.push(format!("diagnostics shutdown: {error}"));
        }

        if failures.is_empty() {
            Ok(())
        } else {
            let error = anyhow::anyhow!("application cleanup failed: {}", failures.join("; "));
            self.diagnostics.record_error(
                ErrorSeverity::Error,
                "application",
                "shutdown",
                error.as_ref(),
                Some(file!()),
            );
            Err(error)
        }
    }
}

impl Drop for ApplicationLifecycle {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::error!(%error, "application lifecycle cleanup failed");
            if let Ok(resources) = self.resources.lock() {
                resources.diagnostics.record_error(
                    ErrorSeverity::Error,
                    "application",
                    "drop_cleanup",
                    error.as_ref(),
                    Some(file!()),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        sync::{Arc, Mutex},
        time::Instant,
    };

    use abi_stable::std_types::{ROption, RVec};
    use explorer_extension_api::{
        IncrementalResultBatchV1, IncrementalResultEntryV1, JobContextV1, JobTerminalV1,
        PluginItemResultV1, PluginValueV1, SinkSubmitStatusV1,
    };
    use explorer_extension_host::{
        ExtensionJobAuthorityV1, ExtensionJobRuntimeRequestV1, ExtensionJobRuntimeV1,
        ExtensionJobUiIngressV1, ExtensionResultBufferConfigV1,
    };
    use explorer_model::{FileEntry, FileEntryMetadata, LocationDescriptor, ShellItemId, ViewMode};
    use explorer_ui::ExtensionUiPumpPortV1 as _;

    use super::{
        ApplicationExtensionReadyProjectorV1, ApplicationExtensionUiPumpV1,
        SafeModeIncidentOfferV1, SafeModeIncidentPortV1, confirm_offered_safe_mode_incident_v1,
        confirm_presented_safe_mode_incident_v1, emit_post_commit_safe_mode_telemetry_v1,
        should_restore_saved_tabs,
    };

    struct FakeSafeModePortV1 {
        denied: bool,
        confirmed: Mutex<Vec<u8>>,
    }

    impl SafeModeIncidentPortV1 for FakeSafeModePortV1 {
        type IncidentId = u8;
        type Error = ();

        fn offers(&self) -> Vec<SafeModeIncidentOfferV1<Self::IncidentId>> {
            Vec::new()
        }

        fn denies_native_callbacks(&self) -> bool {
            self.denied
        }

        fn confirm(&self, incident_id: Self::IncidentId) -> Result<(), Self::Error> {
            self.confirmed.lock().unwrap().push(incident_id);
            Ok(())
        }
    }

    struct CountingProjectorV1 {
        calls: Arc<AtomicUsize>,
        fail: bool,
    }

    impl ApplicationExtensionReadyProjectorV1 for CountingProjectorV1 {
        fn project_ready(
            &mut self,
            _pump: &mut explorer_extension_host::ExtensionJobUiPumpV1,
            _runtime: &Arc<ExtensionJobRuntimeV1>,
            _ingress: &ExtensionJobUiIngressV1,
        ) -> Result<usize, explorer_extension_host::ExtensionJobUiPumpErrorV1> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(explorer_extension_host::ExtensionJobUiPumpErrorV1::WrongUiThread);
            }
            Ok(0)
        }
    }

    struct ApplyingProjectorV1 {
        calls: Arc<AtomicUsize>,
    }

    impl ApplicationExtensionReadyProjectorV1 for ApplyingProjectorV1 {
        fn project_ready(
            &mut self,
            pump: &mut explorer_extension_host::ExtensionJobUiPumpV1,
            runtime: &Arc<ExtensionJobRuntimeV1>,
            ingress: &ExtensionJobUiIngressV1,
        ) -> Result<usize, explorer_extension_host::ExtensionJobUiPumpErrorV1> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let ready = pump.take_ready(16)?;
            let mut applied = 0;
            for signal in ready.signals {
                let (item, location, source) = signal.generations();
                for batch in runtime.drain(signal.job(), item, location, source, 16) {
                    if runtime
                        .apply_accepted_batch(&batch, |_| ("fixture-item".to_owned(), 1))
                        .is_some()
                    {
                        ingress.notify_applied(&batch);
                        applied += 1;
                    }
                }
            }
            Ok(applied)
        }
    }

    fn runtime() -> Arc<ExtensionJobRuntimeV1> {
        Arc::new(ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(4, 4, 16, 16, 16, 16, 16, 16, 4096, 4096, 4096)
                .unwrap(),
        ))
    }

    fn request() -> ExtensionJobRuntimeRequestV1 {
        ExtensionJobRuntimeRequestV1 {
            authority: ExtensionJobAuthorityV1::for_integration_test("app-fixture"),
            job_generation: 1,
            item_generation: 1,
            location_generation: 1,
            source_generation: 1,
            has_item: true,
            input_stream: None,
        }
    }

    fn batch(context: &JobContextV1) -> IncrementalResultBatchV1 {
        IncrementalResultBatchV1 {
            job: context.job,
            sink_capability: context.sink.capability,
            job_generation: context.job_generation,
            location: context.location,
            location_generation: context.location_generation,
            source_generation: context.source_generation,
            sequence: 0,
            entries: RVec::from(vec![IncrementalResultEntryV1 {
                item: context.item.into_option().unwrap(),
                item_generation: context.item_generation,
                source_generation: context.source_generation,
                result: PluginItemResultV1::value(
                    PluginValueV1::text("fixture").unwrap(),
                    ROption::RNone,
                ),
            }]),
        }
    }

    fn queued_fixture() -> (
        Arc<ExtensionJobRuntimeV1>,
        ExtensionJobUiIngressV1,
        explorer_extension_host::ExtensionJobUiInboxV1,
        JobContextV1,
    ) {
        let runtime = runtime();
        let (ingress, inbox) = ExtensionJobUiIngressV1::new_integration_pair(Arc::clone(&runtime));
        let context = runtime.open_job_for_integration_test(request()).unwrap();
        assert_eq!(
            runtime
                .submit_for_integration_test(&context, batch(&context))
                .status,
            SinkSubmitStatusV1::ACCEPTED
        );
        (runtime, ingress, inbox, context)
    }

    fn directory_entries(count: u64) -> Vec<FileEntry> {
        (0..count)
            .map(|index| FileEntry {
                id: ShellItemId::from_provider_bytes(index.to_le_bytes()).unwrap(),
                location: LocationDescriptor::file_system(format!(r"C:\fixture\{index}.txt")),
                display_name: format!("{index}.txt"),
                is_container: false,
                metadata: FileEntryMetadata::default(),
            })
            .collect()
    }

    #[test]
    fn directory_fixture_is_visible_before_extension_projection_runs() {
        let root = explorer_ui::ExplorerRoot::for_directory_fixture(
            explorer_ui::UiTokens::default(),
            directory_entries(1_000),
            ViewMode::Details,
        );
        let calls = Arc::new(AtomicUsize::new(0));
        assert_eq!(root.fixture_visible_entry_count(), Some(1_000));
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let (runtime, ingress, inbox, context) = queued_fixture();
        let mut app_pump = ApplicationExtensionUiPumpV1::with_ready_projector(
            inbox,
            ingress,
            Box::new(ApplyingProjectorV1 {
                calls: Arc::clone(&calls),
            }),
        )
        .unwrap();
        let now = Instant::now();
        assert!(!app_pump.poll_due(now));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(root.fixture_visible_entry_count(), Some(1_000));
        let deadline = app_pump.pump.next_deadline().unwrap().unwrap();
        assert!(app_pump.poll_due(deadline));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(matches!(
            runtime.finish_for_integration_test(context.job, JobTerminalV1::COMPLETED),
            explorer_extension_host::ExtensionJobFinishOutcomeV1::Published(
                JobTerminalV1::COMPLETED
            )
        ));
        runtime.retire(context.job).unwrap();
    }

    #[test]
    fn projector_injection_runs_before_poll_and_neither_deferred_nor_error_consumes_ready_work() {
        let (runtime, ingress, inbox, context) = queued_fixture();
        let mut deferred = ApplicationExtensionUiPumpV1::new(inbox, ingress).unwrap();
        assert!(!deferred.poll_due(Instant::now()));
        assert_eq!(deferred.pump.take_ready(1).unwrap().signals.len(), 1);
        assert!(matches!(
            runtime.finish_for_integration_test(context.job, JobTerminalV1::COMPLETED),
            explorer_extension_host::ExtensionJobFinishOutcomeV1::Published(
                JobTerminalV1::COMPLETED
            )
        ));
        runtime.retire(context.job).unwrap();

        let (runtime, ingress, inbox, context) = queued_fixture();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut app_pump = ApplicationExtensionUiPumpV1::with_ready_projector(
            inbox,
            ingress,
            Box::new(CountingProjectorV1 {
                calls: Arc::clone(&calls),
                fail: true,
            }),
        )
        .unwrap();
        app_pump.set_ready_projector(Box::new(CountingProjectorV1 {
            calls: Arc::clone(&calls),
            fail: true,
        }));
        assert!(!app_pump.poll_due(Instant::now()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(app_pump.pump.take_ready(1).unwrap().signals.len(), 1);
        assert!(matches!(
            runtime.finish_for_integration_test(context.job, JobTerminalV1::COMPLETED),
            explorer_extension_host::ExtensionJobFinishOutcomeV1::Published(
                JobTerminalV1::COMPLETED
            )
        ));
        runtime.retire(context.job).unwrap();
    }

    #[test]
    fn explicit_start_location_overrides_saved_tabs() {
        let configured = explorer_model::HistoryEntry::new(
            LocationDescriptor::file_system(r"D:\requested"),
            "requested",
        );

        assert!(!should_restore_saved_tabs(Some(&configured)));
        assert!(should_restore_saved_tabs(None));
    }

    #[test]
    fn safe_mode_offer_remains_denied_until_explicit_confirmation() {
        let port = FakeSafeModePortV1 {
            denied: true,
            confirmed: Mutex::new(Vec::new()),
        };
        let mut offers = vec![SafeModeIncidentOfferV1 {
            incident_id: 7,
            presentation_token: 1,
            kind: explorer_extension_host::NativeSafeModeIncidentKindV1::UnsafeMarkerState,
            suspect: None,
        }];

        assert!(port.denies_native_callbacks());
        assert_eq!(offers.len(), 1);
        assert!(port.confirmed.lock().unwrap().is_empty());

        assert_eq!(
            confirm_offered_safe_mode_incident_v1(&port, &mut offers, 7),
            Ok(true)
        );
        assert!(offers.is_empty());
        assert_eq!(port.confirmed.lock().unwrap().as_slice(), &[7]);
        assert_eq!(
            confirm_offered_safe_mode_incident_v1(&port, &mut offers, 7),
            Ok(false)
        );
        assert_eq!(port.confirmed.lock().unwrap().as_slice(), &[7]);
    }

    #[test]
    fn stale_safe_mode_presenter_token_does_not_confirm_a_shifted_offer() {
        let port = FakeSafeModePortV1 {
            denied: true,
            confirmed: Mutex::new(Vec::new()),
        };
        let mut offers = vec![
            SafeModeIncidentOfferV1 {
                incident_id: 7,
                presentation_token: 101,
                kind: explorer_extension_host::NativeSafeModeIncidentKindV1::UnsafeMarkerState,
                suspect: None,
            },
            SafeModeIncidentOfferV1 {
                incident_id: 9,
                presentation_token: 202,
                kind: explorer_extension_host::NativeSafeModeIncidentKindV1::UnsafeMarkerState,
                suspect: None,
            },
        ];

        assert_eq!(
            confirm_presented_safe_mode_incident_v1(&port, &mut offers, 101),
            Ok(true)
        );
        assert_eq!(port.confirmed.lock().unwrap().as_slice(), &[7]);

        assert_eq!(
            confirm_presented_safe_mode_incident_v1(&port, &mut offers, 101),
            Ok(false)
        );
        assert_eq!(port.confirmed.lock().unwrap().as_slice(), &[7]);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].incident_id(), 9);
    }

    #[test]
    fn post_commit_safe_mode_telemetry_failure_does_not_mask_confirmation() {
        let attempts = AtomicUsize::new(0);
        emit_post_commit_safe_mode_telemetry_v1(|| {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(())
        });
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
