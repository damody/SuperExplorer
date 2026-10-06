//! Report failures even when setup launched the application without a console.
use explorer_common::{DiagnosticsConfig, ErrorSeverity, record_process_error_message};
use std::path::Path;
use windows::{
    Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
    core::PCWSTR,
};

fn message(stage: &str, reason: &str, log: Option<&Path>) -> String {
    let location = log.map_or_else(
        || "無法取得記錄位置。請執行隨附的啟動診斷工具。".to_owned(),
        |path| format!("診斷記錄：{}", path.display()),
    );
    format!(
        "SuperExplorer 無法正常啟動或繼續執行。\n\n失敗階段：{stage}\n原因：{reason}\n\n{location}\n\n請保留這個訊息或診斷記錄，以便查明原因。"
    )
}

pub(super) fn report(stage: &str, reason: &str, log: Option<&Path>) {
    record_process_error_message(
        ErrorSeverity::Critical,
        "application",
        "startup_failure",
        reason,
        Some(file!()),
    );
    explorer_common::write_stderr_lossy(&format!("SuperExplorer failure ({stage}): {reason}"));
    let fallback = DiagnosticsConfig::from_environment(env!("CARGO_PKG_VERSION"))
        .error_log_candidates
        .into_iter()
        .find(|path| path.is_file());
    let text = message(stage, reason, log.or(fallback.as_deref()));
    let text: Vec<u16> = text
        .replace('\0', " ")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let title: Vec<u16> = "SuperExplorer 啟動失敗"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: both NUL-terminated UTF-16 buffers remain live throughout this
    // synchronous dialog. No GPUI window or GPU initialization is required.
    #[expect(unsafe_code, reason = "startup error dialog uses the Win32 API")]
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failure_message_includes_reason_stage_and_log_location() {
        let text = message(
            "建立視窗",
            "GPU device failed: 0x887A0004",
            Some(Path::new("C:\\logs\\error.log")),
        );
        assert!(text.contains("建立視窗"));
        assert!(text.contains("GPU device failed: 0x887A0004"));
        assert!(text.contains("C:\\logs\\error.log"));
    }
    #[test]
    fn missing_log_still_explains_how_to_collect_diagnostics() {
        assert!(message("啟動", "access denied", None).contains("啟動診斷工具"));
    }
}
