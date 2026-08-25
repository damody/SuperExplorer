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

#[link(name = "kernel32")]
unsafe extern "system" {
    fn AllocConsole() -> i32;
}

fn main() {
    let diagnostics_console = diagnostics_console_requested();
    if diagnostics_console {
        // SAFETY: AllocConsole takes no pointers and creates one console for
        // this process. Failure is non-fatal because persistent logging still
        // captures client diagnostics.
        let _ = unsafe { AllocConsole() };
    }
    let build = AppBuildInfo::current();
    let diagnostics =
        match initialize_diagnostics(DiagnosticsConfig::from_environment(build.package_version)) {
            Ok(diagnostics) => diagnostics,
            Err(error) => {
                eprintln!("Explorer diagnostics initialization failed: {error}");
                return;
            }
        };
    if diagnostics_console {
        eprintln!(
            "SuperExplorer diagnostics console is active. Persistent error log: {}",
            diagnostics.error_log_path().map_or_else(
                || "Unavailable".to_owned(),
                |path| path.display().to_string()
            )
        );
    }
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
    let plugin_dlls = parse_plugin_dll_arguments()?;
    diagnostics.record_event(
        "startup",
        &[
            ("version", build.package_version),
            ("revision", build.git_revision),
        ],
    )?;

    let mut lifecycle =
        ApplicationLifecycle::start_with_plugins(diagnostics.clone(), &plugin_dlls)?;

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

fn parse_plugin_dll_arguments() -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut arguments = std::env::args_os().skip(1);
    let mut plugin_dlls = Vec::new();

    while let Some(argument) = arguments.next() {
        if argument == "--diagnostics-console" {
            continue;
        }
        if argument != "--plugin-dll" {
            anyhow::bail!("unsupported argument: {}", argument.to_string_lossy());
        }
        let path = arguments
            .next()
            .ok_or_else(|| anyhow::anyhow!("--plugin-dll requires an absolute DLL path"))?;
        plugin_dlls.push(path.into());
    }

    Ok(plugin_dlls)
}

fn diagnostics_console_requested() -> bool {
    std::env::args_os().any(|argument| argument == "--diagnostics-console")
}
