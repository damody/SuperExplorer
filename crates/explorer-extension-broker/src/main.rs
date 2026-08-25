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
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use explorer_common::configure_background_command;
use explorer_extension_protocol::{
    BrokerRequestId, Frame, FrameDecoder, MessageKind, PROTOCOL_VERSION, SessionNonce,
    StartPayload, authenticate,
};
use std::{
    io::{BufRead as _, Read as _, Write as _},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const MAXIMUM_FRAME: usize = 4 * 1024 * 1024;

fn main() {
    if std::env::args().any(|arg| arg == "--version-json") {
        println!(
            r#"{{"protocol":{PROTOCOL_VERSION},"build":"{}","arch":"x64","role":"supervisor"}}"#,
            env!("CARGO_PKG_VERSION")
        );
        return;
    }
    if run().is_err() {
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let nonce = nonce_from_environment()?;
    let mut decoder = FrameDecoder::new(MAXIMUM_FRAME);
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let mut buffer = [0_u8; 16 * 1024];
    let mut handshaken = false;
    let mut last_request_id = 0_u64;
    let mut prepared_worker: Option<PreparedWorker> = None;
    let mut active_preview: Option<ActivePreview> = None;
    let mut preview_quarantine =
        explorer_extension_broker::QuarantineRegistry::new(2, Duration::from_secs(60), 256);
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            decoder.finish()?;
            return Ok(());
        }
        decoder.push(&buffer[..count])?;
        while let Some(request) = decoder.next_frame()? {
            authenticate(&request, nonce)?;
            let request_number = request.request_id.0;
            let (response, shutdown) = match request.kind {
                MessageKind::Hello if !handshaken && request_number == 0 => {
                    handshaken = true;
                    prepared_worker = prepare_worker().ok();
                    (
                        Frame::new(
                            MessageKind::HelloAck,
                            0,
                            nonce,
                            request.request_id,
                            compatibility_marker().into_bytes(),
                        ),
                        false,
                    )
                }
                MessageKind::Start if handshaken && request_number > last_request_id => {
                    last_request_id = request_number;
                    let preview_digest = preview_handler_digest(&request);
                    let start_payload = StartPayload::decode(&request.payload).ok();
                    let preview_session_start = start_payload.as_ref().is_some_and(|payload| {
                        payload.operation == explorer_extension_protocol::OperationClass::Preview
                            && payload.flags & 0x4000_0000 != 0
                    });
                    let preview_session_command = start_payload.as_ref().is_some_and(|payload| {
                        payload.operation == explorer_extension_protocol::OperationClass::Preview
                            && payload.flags & 0x2000_0000 != 0
                    });
                    let response = if preview_session_command {
                        let unload = start_payload.as_ref().is_some_and(|payload| {
                            matches!(
                                explorer_extension_protocol::PreviewMessage::decode(
                                    &payload.descriptor
                                ),
                                Ok(explorer_extension_protocol::PreviewMessage::Unload { .. })
                            )
                        });
                        let response =
                            run_preview_command(&request, nonce, active_preview.as_mut());
                        if unload {
                            drop(active_preview.take());
                        }
                        response
                    } else if preview_digest.as_ref().is_some_and(|digest| {
                        preview_quarantine.is_quarantined(digest, Instant::now())
                    }) {
                        terminal(nonce, request.request_id, "preview-quarantined")
                    } else if preview_session_start {
                        drop(active_preview.take());
                        let worker = prepared_worker.take().or_else(|| prepare_worker().ok());
                        let replacement = std::thread::spawn(prepare_worker);
                        let (response, session) =
                            start_preview_session(&request, nonce, &mut output, worker);
                        active_preview = session;
                        prepared_worker = replacement.join().ok().and_then(Result::ok);
                        response
                    } else {
                        let worker = prepared_worker.take().or_else(|| prepare_worker().ok());
                        let replacement = std::thread::spawn(prepare_worker);
                        let response = run_worker(&request, nonce, &mut output, worker);
                        prepared_worker = replacement.join().ok().and_then(Result::ok);
                        if let Some(digest) = preview_digest
                            && preview_terminal_is_failure(&response)
                        {
                            preview_quarantine.record_failure(digest, Instant::now());
                        }
                        response
                    };
                    (response, false)
                }
                MessageKind::Shutdown if handshaken && request_number > last_request_id => {
                    last_request_id = request_number;
                    active_preview = None;
                    (
                        Frame::new(
                            MessageKind::Terminal,
                            0,
                            nonce,
                            request.request_id,
                            b"shutdown".to_vec(),
                        ),
                        true,
                    )
                }
                MessageKind::Cancel if handshaken && request_number > last_request_id => {
                    last_request_id = request_number;
                    (terminal(nonce, request.request_id, "cancelled"), false)
                }
                _ => (terminal(nonce, request.request_id, "invalid-request"), true),
            };
            output.write_all(&response.encode(MAXIMUM_FRAME)?)?;
            output.flush()?;
            if shutdown {
                return Ok(());
            }
        }
    }
}

