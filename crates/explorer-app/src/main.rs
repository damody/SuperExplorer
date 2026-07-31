#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
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

#[cfg(not(windows))]
compile_error!("explorer-app supports Windows targets only");

use explorer_app::application::ApplicationLifecycle;
use explorer_common::{
    AppBuildInfo, DiagnosticsConfig, DiagnosticsSession, ErrorSeverity, initialize_diagnostics,
    install_panic_hook,
};
use explorer_jobs::JobSchedulerConfig;
use explorer_model::WorkspaceModel;
use explorer_search::SearchSource;
use explorer_shell_win::ShellPlatform;
use explorer_ui::ExplorerUiState;

fn main() {
    let build = AppBuildInfo::current();
    let diagnostics =
        match initialize_diagnostics(DiagnosticsConfig::from_environment(build.package_version)) {
            Ok(diagnostics) => diagnostics,
            Err(error) => {
                eprintln!("Explorer diagnostics initialization failed: {error}");
                return;
            }
        };
    install_panic_hook(diagnostics.clone());
    if let Err(error) = run(build, &diagnostics) {
        diagnostics.record_error(
            ErrorSeverity::Critical,
            "application",
            "run",
            error.as_ref(),
            Some(file!()),
        );
        tracing::error!(%error, "Explorer stopped after a controlled application failure");
    }
}

fn run(build: AppBuildInfo, diagnostics: &DiagnosticsSession) -> anyhow::Result<()> {
    diagnostics.record_event(
        "startup",
        &[
            ("version", build.package_version),
            ("revision", build.git_revision),
        ],
    )?;

    let mut lifecycle = ApplicationLifecycle::start(diagnostics.clone())?;

    let model = WorkspaceModel::new();
    let ui = ExplorerUiState::default();
    let jobs = JobSchedulerConfig::default();
    let shell = ShellPlatform::windows();
    let search_source = SearchSource::WindowsIndex;

    tracing::info!(
        version = build.package_version,
        revision = build.git_revision,
        lifecycle = ?model.lifecycle(),
        ui_lifecycle = ?ui.model().lifecycle(),
        maximum_queued_jobs = jobs.maximum_queued_jobs,
        requires_sta = shell.requires_sta,
        ?search_source,
        "Explorer bootstrap composition is ready"
    );
    diagnostics.record_event("composition_ready", &[])?;
    lifecycle.run_gpui()?;
    lifecycle.shutdown()?;
    Ok(())
}
