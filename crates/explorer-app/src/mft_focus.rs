//! Versioned focus-lease wire contract and authorization policy.

use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::{
    ffi::c_void,
    ptr,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[cfg(windows)]
use crate::mft_persistence::{FocusLeaseRegistryV1, MonotonicMillis};

#[cfg(windows)]
use windows::Win32::{
    Foundation::{CloseHandle as WinCloseHandle, HANDLE, WAIT_OBJECT_0},
    System::{
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        Threading::{CreateEventW, WaitForSingleObject},
    },
};

pub(crate) const FOCUS_PIPE_NAME: &str = r"\\.\pipe\SuperExplorerMftFocusLeaseV1";
pub(crate) const FOCUS_MAGIC: u32 = u32::from_le_bytes(*b"SEFL");
pub(crate) const FOCUS_SCHEMA: u16 = 1;
pub(crate) const FOCUS_FRAME_BYTES: usize = 32;
pub(crate) const MAX_FOCUS_CONNECTIONS: usize = 32;
#[cfg(windows)]
const FOCUS_LEASE_TTL: Duration = Duration::from_secs(15);
#[cfg(windows)]
const FRAME_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub(crate) enum FocusOperationV1 {
    AcquireOrRenew = 1,
    Release = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FocusFrameV1 {
    pub(crate) operation: FocusOperationV1,
    pub(crate) lease_id: u128,
    pub(crate) sequence: u64,
}

impl FocusFrameV1 {
    pub(crate) fn encode(self) -> [u8; FOCUS_FRAME_BYTES] {
        let mut bytes = [0_u8; FOCUS_FRAME_BYTES];
        bytes[0..4].copy_from_slice(&FOCUS_MAGIC.to_le_bytes());
        bytes[4..6].copy_from_slice(&FOCUS_SCHEMA.to_le_bytes());
        bytes[6..8].copy_from_slice(&(self.operation as u16).to_le_bytes());
        bytes[8..24].copy_from_slice(&self.lease_id.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.sequence.to_le_bytes());
        bytes
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        let bytes: &[u8; FOCUS_FRAME_BYTES] = bytes
            .try_into()
            .map_err(|_| "invalid focus lease frame length")?;
        if u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) != FOCUS_MAGIC
            || u16::from_le_bytes([bytes[4], bytes[5]]) != FOCUS_SCHEMA
        {
            return Err("invalid focus lease frame header");
        }
        let operation = match u16::from_le_bytes([bytes[6], bytes[7]]) {
            1 => FocusOperationV1::AcquireOrRenew,
            2 => FocusOperationV1::Release,
            _ => return Err("invalid focus lease operation"),
        };
        let mut lease_bytes = [0_u8; 16];
        lease_bytes.copy_from_slice(&bytes[8..24]);
        let lease_id = u128::from_le_bytes(lease_bytes);
        if lease_id == 0 {
            return Err("invalid focus lease identifier");
        }
        Ok(Self {
            operation,
            lease_id,
            sequence: u64::from_le_bytes([
                bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30],
                bytes[31],
            ]),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FocusClientIdentityV1 {
    pub process_id: u32,
    pub process_creation_100ns: u64,
    pub session_id: u32,
    pub user_sid: Vec<u8>,
    pub image_path: PathBuf,
    pub image_file_identity: u128,
}

pub fn authorize_focus_client(
    client: &FocusClientIdentityV1,
    active_session_id: u32,
    active_user_sid: &[u8],
    protected_image_path: &Path,
    protected_image_file_identity: u128,
) -> Result<u64, &'static str> {
    if client.process_id == 0 || client.process_creation_100ns == 0 {
        return Err("invalid focus client process identity");
    }
    if client.session_id != active_session_id || client.user_sid != active_user_sid {
        return Err("focus client is outside the active interactive identity");
    }
    if client.image_path != protected_image_path
        || client.image_file_identity != protected_image_file_identity
    {
        return Err("focus client is not the protected installed image");
    }
    Ok((u64::from(client.process_id) << 32) ^ client.process_creation_100ns)
}

#[cfg(windows)]
const INVALID_HANDLE_VALUE: isize = -1;
#[cfg(windows)]
const ERROR_PIPE_CONNECTED: u32 = 535;
#[cfg(windows)]
const ERROR_MORE_DATA: u32 = 234;
#[cfg(windows)]
const ERROR_IO_PENDING: u32 = 997;

#[cfg(windows)]
#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: *mut c_void,
    inherit_handle: i32,
}

#[cfg(windows)]
#[link(name = "kernel32")]
#[expect(
    unsafe_code,
    reason = "focus lease transport uses Win32 named-pipe and handle APIs"
)]
// SAFETY: These declarations match the documented system ABI. Callers keep
// referenced UTF-16 names, buffers, byte counts, and OVERLAPPED values live.
unsafe extern "system" {
    fn CreateNamedPipeW(
        name: *const u16,
        open_mode: u32,
        pipe_mode: u32,
        max_instances: u32,
        out_buffer_size: u32,
        in_buffer_size: u32,
        default_timeout: u32,
        security_attributes: *const SecurityAttributes,
    ) -> isize;
    fn ConnectNamedPipe(pipe: isize, overlapped: *mut c_void) -> i32;
    fn DisconnectNamedPipe(pipe: isize) -> i32;
    fn ReadFile(
        handle: isize,
        buffer: *mut c_void,
        bytes: u32,
        read: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn WriteFile(
        handle: isize,
        buffer: *const c_void,
        bytes: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CloseHandle(handle: isize) -> i32;
    fn GetLastError() -> u32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const SecurityAttributes,
        creation: u32,
        flags: u32,
        template: isize,
    ) -> isize;
    fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
}

#[cfg(windows)]
enum ReporterCommandV1 {
    Focus(u128, bool),
}

#[cfg(windows)]
static REPORTER: OnceLock<mpsc::Sender<ReporterCommandV1>> = OnceLock::new();
#[cfg(windows)]
static NEXT_LEASE: AtomicU64 = AtomicU64::new(1);

#[cfg(windows)]
pub struct FocusWindowReporterV1 {
    lease_id: u128,
    sender: mpsc::Sender<ReporterCommandV1>,
}

#[cfg(windows)]
impl FocusWindowReporterV1 {
    pub fn new() -> Self {
        let sender = REPORTER
            .get_or_init(|| {
                let (sender, receiver) = mpsc::channel();
                std::thread::spawn(move || focus_reporter_worker(receiver));
                sender
            })
            .clone();
        let lease_id = (u128::from(std::process::id()) << 64)
            | u128::from(NEXT_LEASE.fetch_add(1, Ordering::Relaxed));
        Self { lease_id, sender }
    }

    pub fn set_focused(&self, focused: bool) {
        let _ = self
            .sender
            .send(ReporterCommandV1::Focus(self.lease_id, focused));
    }
}

#[cfg(windows)]
impl Drop for FocusWindowReporterV1 {
    fn drop(&mut self) {
        let _ = self
            .sender
            .send(ReporterCommandV1::Focus(self.lease_id, false));
    }
}

#[cfg(windows)]
fn focus_reporter_worker(receiver: mpsc::Receiver<ReporterCommandV1>) {
    let mut focused = std::collections::HashSet::<u128>::new();
    let mut pipe = None::<OwnedHandle>;
    let mut sequence = 0_u64;
    loop {
        match receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(ReporterCommandV1::Focus(id, value)) => {
                if value {
                    focused.insert(id);
                } else {
                    focused.remove(&id);
                }
                if let Some(handle) = pipe.as_ref() {
                    sequence = sequence.saturating_add(1);
                    let operation = if value {
                        FocusOperationV1::AcquireOrRenew
                    } else {
                        FocusOperationV1::Release
                    };
                    if send_focus_frame(
                        handle.0,
                        FocusFrameV1 {
                            operation,
                            lease_id: id,
                            sequence,
                        },
                    )
                    .is_err()
                    {
                        pipe = None;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if focused.is_empty() {
            continue;
        }
        if pipe.is_none() {
            pipe = match connect_focus_pipe() {
                Ok(handle) => Some(handle),
                Err(error) => {
                    tracing::warn!(%error, "unable to connect MFT focus lease pipe");
                    None
                }
            };
        }
        let Some(handle) = pipe.as_ref() else {
            continue;
        };
        for id in focused.iter().copied() {
            sequence = sequence.saturating_add(1);
            if let Err(error) = send_focus_frame(
                handle.0,
                FocusFrameV1 {
                    operation: FocusOperationV1::AcquireOrRenew,
                    lease_id: id,
                    sequence,
                },
            ) {
                tracing::warn!(%error, "MFT focus lease acquire/renew failed");
                pipe = None;
                break;
            }
        }
    }
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "opening the focus lease named pipe requires Win32 handle APIs"
)]
// SAFETY: The pipe name remains NUL-terminated and live for both calls; the
// returned sentinel is checked before ownership is transferred to OwnedHandle.
fn connect_focus_pipe() -> Result<OwnedHandle, String> {
    let name = wide(FOCUS_PIPE_NAME);
    let _ = unsafe { WaitNamedPipeW(name.as_ptr(), 250) };
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            0x8000_0000 | 0x4000_0000,
            0,
            ptr::null(),
            3,
            // The shared read/write helpers use OVERLAPPED on both ends.
            0x4000_0000,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!("focus lease pipe unavailable ({})", unsafe {
            GetLastError()
        }));
    }
    Ok(OwnedHandle(handle))
}

#[cfg(windows)]
fn send_focus_frame(handle: isize, frame: FocusFrameV1) -> Result<(), String> {
    write_all(handle, &frame.encode(), &|| false)?;
    let mut response = [0_u8; 8];
    read_frame_with_deadline(handle, &mut response, &|| false)?;
    if u64::from_le_bytes(response) != 0 {
        return Err("focus lease request was rejected".to_owned());
    }
    Ok(())
}

#[cfg(windows)]
#[link(name = "advapi32")]
#[expect(
    unsafe_code,
    reason = "constructing the focus pipe ACL requires the Win32 SDDL converter"
)]
// SAFETY: The signature matches advapi32; the returned local allocation is
// captured by LocalMemory and released with the corresponding allocator.
unsafe extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        descriptor: *const u16,
        revision: u32,
        result: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