struct ActivePreview {
    worker: PreparedWorker,
    responses: mpsc::Receiver<String>,
    _reader: std::thread::JoinHandle<()>,
}

fn start_preview_session(
    request: &Frame,
    nonce: SessionNonce,
    output: &mut impl std::io::Write,
    worker: Option<PreparedWorker>,
) -> (Frame, Option<ActivePreview>) {
    let Some(mut worker) = worker else {
        return (
            terminal(nonce, request.request_id, "worker-spawn-failed"),
            None,
        );
    };
    let worker_pid = worker.child.id();
    let Some(input) = worker.input.as_mut() else {
        return (
            terminal_with_feature(nonce, request.request_id, worker_pid, "worker-pipe-failed"),
            None,
        );
    };
    if write_worker_packet(input, &request.payload).is_err() {
        return (
            terminal_with_feature(nonce, request.request_id, worker_pid, "worker-disconnect"),
            None,
        );
    }
    let progress = Frame::new(
        MessageKind::Progress,
        worker_pid,
        nonce,
        request.request_id,
        b"preview-worker-started".to_vec(),
    );
    if progress
        .encode(MAXIMUM_FRAME)
        .ok()
        .and_then(|frame| output.write_all(&frame).ok())
        .is_none()
        || output.flush().is_err()
    {
        return (
            terminal_with_feature(nonce, request.request_id, worker_pid, "client-disconnect"),
            None,
        );
    }
    let Some(stdout) = worker.output.take() else {
        return (
            terminal_with_feature(nonce, request.request_id, worker_pid, "worker-pipe-failed"),
            None,
        );
    };
    let (sender, responses) = mpsc::sync_channel(8);
    let reader = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stdout);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    let line = line.trim_end_matches(['\r', '\n']).to_owned();
                    if sender.send(line).is_err() {
                        return;
                    }
                }
            }
        }
    });
    match responses.recv_timeout(Duration::from_secs(10)) {
        Ok(response) if response.starts_with("preview-ready:") => (
            Frame::new(
                MessageKind::Terminal,
                worker_pid,
                nonce,
                request.request_id,
                response.into_bytes(),
            ),
            Some(ActivePreview {
                worker,
                responses,
                _reader: reader,
            }),
        ),
        Ok(response) => (
            Frame::new(
                MessageKind::Terminal,
                worker_pid,
                nonce,
                request.request_id,
                response.into_bytes(),
            ),
            None,
        ),
        Err(_) => (
            terminal_with_feature(nonce, request.request_id, worker_pid, "timeout"),
            None,
        ),
    }
}

fn run_preview_command(
    request: &Frame,
    nonce: SessionNonce,
    active: Option<&mut ActivePreview>,
) -> Frame {
    let Some(active) = active else {
        return terminal(nonce, request.request_id, "preview-not-active");
    };
    let Ok(start) = StartPayload::decode(&request.payload) else {
        return terminal(nonce, request.request_id, "preview-malformed");
    };
    if explorer_extension_protocol::PreviewMessage::decode(&start.descriptor).is_err() {
        return terminal(nonce, request.request_id, "preview-malformed");
    }
    let worker_pid = active.worker.child.id();
    let Some(input) = active.worker.input.as_mut() else {
        return terminal_with_feature(nonce, request.request_id, worker_pid, "worker-disconnect");
    };
    if write_worker_packet(input, &start.descriptor).is_err() {
        return terminal_with_feature(nonce, request.request_id, worker_pid, "worker-disconnect");
    }
    match active.responses.recv_timeout(Duration::from_secs(2)) {
        Ok(response) => Frame::new(
            MessageKind::Terminal,
            worker_pid,
            nonce,
            request.request_id,
            response.into_bytes(),
        ),
        Err(_) => terminal_with_feature(nonce, request.request_id, worker_pid, "timeout"),
    }
}

fn write_worker_packet(output: &mut impl std::io::Write, payload: &[u8]) -> std::io::Result<()> {
    let length = u32::try_from(payload.len())
        .map_err(|_| std::io::Error::other("worker packet is oversized"))?;
    output.write_all(&length.to_le_bytes())?;
    output.write_all(payload)?;
    output.flush()
}

