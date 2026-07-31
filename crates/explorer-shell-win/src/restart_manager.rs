//! Bounded Windows Restart Manager adapter for locked-delete recovery.
#![allow(
    unsafe_code,
    reason = "Restart Manager and process identity checks require audited Win32 FFI"
)]

use std::{
    collections::HashSet,
    ffi::OsStr,
    mem::size_of,
    os::windows::ffi::OsStrExt as _,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::SyncSender,
    },
    time::{Duration, Instant},
};

use explorer_common::{ExplorerError, ExplorerErrorKind, RoadmapLimits};
use explorer_model::{
    CancellationToken, ExplorerEvent, LockOwner, LockOwnerApplicationType, LockOwnerCloseOutcome,
    LockOwnerCloseRequest, LockOwnerCloseResult, LockOwnerCloseTerminal, LockOwnerDiscoveryRequest,
    LockOwnerDiscoveryTerminal, LockOwnerEligibility, LockOwnerIdentity, RequestContext,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_MORE_DATA, ERROR_SUCCESS, FILETIME, HANDLE, WIN32_ERROR},
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::{
            RestartManager::{
                CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RM_UNIQUE_PROCESS, RmAddFilter,
                RmCancelCurrentTask, RmConsole, RmCritical, RmEndSession, RmExplorer, RmGetList,
                RmMainWindow, RmNoShutdown, RmOtherWindow, RmRegisterResources, RmService,
                RmShutdown, RmStartSession, RmUnknownApp,
            },
            Threading::{
                GetCurrentProcess, GetCurrentProcessId, GetExitCodeProcess, GetProcessTimes,
                IsProcessCritical, OpenProcess, OpenProcessToken, PROCESS_NAME_FORMAT,
                PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
        },
    },
    core::{BOOL, PCWSTR, PWSTR},
};

struct RestartSession(u32);

const MAXIMUM_LOCK_WORKERS: usize = 2;
static ACTIVE_LOCK_WORKERS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_RESTART_SESSIONS: AtomicUsize = AtomicUsize::new(0);

struct LockWorkerGuard;

impl Drop for LockWorkerGuard {
    fn drop(&mut self) {
        ACTIVE_LOCK_WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) fn start_discovery(
    context: RequestContext,
    request: LockOwnerDiscoveryRequest,
    events: SyncSender<ExplorerEvent>,
) {
    if !acquire_worker() {
        let _ = events.try_send(ExplorerEvent::LockOwnersDiscovered {
            context,
            outcome: LockOwnerDiscoveryTerminal::Failed(restart_error(
                "start lock-owner discovery",
                "lock-recovery worker capacity is exhausted",
                None,
            )),
        });
        return;
    }
    let cancellation = context.cancellation.clone();
    let failure_context = context.clone();
    let failure_events = events.clone();
    let result = std::thread::Builder::new()
        .name("explorer-lock-discovery".to_owned())
        .spawn(move || {
            let _guard = LockWorkerGuard;
            let outcome = discover(&request, &cancellation);
            let _ = events.try_send(ExplorerEvent::LockOwnersDiscovered { context, outcome });
        });
    if result.is_err() {
        ACTIVE_LOCK_WORKERS.fetch_sub(1, Ordering::AcqRel);
        let _ = failure_events.try_send(ExplorerEvent::LockOwnersDiscovered {
            context: failure_context,
            outcome: LockOwnerDiscoveryTerminal::Failed(restart_error(
                "start lock-owner discovery",
                "failed to spawn the bounded discovery worker",
                None,
            )),
        });
    }
}

pub(crate) fn start_close(
    context: RequestContext,
    request: LockOwnerCloseRequest,
    events: SyncSender<ExplorerEvent>,
) {
    if !acquire_worker() {
        let _ = events.try_send(ExplorerEvent::LockOwnersClosed {
            context,
            outcome: LockOwnerCloseTerminal::Failed(restart_error(
                "start lock-owner shutdown",
                "lock-recovery worker capacity is exhausted",
                None,
            )),
        });
        return;
    }
    let cancellation = context.cancellation.clone();
    let failure_context = context.clone();
    let failure_events = events.clone();
    let result = std::thread::Builder::new()
        .name("explorer-lock-shutdown".to_owned())
        .spawn(move || {
            let _guard = LockWorkerGuard;
            let outcome = close(&request, &cancellation);
            let _ = events.try_send(ExplorerEvent::LockOwnersClosed { context, outcome });
        });
    if result.is_err() {
        ACTIVE_LOCK_WORKERS.fetch_sub(1, Ordering::AcqRel);
        let _ = failure_events.try_send(ExplorerEvent::LockOwnersClosed {
            context: failure_context,
            outcome: LockOwnerCloseTerminal::Failed(restart_error(
                "start lock-owner shutdown",
                "failed to spawn the bounded shutdown worker",
                None,
            )),
        });
    }
}

fn acquire_worker() -> bool {
    ACTIVE_LOCK_WORKERS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
            (active < MAXIMUM_LOCK_WORKERS).then_some(active + 1)
        })
        .is_ok()
}