#[expect(
    unsafe_code,
    reason = "the SDDL converter returns memory owned by Win32 LocalFree"
)]
// SAFETY: The declaration matches kernel32 LocalFree and is called only for a
// non-null pointer returned by the SDDL conversion API.
unsafe extern "system" {
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
}

#[cfg(windows)]
struct OwnedHandle(isize);

#[cfg(windows)]
impl Drop for OwnedHandle {
    #[expect(
        unsafe_code,
        reason = "releasing a raw Win32 focus-pipe handle requires CloseHandle"
    )]
    // SAFETY: OwnedHandle is the unique owner and rejects null and invalid
    // sentinel handles before releasing the handle exactly once.
    fn drop(&mut self) {
        if self.0 != 0 && self.0 != INVALID_HANDLE_VALUE {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(windows)]
struct LocalMemory(*mut c_void);

#[cfg(windows)]
impl Drop for LocalMemory {
    #[expect(
        unsafe_code,
        reason = "releasing the Win32 SDDL allocation requires LocalFree"
    )]
    // SAFETY: LocalMemory owns only the allocation returned by the SDDL API and
    // invokes LocalFree once after checking for null.
    fn drop(&mut self) {
        if !self.0.is_null() {
            let _ = unsafe { LocalFree(self.0) };
        }
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "serving focus leases requires raw Win32 ACL and named-pipe creation APIs"
)]
// SAFETY: The SDDL and pipe-name buffers remain live through synchronous calls;
// the descriptor and pipe handles immediately enter their matching RAII owners.
pub fn serve_focus_leases(
    stopped: impl Fn() -> bool + Send + Sync + 'static,
    authorize: impl Fn(isize) -> Result<(u64, isize), String> + Send + Sync + 'static,
    leases: Arc<Mutex<FocusLeaseRegistryV1>>,
    now: impl Fn() -> MonotonicMillis + Send + Sync + 'static,
) {
    let stopped = Arc::new(stopped);
    let authorize = Arc::new(authorize);
    let now = Arc::new(now);
    let active = Arc::new(AtomicUsize::new(0));
    let name = wide(FOCUS_PIPE_NAME);
    let sddl = wide("D:P(A;;GA;;;SY)(A;;GRGW;;;IU)");
    let mut descriptor = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &raw mut descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return;
    }
    let descriptor = LocalMemory(descriptor);
    let attributes = SecurityAttributes {
        length: size_of::<SecurityAttributes>() as u32,
        descriptor: descriptor.0,
        inherit_handle: 0,
    };
    let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();
    while !stopped() {
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                let worker = workers.swap_remove(index);
                let _ = worker.join();
            } else {
                index += 1;
            }
        }
        if active.load(Ordering::Acquire) >= MAX_FOCUS_CONNECTIONS {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        let raw = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                0x0000_0003 | 0x4000_0000,
                // message type/read mode and reject remote clients
                0x0000_0004 | 0x0000_0002 | 0x0000_0008,
                MAX_FOCUS_CONNECTIONS as u32,
                64,
                64,
                100,
                &raw const attributes,
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        let pipe = OwnedHandle(raw);
        let connected = connect_overlapped(pipe.0, &*stopped).is_ok();
        if !connected {
            continue;
        }
        active.fetch_add(1, Ordering::AcqRel);
        let stopped = Arc::clone(&stopped);
        let authorize = Arc::clone(&authorize);
        let leases = Arc::clone(&leases);
        let now = Arc::clone(&now);
        let active = Arc::clone(&active);
        workers.push(std::thread::spawn(move || {
            handle_focus_connection(pipe, &*stopped, &*authorize, &leases, &*now);
            active.fetch_sub(1, Ordering::AcqRel);
        }));
    }
    for worker in workers {
        let _ = worker.join();
    }
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "closing an accepted focus pipe session requires DisconnectNamedPipe"
)]
// SAFETY: The OwnedHandle keeps the accepted pipe live throughout the worker;
// disconnect is issued only for that handle before its final CloseHandle.
fn handle_focus_connection(
    pipe: OwnedHandle,
    stopped: &dyn Fn() -> bool,
    authorize: &dyn Fn(isize) -> Result<(u64, isize), String>,
    leases: &Arc<Mutex<FocusLeaseRegistryV1>>,
    now: &dyn Fn() -> MonotonicMillis,
) {
    let Ok((owner, process_handle)) = authorize(pipe.0) else {
        let _ = unsafe { DisconnectNamedPipe(pipe.0) };
        return;
    };
    // Holding this query/synchronize handle for the entire connection prevents
    // a PID from being recycled underneath an accepted lease owner.
    let _process_handle = OwnedHandle(process_handle);
    let mut last_sequence = 0_u64;
    while !stopped() {
        let mut bytes = [0_u8; FOCUS_FRAME_BYTES];
        if read_frame_with_deadline(pipe.0, &mut bytes, stopped).is_err() {
            break;
        }
        let Ok(frame) = FocusFrameV1::decode(&bytes) else {
            break;
        };
        if frame.sequence <= last_sequence {
            break;
        }
        last_sequence = frame.sequence;
        let accepted = leases
            .lock()
            .is_ok_and(|mut registry| match frame.operation {
                FocusOperationV1::AcquireOrRenew => registry
                    .acquire_or_renew(frame.lease_id, owner, now(), FOCUS_LEASE_TTL)
                    .is_ok(),
                FocusOperationV1::Release => registry.release(frame.lease_id, owner),
            });
        let response = if accepted { 0_u64 } else { 1_u64 }.to_le_bytes();
        if write_all(pipe.0, &response, stopped).is_err() {
            break;
        }
    }
    if let Ok(mut registry) = leases.lock() {
        registry.disconnect(owner);
    }
    let _ = unsafe { DisconnectNamedPipe(pipe.0) };
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "focus frame reads require submitting a raw buffer to Win32 ReadFile"
)]
// SAFETY: Each closure invocation exposes only the remaining initialized slice
// capacity and keeps the slice and OVERLAPPED storage live until completion.
fn read_frame_with_deadline(
    handle: isize,
    bytes: &mut [u8],
    stopped: &dyn Fn() -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + FRAME_DEADLINE;
    let mut offset = 0;
    while offset < bytes.len() {
        if stopped() || Instant::now() >= deadline {
            return Err("focus lease read canceled or timed out".to_owned());
        }
        let read = overlapped_io(
            handle,
            deadline,
            stopped,
            |overlapped, transferred| unsafe {
                ReadFile(
                    handle,
                    bytes[offset..].as_mut_ptr().cast(),
                    (bytes.len() - offset) as u32,
                    transferred,
                    overlapped.cast(),
                )
            },
        )?;
        if read == 0 {
            return Err("focus lease client disconnected".to_owned());
        }
        offset += read as usize;
    }
    Ok(())
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "focus frame writes require submitting a raw buffer to Win32 WriteFile"
)]
// SAFETY: Each closure invocation exposes only the remaining immutable bytes,
// whose storage and OVERLAPPED state remain live until completion.
fn write_all(handle: isize, bytes: &[u8], stopped: &dyn Fn() -> bool) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        let written = overlapped_io(
            handle,
            Instant::now() + FRAME_DEADLINE,
            stopped,
            |overlapped, transferred| unsafe {
                WriteFile(
                    handle,
                    bytes[offset..].as_ptr().cast(),
                    (bytes.len() - offset) as u32,
                    transferred,
                    overlapped.cast(),
                )
            },
        )?;
        if written == 0 {
            return Err("focus lease client disconnected".to_owned());
        }
        offset += written as usize;
    }
    Ok(())
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "asynchronous named-pipe connection requires Win32 event and OVERLAPPED APIs"
)]
// SAFETY: The event is owned for the whole OVERLAPPED lifetime, the structure
// remains pinned on this stack until completion, and Win32 errors are checked.
fn connect_overlapped(handle: isize, stopped: &dyn Fn() -> bool) -> Result<(), String> {
    let event =
        unsafe { CreateEventW(None, true, false, None) }.map_err(|error| error.to_string())?;
    let _event_guard = WinEvent(event);
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };
    let ok = unsafe { ConnectNamedPipe(handle, (&raw mut overlapped).cast()) };
    if ok != 0 {
        return Ok(());
    }
    let error = unsafe { GetLastError() };
    if error == ERROR_PIPE_CONNECTED {
        return Ok(());
    }
    if error != ERROR_IO_PENDING {
        return Err(format!("focus lease connect failed ({error})"));
    }
    wait_overlapped(
        handle,
        &mut overlapped,
        Instant::now() + FRAME_DEADLINE,
        stopped,
    )
    .map(|_| ())
}