fn preview_handler_digest(request: &Frame) -> Option<String> {
    let payload = StartPayload::decode(&request.payload).ok()?;
    if payload.operation != explorer_extension_protocol::OperationClass::Preview
        || payload.flags & 0x2000_0000 != 0
    {
        return None;
    }
    let identity = if payload.flags & 0x4000_0000 != 0 {
        explorer_extension_protocol::PreviewStartPayload::decode(&payload.descriptor)
            .ok()?
            .item_descriptor
    } else {
        payload.descriptor
    };
    Some({
        let hash = identity
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
            });
        format!("preview:{hash:016x}")
    })
}

fn preview_terminal_is_failure(response: &Frame) -> bool {
    response.payload == b"preview-unavailable"
        || response.payload == b"preview-malformed"
        || response.payload == b"worker-crash"
        || response.payload == b"worker-disconnect"
        || response.payload == b"timeout"
}

fn compatibility_marker() -> String {
    format!(
        r#"{{"protocol":{},"build":"{}","arch":"x64","role":"supervisor","pid":{}}}"#,
        PROTOCOL_VERSION,
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    )
}

fn run_worker(
    request: &Frame,
    nonce: SessionNonce,
    output: &mut impl std::io::Write,
    worker: Option<PreparedWorker>,
) -> Frame {
    let Ok(payload) = StartPayload::decode(&request.payload) else {
        return terminal(nonce, request.request_id, "malformed-start");
    };
    let Some(mut worker) = worker else {
        return terminal(nonce, request.request_id, "worker-spawn-failed");
    };
    let worker_pid = worker.child.id();
    let Some(mut input) = worker.input.take() else {
        return terminal_with_feature(nonce, request.request_id, worker_pid, "worker-pipe-failed");
    };
    let Ok(worker_payload) = payload.encode() else {
        return terminal_with_feature(nonce, request.request_id, worker_pid, "malformed-start");
    };
    let Ok(worker_payload_length) = u32::try_from(worker_payload.len()) else {
        return terminal_with_feature(nonce, request.request_id, worker_pid, "malformed-start");
    };
    if input
        .write_all(&worker_payload_length.to_le_bytes())
        .and_then(|()| input.write_all(&worker_payload))
        .is_err()
    {
        return terminal_with_feature(nonce, request.request_id, worker_pid, "worker-disconnect");
    }
    drop(input);
    let progress = Frame::new(
        MessageKind::Progress,
        worker_pid,
        nonce,
        request.request_id,
        b"worker-started".to_vec(),
    );
    let Ok(progress) = progress.encode(MAXIMUM_FRAME) else {
        return terminal_with_feature(nonce, request.request_id, worker_pid, "protocol");
    };
    if output
        .write_all(&progress)
        .and_then(|()| output.flush())
        .is_err()
    {
        return terminal_with_feature(nonce, request.request_id, worker_pid, "client-disconnect");
    }
    let deadline = Instant::now()
        + match payload.operation {
            explorer_extension_protocol::OperationClass::ContextMenu => Duration::from_secs(300),
            // Image providers can legitimately take several seconds on a cold cache or for a
            // large JPEG. They still run in a disposable worker, so this does not block the UI
            // or weaken crash isolation.
            explorer_extension_protocol::OperationClass::Thumbnail => Duration::from_secs(8),
            _ => Duration::from_secs(3),
        };
    loop {
        match worker.child.try_wait() {
            Ok(Some(status)) if status.success() => {
                let Some(output) = worker.output.take() else {
                    return terminal_with_feature(
                        nonce,
                        request.request_id,
                        worker_pid,
                        "worker-disconnect",
                    );
                };
                let mut bytes = Vec::new();
                if output
                    .take((MAXIMUM_FRAME + 1) as u64)
                    .read_to_end(&mut bytes)
                    .is_err()
                    || bytes.len() > MAXIMUM_FRAME
                {
                    return terminal_with_feature(
                        nonce,
                        request.request_id,
                        worker_pid,
                        "worker-oversized",
                    );
                }
                return Frame::new(
                    MessageKind::Terminal,
                    worker_pid,
                    nonce,
                    request.request_id,
                    if bytes.is_empty() {
                        b"success".to_vec()
                    } else {
                        bytes
                    },
                );
            }
            Ok(Some(_)) => {
                return terminal_with_feature(
                    nonce,
                    request.request_id,
                    worker_pid,
                    "worker-crash",
                );
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = worker.child.kill();
                let _ = worker.child.wait();
                return terminal_with_feature(nonce, request.request_id, worker_pid, "timeout");
            }
            Err(_) => {
                return terminal_with_feature(
                    nonce,
                    request.request_id,
                    worker_pid,
                    "worker-disconnect",
                );
            }
        }
    }
}

