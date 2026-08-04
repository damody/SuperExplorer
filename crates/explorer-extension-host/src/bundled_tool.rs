#![allow(unsafe_code)]

use explorer_extension_api::{
    AbiToolExecutorV1, MAX_TOOL_ARGUMENT_BYTES_V1, MAX_TOOL_ARGUMENTS_V1, MAX_TOOL_OUTPUT_BYTES_V1,
    ToolExecuteOutcomeV1, ToolExecuteRequestV1, ToolExecuteStatusV1, ToolHandleV1,
};
use std::{
    io::Read,
    mem::size_of_val,
    os::windows::io::AsRawHandle,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    },
};

struct OwnedJob(HANDLE);
impl OwnedJob {
    fn create() -> windows::core::Result<Self> {
        // SAFETY: unnamed Job Object with no caller-provided pointers.
        let handle = unsafe { CreateJobObjectW(None, None) }?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the information class, pointer and size describe `limits` exactly.
        unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of_val(&limits) as u32,
            )
        }?;
        Ok(Self(handle))
    }
    fn assign(&self, child: &std::process::Child) -> windows::core::Result<()> {
        // SAFETY: both handles remain live for the synchronous call.
        unsafe { AssignProcessToJobObject(self.0, HANDLE(child.as_raw_handle())) }
    }
    fn terminate(&self) {
        let _ = unsafe { TerminateJobObject(self.0, 1) };
    }
}
impl Drop for OwnedJob {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn bounded_pipe(
    mut pipe: impl Read + Send + 'static,
    limit: usize,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.by_ref()
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

#[derive(Clone)]
struct AttestedToolExecutorV1 {
    executable: PathBuf,
}
impl AbiToolExecutorV1 for AttestedToolExecutorV1 {
    fn execute(&self, request: ToolExecuteRequestV1) -> ToolExecuteOutcomeV1 {
        let rejected = || outcome(ToolExecuteStatusV1::REJECTED, -1, Vec::new(), Vec::new());
        if request.arguments.len() > MAX_TOOL_ARGUMENTS_V1
            || request
                .arguments
                .iter()
                .any(|v| v.len() > MAX_TOOL_ARGUMENT_BYTES_V1)
            || request.max_output_bytes as usize > MAX_TOOL_OUTPUT_BYTES_V1
            || request.timeout_millis == 0
        {
            return rejected();
        }
        let job = match OwnedJob::create() {
            Ok(job) => job,
            Err(_) => return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new()),
        };
        let mut child = match Command::new(&self.executable)
            .args(request.arguments.iter().map(|v| v.as_str()))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new()),
        };
        if job.assign(&child).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new());
        }
        let stdout = bounded_pipe(
            child.stdout.take().expect("piped stdout"),
            request.max_output_bytes as usize,
        );
        let stderr = bounded_pipe(
            child.stderr.take().expect("piped stderr"),
            request.max_output_bytes as usize,
        );
        let deadline = Instant::now() + Duration::from_millis(u64::from(request.timeout_millis));
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                Ok(None) => {
                    job.terminate();
                    let _ = child.wait();
                    let _ = stdout.join();
                    let _ = stderr.join();
                    return outcome(ToolExecuteStatusV1::TIMED_OUT, -1, Vec::new(), Vec::new());
                }
                Err(_) => return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new()),
            }
        }
        let status = match child.wait() {
            Ok(v) => v,
            Err(_) => return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new()),
        };
        let stdout = stdout.join().ok().and_then(Result::ok).unwrap_or_default();
        let stderr = stderr.join().ok().and_then(Result::ok).unwrap_or_default();
        let limit = request.max_output_bytes as usize;
        if stdout.len().saturating_add(stderr.len()) > limit {
            return rejected();
        }
        outcome(
            ToolExecuteStatusV1::COMPLETED,
            status.code().unwrap_or(-1),
            stdout,
            stderr,
        )
    }
}
fn outcome(
    status: ToolExecuteStatusV1,
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> ToolExecuteOutcomeV1 {
    ToolExecuteOutcomeV1 {
        status,
        exit_code,
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

/// Mint only after the package validator has checked target, digest, size, and license inventory.
pub fn mint_attested_tool_handle_v1(executable: PathBuf) -> Result<ToolHandleV1, std::io::Error> {
    if !executable.is_absolute() || !executable.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "attested tool payload unavailable",
        ));
    }
    Ok(ToolHandleV1::from_host(AttestedToolExecutorV1 {
        executable,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_path_lookup_and_unbounded_request() {
        assert!(mint_attested_tool_handle_v1(PathBuf::from("tokei.exe")).is_err());
        assert_eq!(MAX_TOOL_OUTPUT_BYTES_V1, 8 * 1024 * 1024);
    }
}