impl RestartSession {
    fn start() -> Result<Self, ExplorerError> {
        let mut handle = 0_u32;
        let key_length = usize::try_from(CCH_RM_SESSION_KEY).unwrap_or(32) + 1;
        let mut key = vec![0_u16; key_length];
        // SAFETY: handle and key are writable for the documented call duration.
        win32_result(
            unsafe { RmStartSession(&raw mut handle, None, PWSTR(key.as_mut_ptr())) },
            "start Restart Manager session",
        )?;
        ACTIVE_RESTART_SESSIONS.fetch_add(1, Ordering::AcqRel);
        Ok(Self(handle))
    }

    fn register(&self, resources: &[std::path::PathBuf]) -> Result<(), ExplorerError> {
        let wide = resources
            .iter()
            .map(|path| {
                path.as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let pointers = wide
            .iter()
            .map(|value| PCWSTR(value.as_ptr()))
            .collect::<Vec<_>>();
        // SAFETY: every pointer is backed by `wide` through the synchronous registration call.
        win32_result(
            unsafe { RmRegisterResources(self.0, Some(&pointers), None, None) },
            "register Restart Manager resources",
        )
    }
}

impl Drop for RestartSession {
    fn drop(&mut self) {
        // SAFETY: this object uniquely owns the live Restart Manager session handle.
        let _ = unsafe { RmEndSession(self.0) };
        ACTIVE_RESTART_SESSIONS.fetch_sub(1, Ordering::AcqRel);
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: this object uniquely owns a real process/token handle.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Discovers bounded, privacy-safe owners of locked delete resources.
pub(crate) fn discover(
    request: &LockOwnerDiscoveryRequest,
    cancellation: &CancellationToken,
) -> LockOwnerDiscoveryTerminal {
    let limits = RoadmapLimits::default();
    if cancellation.is_cancelled() {
        return LockOwnerDiscoveryTerminal::Cancelled;
    }
    let resources = match resource_paths(&request.resources, limits.lock_recovery_max_resources) {
        Ok(resources) => resources,
        Err(error) => return LockOwnerDiscoveryTerminal::Unavailable(error),
    };
    let session = match RestartSession::start().and_then(|session| {
        session.register(&resources)?;
        Ok(session)
    }) {
        Ok(session) => session,
        Err(error) => return LockOwnerDiscoveryTerminal::Unavailable(error),
    };
    if cancellation.is_cancelled() {
        return LockOwnerDiscoveryTerminal::Cancelled;
    }
    match query_owners(&session, limits) {
        Ok(owners) if owners.is_empty() => LockOwnerDiscoveryTerminal::Empty,
        Ok(owners) => LockOwnerDiscoveryTerminal::Ready(owners),
        Err(error) => LockOwnerDiscoveryTerminal::Failed(error),
    }
}

/// Gracefully closes only explicitly selected, revalidated eligible owners.
pub(crate) fn close(
    request: &LockOwnerCloseRequest,
    cancellation: &CancellationToken,
) -> LockOwnerCloseTerminal {
    let limits = RoadmapLimits::default();
    if cancellation.is_cancelled() {
        return LockOwnerCloseTerminal::Cancelled;
    }
    let resources = match resource_paths(&request.resources, limits.lock_recovery_max_resources) {
        Ok(resources) => resources,
        Err(error) => return LockOwnerCloseTerminal::Failed(error),
    };
    if request.owners.is_empty() || request.owners.len() > limits.lock_recovery_max_owners {
        return LockOwnerCloseTerminal::Failed(restart_error(
            "close lock owners",
            "invalid owner selection",
            None,
        ));
    }
    let session = match RestartSession::start().and_then(|session| {
        session.register(&resources)?;
        Ok(session)
    }) {
        Ok(session) => session,
        Err(error) => return LockOwnerCloseTerminal::Failed(error),
    };
    let discovered = match query_owners(&session, limits) {
        Ok(owners) => owners,
        Err(error) => return LockOwnerCloseTerminal::Failed(error),
    };
    let mut outcomes = Vec::with_capacity(request.owners.len());
    let mut eligible = HashSet::new();
    for identity in &request.owners {
        let Some(owner) = discovered.iter().find(|owner| owner.identity == *identity) else {
            outcomes.push(LockOwnerCloseOutcome {
                identity: *identity,
                result: LockOwnerCloseResult::AlreadyExited,
            });
            continue;
        };
        if owner.can_close() && process_identity_matches(*identity) {
            eligible.insert(*identity);
        } else {
            outcomes.push(LockOwnerCloseOutcome {
                identity: *identity,
                result: if process_identity_matches(*identity) {
                    LockOwnerCloseResult::Protected
                } else {
                    LockOwnerCloseResult::StaleIdentity
                },
            });
        }
    }
    for owner in &discovered {
        if !eligible.contains(&owner.identity) {
            let process = unique_process(owner.identity);
            // SAFETY: the process identity is copied and valid for this synchronous filter call.
            let _ = unsafe {
                RmAddFilter(
                    session.0,
                    PCWSTR::null(),
                    Some(&raw const process),
                    PCWSTR::null(),
                    RmNoShutdown,
                )
            };
        }
    }
    if !eligible.is_empty() {
        let done = Arc::new(AtomicBool::new(false));
        let deadline_done = Arc::clone(&done);
        let deadline_expired = Arc::new(AtomicBool::new(false));
        let watchdog_expired = Arc::clone(&deadline_expired);
        let deadline_cancellation = cancellation.clone();
        let session_handle = session.0;
        let timeout = Duration::from_millis(limits.lock_shutdown_timeout_ms);
        let shutdown_deadline = Instant::now() + timeout;
        let watchdog = std::thread::spawn(move || {
            let started = Instant::now();
            while !deadline_done.load(Ordering::Acquire)
                && !deadline_cancellation.is_cancelled()
                && started.elapsed() < timeout
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            if !deadline_done.load(Ordering::Acquire) {
                if !deadline_cancellation.is_cancelled() {
                    watchdog_expired.store(true, Ordering::Release);
                }
                // SAFETY: the owning call keeps the session alive until this watchdog joins.
                let _ = unsafe { RmCancelCurrentTask(session_handle) };
            }
        });
        // Zero flags request documented graceful shutdown; force shutdown is deliberately absent.
        // SAFETY: the session is registered and filters exclude every unselected/ineligible owner.
        let _shutdown = unsafe { RmShutdown(session.0, 0, None) };
        done.store(true, Ordering::Release);
        let _ = watchdog.join();
        let timed_out = deadline_expired.load(Ordering::Acquire);
        for identity in eligible {
            while process_identity_matches(identity)
                && !cancellation.is_cancelled()
                && Instant::now() < shutdown_deadline
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            outcomes.push(LockOwnerCloseOutcome {
                identity,
                result: if process_identity_matches(identity) {
                    unclosed_owner_result(timed_out)
                } else {
                    LockOwnerCloseResult::Closed
                },
            });
        }
    }
    outcomes.sort_by_key(|outcome| outcome.identity.process_id);
    if cancellation.is_cancelled() {
        return LockOwnerCloseTerminal::Cancelled;
    }
    if outcomes.iter().all(|outcome| {
        matches!(
            outcome.result,
            LockOwnerCloseResult::Closed | LockOwnerCloseResult::AlreadyExited
        )
    }) {
        LockOwnerCloseTerminal::Closed(outcomes)
    } else {
        LockOwnerCloseTerminal::Partial(outcomes)
    }
}

const fn unclosed_owner_result(timed_out: bool) -> LockOwnerCloseResult {
    if timed_out {
        LockOwnerCloseResult::Timeout
    } else {
        LockOwnerCloseResult::Refused
    }
}

fn resource_paths(
    resources: &[explorer_model::LocationDescriptor],
    maximum: usize,
) -> Result<Vec<std::path::PathBuf>, ExplorerError> {
    if resources.is_empty() || resources.len() > maximum {
        return Err(restart_error(
            "validate locked resources",
            "resource count is outside the bounded contract",
            None,
        ));
    }
    resources
        .iter()
        .map(|resource| {
            resource
                .path()
                .map(std::path::Path::to_path_buf)
                .ok_or_else(|| {
                    restart_error(
                        "validate locked resources",
                        "Restart Manager requires a filesystem path",
                        None,
                    )
                })
        })
        .collect()
}

fn query_owners(
    session: &RestartSession,
    limits: RoadmapLimits,
) -> Result<Vec<LockOwner>, ExplorerError> {
    query_owners_from(&mut NativeOwnerListSource { session }, limits)
}

#[derive(Clone, Copy)]
struct OwnerListCall {
    status: WIN32_ERROR,
    needed: u32,
    count: u32,
}

trait OwnerListSource {
    fn get_list(&mut self, entries: Option<&mut [RM_PROCESS_INFO]>) -> OwnerListCall;
}

struct NativeOwnerListSource<'a> {
    session: &'a RestartSession,
}

impl OwnerListSource for NativeOwnerListSource<'_> {
    fn get_list(&mut self, entries: Option<&mut [RM_PROCESS_INFO]>) -> OwnerListCall {
        let mut needed = 0_u32;
        let mut count = entries
            .as_ref()
            .and_then(|entries| u32::try_from(entries.len()).ok())
            .unwrap_or_default();
        let mut reboot = 0_u32;
        let pointer = entries.map(<[RM_PROCESS_INFO]>::as_mut_ptr);
        // SAFETY: output counters and optional caller-owned entry storage remain writable for the
        // synchronous Restart Manager call.
        let status = unsafe {
            RmGetList(
                self.session.0,
                &raw mut needed,
                &raw mut count,
                pointer,
                &raw mut reboot,
            )
        };
        OwnerListCall {
            status,
            needed,
            count,
        }
    }
}

fn query_owners_from(
    source: &mut impl OwnerListSource,
    limits: RoadmapLimits,
) -> Result<Vec<LockOwner>, ExplorerError> {
    let first = source.get_list(None);
    if first.status == ERROR_SUCCESS && first.needed == 0 {
        return Ok(Vec::new());
    }
    if first.status != ERROR_MORE_DATA {
        return Err(restart_error(
            "query Restart Manager owners",
            "owner sizing query failed",
            Some(first.status),
        ));
    }
    let requested = usize::try_from(first.needed).map_err(|_| {
        restart_error(
            "query Restart Manager owners",
            "owner count does not fit memory limits",
            None,
        )
    })?;
    if requested > limits.lock_recovery_max_owners {
        return Err(restart_error(
            "query Restart Manager owners",
            "owner count exceeds configured bound",
            None,
        ));
    }
    let mut entries = vec![RM_PROCESS_INFO::default(); requested];
    let mut completed = false;
    let mut count = 0_u32;
    for _ in 0..3 {
        let call = source.get_list(Some(&mut entries));
        count = call.count;
        if call.status == ERROR_SUCCESS {
            completed = true;
            break;
        }
        if call.status != ERROR_MORE_DATA {
            win32_result(call.status, "query Restart Manager owners")?;
        }
        let grown = usize::try_from(call.needed).map_err(|_| {
            restart_error(
                "query Restart Manager owners",
                "grown owner count does not fit memory limits",
                None,
            )
        })?;
        if grown <= entries.len() || grown > limits.lock_recovery_max_owners {
            return Err(restart_error(
                "query Restart Manager owners",
                "unstable owner list exceeds its bounded growth contract",
                None,
            ));
        }
        entries.resize(grown, RM_PROCESS_INFO::default());
    }
    if !completed {
        return Err(restart_error(
            "query Restart Manager owners",
            "owner list did not stabilize within the retry bound",
            None,
        ));
    }
    entries.truncate(usize::try_from(count).unwrap_or(entries.len()));
    Ok(entries
        .into_iter()
        .map(|entry| owner_from_process_info(&entry, limits.lock_recovery_max_name_bytes))
        .collect())
}

fn owner_from_process_info(info: &RM_PROCESS_INFO, max_name_bytes: usize) -> LockOwner {
    let identity = LockOwnerIdentity {
        process_id: info.Process.dwProcessId,
        creation_time_100ns: filetime_value(info.Process.ProcessStartTime),
    };
    let application_type = application_type(info.ApplicationType);
    let display_name = bounded_wide(&info.strAppName, max_name_bytes);
    let eligibility = process_eligibility(identity, application_type);
    LockOwner {
        identity,
        display_name: if display_name.is_empty() {
            format!("Process {}", identity.process_id)
        } else {
            display_name
        },
        application_type,
        restartable: info.bRestartable.as_bool(),
        eligibility,
    }
}

fn application_type(
    value: windows::Win32::System::RestartManager::RM_APP_TYPE,
) -> LockOwnerApplicationType {
    match value {
        value if value == RmMainWindow => LockOwnerApplicationType::MainWindow,
        value if value == RmOtherWindow => LockOwnerApplicationType::OtherWindow,
        value if value == RmService => LockOwnerApplicationType::Service,
        value if value == RmExplorer => LockOwnerApplicationType::Explorer,
        value if value == RmConsole => LockOwnerApplicationType::Console,
        value if value == RmCritical => LockOwnerApplicationType::Critical,
        value if value == RmUnknownApp => LockOwnerApplicationType::Unknown,
        _ => LockOwnerApplicationType::Unknown,
    }
}

fn process_eligibility(
    identity: LockOwnerIdentity,
    application_type: LockOwnerApplicationType,
) -> LockOwnerEligibility {
    if identity.process_id == 0 || identity.process_id == 4 {
        return LockOwnerEligibility::System;
    }
    if identity.process_id == unsafe { GetCurrentProcessId() } {
        return LockOwnerEligibility::ThisApplication;
    }
    if application_type == LockOwnerApplicationType::Critical {
        return LockOwnerEligibility::Critical;
    }
    if application_type == LockOwnerApplicationType::Service {
        return LockOwnerEligibility::Service;
    }
    // SAFETY: requested rights are query-only and the returned handle is RAII-owned.
    let Ok(handle) = (unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            identity.process_id,
        )
    }) else {
        return LockOwnerEligibility::Protected;
    };
    let handle = OwnedHandle(handle);
    if process_creation_time(handle.0) != Some(identity.creation_time_100ns) {
        return LockOwnerEligibility::IdentityUnavailable;
    }
    let mut critical = BOOL(0);
    // SAFETY: the process handle is live and the BOOL output is writable.
    if unsafe { IsProcessCritical(handle.0, &raw mut critical) }.is_ok() && critical.as_bool() {
        return LockOwnerEligibility::Critical;
    }
    if process_image_name(handle.0).is_some_and(|name| is_application_image_name(&name)) {
        return LockOwnerEligibility::ThisApplication;
    }
    let target_elevated = process_elevated(handle.0);
    // SAFETY: GetCurrentProcess returns a non-owning pseudo handle valid in this process.
    let current_elevated = process_elevated(unsafe { GetCurrentProcess() });
    if target_elevated == Some(true) && current_elevated != Some(true) {
        return LockOwnerEligibility::Elevated;
    }
    LockOwnerEligibility::Eligible
}

