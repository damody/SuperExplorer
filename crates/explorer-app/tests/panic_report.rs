use std::{fs, process::Command};

#[test]
fn controlled_panic_writes_a_redacted_report_and_returns_failure() {
    if std::env::var_os("EXPLORER_PANIC_TEST_CHILD").is_some() {
        let sensitive = std::env::var("EXPLORER_PANIC_SENSITIVE").expect("sensitive marker");
        let diagnostics = explorer_common::initialize_diagnostics(
            explorer_common::DiagnosticsConfig::from_environment("0.1.0"),
        )
        .expect("initialize child diagnostics");
        explorer_common::install_panic_hook(diagnostics);
        panic!("controlled panic for diagnostics integration test at {sensitive}");
    }

    let log_root = tempfile::tempdir().expect("temporary diagnostics directory");
    let sensitive = log_root.path().join("sensitive-marker");
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .arg("--exact")
        .arg("controlled_panic_writes_a_redacted_report_and_returns_failure")
        .arg("--nocapture")
        .env("EXPLORER_LOG_DIR", log_root.path())
        .env("EXPLORER_PANIC_TEST_CHILD", "1")
        .env("EXPLORER_PANIC_SENSITIVE", &sensitive)
        .env("USERPROFILE", &sensitive)
        .output()
        .expect("run controlled panic subprocess");

    assert!(!output.status.success());
    let log = fs::read_to_string(log_root.path().join("error.log")).expect("read panic log");
    assert!(log.contains("severity=critical"));
    assert!(log.contains("operation=\"panic\""));
    assert!(log.contains("controlled panic for diagnostics integration test"));
    assert!(log.contains("version=0.1.0"));
    assert!(log.contains("thread=controlled_panic_writes_a_redacted_report_and_returns_failure"));
    assert!(log.contains("location="));
    assert!(log.contains("backtrace_available="));
    assert!(!log.contains(&sensitive.to_string_lossy().to_string()));
}
