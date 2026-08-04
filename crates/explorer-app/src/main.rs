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
    let plugin_dll = parse_plugin_dll_argument()?;
    diagnostics.record_event(
        "startup",
        &[
            ("version", build.package_version),
            ("revision", build.git_revision),
        ],
    )?;

    let mut lifecycle =
        ApplicationLifecycle::start_with_plugin(diagnostics.clone(), plugin_dll.as_deref())?;

    let model = WorkspaceModel::new();
    let ui = ExplorerUiState::default();
    let jobs = JobSchedulerConfig::default();
    let shell = ShellPlatform::windows();
    tracing::info!(
        version = build.package_version,
        revision = build.git_revision,
        lifecycle = ?model.lifecycle(),
        ui_lifecycle = ?ui.model().lifecycle(),
        maximum_queued_jobs = jobs.maximum_queued_jobs,
        requires_sta = shell.requires_sta,
        "Explorer bootstrap composition is ready"
    );
    diagnostics.record_event("composition_ready", &[])?;
    lifecycle.run_gpui()?;
    lifecycle.shutdown()?;
    Ok(())
}

fn parse_plugin_dll_argument() -> anyhow::Result<Option<std::path::PathBuf>> {
    let mut arguments = std::env::args_os().skip(1);
    let mut plugin_dll = None;

    while let Some(argument) = arguments.next() {
        if argument != "--plugin-dll" {
            anyhow::bail!("unsupported argument: {}", argument.to_string_lossy());
        }
        if plugin_dll.is_some() {
            anyhow::bail!("--plugin-dll may only be supplied once");
        }
        let path = arguments
            .next()
            .ok_or_else(|| anyhow::anyhow!("--plugin-dll requires an absolute DLL path"))?;
        plugin_dll = Some(path.into());
    }

    Ok(plugin_dll)
}