fn is_application_image_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "superexplorer.exe"
            | "explorer-app.exe"
            | "explorer-extension-broker.exe"
            | "explorer-extension-worker.exe"
    )
}

fn process_identity_matches(identity: LockOwnerIdentity) -> bool {
    // SAFETY: query-only process handle is immediately RAII-owned.
    let Ok(handle) = (unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            identity.process_id,
        )
    }) else {
        return false;
    };
    let handle = OwnedHandle(handle);
    let mut exit_code = 0_u32;
    // SAFETY: the query-limited process handle and writable exit-code output are valid.
    if unsafe { GetExitCodeProcess(handle.0, &raw mut exit_code) }.is_err() || exit_code != 259 {
        return false;
    }
    process_creation_time(handle.0) == Some(identity.creation_time_100ns)
}

fn process_creation_time(handle: HANDLE) -> Option<u64> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: handle is queryable and all FILETIME outputs are writable.
    unsafe {
        GetProcessTimes(
            handle,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    }
    .ok()
    .map(|()| filetime_value(creation))
}

fn process_elevated(process: HANDLE) -> Option<bool> {
    let mut token = HANDLE::default();
    // SAFETY: token output is writable and requested access is query-only.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) }.ok()?;
    let token = OwnedHandle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0_u32;
    // SAFETY: elevation storage is correctly sized for TokenElevation.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            u32::try_from(size_of::<TOKEN_ELEVATION>()).ok()?,
            &raw mut returned,
        )
    }
    .ok()?;
    Some(elevation.TokenIsElevated != 0)
}

