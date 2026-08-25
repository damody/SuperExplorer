//! Windows Job Object contained process adapter.

#![allow(unsafe_code)]

use std::{
    future::Future,
    io::Read,
    mem::size_of_val,
    os::windows::io::AsRawHandle,
    pin::Pin,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
    thread,
    time::{Duration, Instant},
};

use explorer_automation::{
    AutomationError, AutomationErrorKind, AutomationFuture, AutomationResult, ProcessHost,
    ProcessPolicy, ProcessRequest, ProcessResult,
};
use explorer_common::configure_background_command;
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    },
};

/// Runs every child inside a kill-on-close Job Object.
#[derive(Clone, Copy, Debug)]
pub struct WindowsJobProcessHost {
    output_limit: usize,
}

impl Default for WindowsJobProcessHost {
    fn default() -> Self {
        Self {
            output_limit: 1024 * 1024,
        }
    }
}

impl ProcessHost for WindowsJobProcessHost {
    fn run(&self, request: ProcessRequest) -> AutomationFuture<ProcessResult> {
        if let Err(error) = ProcessPolicy::validate_direct(&request.executable) {
            return Box::pin(async move { Err(error) });
        }
        JobFuture::spawn(request, self.output_limit)
    }

    fn run_script(&self, request: ProcessRequest) -> AutomationFuture<ProcessResult> {
        JobFuture::spawn(request, self.output_limit)
    }
}

struct SharedState {
    result: Option<AutomationResult<ProcessResult>>,
    waker: Option<Waker>,
    cancelled: Arc<AtomicBool>,
}

struct JobFuture {
    state: Arc<Mutex<SharedState>>,
}

impl JobFuture {
    fn spawn(request: ProcessRequest, output_limit: usize) -> AutomationFuture<ProcessResult> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(SharedState {
            result: None,
            waker: None,
            cancelled: Arc::clone(&cancelled),
        }));
        let worker_state = Arc::clone(&state);
        thread::spawn(move || {
            let result = run_contained(&request, output_limit, &cancelled);
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

impl Future for JobFuture {
    type Output = AutomationResult<ProcessResult>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let Ok(mut state) = self.state.lock() else {
            return Poll::Ready(Err(process_error("process.state")));
        };
        if let Some(result) = state.result.take() {
            Poll::Ready(result)
        } else {
            state.waker = Some(context.waker().clone());
            Poll::Pending
        }
    }
}

impl Drop for JobFuture {
    fn drop(&mut self) {
        if let Ok(state) = self.state.lock() {
            state.cancelled.store(true, Ordering::Release);
        }
    }
}

struct OwnedJob(HANDLE);

impl OwnedJob {
    fn create() -> AutomationResult<Self> {
        // SAFETY: no security attributes or name are passed; the returned handle is owned below.
        let handle = unsafe { CreateJobObjectW(None, None) }
            .map_err(|_| process_error("process.job_create"))?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: pointer and byte size match JOBOBJECT_EXTENDED_LIMIT_INFORMATION.
        unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                u32::try_from(size_of_val(&limits))
                    .map_err(|_| process_error("process.job_limit"))?,
            )
        }
        .map_err(|_| process_error("process.job_limit"))?;
        Ok(Self(handle))
    }

    fn assign(&self, child: &std::process::Child) -> AutomationResult<()> {
        let process = HANDLE(child.as_raw_handle());
        // SAFETY: both handles are valid during this synchronous assignment.
        unsafe { AssignProcessToJobObject(self.0, process) }
            .map_err(|_| process_error("process.job_assign"))
    }

    fn terminate(&self) {
        // SAFETY: self owns a valid Job Object handle; failure is best-effort during cleanup.
        let _ = unsafe { TerminateJobObject(self.0, 1) };
    }
}

impl Drop for OwnedJob {
    fn drop(&mut self) {
        // SAFETY: this handle is owned exactly once by this value.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn run_contained(
    request: &ProcessRequest,
    output_limit: usize,
    cancelled: &AtomicBool,
) -> AutomationResult<ProcessResult> {
    let job = OwnedJob::create()?;
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
    job.assign(&child)?;
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
        if cancelled.load(Ordering::Acquire) {
            job.terminate();
            let _ = child.wait();
            return Err(AutomationError::new(
                AutomationErrorKind::Cancelled,
                "process.run",
                false,
                "The process was cancelled",
            ));
        }
        if started.elapsed() >= Duration::from_millis(request.timeout_ms) {
            job.terminate();
            let _ = child.wait();
            return Err(AutomationError::new(
                AutomationErrorKind::Timeout,
                "process.run",
                true,
                "The process tree timed out",
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
    let mut kept = Vec::new();
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
        "The Windows process could not be completed",
    )
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::atomic::AtomicBool};

    use explorer_automation::{AutomationErrorKind, CorrelationId, ProcessRequest};

    use super::run_contained;

    #[test]
    fn contained_host_captures_background_console_output() {
        let request = ProcessRequest {
            executable: PathBuf::from("where.exe"),
            arguments: vec!["cmd.exe".into()],
            cwd: std::env::current_dir().expect("current directory"),
            timeout_ms: 5_000,
            correlation_id: CorrelationId::new(),
        };
        let result = run_contained(&request, 4096, &AtomicBool::new(false))
            .expect("run controlled console command");
        assert_eq!(result.exit_code, 0);
        assert!(String::from_utf8_lossy(&result.stdout).contains("cmd.exe"));
    }

    #[test]
    fn contained_host_preserves_spawn_failure() {
        let request = ProcessRequest {
            executable: PathBuf::from("missing-superexplorer-background-command.exe"),
            arguments: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            timeout_ms: 100,
            correlation_id: CorrelationId::new(),
        };
        assert!(run_contained(&request, 4096, &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn timeout_terminates_a_job_containing_a_descendant_process() {
        let request = ProcessRequest {
            executable: PathBuf::from("cmd.exe"),
            arguments: vec![
                "/D".into(),
                "/S".into(),
                "/C".into(),
                "start \"\" /B ping.exe -n 30 127.0.0.1 >nul & ping.exe -n 30 127.0.0.1 >nul"
                    .into(),
            ],
            cwd: std::env::current_dir().expect("current directory"),
            timeout_ms: 50,
            correlation_id: CorrelationId::new(),
        };
        let error =
            run_contained(&request, 1024, &AtomicBool::new(false)).expect_err("contained timeout");
        assert_eq!(error.kind, AutomationErrorKind::Timeout);
    }
}
