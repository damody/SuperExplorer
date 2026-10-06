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

use explorer_app::{
    application::ApplicationLifecycle,
    explorer_handoff, explorer_import,
    launch_coordination::{LaunchKind, LaunchSession},
    win_e_hotkey,
};
use explorer_common::{
    AppBuildInfo, DiagnosticsConfig, DiagnosticsSession, ErrorSeverity, initialize_diagnostics,
    install_panic_hook,
};
use explorer_jobs::JobSchedulerConfig;
use explorer_model::WorkspaceModel;
use explorer_shell_win::ShellPlatform;
use explorer_ui::ExplorerUiState;

mod startup_failure;
mod startup_privileges;

#[link(name = "kernel32")]
#[expect(
    unsafe_code,
    reason = "AllocConsole is exposed only through the Win32 system ABI"
)]
// SAFETY: The declaration matches kernel32's parameterless BOOL-returning
// AllocConsole signature and is invoked only by this process entry point.
unsafe extern "system" {
    fn AllocConsole() -> i32;
}

fn main() {
    match startup_privileges::relaunch_if_elevated() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            startup_failure::report("切換為一般使用者啟動", &format!("{error:#}"), None);
            std::process::exit(1);
        }
    }
    let diagnostics_console = diagnostics_console_requested();
    if diagnostics_console {
        // SAFETY: AllocConsole takes no pointers and creates one console for
        // this process. Failure is non-fatal because persistent logging still
        // captures client diagnostics.
        #[expect(
            unsafe_code,
            reason = "creating the optional diagnostics console requires calling AllocConsole"
        )]
        let _ = unsafe { AllocConsole() };
    }
    let build = AppBuildInfo::current();
    let diagnostics =
        match initialize_diagnostics(DiagnosticsConfig::from_environment(build.package_version)) {
            Ok(diagnostics) => diagnostics,
            Err(error) => {
                startup_failure::report("初始化診斷記錄", &error.to_string(), None);
                std::process::exit(1);
            }
        };
    if diagnostics_console {
        explorer_common::write_stderr_lossy(&format!(
            "SuperExplorer diagnostics console is active. Persistent error log: {}",
            diagnostics.error_log_path().map_or_else(
                || "Unavailable".to_owned(),
                |path| path.display().to_string()
            )
        ));
    }
    install_panic_hook(diagnostics.clone());
    explorer_shell_win::install_native_crash_log();
    let run_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(build, &diagnostics, diagnostics_console)
    }));
    match run_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            diagnostics.record_error(
                ErrorSeverity::Critical,
                "application",
                "run",
                error.as_ref(),
                Some(file!()),
            );
            tracing::error!(%error, "Explorer stopped after a controlled application failure");
            startup_failure::report(
                "啟動或執行程式",
                &format!("{error:#}"),
                diagnostics.error_log_path().as_deref(),
            );
            std::process::exit(1);
        }
        Err(payload) => {
            explorer_common::log_isolated_panic(
                "application",
                "run",
                payload.as_ref(),
                Some(file!()),
            );
            let reason = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("程式發生未預期的錯誤，請查看診斷記錄。");
            startup_failure::report(
                "啟動或執行程式",
                reason,
                diagnostics.error_log_path().as_deref(),
            );
            std::process::exit(1);
        }
    }
}

fn run(
    build: AppBuildInfo,
    diagnostics: &DiagnosticsSession,
    diagnostics_console: bool,
) -> anyhow::Result<()> {
    let plugin_dlls = parse_plugin_dll_arguments()?;
    let launch_session = if LaunchKind::classify(diagnostics_console, !plugin_dlls.is_empty())
        == LaunchKind::Ordinary
    {
        Some(LaunchSession::acquire()?)
    } else {
        None
    };
    let repeated_launch = launch_session
        .as_ref()
        .is_some_and(LaunchSession::is_repeated);
    let explicit_target = [
        "EXPLORER_INITIAL_PATH",
        explorer_import::WIN_E_ENV,
        explorer_import::INITIAL_TABS_ENV,
        explorer_import::INITIAL_TABS_FILE_ENV,
        explorer_import::RESTORE_WINDOW_ID_ENV,
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some());
    let reuse_window = explorer_app::launch_coordination::should_activate_existing_window(
        repeated_launch,
        explicit_target,
    );
    diagnostics.record_event(
        "startup",
        &[
            ("version", build.package_version),
            ("revision", build.git_revision),
            (
                "repeated_launch",
                if repeated_launch { "true" } else { "false" },
            ),
        ],
    )?;

    if reuse_window && explorer_app::launch_coordination::activate_existing_window() {
        diagnostics.record_event("startup_existing_window_activated", &[])?;
        tracing::info!("Ordinary repeated launch activated existing tabs");
        return Ok(());
    }

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
    let import = explorer_import::consume_launch_import(
        launch_session
            .as_ref()
            .is_some_and(|session| !session.is_repeated() || reuse_window),
    );
    if std::env::var_os("EXPLORER_VISUAL_FIXTURE").is_none()
        && std::env::var_os("EXPLORER_AUTO_CLOSE_MS").is_none()
    {
        win_e_hotkey::start_win_e_hotkey();
        explorer_handoff::start_handoff_server();
    }
    // A repeated-launch marker is not a navigation request. If its owner has no
    // usable main window, use the normal saved-session restore instead of C:\.
    let initial_path = None;
    let restore_window_id = explorer_import::parse_restore_window_id();
    lifecycle.run_gpui_with_launch(
        initial_path,
        import.this_window,
        import.this_pc,
        restore_window_id,
        import.imported_window_count,
        import.restore_session_windows,
    )?;
    explorer_handoff::stop_handoff_server();
    win_e_hotkey::stop_win_e_hotkey();
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