fn process_image_name(process: HANDLE) -> Option<String> {
    let mut buffer = vec![0_u16; 32_768];
    let mut length = u32::try_from(buffer.len()).ok()?;
    // SAFETY: buffer and length are writable and process has query-limited access.
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT::default(),
            PWSTR(buffer.as_mut_ptr()),
            &raw mut length,
        )
    }
    .ok()?;
    let path = String::from_utf16_lossy(buffer.get(..usize::try_from(length).ok()?)?);
    std::path::Path::new(&path)
        .file_name()
        .and_then(OsStr::to_str)
        .map(str::to_owned)
}

fn unique_process(identity: LockOwnerIdentity) -> RM_UNIQUE_PROCESS {
    RM_UNIQUE_PROCESS {
        dwProcessId: identity.process_id,
        ProcessStartTime: FILETIME {
            dwLowDateTime: u32::try_from(identity.creation_time_100ns & u64::from(u32::MAX))
                .unwrap_or_default(),
            dwHighDateTime: u32::try_from(identity.creation_time_100ns >> 32).unwrap_or_default(),
        },
    }
}

const fn filetime_value(value: FILETIME) -> u64 {
    (value.dwHighDateTime as u64) << 32 | value.dwLowDateTime as u64
}

fn bounded_wide(value: &[u16], maximum_bytes: usize) -> String {
    let length = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len())
        .min(maximum_bytes / 2);
    String::from_utf16_lossy(&value[..length])
}