struct PreparedWorker {
    child: std::process::Child,
    input: Option<std::process::ChildStdin>,
    output: Option<std::process::ChildStdout>,
    _job: WorkerJob,
}

impl Drop for PreparedWorker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn prepare_worker() -> Result<PreparedWorker, ()> {
    let executable = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join("explorer-extension-worker.exe"))
        })
        .ok_or(())?;
    let mut command = Command::new(executable);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_background_command(&mut command);
    let job = WorkerJob::create().map_err(|_| ())?;
    let mut child = command.spawn().map_err(|_| ())?;
    if job.assign(&child).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Err(());
    }
    let Some(input) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(());
    };
    let Some(output) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(());
    };
    Ok(PreparedWorker {
        child,
        input: Some(input),
        output: Some(output),
        _job: job,
    })
}

#[cfg(windows)]
struct WorkerJob(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "WorkerJob uniquely owns a kernel handle whose operations are valid across Windows threads"
)]
unsafe impl Send for WorkerJob {}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Windows Job Object creation, limit configuration, assignment, and handle cleanup require audited FFI"
)]
impl WorkerJob {
    fn create() -> windows::core::Result<Self> {
        use std::mem::size_of_val;
        use windows::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOB_OBJECT_LIMIT_PROCESS_TIME,
            JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        };
        let handle = unsafe { CreateJobObjectW(None, None) }?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_PROCESS_TIME
            // A user-selected Shell verb may legitimately launch 7zG, Code, Git, or another
            // external application. Without silent breakaway that process inherits this worker's
            // one-process Job and CreateProcess fails with ERROR_NOT_ENOUGH_QUOTA (1816).
            | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        limits.BasicLimitInformation.PerProcessUserTimeLimit = 50_000_000;
        limits.ProcessMemoryLimit = 256 * 1024 * 1024;
        unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                u32::try_from(size_of_val(&limits)).unwrap_or(u32::MAX),
            )
        }?;
        Ok(Self(handle))
    }

    fn assign(&self, child: &std::process::Child) -> windows::core::Result<()> {
        use std::os::windows::io::AsRawHandle as _;
        use windows::Win32::{Foundation::HANDLE, System::JobObjects::AssignProcessToJobObject};
        unsafe { AssignProcessToJobObject(self.0, HANDLE(child.as_raw_handle())) }
    }
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the uniquely owned Job Object handle is closed exactly once"
)]
impl Drop for WorkerJob {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(not(windows))]
struct WorkerJob;

#[cfg(not(windows))]
impl WorkerJob {
    fn create() -> std::io::Result<Self> {
        Ok(Self)
    }
    fn assign(&self, _child: &std::process::Child) -> std::io::Result<()> {
        Ok(())
    }
}

fn terminal(nonce: SessionNonce, id: BrokerRequestId, text: &str) -> Frame {
    terminal_with_feature(nonce, id, 0, text)
}

fn terminal_with_feature(
    nonce: SessionNonce,
    id: BrokerRequestId,
    feature_bits: u32,
    text: &str,
) -> Frame {
    Frame::new(
        MessageKind::Terminal,
        feature_bits,
        nonce,
        id,
        text.as_bytes().to_vec(),
    )
}

fn nonce_from_environment() -> Result<SessionNonce, Box<dyn std::error::Error>> {
    let text = std::env::var("EXPLORER_BROKER_NONCE")?;
    if text.len() != 32 {
        return Err("invalid nonce length".into());
    }
    let mut bytes = [0_u8; 16];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)?;
    }
    Ok(SessionNonce(bytes))
}

#[cfg(all(test, windows))]
mod tests {
    use std::{process::Command, time::Duration};

    use super::WorkerJob;

    #[test]
    fn worker_job_allows_user_invoked_shell_command_child_to_break_away() {
        use std::os::windows::process::CommandExt as _;

        let script = concat!(
            "$ErrorActionPreference='Stop'; ",
            "Start-Sleep -Milliseconds 500; ",
            "$child=Start-Process -FilePath $env:ComSpec ",
            "-ArgumentList '/d','/c','exit','0' -Wait -PassThru; ",
            "exit $child.ExitCode"
        );
        let mut parent = Command::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x0800_0000)
            .spawn()
            .expect("spawn controlled worker parent");
        let job = WorkerJob::create().expect("create constrained worker job");
        job.assign(&parent)
            .expect("assign controlled worker parent");

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = parent.try_wait().expect("poll controlled worker parent") {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "controlled worker parent timed out"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        assert!(
            status.success(),
            "an explicitly launched external command must not fail with ERROR_NOT_ENOUGH_QUOTA"
        );
    }
}
