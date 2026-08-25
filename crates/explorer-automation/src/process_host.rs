//! Direct process execution policy and bounded native adapter.

#![allow(clippy::missing_errors_doc)]

use std::{
    ffi::OsStr,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    task::{Poll, Waker},
    thread,
    time::{Duration, Instant},
};

use crate::{
    AutomationError, AutomationErrorKind, AutomationFuture, AutomationResult, ProcessHost,
    ProcessRequest, ProcessResult,
};
use explorer_common::configure_background_command;

const DEFAULT_OUTPUT_LIMIT: usize = 1024 * 1024;

/// Conservative static classification for user-authored script processes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptDeletionRisk {
    NoDeletionDetected,
    DeletionCapable,
    Indeterminate,
}

/// Validates direct executables and prepares fixed script interpreters.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessPolicy;

impl ProcessPolicy {
    /// Rejects shell hosts and script files from the direct executable API.
    pub fn validate_direct(executable: &Path) -> AutomationResult<()> {
        let name = executable
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let extension = executable
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "cmd" | "cmd.exe" | "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
        ) || matches!(extension.as_str(), "bat" | "cmd" | "ps1")
        {
            return Err(AutomationError::new(
                AutomationErrorKind::Authorization,
                "process.run",
                false,
                "Shell hosts and script files require process.run_script",
            ));
        }
        Ok(())
    }

    /// Selects a fixed interpreter and separate arguments for BAT, CMD, or PowerShell files.
    pub fn script_command(
        script: &Path,
        arguments: &[String],
    ) -> AutomationResult<(PathBuf, Vec<String>)> {
        let extension = script
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let script_text = script.to_string_lossy().into_owned();
        match extension.as_str() {
            "bat" | "cmd" => {
                let mut fixed = vec!["/D".into(), "/S".into(), "/C".into(), script_text];
                fixed.extend_from_slice(arguments);
                Ok((PathBuf::from("cmd.exe"), fixed))
            }
            "ps1" => {
                let mut fixed = vec![
                    "-NoLogo".into(),
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-File".into(),
                    script_text,
                ];
                fixed.extend_from_slice(arguments);
                Ok((PathBuf::from("powershell.exe"), fixed))
            }
            _ => Err(AutomationError::new(
                AutomationErrorKind::InvalidInput,
                "process.run_script",
                false,
                "Only BAT, CMD, and PowerShell scripts are supported",
            )),
        }
    }

    /// Conservatively identifies common deletion syntax; ambiguous dynamic scripts need consent.
    #[must_use]
    pub fn scan_script(script: &str, extension: &str) -> ScriptDeletionRisk {
        let normalized = script.to_ascii_lowercase();
        let known = match extension.to_ascii_lowercase().as_str() {
            "bat" | "cmd" => [" del ", "erase ", "rd ", "rmdir "].iter().any(|token| {
                normalized.starts_with(token.trim_start()) || normalized.contains(token)
            }),
            "ps1" => ["remove-item", " ri ", " del ", "erase "]
                .iter()
                .any(|token| normalized.contains(token)),
            _ => return ScriptDeletionRisk::Indeterminate,
        };
        if known {
            ScriptDeletionRisk::DeletionCapable
        } else if normalized.contains("invoke-expression")
            || normalized.contains("iex ")
            || normalized.contains("call ")
            || normalized.contains('%')
        {
            ScriptDeletionRisk::Indeterminate
        } else {
            ScriptDeletionRisk::NoDeletionDetected
        }
    }
}

/// Direct executable host with timeout and independently bounded stdout/stderr capture.
#[derive(Clone, Copy, Debug)]
pub struct NativeProcessHost {
    output_limit: usize,
}

impl Default for NativeProcessHost {
    fn default() -> Self {
        Self {
            output_limit: DEFAULT_OUTPUT_LIMIT,
        }
    }
}

impl NativeProcessHost {
    #[must_use]
    pub fn with_output_limit(output_limit: usize) -> Self {
        Self {
            output_limit: output_limit.max(1),
        }
    }
}

impl ProcessHost for NativeProcessHost {
    fn run(&self, request: ProcessRequest) -> AutomationFuture<ProcessResult> {
        if let Err(error) = ProcessPolicy::validate_direct(&request.executable) {
            return Box::pin(async move { Err(error) });
        }
        ProcessFuture::spawn(request, self.output_limit)
    }

    fn run_script(&self, request: ProcessRequest) -> AutomationFuture<ProcessResult> {
        ProcessFuture::spawn(request, self.output_limit)
    }
}

struct ProcessFutureState {
    result: Option<AutomationResult<ProcessResult>>,
    waker: Option<Waker>,
}

struct ProcessFuture {
    state: Arc<Mutex<ProcessFutureState>>,
}