#[cfg(windows)]
struct WinEvent(HANDLE);

#[cfg(windows)]
impl Drop for WinEvent {
    #[expect(
        unsafe_code,
        reason = "releasing the OVERLAPPED completion event requires Win32 CloseHandle"
    )]
    // SAFETY: WinEvent uniquely owns the successfully created event and closes
    // it exactly once after no pending operation can reference the guard.
    fn drop(&mut self) {
        let _ = unsafe { WinCloseHandle(self.0) };
    }
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "focus pipe transfers require Win32 event creation and last-error inspection"
)]
// SAFETY: The event, OVERLAPPED structure, byte counter, and submitted buffer
// all outlive synchronous completion or wait_overlapped cancellation/drain.
fn overlapped_io(
    handle: isize,
    deadline: Instant,
    stopped: &dyn Fn() -> bool,
    submit: impl FnOnce(*mut OVERLAPPED, *mut u32) -> i32,
) -> Result<u32, String> {
    let event =
        unsafe { CreateEventW(None, true, false, None) }.map_err(|error| error.to_string())?;
    let _event_guard = WinEvent(event);
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };
    let mut immediate = 0_u32;
    let ok = submit(&raw mut overlapped, &raw mut immediate);
    if ok != 0 {
        return Ok(immediate);
    }
    let error = unsafe { GetLastError() };
    if error != ERROR_IO_PENDING && error != ERROR_MORE_DATA {
        return Err(format!("focus lease overlapped I/O failed ({error})"));
    }
    wait_overlapped(handle, &mut overlapped, deadline, stopped)
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "bounded OVERLAPPED completion requires Win32 wait, cancel, and result APIs"
)]
// SAFETY: The caller keeps the handle and OVERLAPPED storage live; cancellation
// is drained before return, and result buffers are valid u32 out-parameters.
fn wait_overlapped(
    handle: isize,
    overlapped: &mut OVERLAPPED,
    deadline: Instant,
    stopped: &dyn Fn() -> bool,
) -> Result<u32, String> {
    loop {
        if stopped() || Instant::now() >= deadline {
            let _ =
                unsafe { CancelIoEx(HANDLE(handle as *mut c_void), Some(&raw const *overlapped)) };
            let mut ignored = 0_u32;
            let _ = unsafe {
                GetOverlappedResult(
                    HANDLE(handle as *mut c_void),
                    overlapped,
                    &raw mut ignored,
                    true,
                )
            };
            return Err("focus lease overlapped I/O canceled or timed out".to_owned());
        }
        if unsafe { WaitForSingleObject(overlapped.hEvent, 20) } == WAIT_OBJECT_0 {
            let mut transferred = 0_u32;
            unsafe {
                GetOverlappedResult(
                    HANDLE(handle as *mut c_void),
                    overlapped,
                    &raw mut transferred,
                    false,
                )
            }
            .map_err(|error| error.to_string())?;
            return Ok(transferred);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> FocusClientIdentityV1 {
        FocusClientIdentityV1 {
            process_id: 42,
            process_creation_100ns: 99,
            session_id: 3,
            user_sid: vec![1, 2, 3],
            image_path: PathBuf::from(r"C:\Program Files\SuperExplorer\SuperExplorer.exe"),
            image_file_identity: 77,
        }
    }

    #[test]
    fn frame_round_trip_and_rejects_malformed_input() {
        let frame = FocusFrameV1 {
            operation: FocusOperationV1::AcquireOrRenew,
            lease_id: 7,
            sequence: 9,
        };
        assert_eq!(FocusFrameV1::decode(&frame.encode()).unwrap(), frame);
        assert!(FocusFrameV1::decode(&frame.encode()[..31]).is_err());
        let mut malformed = frame.encode();
        malformed[6..8].copy_from_slice(&99_u16.to_le_bytes());
        assert!(FocusFrameV1::decode(&malformed).is_err());
    }

    #[test]
    fn authorization_rejects_session_sid_copied_image_and_pid_reuse() {
        let expected = identity();
        assert!(
            authorize_focus_client(&expected, 3, &[1, 2, 3], &expected.image_path, 77,).is_ok()
        );
        let mut wrong = expected.clone();
        wrong.session_id = 4;
        assert!(authorize_focus_client(&wrong, 3, &[1, 2, 3], &expected.image_path, 77).is_err());
        wrong = expected.clone();
        wrong.user_sid = vec![4];
        assert!(authorize_focus_client(&wrong, 3, &[1, 2, 3], &expected.image_path, 77).is_err());
        wrong = expected.clone();
        wrong.image_path = PathBuf::from(r"C:\Temp\SuperExplorer.exe");
        assert!(authorize_focus_client(&wrong, 3, &[1, 2, 3], &expected.image_path, 77).is_err());
        wrong = expected.clone();
        wrong.process_creation_100ns = 0;
        assert!(authorize_focus_client(&wrong, 3, &[1, 2, 3], &expected.image_path, 77).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn stalled_partial_client_is_canceled_and_server_stop_is_bounded() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let stopped = Arc::new(AtomicBool::new(false));
        let server_stopped = Arc::clone(&stopped);
        let leases = Arc::new(Mutex::new(FocusLeaseRegistryV1::default()));
        let server = std::thread::spawn(move || {
            serve_focus_leases(
                move || server_stopped.load(Ordering::Acquire),
                |_| Ok((1, 0)),
                leases,
                || MonotonicMillis(0),
            )
        });
        let client = (0..20)
            .find_map(|_| {
                let connected = connect_focus_pipe().ok();
                if connected.is_none() {
                    std::thread::sleep(Duration::from_millis(25));
                }
                connected
            })
            .expect("focus test pipe should accept a client");
        write_all(client.0, &[0_u8; 8], &|| false).unwrap();
        let started = Instant::now();
        stopped.store(true, Ordering::Release);
        server.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn endpoint_and_resource_contracts_are_versioned_and_bounded() {
        assert_ne!(FOCUS_PIPE_NAME, r"\\.\pipe\SuperExplorerMftFolderSizeV1");
        assert_eq!(FOCUS_FRAME_BYTES, 32);
        assert!(MAX_FOCUS_CONNECTIONS <= 32);
        assert!(FOCUS_LEASE_TTL <= Duration::from_secs(15));
    }
}
