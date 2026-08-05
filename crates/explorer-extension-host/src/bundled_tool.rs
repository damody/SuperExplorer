#![allow(unsafe_code)]

use crate::runtime_authority::{AuthorityAdapterV1, AuthorityEnvelopeV1, RuntimeAuthorityV1};
use explorer_extension_api::{
    AbiToolExecutorV1, MAX_TOOL_ARGUMENT_BYTES_V1, MAX_TOOL_ARGUMENTS_V1, MAX_TOOL_OUTPUT_BYTES_V1,
    ToolExecuteOutcomeV1, ToolExecuteRequestV1, ToolExecuteStatusV1, ToolHandleV1,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    mem::{size_of, size_of_val},
    os::windows::fs::OpenOptionsExt,
    os::windows::{ffi::OsStrExt, io::FromRawHandle},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Security::SECURITY_ATTRIBUTES,
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject,
        },
        System::{
            Pipes::CreatePipe,
            Threading::{
                CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetExitCodeProcess,
                PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOW,
                TerminateProcess, WaitForSingleObject,
            },
        },
    },
    core::{PCWSTR, PWSTR},
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
    fn terminate(&self) {
        let _ = unsafe { TerminateJobObject(self.0, 1) };
    }
}
impl Drop for OwnedJob {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

struct SuspendedProcessV1 {
    process: HANDLE,
    thread: HANDLE,
    stdout: Option<File>,
    stderr: Option<File>,
}

impl SuspendedProcessV1 {
    fn resume(&mut self) -> io::Result<()> {
        // SAFETY: the thread handle comes directly from successful CreateProcessW.
        if unsafe { ResumeThread(self.thread) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        // The primary thread handle is no longer needed after the one resume.
        let _ = unsafe { CloseHandle(self.thread) };
        self.thread = HANDLE::default();
        Ok(())
    }

    fn has_exited(&self) -> io::Result<bool> {
        // SAFETY: process remains owned by this value.
        match unsafe { WaitForSingleObject(self.process, 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }

    fn exit_code(&self) -> io::Result<i32> {
        let mut code = 0_u32;
        // SAFETY: output pointer is valid and process is signalled before this call.
        unsafe { GetExitCodeProcess(self.process, &mut code) }
            .map_err(|error| io::Error::from_raw_os_error(error.code().0))?;
        Ok(i32::from_ne_bytes(code.to_ne_bytes()))
    }

    fn terminate_and_reap(&self) {
        // SAFETY: the process handle is owned and may still be suspended.
        let _ = unsafe { TerminateProcess(self.process, 1) };
        // A finite wait here is safe because termination has already been
        // requested and prevents leaking an unassigned suspended process.
        let _ = unsafe { WaitForSingleObject(self.process, 5_000) };
    }
}

impl Drop for SuspendedProcessV1 {
    fn drop(&mut self) {
        if !self.thread.is_invalid() {
            let _ = unsafe { CloseHandle(self.thread) };
        }
        if !self.process.is_invalid() {
            let _ = unsafe { CloseHandle(self.process) };
        }
    }
}

fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty()
        && !argument
            .chars()
            .any(|character| character.is_whitespace() || character == '"')
    {
        return argument.to_owned();
    }
    let mut quoted = String::from("\"");
    let mut backslashes = 0_usize;
    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
        } else {
            if character == '"' {
                quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
            } else {
                quoted.extend(std::iter::repeat_n('\\', backslashes));
            }
            backslashes = 0;
            quoted.push(character);
        }
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

fn environment_allowlist_block() -> Vec<u16> {
    let mut variables = ["SystemRoot", "WINDIR", "TEMP", "TMP"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok().map(|value| (name, value)))
        .collect::<Vec<_>>();
    variables.sort_unstable_by_key(|(name, _)| name.to_ascii_uppercase());
    let mut block = Vec::new();
    for (name, value) in variables {
        block.extend(format!("{name}={value}").encode_utf16());
        block.push(0);
    }
    block.push(0);
    block
}

fn create_inheritable_pipe() -> io::Result<(HANDLE, HANDLE)> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: true.into(),
    };
    // SAFETY: both output pointers and the attributes record remain valid.
    unsafe { CreatePipe(&mut read, &mut write, Some(&raw const attributes), 0) }
        .map_err(|error| io::Error::from_raw_os_error(error.code().0))?;
    Ok((read, write))
}

fn close_handle(handle: HANDLE) {
    if !handle.is_invalid() {
        let _ = unsafe { CloseHandle(handle) };
    }
}

fn spawn_suspended(executable: &Path, arguments: &[String]) -> io::Result<SuspendedProcessV1> {
    use windows::Win32::Foundation::SetHandleInformation;

    let (stdout_read, stdout_write) = create_inheritable_pipe()?;
    let (stderr_read, stderr_write) = match create_inheritable_pipe() {
        Ok(pipe) => pipe,
        Err(error) => {
            close_handle(stdout_read);
            close_handle(stdout_write);
            return Err(error);
        }
    };
    let (stdin_read, stdin_write) = match create_inheritable_pipe() {
        Ok(pipe) => pipe,
        Err(error) => {
            for handle in [stdout_read, stdout_write, stderr_read, stderr_write] {
                close_handle(handle);
            }
            return Err(error);
        }
    };
    for handle in [stdout_read, stderr_read, stdin_write] {
        if let Err(error) =
            unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT.0, Default::default()) }
        {
            for candidate in [
                stdout_read,
                stdout_write,
                stderr_read,
                stderr_write,
                stdin_read,
                stdin_write,
            ] {
                close_handle(candidate);
            }
            return Err(io::Error::from_raw_os_error(error.code().0));
        }
    }

    let executable_wide = executable
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut command_line = quote_windows_argument(&executable.to_string_lossy());
    for argument in arguments {
        command_line.push(' ');
        command_line.push_str(&quote_windows_argument(argument));
    }
    let mut command_line = command_line
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let current_directory = executable.parent().unwrap_or_else(|| Path::new("."));
    let current_directory = current_directory
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let environment = environment_allowlist_block();
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESTDHANDLES,
        hStdInput: stdin_read,
        hStdOutput: stdout_write,
        hStdError: stderr_write,
        ..Default::default()
    };
    let mut information = PROCESS_INFORMATION::default();
    let created = unsafe {
        CreateProcessW(
            PCWSTR(executable_wide.as_ptr()),
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            true,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            Some(environment.as_ptr().cast()),
            PCWSTR(current_directory.as_ptr()),
            &raw const startup,
            &mut information,
        )
    };
    for handle in [stdout_write, stderr_write, stdin_read, stdin_write] {
        close_handle(handle);
    }
    if let Err(error) = created {
        close_handle(stdout_read);
        close_handle(stderr_read);
        return Err(io::Error::from_raw_os_error(error.code().0));
    }
    // SAFETY: these are unique parent-side pipe handles after successful creation.
    let stdout = unsafe { File::from_raw_handle(stdout_read.0) };
    let stderr = unsafe { File::from_raw_handle(stderr_read.0) };
    Ok(SuspendedProcessV1 {
        process: information.hProcess,
        thread: information.hThread,
        stdout: Some(stdout),
        stderr: Some(stderr),
    })
}