fn win32_result(result: WIN32_ERROR, operation: &'static str) -> Result<(), ExplorerError> {
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(restart_error(
            operation,
            "Restart Manager returned a Windows error",
            Some(result),
        ))
    }
}

fn restart_error(
    operation: &'static str,
    detail: &'static str,
    code: Option<WIN32_ERROR>,
) -> ExplorerError {
    let mut error = ExplorerError::new(
        ExplorerErrorKind::Availability,
        operation,
        true,
        "Windows 無法判斷或關閉正在使用此檔案的程式。",
        detail,
    );
    if let Some(code) = code {
        error = error.with_native_code(i32::try_from(code.0).unwrap_or(i32::MAX));
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        fs::OpenOptions,
        io::BufRead as _,
        os::windows::fs::OpenOptionsExt as _,
        process::{Command, Stdio},
    };

    struct FakeOwnerListSource {
        responses: VecDeque<(WIN32_ERROR, u32, Vec<RM_PROCESS_INFO>)>,
    }

    impl OwnerListSource for FakeOwnerListSource {
        fn get_list(&mut self, entries: Option<&mut [RM_PROCESS_INFO]>) -> OwnerListCall {
            let (status, needed, supplied) = self.responses.pop_front().expect("fake response");
            if let Some(entries) = entries {
                for (destination, source) in entries.iter_mut().zip(&supplied) {
                    *destination = *source;
                }
            }
            OwnerListCall {
                status,
                needed,
                count: u32::try_from(supplied.len()).expect("fake count"),
            }
        }
    }

    fn fake_process(process_id: u32) -> RM_PROCESS_INFO {
        let mut entry = RM_PROCESS_INFO::default();
        entry.Process.dwProcessId = process_id;
        entry
    }

    #[test]
    fn locked_delete_fake_owner_list_covers_empty_growth_denied_and_unstable_results() {
        let limits = RoadmapLimits::default();
        let mut empty = FakeOwnerListSource {
            responses: VecDeque::from([(ERROR_SUCCESS, 0, Vec::new())]),
        };
        assert!(
            query_owners_from(&mut empty, limits)
                .expect("empty list")
                .is_empty()
        );

        let mut growing = FakeOwnerListSource {
            responses: VecDeque::from([
                (ERROR_MORE_DATA, 1, Vec::new()),
                (ERROR_MORE_DATA, 2, Vec::new()),
                (ERROR_SUCCESS, 2, vec![fake_process(0), fake_process(4)]),
            ]),
        };
        let owners = query_owners_from(&mut growing, limits).expect("bounded buffer growth");
        assert_eq!(owners.len(), 2);
        assert!(growing.responses.is_empty());

        let mut denied = FakeOwnerListSource {
            responses: VecDeque::from([(WIN32_ERROR(5), 0, Vec::new())]),
        };
        assert!(query_owners_from(&mut denied, limits).is_err());

        let mut unstable = FakeOwnerListSource {
            responses: VecDeque::from([
                (ERROR_MORE_DATA, 1, Vec::new()),
                (ERROR_MORE_DATA, 2, Vec::new()),
                (ERROR_MORE_DATA, 3, Vec::new()),
                (ERROR_MORE_DATA, 4, Vec::new()),
            ]),
        };
        assert!(query_owners_from(&mut unstable, limits).is_err());
        assert!(unstable.responses.is_empty());
    }

    #[test]
    fn locked_delete_cancellation_and_pid_reuse_fail_closed_before_mutation() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert!(matches!(
            discover(
                &LockOwnerDiscoveryRequest {
                    resources: vec![explorer_model::LocationDescriptor::file_system(
                        r"C:\controlled-cancelled-fixture"
                    )],
                },
                &cancellation,
            ),
            LockOwnerDiscoveryTerminal::Cancelled
        ));

        let current = unsafe { GetCurrentProcess() };
        let creation_time = process_creation_time(current).expect("current process creation time");
        assert!(!process_identity_matches(LockOwnerIdentity {
            process_id: unsafe { GetCurrentProcessId() },
            creation_time_100ns: creation_time.saturating_add(1),
        }));
    }

    #[test]
    fn locked_delete_wide_owner_name_is_nul_trimmed_and_bounded() {
        let mut value = [0_u16; 16];
        value[..5].copy_from_slice(&"owner".encode_utf16().collect::<Vec<_>>());
        assert_eq!(bounded_wide(&value, 64), "owner");
        assert_eq!(bounded_wide(&value, 4), "ow");
    }

    #[test]
    fn renamed_and_legacy_application_images_are_both_protected() {
        for name in [
            "SuperExplorer.exe",
            "SUPEREXPLORER.EXE",
            "explorer-app.exe",
            "Explorer-Extension-Broker.exe",
            "explorer-extension-worker.exe",
        ] {
            assert!(is_application_image_name(name), "unprotected image: {name}");
        }
        assert!(!is_application_image_name("unrelated-editor.exe"));
    }

    #[test]
    fn locked_delete_protected_process_classes_are_never_eligible() {
        assert_eq!(
            process_eligibility(
                LockOwnerIdentity {
                    process_id: 4,
                    creation_time_100ns: 0,
                },
                LockOwnerApplicationType::Unknown,
            ),
            LockOwnerEligibility::System
        );
        assert_eq!(
            process_eligibility(
                LockOwnerIdentity {
                    process_id: unsafe { GetCurrentProcessId() },
                    creation_time_100ns: 0,
                },
                LockOwnerApplicationType::Unknown,
            ),
            LockOwnerEligibility::ThisApplication
        );
    }

    #[test]
    fn locked_delete_real_restart_manager_discovers_owned_fixture_without_exposing_paths() {
        let root = std::env::temp_dir().join(format!(
            "superexplorer-rm-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("create owned fixture root");
        let path = root.join("locked.txt");
        let handle = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&path)
            .expect("open exclusive fixture");
        let outcome = discover(
            &LockOwnerDiscoveryRequest {
                resources: vec![explorer_model::LocationDescriptor::file_system(&path)],
            },
            &CancellationToken::new(),
        );
        let LockOwnerDiscoveryTerminal::Ready(owners) = outcome else {
            panic!("Restart Manager must report the owned exclusive handle");
        };
        let owner = owners
            .iter()
            .find(|owner| owner.identity.process_id == std::process::id())
            .expect("current test process owns the fixture");
        assert_eq!(owner.eligibility, LockOwnerEligibility::ThisApplication);
        assert!(!owner.display_name.contains('\\'));
        assert!(
            !owner
                .display_name
                .contains(&path.to_string_lossy().to_string())
        );
        drop(handle);
        std::fs::remove_file(&path).expect("remove fixture file");
        std::fs::remove_dir(&root).expect("remove fixture root");
    }

    #[test]
    fn locked_delete_owned_helper_accepts_graceful_restart_manager_close() {
        let binary = std::env::current_exe()
            .expect("test executable")
            .parent()
            .and_then(std::path::Path::parent)
            .expect("target debug directory")
            .join("explorer-lock-holder.exe");
        assert!(
            binary.is_file(),
            "build the owned lock-holder target: {binary:?}"
        );
        let root = std::env::temp_dir().join(format!(
            "superexplorer-rm-helper-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("fixture root");
        let path = root.join("locked.txt");
        std::fs::write(&path, b"controlled helper lock").expect("fixture file");
        let mut child = Command::new(&binary)
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start owned helper");
        let mut ready = String::new();
        std::io::BufReader::new(child.stdout.take().expect("helper stdout"))
            .read_line(&mut ready)
            .expect("helper ready line");
        assert!(
            ready.starts_with("READY "),
            "unexpected helper output: {ready:?}"
        );
        let resources = vec![explorer_model::LocationDescriptor::file_system(&path)];
        let discovery = discover(
            &LockOwnerDiscoveryRequest {
                resources: resources.clone(),
            },
            &CancellationToken::new(),
        );
        let LockOwnerDiscoveryTerminal::Ready(owners) = discovery else {
            let _ = child.kill();
            panic!("owned helper must be discoverable");
        };
        let owner = owners
            .into_iter()
            .find(|owner| owner.identity.process_id == child.id())
            .expect("helper owner identity");
        eprintln!("owned-helper-restart-manager-owner={owner:?}");
        assert!(owner.can_close(), "owned helper eligibility: {owner:?}");
        let outcome = close(
            &LockOwnerCloseRequest {
                resources,
                owners: vec![owner.identity],
            },
            &CancellationToken::new(),
        );
        let closed = matches!(outcome, LockOwnerCloseTerminal::Closed(ref outcomes)
                if outcomes.iter().any(|outcome| outcome.identity == owner.identity
                    && matches!(outcome.result, LockOwnerCloseResult::Closed | LockOwnerCloseResult::AlreadyExited)));
        if !closed {
            let _ = child.kill();
            let _ = child.wait();
        }
        assert!(closed, "graceful close outcome: {outcome:?}");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait().expect("helper status").is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                panic!("owned helper did not exit after graceful close");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let delete = explorer_model::FileOperationRequest {
            kind: explorer_model::FileOperationKind::PermanentDelete {
                items: vec![explorer_model::ItemDescriptor {
                    id: explorer_model::ShellItemId::from_provider_bytes(b"lock-helper".to_vec())
                        .expect("fixture identity"),
                    location: explorer_model::LocationDescriptor::file_system(&path),
                }],
                confirmed: true,
            },
            flags: explorer_model::FileOperationFlags {
                allow_undo: false,
                require_confirmation: true,
                ..explorer_model::FileOperationFlags::default()
            },
        };
        let operation_context = RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::new(1),
        );
        let (events, _receiver) = std::sync::mpsc::sync_channel(16);
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("operation STA");
        let delete_outcome = crate::file_operation::execute(&operation_context, &delete, &events);
        assert!(
            matches!(
                delete_outcome,
                Ok(explorer_model::OperationTerminal::Finished
                    | explorer_model::OperationTerminal::Partial { .. })
            ),
            "delete retry outcome: {delete_outcome:?}"
        );
        assert!(
            !path.exists(),
            "retry permanently deletes the unlocked fixture"
        );
        let recycle_path = root.join("recycle.txt");
        std::fs::write(&recycle_path, b"controlled recycle outcome").expect("recycle fixture");
        let recycle = explorer_model::FileOperationRequest {
            kind: explorer_model::FileOperationKind::RecycleDelete {
                items: vec![explorer_model::ItemDescriptor {
                    id: explorer_model::ShellItemId::from_provider_bytes(
                        b"recycle-helper".to_vec(),
                    )
                    .expect("recycle identity"),
                    location: explorer_model::LocationDescriptor::file_system(&recycle_path),
                }],
            },
            flags: explorer_model::FileOperationFlags::default(),
        };
        let recycle_outcome = crate::file_operation::execute(&operation_context, &recycle, &events);
        assert!(
            matches!(
                recycle_outcome,
                Ok(explorer_model::OperationTerminal::Finished
                    | explorer_model::OperationTerminal::Partial { .. })
            ),
            "recycle retry outcome: {recycle_outcome:?}"
        );
        assert!(
            !recycle_path.exists(),
            "recycle removes the owned fixture from its source"
        );
        std::fs::remove_dir(&root).expect("delete fixture root");
    }

    #[test]
    fn locked_delete_unclosed_owner_classifies_refused_and_watchdog_timeout_separately() {
        assert_eq!(unclosed_owner_result(false), LockOwnerCloseResult::Refused);
        assert_eq!(unclosed_owner_result(true), LockOwnerCloseResult::Timeout);
    }

    #[test]
    fn locked_delete_ten_cycle_soak_releases_helpers_workers_and_sessions() {
        let binary = std::env::current_exe()
            .expect("test executable")
            .parent()
            .and_then(std::path::Path::parent)
            .expect("target debug directory")
            .join("explorer-lock-holder.exe");
        assert!(binary.is_file(), "build owned lock holder: {binary:?}");
        let root = std::env::temp_dir().join(format!(
            "superexplorer-rm-soak-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("soak fixture root");
        let workers_before = ACTIVE_LOCK_WORKERS.load(Ordering::Acquire);
        let sessions_before = ACTIVE_RESTART_SESSIONS.load(Ordering::Acquire);

        for cycle in 0..10 {
            let path = root.join(format!("cycle-{cycle}.txt"));
            std::fs::write(&path, b"bounded Restart Manager soak").expect("soak fixture");
            let mut child = Command::new(&binary)
                .arg(&path)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("start soak helper");
            let mut ready = String::new();
            std::io::BufReader::new(child.stdout.take().expect("helper stdout"))
                .read_line(&mut ready)
                .expect("helper ready line");
            assert!(ready.starts_with("READY "));
            let resources = vec![explorer_model::LocationDescriptor::file_system(&path)];
            let LockOwnerDiscoveryTerminal::Ready(owners) = discover(
                &LockOwnerDiscoveryRequest {
                    resources: resources.clone(),
                },
                &CancellationToken::new(),
            ) else {
                let _ = child.kill();
                panic!("cycle {cycle} helper must be discoverable");
            };
            let owner = owners
                .into_iter()
                .find(|owner| owner.identity.process_id == child.id())
                .expect("cycle owner");
            let outcome = close(
                &LockOwnerCloseRequest {
                    resources,
                    owners: vec![owner.identity],
                },
                &CancellationToken::new(),
            );
            assert!(
                matches!(outcome, LockOwnerCloseTerminal::Closed(ref outcomes)
                    if outcomes.iter().any(|outcome| outcome.identity == owner.identity
                        && matches!(outcome.result, LockOwnerCloseResult::Closed | LockOwnerCloseResult::AlreadyExited))),
                "cycle={cycle} outcome={outcome:?}"
            );
            assert!(child.wait().expect("reap soak helper").success());
            std::fs::remove_file(path).expect("remove soak fixture");
            assert!(
                ACTIVE_LOCK_WORKERS.load(Ordering::Acquire) <= workers_before,
                "cycle {cycle} leaked an active lock worker"
            );
            assert!(
                ACTIVE_RESTART_SESSIONS.load(Ordering::Acquire) <= sessions_before,
                "cycle {cycle} leaked an active Restart Manager session"
            );
        }
        std::fs::remove_dir(root).expect("remove soak fixture root");
        assert!(ACTIVE_LOCK_WORKERS.load(Ordering::Acquire) <= workers_before);
        assert!(ACTIVE_RESTART_SESSIONS.load(Ordering::Acquire) <= sessions_before);
    }
}
