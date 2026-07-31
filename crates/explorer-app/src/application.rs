//! Production process composition root.

use std::{
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
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
        let dpi_outcome = initialize_dpi_awareness()?;
        let dpi_outcome_text = format!("{dpi_outcome:?}");
        diagnostics.record_event("windows_prerequisites_ready", &[("dpi", &dpi_outcome_text)])?;
        let shell_sta = Arc::new(ShellStaHandle::start()?);
        diagnostics.record_event("shell_sta_ready", &[])?;
        let automation = AutomationComposition::start()?;
        let script_count = automation.snapshots()?.len().to_string();
        diagnostics.record_event("automation_ready", &[("scripts", &script_count)])?;
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
                broker_warmup,
                broker,
                shell_sta: Some(shell_sta),
                shutdown: false,
            })),
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

    fn diagnostics(&self) -> Result<DiagnosticsSession, Error> {
        self.resources
            .lock()
            .map(|resources| resources.diagnostics.clone())
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
    if let Some(observer) = durable_observer {
        root.attach_durable_state_observer(observer, window, cx);
    }
    if let Some(observer) = reset_observer {
        root.attach_session_reset_observer(observer);
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
        window,
        cx,
    );
    root.configure_shell_icon_scale(window.scale_factor());
    root.attach_pointer_capture_factory(Arc::new(|hwnd| {
        crate::pointer_capture::NativePointerCapture::acquire(hwnd)
    }));
    root.attach_text_inputs(cx);
    root.attach_focus_handle(focus_handle);
    root
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
    use super::should_restore_saved_tabs;

    #[test]
    fn explicit_start_location_overrides_saved_tabs() {
        let configured = explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\requested"),
            "requested",
        );

        assert!(!should_restore_saved_tabs(Some(&configured)));
        assert!(should_restore_saved_tabs(None));
    }
}