fn bounded_pipe(
    mut pipe: impl Read + Send + 'static,
    limit: usize,
) -> thread::JoinHandle<io::Result<Vec<u8>>> {
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
    /// Share-read-only handle retained for the handle's complete lifetime.
    /// Windows therefore rejects write/delete/name replacement of the exact
    /// attested executable while any cloned ToolHandle remains usable.
    _locked_payload: Arc<File>,
    size: u64,
    sha256: [u8; 32],
    authority: BundledToolAuthorityV1,
}

/// Opaque use-time grant minted only from one sealed contribution declaring
/// `tools.execute_bundled`. It contains no executable path or plugin bytes.
#[derive(Clone)]
pub struct BundledToolAuthorityV1 {
    runtime: Arc<RuntimeAuthorityV1>,
    envelope: AuthorityEnvelopeV1,
}

impl std::fmt::Debug for BundledToolAuthorityV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BundledToolAuthorityV1")
            .finish_non_exhaustive()
    }
}

impl BundledToolAuthorityV1 {
    pub(crate) fn from_host(
        runtime: Arc<RuntimeAuthorityV1>,
        envelope: AuthorityEnvelopeV1,
    ) -> Self {
        Self { runtime, envelope }
    }

    fn revalidate(&self) -> bool {
        self.runtime
            .revalidate(&self.envelope, AuthorityAdapterV1::Tool)
            .is_ok()
    }
}
impl AbiToolExecutorV1 for AttestedToolExecutorV1 {
    fn execute(&self, request: ToolExecuteRequestV1) -> ToolExecuteOutcomeV1 {
        let rejected = || outcome(ToolExecuteStatusV1::REJECTED, -1, Vec::new(), Vec::new());
        if !self.authority.revalidate()
            || request.arguments.len() > MAX_TOOL_ARGUMENTS_V1
            || request
                .arguments
                .iter()
                .any(|v| v.len() > MAX_TOOL_ARGUMENT_BYTES_V1)
            || request.max_output_bytes as usize > MAX_TOOL_OUTPUT_BYTES_V1
            || request.timeout_millis == 0
        {
            return rejected();
        }
        let Ok((verified_payload, size, sha256)) = open_and_attest_payload(&self.executable) else {
            return rejected();
        };
        if size != self.size || sha256 != self.sha256 {
            return rejected();
        }
        if !self.authority.revalidate() {
            return rejected();
        }
        let job = match OwnedJob::create() {
            Ok(job) => job,
            Err(_) => return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new()),
        };
        let arguments = request
            .arguments
            .iter()
            .map(|value| value.as_str().to_owned())
            .collect::<Vec<_>>();
        let mut child = match spawn_suspended(&self.executable, &arguments) {
            Ok(child) => child,
            Err(_) => return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new()),
        };
        // Keep the use-time verified object locked until CreateProcess has
        // resolved and opened the executable path.
        drop(verified_payload);
        // The child has not executed user code yet. Assignment failure closes
        // the kill-on-close job while the primary thread is still suspended.
        if unsafe { AssignProcessToJobObject(job.0, child.process) }.is_err() {
            child.terminate_and_reap();
            return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new());
        }
        if child.resume().is_err() {
            job.terminate();
            return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new());
        }
        let stdout = bounded_pipe(
            child.stdout.take().expect("owned stdout pipe"),
            request.max_output_bytes as usize,
        );
        let stderr = bounded_pipe(
            child.stderr.take().expect("owned stderr pipe"),
            request.max_output_bytes as usize,
        );
        let deadline = Instant::now() + Duration::from_millis(u64::from(request.timeout_millis));
        loop {
            if !self.authority.revalidate() {
                job.terminate();
                while child.has_exited().is_ok_and(|exited| !exited) {
                    thread::yield_now();
                }
                let _ = stdout.join();
                let _ = stderr.join();
                return outcome(ToolExecuteStatusV1::CANCELLED, -1, Vec::new(), Vec::new());
            }
            match child.has_exited() {
                Ok(true) => break,
                Ok(false) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                Ok(false) => {
                    job.terminate();
                    while child.has_exited().is_ok_and(|exited| !exited) {
                        thread::yield_now();
                    }
                    let _ = stdout.join();
                    let _ = stderr.join();
                    return outcome(ToolExecuteStatusV1::TIMED_OUT, -1, Vec::new(), Vec::new());
                }
                Err(_) => return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new()),
            }
        }
        let exit_code = match child.exit_code() {
            Ok(value) => value,
            Err(_) => return outcome(ToolExecuteStatusV1::FAILED, -1, Vec::new(), Vec::new()),
        };
        let stdout = stdout.join().ok().and_then(Result::ok).unwrap_or_default();
        let stderr = stderr.join().ok().and_then(Result::ok).unwrap_or_default();
        let limit = request.max_output_bytes as usize;
        if stdout.len().saturating_add(stderr.len()) > limit {
            return outcome(
                ToolExecuteStatusV1::OUTPUT_TRUNCATED,
                exit_code,
                stdout.into_iter().take(limit).collect(),
                Vec::new(),
            );
        }
        outcome(ToolExecuteStatusV1::COMPLETED, exit_code, stdout, stderr)
    }
}