impl ProcessFuture {
    fn spawn(request: ProcessRequest, output_limit: usize) -> AutomationFuture<ProcessResult> {
        let state = Arc::new(Mutex::new(ProcessFutureState {
            result: None,
            waker: None,
        }));
        let worker_state = Arc::clone(&state);
        thread::spawn(move || {
            let result = run_blocking(&request, output_limit);
            if let Ok(mut state) = worker_state.lock() {
                state.result = Some(result);
                if let Some(waker) = state.waker.take() {
                    waker.wake();
                }
            }
        });
        Box::pin(Self { state })
    }
}

impl Future for ProcessFuture {
    type Output = AutomationResult<ProcessResult>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        let Ok(mut state) = self.state.lock() else {
            return Poll::Ready(Err(process_error("process.run")));
        };
        if let Some(result) = state.result.take() {
            Poll::Ready(result)
        } else {
            state.waker = Some(context.waker().clone());
            Poll::Pending
        }
    }
}

use std::future::Future;

fn run_blocking(request: &ProcessRequest, output_limit: usize) -> AutomationResult<ProcessResult> {
    let mut command = Command::new(&request.executable);
    command
        .args(&request.arguments)
        .current_dir(&request.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_background_command(&mut command);
    let mut child = command
        .spawn()
        .map_err(|_| process_error("process.spawn"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| process_error("process.stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| process_error("process.stderr"))?;
    let stdout_reader = thread::spawn(move || bounded_read(stdout, output_limit));
    let stderr_reader = thread::spawn(move || bounded_read(stderr, output_limit));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| process_error("process.wait"))?
        {
            break status;
        }
        if started.elapsed() >= Duration::from_millis(request.timeout_ms) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AutomationError::new(
                AutomationErrorKind::Timeout,
                "process.run",
                true,
                "The process timed out",
            )
            .with_correlation(request.correlation_id));
        }
        thread::sleep(Duration::from_millis(5));
    };
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| process_error("process.stdout"))?;
    let (stderr, stderr_truncated) = stderr_reader
        .join()
        .map_err(|_| process_error("process.stderr"))?;
    Ok(ProcessResult {
        exit_code: status.code().unwrap_or(-1),
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
    })
}

fn bounded_read(mut reader: impl Read, limit: usize) -> (Vec<u8>, bool) {
    let mut kept = Vec::with_capacity(limit.min(8 * 1024));
    let mut buffer = [0_u8; 8 * 1024];
    let mut truncated = false;
    while let Ok(count) = reader.read(&mut buffer) {
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..count.min(remaining)]);
        truncated |= count > remaining;
    }
    (kept, truncated)
}

fn process_error(operation: &'static str) -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::Process,
        operation,
        true,
        "The process could not be completed",
    )
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        time::Duration,
    };

    use crate::{CorrelationId, ProcessRequest};

    use super::{ProcessPolicy, ScriptDeletionRisk, run_blocking};

    #[test]
    fn direct_policy_rejects_shells_and_script_files() {
        for path in ["cmd.exe", "powershell.exe", "tool.ps1", "tool.bat"] {
            assert!(ProcessPolicy::validate_direct(Path::new(path)).is_err());
        }
        assert!(ProcessPolicy::validate_direct(Path::new("tool.exe")).is_ok());
    }

    #[test]
    fn scripts_use_fixed_interpreters_and_conservative_deletion_scan() {
        let (host, args) = ProcessPolicy::script_command(
            Path::new("tools/summarize.ps1"),
            &["-Input".into(), "note.txt".into()],
        )
        .expect("PowerShell plan");
        assert_eq!(host, PathBuf::from("powershell.exe"));
        assert!(args.iter().any(|value| value == "-NoProfile"));
        assert_eq!(
            ProcessPolicy::scan_script("Remove-Item $path", "ps1"),
            ScriptDeletionRisk::DeletionCapable
        );
        assert_eq!(
            ProcessPolicy::scan_script("Invoke-Expression $dynamic", "ps1"),
            ScriptDeletionRisk::Indeterminate
        );
    }

    #[test]
    fn native_host_captures_background_console_output() {
        let request = ProcessRequest {
            executable: PathBuf::from("where.exe"),
            arguments: vec!["cmd.exe".into()],
            cwd: std::env::current_dir().expect("current directory"),
            timeout_ms: u64::try_from(Duration::from_secs(5).as_millis()).expect("timeout"),
            correlation_id: CorrelationId::new(),
        };
        let result = run_blocking(&request, 4096).expect("run controlled console command");
        assert_eq!(result.exit_code, 0);
        assert!(String::from_utf8_lossy(&result.stdout).contains("cmd.exe"));
    }

    #[test]
    fn native_host_preserves_spawn_failure() {
        let request = ProcessRequest {
            executable: PathBuf::from("missing-superexplorer-background-command.exe"),
            arguments: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            timeout_ms: 100,
            correlation_id: CorrelationId::new(),
        };
        assert!(run_blocking(&request, 4096).is_err());
    }
}