fn attest_file(file: &File) -> io::Result<(u64, [u8; 32])> {
    let mut file = file.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "attested tool payload is not a regular file",
        ));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok((metadata.len(), digest.finalize().into()))
}

fn open_and_attest_payload(path: &PathBuf) -> io::Result<(Arc<File>, u64, [u8; 32])> {
    // FILE_SHARE_READ only: other readers and CreateProcess are permitted,
    // while write/delete opens required for payload substitution are denied.
    let file = Arc::new(OpenOptions::new().read(true).share_mode(1).open(path)?);
    let (size, sha256) = attest_file(&file)?;
    Ok((file, size, sha256))
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
pub fn mint_attested_tool_handle_v1(
    executable: PathBuf,
    authority: BundledToolAuthorityV1,
) -> Result<ToolHandleV1, io::Error> {
    if !executable.is_absolute() || !executable.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "attested tool payload unavailable",
        ));
    }
    let executable = executable.canonicalize()?;
    let (locked_payload, size, sha256) = open_and_attest_payload(&executable)?;
    Ok(ToolHandleV1::from_host(AttestedToolExecutorV1 {
        executable,
        _locked_payload: locked_payload,
        size,
        sha256,
        authority,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_authority::AuthorityClaimsV1;
    use std::io::Write;

    fn tool_authority_for(capability: &str) -> BundledToolAuthorityV1 {
        let runtime = Arc::new(RuntimeAuthorityV1::new().unwrap());
        let envelope = runtime
            .issue(AuthorityClaimsV1 {
                package_id: "tool-test".into(),
                feature_id: "tool".into(),
                interface_id: "tool".into(),
                incarnation: 1,
                capability: capability.into(),
                authorized_root_sha256: "a".repeat(64),
                location_generation: 1,
                item_generation: 1,
                refresh_generation: 1,
                container_generation: 1,
                job_generation: 1,
            })
            .unwrap();
        BundledToolAuthorityV1::from_host(runtime, envelope)
    }

    fn tool_authority() -> BundledToolAuthorityV1 {
        tool_authority_for("tools.execute_bundled")
    }

    fn request(
        test_name: &str,
        timeout_millis: u32,
        max_output_bytes: u32,
    ) -> ToolExecuteRequestV1 {
        ToolExecuteRequestV1 {
            arguments: vec![
                "--exact".into(),
                format!("bundled_tool::tests::{test_name}").into(),
                "--nocapture".into(),
            ]
            .into(),
            timeout_millis,
            max_output_bytes,
        }
    }

    #[test]
    fn child_fixture_echo() {
        if std::env::args().any(|argument| argument == "--exact") {
            assert!(std::env::var_os("PATH").is_none());
        }
        println!("shell-free-child-ok");
    }

    #[test]
    fn child_fixture_large_output() {
        println!("{}", "x".repeat(16 * 1024));
        eprintln!("{}", "y".repeat(16 * 1024));
    }

    #[test]
    fn child_fixture_slow() {
        thread::sleep(Duration::from_millis(1_000));
    }

    #[test]
    fn child_fixture_nonzero_exit() {
        if std::env::args().any(|argument| argument == "--exact") {
            panic!("intentional nonzero fixture");
        }
    }

    #[test]
    fn windows_argument_quoting_keeps_metacharacters_literal() {
        assert_eq!(quote_windows_argument("a&b.rs"), "a&b.rs");
        assert_eq!(quote_windows_argument("$(whoami).lua"), "$(whoami).lua");
        assert_eq!(quote_windows_argument("two words.rs"), "\"two words.rs\"");
        assert_eq!(
            quote_windows_argument("quote\"name.rs"),
            "\"quote\\\"name.rs\""
        );
        assert_eq!(quote_windows_argument(""), "\"\"");
    }

    #[test]
    fn suspended_job_process_completes_with_allowlisted_environment() {
        let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
        let handle = mint_attested_tool_handle_v1(executable, tool_authority()).unwrap();
        let result = handle.execute(request("child_fixture_echo", 2_000, 4_096));
        assert_eq!(result.status, ToolExecuteStatusV1::COMPLETED);
        assert_eq!(result.exit_code, 0);
        assert!(String::from_utf8_lossy(&result.stdout).contains("shell-free-child-ok"));
    }

    #[test]
    fn timeout_cancel_and_output_truncation_are_distinct_terminals() {
        let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
        let timed = mint_attested_tool_handle_v1(executable.clone(), tool_authority()).unwrap();
        assert_eq!(
            timed
                .execute(request("child_fixture_slow", 25, 4_096))
                .status,
            ToolExecuteStatusV1::TIMED_OUT
        );

        let authority = tool_authority();
        let runtime = Arc::clone(&authority.runtime);
        let cancelled = mint_attested_tool_handle_v1(executable.clone(), authority).unwrap();
        let worker =
            thread::spawn(move || cancelled.execute(request("child_fixture_slow", 2_000, 4_096)));
        thread::sleep(Duration::from_millis(500));
        runtime.revoke_feature("tool-test", "tool").unwrap();
        assert_eq!(
            worker.join().unwrap().status,
            ToolExecuteStatusV1::CANCELLED
        );

        let truncated = mint_attested_tool_handle_v1(executable, tool_authority()).unwrap();
        let result = truncated.execute(request("child_fixture_large_output", 2_000, 128));
        assert_eq!(result.status, ToolExecuteStatusV1::OUTPUT_TRUNCATED);
        assert!(result.stdout.len() <= 128);
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn spawn_failure_and_nonzero_exit_are_truthful() {
        let directory = tempfile::tempdir().unwrap();
        let invalid = directory.path().join("not-a-process.exe");
        std::fs::write(&invalid, b"not a PE image").unwrap();
        let handle = mint_attested_tool_handle_v1(invalid, tool_authority()).unwrap();
        assert_eq!(
            handle.execute(request("unused", 1_000, 1_024)).status,
            ToolExecuteStatusV1::FAILED
        );

        let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
        let handle = mint_attested_tool_handle_v1(executable, tool_authority()).unwrap();
        let result = handle.execute(request("child_fixture_nonzero_exit", 2_000, 4_096));
        assert_eq!(result.status, ToolExecuteStatusV1::COMPLETED);
        assert_ne!(result.exit_code, 0);
        assert!(String::from_utf8_lossy(&result.stderr).contains("intentional nonzero fixture"));
    }

    #[test]
    fn refuses_path_lookup_and_unbounded_request() {
        assert!(
            mint_attested_tool_handle_v1(PathBuf::from("tokei.exe"), tool_authority()).is_err()
        );
        let missing = std::env::temp_dir().join("superexplorer-missing-attested-tool.exe");
        assert!(mint_attested_tool_handle_v1(missing, tool_authority()).is_err());
        assert_eq!(MAX_TOOL_OUTPUT_BYTES_V1, 8 * 1024 * 1024);
    }

    #[test]
    fn payload_tampered_after_attestation_is_rejected_before_spawn() {
        let directory = tempfile::tempdir().unwrap();
        let payload = directory.path().join("tool.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &payload).unwrap();
        let (size, sha256) = attest_file(&File::open(&payload).unwrap()).unwrap();
        File::options()
            .append(true)
            .open(&payload)
            .unwrap()
            .write_all(b"tampered")
            .unwrap();
        let (locked_payload, _, _) = open_and_attest_payload(&payload).unwrap();
        let executor = AttestedToolExecutorV1 {
            executable: payload.clone(),
            _locked_payload: locked_payload,
            size,
            sha256,
            authority: tool_authority(),
        };
        let outcome = executor.execute(ToolExecuteRequestV1 {
            arguments: Vec::new().into(),
            timeout_millis: 1_000,
            max_output_bytes: 1_024,
        });
        assert_eq!(outcome.status, ToolExecuteStatusV1::REJECTED);
    }

    #[test]
    fn retained_payload_handle_rejects_name_substitution_until_handle_drop() {
        let directory = tempfile::tempdir().unwrap();
        let payload = directory.path().join("tool.exe");
        let moved = directory.path().join("tool-old.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &payload).unwrap();
        let handle = mint_attested_tool_handle_v1(payload.clone(), tool_authority()).unwrap();

        assert!(std::fs::rename(&payload, &moved).is_err());
        assert!(payload.is_file());
        drop(handle);
        std::fs::rename(&payload, &moved).unwrap();
        assert!(moved.is_file());
    }

    #[test]
    fn wrong_capability_and_feature_revoke_reject_before_spawn() {
        let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
        let wrong =
            mint_attested_tool_handle_v1(executable.clone(), tool_authority_for("filesystem.read"))
                .unwrap();
        let request = ToolExecuteRequestV1 {
            arguments: Vec::new().into(),
            timeout_millis: 1_000,
            max_output_bytes: 1_024,
        };
        assert_eq!(
            wrong.execute(request.clone()).status,
            ToolExecuteStatusV1::REJECTED
        );

        let authority = tool_authority();
        let runtime = Arc::clone(&authority.runtime);
        let handle = mint_attested_tool_handle_v1(executable, authority).unwrap();
        assert_eq!(runtime.revoke_feature("tool-test", "tool"), Ok(1));
        assert_eq!(
            handle.execute(request).status,
            ToolExecuteStatusV1::REJECTED
        );
    }
}
