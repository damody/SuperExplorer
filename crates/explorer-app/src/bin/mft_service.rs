//! Privileged, read-only NTFS index refresher installed as a Windows service.

#![cfg(windows)]

#[path = "../mft_focus.rs"]
mod mft_focus;
#[path = "../mft_journal.rs"]
mod mft_journal;
#[path = "../mft_migration.rs"]
mod mft_migration;
#[path = "../mft_persistence.rs"]
mod mft_persistence;
#[path = "../mft_query.rs"]
mod mft_query;
#[path = "../mft_runtime.rs"]
mod mft_runtime;
#[path = "../mft_size_map.rs"]
mod mft_size_map;
#[path = "../mft_sqlite.rs"]
mod mft_sqlite;

use std::os::windows::ffi::OsStrExt as _;
use std::{
    collections::{HashMap, HashSet},
    ffi::c_void,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// A visible metadata column retries partial MFT results once per second. Treat
// those authenticated query arrivals as a short service-local foreground
// lease so an inexact volume can rebuild even when the separate window-focus
// pipe is unavailable. When the column is hidden the queries stop and this
// lease expires promptly, so hidden count columns do not keep MFT work alive.
const QUERY_DEMAND_LEASE_ID_V1: u128 = u128::MAX;
const QUERY_DEMAND_LEASE_OWNER_V1: u64 = u64::MAX;
const QUERY_DEMAND_LEASE_TTL_V1: Duration = Duration::from_secs(5);

fn renew_query_demand_lease(focus_leases: &Arc<Mutex<mft_persistence::FocusLeaseRegistryV1>>) {
    if let Ok(mut leases) = focus_leases.lock() {
        let _ = leases.acquire_or_renew(
            QUERY_DEMAND_LEASE_ID_V1,
            QUERY_DEMAND_LEASE_OWNER_V1,
            monotonic_now(),
            QUERY_DEMAND_LEASE_TTL_V1,
        );
    }
}
use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE},
        Security::{EqualSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser},
        Storage::FileSystem::{REPLACE_FILE_FLAGS, ReplaceFileW},
        System::{
            Pipes::GetNamedPipeClientProcessId,
            RemoteDesktop::{
                ProcessIdToSessionId, WTSGetActiveConsoleSessionId, WTSQueryUserToken,
            },
            Threading::{
                GetProcessTimes, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
        },
    },
    core::{PCWSTR, PWSTR},
};

const SERVICE_NAME: &str = "SuperExplorerMft";
const SERVICE_RUNNING: u32 = 4;
const SERVICE_STOPPED: u32 = 1;
const SERVICE_STOP_PENDING: u32 = 3;
const SERVICE_ACCEPT_STOP: u32 = 1;
const SERVICE_ACCEPT_SHUTDOWN: u32 = 4;
const SERVICE_CONTROL_STOP: u32 = 1;
const SERVICE_CONTROL_SHUTDOWN: u32 = 5;
const SERVICE_WIN32_OWN_PROCESS: u32 = 0x10;
const SQLITE_WRITER_OPEN_RESERVATION_BYTES: u64 = 1024 * 1024;
const QUERY_FALLBACK_SCAN_BUDGET: Duration = Duration::from_secs(2);
const QUERY_FALLBACK_MAX_ENTRIES: usize = 100_000;

static STOPPED: AtomicBool = AtomicBool::new(false);
static LIFECYCLE_BARRIER: mft_persistence::LifecycleBarrierV1 =
    mft_persistence::LifecycleBarrierV1::new();

fn with_lifecycle_permit<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    LIFECYCLE_BARRIER.invoke(operation)
}
static RECOVERY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static PERSISTED_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static MONOTONIC_EPOCH: OnceLock<Instant> = OnceLock::new();
static PROTECTED_FOCUS_IMAGE: OnceLock<ProtectedFocusImageV1> = OnceLock::new();

fn persisted_write_guard() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    PERSISTED_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "MFT persisted-write lock is poisoned".to_owned())
}

struct ProtectedFocusImageV1 {
    path: PathBuf,
    file_identity: u128,
    service_creation_100ns: u64,
}

struct QueryActivityGuardV1<'a>(&'a AtomicUsize);

impl<'a> QueryActivityGuardV1<'a> {
    fn enter(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self(counter)
    }
}

impl Drop for QueryActivityGuardV1<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn monotonic_now() -> mft_persistence::MonotonicMillis {
    let elapsed = MONOTONIC_EPOCH.get_or_init(Instant::now).elapsed();
    mft_persistence::MonotonicMillis(elapsed.as_millis().try_into().unwrap_or(u64::MAX))
}

// `Instant::elapsed().as_millis()` floors sub-millisecond precision. Recording
// the next millisecond as the durability reference prevents that quantization
// from making two real attempts a fraction of a millisecond less than 10 min
// apart while decisions continue to use the floored current time.
fn monotonic_record_time() -> mft_persistence::MonotonicMillis {
    mft_persistence::MonotonicMillis(monotonic_now().0.saturating_add(1))
}

fn begin_persistence_attempt(
    schedule: &mut mft_persistence::PersistenceScheduleV1,
    focus_leases: &Arc<Mutex<mft_persistence::FocusLeaseRegistryV1>>,
) -> Result<(), String> {
    let now = monotonic_now();
    let focused = focus_leases
        .lock()
        .is_ok_and(|mut leases| leases.any_focused(now));
    if schedule.decision(now, true, focused) != mft_persistence::PersistenceDecisionV1::BeginAttempt
    {
        return Err("MFT persistence gates closed before first write".to_owned());
    }
    schedule
        .record_attempt(monotonic_record_time())
        .map_err(str::to_owned)
}

struct WinHandle(HANDLE);

impl Drop for WinHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

fn filetime_u64(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

fn process_creation_100ns(process: HANDLE) -> Result<u64, String> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(
            process,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    }
    .map_err(|error| error.to_string())?;
    let creation = filetime_u64(creation);
    if creation == 0 {
        return Err("focus process creation identity is invalid".to_owned());
    }
    Ok(creation)
}

fn initialize_protected_focus_image() -> Result<(), String> {
    let path = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .parent()
        .ok_or_else(|| "MFT service install directory is unavailable".to_owned())?
        .join("SuperExplorer.exe");
    let path = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
    let file_identity = u128::from(mft_size_map::file_reference_number(&path)?);
    let service =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, std::process::id()) }
            .map_err(|error| error.to_string())?;
    let service = WinHandle(service);
    let protected = ProtectedFocusImageV1 {
        path,
        file_identity,
        service_creation_100ns: process_creation_100ns(service.0)?,
    };
    PROTECTED_FOCUS_IMAGE
        .set(protected)
        .map_err(|_| "protected focus image was initialized twice".to_owned())
}

fn token_user_buffer(token: HANDLE) -> Result<Vec<u8>, String> {
    let mut needed = 0_u32;
    let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &raw mut needed) };
    if needed < std::mem::size_of::<TOKEN_USER>() as u32 {
        return Err("focus token user is unavailable".to_owned());
    }
    let mut bytes = vec![0_u8; needed as usize];
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(bytes.as_mut_ptr().cast()),
            needed,
            &raw mut needed,
        )
    }
    .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn authorize_focus_pipe(pipe: isize) -> Result<(u64, isize), String> {
    let mut pid = 0_u32;
    unsafe { GetNamedPipeClientProcessId(HANDLE(pipe as *mut c_void), &raw mut pid) }
        .map_err(|error| error.to_string())?;
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .map_err(|error| error.to_string())?;
    let process = WinHandle(process);
    let creation = process_creation_100ns(process.0)?;

    let mut session = u32::MAX;
    unsafe { ProcessIdToSessionId(pid, &raw mut session) }.map_err(|error| error.to_string())?;
    let active_session = unsafe { WTSGetActiveConsoleSessionId() };
    if session != active_session || active_session == u32::MAX {
        return Err("focus process is outside the active session".to_owned());
    }

    let mut client_token = HANDLE::default();
    unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &raw mut client_token) }
        .map_err(|error| error.to_string())?;
    let client_token = WinHandle(client_token);
    let mut active_token = HANDLE::default();
    unsafe { WTSQueryUserToken(active_session, &raw mut active_token) }
        .map_err(|error| error.to_string())?;
    let active_token = WinHandle(active_token);
    let client_user = token_user_buffer(client_token.0)?;
    let active_user = token_user_buffer(active_token.0)?;
    let client_user = unsafe { client_user.as_ptr().cast::<TOKEN_USER>().read_unaligned() };
    let active_user = unsafe { active_user.as_ptr().cast::<TOKEN_USER>().read_unaligned() };
    unsafe { EqualSid(client_user.User.Sid, active_user.User.Sid) }
        .map_err(|_| "focus process user SID does not match active user".to_owned())?;

    let mut image = vec![0_u16; 32_768];
    let mut image_len = image.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(image.as_mut_ptr()),
            &raw mut image_len,
        )
    }
    .map_err(|error| error.to_string())?;
    let image = PathBuf::from(String::from_utf16_lossy(&image[..image_len as usize]));
    let image = std::fs::canonicalize(image).map_err(|error| error.to_string())?;
    let protected = PROTECTED_FOCUS_IMAGE
        .get()
        .ok_or_else(|| "protected focus image is unavailable".to_owned())?;
    if creation < protected.service_creation_100ns {
        return Err("focus process predates the protected service image epoch".to_owned());
    }
    if image != protected.path {
        return Err("focus process is not the installed SuperExplorer image".to_owned());
    }
    let image_identity = mft_size_map::file_reference_number(&image)?;
    let client = mft_focus::FocusClientIdentityV1 {
        process_id: pid,
        process_creation_100ns: creation,
        session_id: session,
        user_sid: vec![1],
        image_path: image,
        image_file_identity: u128::from(image_identity),
    };
    let owner = mft_focus::authorize_focus_client(
        &client,
        active_session,
        &[1],
        &protected.path,
        protected.file_identity,
    )?;
    let raw = process.0.0 as isize;
    std::mem::forget(process);
    Ok((owner, raw))
}

#[repr(C)]
struct ServiceTableEntryW {
    name: *mut u16,
    main: Option<unsafe extern "system" fn(u32, *mut *mut u16)>,
}

#[repr(C)]
struct ServiceStatus {
    service_type: u32,
    current_state: u32,
    controls_accepted: u32,
    win32_exit_code: u32,
    service_specific_exit_code: u32,
    checkpoint: u32,
    wait_hint: u32,
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn StartServiceCtrlDispatcherW(table: *const ServiceTableEntryW) -> i32;
    fn RegisterServiceCtrlHandlerW(
        name: *const u16,
        handler: Option<unsafe extern "system" fn(u32)>,
    ) -> *mut c_void;
    fn SetServiceStatus(handle: *mut c_void, status: *const ServiceStatus) -> i32;
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

unsafe extern "system" fn control_handler(control: u32) {
    if matches!(control, SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN) {
        LIFECYCLE_BARRIER.close();
        STOPPED.store(true, Ordering::Release);
    }
}

unsafe extern "system" fn service_main(_: u32, _: *mut *mut u16) {
    let name = wide(SERVICE_NAME);
    // SAFETY: SCM owns the service callback lifetime and the UTF-16 name is terminated.
    let handle = unsafe { RegisterServiceCtrlHandlerW(name.as_ptr(), Some(control_handler)) };
    if handle.is_null() {
        return;
    }
    report(
        handle,
        SERVICE_RUNNING,
        SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
    );
    run_event_driven_service();
    report(handle, SERVICE_STOP_PENDING, 0);
    report(handle, SERVICE_STOPPED, 0);
}

fn report(handle: *mut c_void, state: u32, controls: u32) {
    let status = ServiceStatus {
        service_type: SERVICE_WIN32_OWN_PROCESS,
        current_state: state,
        controls_accepted: controls,
        win32_exit_code: 0,
        service_specific_exit_code: 0,
        checkpoint: 0,
        wait_hint: 0,
    };
    // SAFETY: handle came from SCM and status points to initialized storage.
    let _ = unsafe { SetServiceStatus(handle, &raw const status) };
}

fn cache_root() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("SuperExplorer")
        .join("MftIndex")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupStoreV1 {
    ExistingCanonicalCatchupPending,
    ExistingCanonicalCleanupCatchupPending,
    LegacyCatchupPending,
    ReplacementRecoveryCatchupPending,
    ExistingCanonical,
    ExistingCanonicalCleanupPending,
    LegacyMigrationPending,
    ReplacementRecoveryPending,
    FreshRebuildRequired,
    LiveBudgetLimited,
    CanonicalLiveBudgetLimited,
    LiveBudgetLimitedCleanupPending,
    InvalidCanonicalQuarantineRequired,
}

impl StartupStoreV1 {
    const fn cleanup_pending(self) -> bool {
        matches!(
            self,
            Self::ExistingCanonicalCleanupCatchupPending
                | Self::ExistingCanonicalCleanupPending
                | Self::LiveBudgetLimitedCleanupPending
        )
    }

    const fn live_budget_limited(self) -> Self {
        if self.cleanup_pending() {
            Self::LiveBudgetLimitedCleanupPending
        } else if matches!(
            self,
            Self::ExistingCanonicalCatchupPending
                | Self::ExistingCanonical
                | Self::CanonicalLiveBudgetLimited
        ) {
            Self::CanonicalLiveBudgetLimited
        } else {
            Self::LiveBudgetLimited
        }
    }

    const fn is_live_budget_limited(self) -> bool {
        matches!(
            self,
            Self::LiveBudgetLimited
                | Self::CanonicalLiveBudgetLimited
                | Self::LiveBudgetLimitedCleanupPending
        )
    }
}

#[derive(Debug)]
struct LiveBudgetStateV1 {
    limits: mft_query::MftCacheBudgetLimitsV1,
    epoch: u64,
    preferred_volume: Option<char>,
    blocked_volumes: HashSet<char>,
    reserved_volume_bytes: usize,
    reserved_file_bytes: usize,
    reserved_persisted_bytes: u64,
    persisted_prune_pending: bool,
}

impl Default for LiveBudgetStateV1 {
    fn default() -> Self {
        Self {
            limits: default_cache_budget_limits(),
            epoch: 0,
            preferred_volume: None,
            blocked_volumes: HashSet::new(),
            reserved_volume_bytes: 0,
            reserved_file_bytes: 0,
            reserved_persisted_bytes: 0,
            persisted_prune_pending: false,
        }
    }
}

struct PersistedBudgetReservationV1 {
    budgets: Arc<Mutex<LiveBudgetStateV1>>,
    bytes: u64,
}

impl PersistedBudgetReservationV1 {
    fn finish(mut self, cache_root: &std::path::Path) {
        if let Ok(mut budgets) = self.budgets.lock() {
            budgets.reserved_persisted_bytes =
                budgets.reserved_persisted_bytes.saturating_sub(self.bytes);
            self.bytes = 0;
            budgets.persisted_prune_pending = persisted_cache_bytes(cache_root)
                > u64::from(budgets.limits.persisted_index_mb) * 1024 * 1024;
        }
    }
}

impl Drop for PersistedBudgetReservationV1 {
    fn drop(&mut self) {
        if self.bytes == 0 {
            return;
        }
        if let Ok(mut budgets) = self.budgets.lock() {
            budgets.reserved_persisted_bytes =
                budgets.reserved_persisted_bytes.saturating_sub(self.bytes);
        }
    }
}

fn reserve_persisted_commit(
    live_budgets: &Arc<Mutex<LiveBudgetStateV1>>,
    cache_root: &std::path::Path,
    bytes: u64,
) -> Result<PersistedBudgetReservationV1, String> {
    let mut budgets = live_budgets
        .lock()
        .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
    let limit = u64::from(budgets.limits.persisted_index_mb) * 1024 * 1024;
    if persisted_cache_bytes(cache_root)
        .saturating_add(budgets.reserved_persisted_bytes)
        .saturating_add(bytes)
        > limit
    {
        return Err("MFT persisted budget has no commit allowance".to_owned());
    }
    budgets.reserved_persisted_bytes = budgets.reserved_persisted_bytes.saturating_add(bytes);
    Ok(PersistedBudgetReservationV1 {
        budgets: Arc::clone(live_budgets),
        bytes,
    })
}

struct LiveBudgetReservationV1 {
    budgets: Arc<Mutex<LiveBudgetStateV1>>,
    volume_bytes: usize,
    file_bytes: usize,
}

impl Drop for LiveBudgetReservationV1 {
    fn drop(&mut self) {
        if let Ok(mut budgets) = self.budgets.lock() {
            budgets.reserved_volume_bytes = budgets
                .reserved_volume_bytes
                .saturating_sub(self.volume_bytes);
            budgets.reserved_file_bytes =
                budgets.reserved_file_bytes.saturating_sub(self.file_bytes);
        }
    }
}

impl LiveBudgetReservationV1 {
    fn release_locked(&mut self, budgets: &mut LiveBudgetStateV1) {
        budgets.reserved_volume_bytes = budgets
            .reserved_volume_bytes
            .saturating_sub(self.volume_bytes);
        budgets.reserved_file_bytes = budgets.reserved_file_bytes.saturating_sub(self.file_bytes);
        self.volume_bytes = 0;
        self.file_bytes = 0;
    }
}

fn reserve_live_scratch_locked(
    budgets: &mut LiveBudgetStateV1,
    shared: &Arc<Mutex<LiveBudgetStateV1>>,
    volume_bytes: usize,
    file_bytes: usize,
) -> Result<LiveBudgetReservationV1, String> {
    let volume_limit = usize::from(budgets.limits.volume_index_mb) * 1024 * 1024;
    let file_limit = usize::from(budgets.limits.file_data_mb) * 1024 * 1024;
    if budgets.reserved_volume_bytes.saturating_add(volume_bytes) > volume_limit
        || budgets.reserved_file_bytes.saturating_add(file_bytes) > file_limit
    {
        return Err("MFT scratch snapshot exceeds the configured live budget".to_owned());
    }
    budgets.reserved_volume_bytes = budgets.reserved_volume_bytes.saturating_add(volume_bytes);
    budgets.reserved_file_bytes = budgets.reserved_file_bytes.saturating_add(file_bytes);
    Ok(LiveBudgetReservationV1 {
        budgets: Arc::clone(shared),
        volume_bytes,
        file_bytes,
    })
}

fn default_cache_budget_limits() -> mft_query::MftCacheBudgetLimitsV1 {
    mft_query::MftCacheBudgetLimitsV1 {
        persisted_index_mb: 1_024,
        volume_index_mb: 1_024,
        file_data_mb: 256,
        aggregate_mb: 512,
        lru_mb: 512,
    }
}

fn trim_index_to_live_budget(
    index: &mut mft_size_map::MftIndexV1,
    volume_remaining: usize,
    file_remaining: usize,
) -> bool {
    let before = index.memory_breakdown();
    let volume_trimmed = index.trim_volume_index_to_bytes(volume_remaining);
    let file_trimmed = index.trim_file_data_to_bytes(file_remaining);
    let after = index.memory_breakdown();
    volume_trimmed
        || file_trimmed
        || before.volume_index_bytes > after.volume_index_bytes
        || before.file_data_bytes > after.file_data_bytes
}

fn enforce_live_budgets_locked(
    live: &mut HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>,
    budgets: &LiveBudgetStateV1,
) -> HashSet<char> {
    let mut letters = live.keys().copied().collect::<Vec<_>>();
    letters.sort_unstable_by_key(|letter| {
        (
            budgets.preferred_volume != Some(*letter),
            letter.to_ascii_uppercase(),
        )
    });
    let mut volume_remaining = (usize::from(budgets.limits.volume_index_mb) * 1024 * 1024)
        .saturating_sub(budgets.reserved_volume_bytes);
    let mut file_remaining = (usize::from(budgets.limits.file_data_mb) * 1024 * 1024)
        .saturating_sub(budgets.reserved_file_bytes);
    let mut trimmed = HashSet::new();
    for letter in letters {
        let Some(runtime) = live.get_mut(&letter) else {
            continue;
        };
        if trim_index_to_live_budget(
            Arc::make_mut(&mut runtime.index),
            volume_remaining,
            file_remaining,
        ) {
            runtime.mark_inexact();
            trimmed.insert(letter);
        }
        let memory = runtime.index.memory_breakdown();
        volume_remaining = volume_remaining.saturating_sub(memory.volume_index_bytes);
        file_remaining = file_remaining.saturating_sub(memory.file_data_bytes);
    }
    trimmed
}

fn prefer_live_volume(
    live_budgets: &Arc<Mutex<LiveBudgetStateV1>>,
    live_volumes: &Arc<Mutex<HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>>>,
    letter: char,
) -> Result<(), String> {
    let mut budgets = live_budgets
        .lock()
        .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
    if budgets.preferred_volume == Some(letter) {
        return Ok(());
    }
    let mut live = live_volumes
        .lock()
        .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
    budgets.preferred_volume = Some(letter);
    budgets.epoch = budgets.epoch.saturating_add(1);
    budgets.blocked_volumes.remove(&letter);
    // Explorer's active location gets first claim on the existing hard
    // budget, but changing tabs must not discard a complete background index
    // when all admitted volumes already fit. Background/restored tabs can
    // legitimately issue column queries too; unconditional displacement here
    // made those requests oscillate the preferred volume and turned an exact
    // foreground Folder Size back into `Partial: 0 KB` after every commit.
    // The ordered budget pass below keeps every fitting index and trims only
    // when the independent hard ceiling is actually exceeded.
    let trimmed = enforce_live_budgets_locked(&mut live, &budgets);
    budgets.blocked_volumes.extend(trimmed);
    Ok(())
}

fn update_runtime_under_live_budget(
    live_budgets: &Arc<Mutex<LiveBudgetStateV1>>,
    live_volumes: &Arc<Mutex<HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>>>,
    letter: char,
    update: impl FnOnce(&mut mft_runtime::VolumeMemoryRuntimeV1) -> Result<(), String>,
) -> Result<(bool, u64), String> {
    let mut budgets = live_budgets
        .lock()
        .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
    let mut live = live_volumes
        .lock()
        .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
    let runtime = live
        .get_mut(&letter)
        .ok_or_else(|| "MFT live volume is unavailable".to_owned())?;
    update(runtime)?;
    let trimmed = enforce_live_budgets_locked(&mut live, &budgets);
    budgets.blocked_volumes.extend(trimmed);
    let exact = live
        .get(&letter)
        .is_some_and(mft_runtime::VolumeMemoryRuntimeV1::is_exact);
    Ok((exact, budgets.epoch))
}

fn update_runtime_and_release_reservation(
    live_budgets: &Arc<Mutex<LiveBudgetStateV1>>,
    live_volumes: &Arc<Mutex<HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>>>,
    letter: char,
    mut reservation: LiveBudgetReservationV1,
    update: impl FnOnce(&mut mft_runtime::VolumeMemoryRuntimeV1) -> Result<(), String>,
) -> Result<(bool, u64), String> {
    let mut budgets = live_budgets
        .lock()
        .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
    let mut live = live_volumes
        .lock()
        .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
    let runtime = live
        .get_mut(&letter)
        .ok_or_else(|| "MFT live volume is unavailable".to_owned())?;
    update(runtime)?;
    reservation.release_locked(&mut budgets);
    let trimmed = enforce_live_budgets_locked(&mut live, &budgets);
    budgets.blocked_volumes.extend(trimmed);
    let exact = live
        .get(&letter)
        .is_some_and(mft_runtime::VolumeMemoryRuntimeV1::is_exact);
    Ok((exact, budgets.epoch))
}

fn observe_batch_under_live_budget(
    live_budgets: &Arc<Mutex<LiveBudgetStateV1>>,
    live_volumes: &Arc<Mutex<HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>>>,
    letter: char,
    changes: Vec<(mft_journal::MftChangeV2, i64)>,
) -> Result<Option<u64>, String> {
    let mut budgets = live_budgets
        .lock()
        .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
    let mut live = live_volumes
        .lock()
        .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
    let mut pending_overflow = false;
    {
        let runtime = live
            .get_mut(&letter)
            .ok_or_else(|| "MFT live volume is unavailable".to_owned())?;
        for (change, observed_usn) in changes {
            runtime.observe(change, observed_usn)?;
            if runtime.pending_bytes() > mft_journal::PENDING_BYTE_LIMIT
                || runtime.pending_count() > mft_journal::PENDING_CHANGE_LIMIT
            {
                runtime.mark_inexact();
                pending_overflow = true;
                break;
            }
        }
    }
    let trimmed = enforce_live_budgets_locked(&mut live, &budgets);
    let limited = trimmed.contains(&letter);
    budgets.blocked_volumes.extend(trimmed);
    if pending_overflow {
        return Err("MFT pending memory bounds exceeded".to_owned());
    }
    Ok(limited.then_some(budgets.epoch))
}

fn reload_canonical_into_live_memory(
    cache: &std::path::Path,
    sqlite_path: &std::path::Path,
    root: &std::path::Path,
    letter: char,
    volume: mft_journal::VolumeIdentityV2,
    live_budgets: &Arc<Mutex<LiveBudgetStateV1>>,
    live_volumes: &Arc<Mutex<HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>>>,
) -> Result<
    Option<(
        mft_size_map::MftIndexV1,
        mft_persistence::JournalCursorV1,
        mft_persistence::JournalCursorV1,
        Vec<mft_journal::MftChangeV2>,
        LiveBudgetReservationV1,
    )>,
    String,
> {
    if !sqlite_path.is_file() {
        return Ok(None);
    }
    let journal = mft_journal::query_journal(root)?;
    let (volume_remaining, file_remaining, reservation) = {
        let mut budgets = live_budgets
            .lock()
            .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
        let live = live_volumes
            .lock()
            .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
        let other = live.iter().filter(|(key, _)| **key != letter).fold(
            (0_usize, 0_usize),
            |(volume_total, file_total), (_, runtime)| {
                let memory = runtime.index.memory_breakdown();
                (
                    volume_total.saturating_add(memory.volume_index_bytes),
                    file_total.saturating_add(memory.file_data_bytes),
                )
            },
        );
        let volume_remaining = (usize::from(budgets.limits.volume_index_mb) * 1024 * 1024)
            .saturating_sub(budgets.reserved_volume_bytes)
            .saturating_sub(other.0);
        let file_remaining = (usize::from(budgets.limits.file_data_mb) * 1024 * 1024)
            .saturating_sub(budgets.reserved_file_bytes)
            .saturating_sub(other.1);
        let reservation = reserve_live_scratch_locked(
            &mut budgets,
            live_budgets,
            volume_remaining,
            file_remaining,
        )?;
        (volume_remaining, file_remaining, reservation)
    };
    let Ok((identity, index, true)) =
        mft_sqlite::MftSqliteStoreV1::load_read_only_bounded_cancelled(
            sqlite_path,
            cache,
            volume,
            journal.journal_id,
            volume_remaining,
            file_remaining,
            || STOPPED.load(Ordering::Acquire),
        )
    else {
        return Ok(None);
    };
    let (index, observed, changes) = catch_up_memory_index(
        root,
        index,
        identity.cursor,
        volume_remaining,
        file_remaining,
    )?;
    let budgets = live_budgets
        .lock()
        .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
    let live = live_volumes
        .lock()
        .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
    let other = live.iter().filter(|(key, _)| **key != letter).fold(
        (0_usize, 0_usize),
        |total, (_, runtime)| {
            let current = runtime.index.memory_breakdown();
            (
                total.0.saturating_add(current.volume_index_bytes),
                total.1.saturating_add(current.file_data_bytes),
            )
        },
    );
    let memory = index.memory_breakdown();
    // Preference changes also advance the budget epoch. Restored/background
    // tabs can therefore change that epoch while this read-only load is in
    // progress even though neither hard limit changed. Revalidate the actual
    // retained bytes against the current limits instead of rejecting a valid
    // foreground snapshot solely because another fitting volume was queried.
    let still_fits = other.0.saturating_add(memory.volume_index_bytes)
        <= usize::from(budgets.limits.volume_index_mb) * 1024 * 1024
        && other.1.saturating_add(memory.file_data_bytes)
            <= usize::from(budgets.limits.file_data_mb) * 1024 * 1024;
    drop(live);
    drop(budgets);
    if !still_fits {
        return Ok(None);
    }
    Ok(Some((
        index,
        identity.cursor,
        observed,
        changes,
        reservation,
    )))
}

fn admit_volume_read_only(
    cache: &std::path::Path,
    letter: char,
    root: &std::path::Path,
    volume_limit_bytes: usize,
    file_limit_bytes: usize,
) -> Result<
    (
        mft_runtime::VolumeMemoryRuntimeV1,
        mft_journal::MftCheckpointV2,
        StartupStoreV1,
    ),
    String,
> {
    let journal = mft_journal::query_journal(root)?;
    let volume = mft_journal::VolumeIdentityV2 {
        serial: mft_size_map::volume_serial_number(root)?,
    };
    let canonical = cache.join(format!("{letter}.mft.sqlite3"));
    let replacement_backup = mft_sqlite::MftSqliteStoreV1::replacement_backup_path(&canonical);
    let temporary = cache.join(format!("{letter}.mft.sqlite3.migration-tmp"));
    let orphan_temporary = [
        temporary.clone(),
        PathBuf::from(format!("{}-journal", temporary.display())),
        PathBuf::from(format!("{}-wal", temporary.display())),
        PathBuf::from(format!("{}-shm", temporary.display())),
    ]
    .iter()
    .any(|path| path.exists());
    let invalid_state = || {
        let cursor = mft_persistence::JournalCursorV1 {
            journal_id: journal.journal_id,
            next_usn: journal.next_usn,
            generation: 0,
        };
        let empty = mft_size_map::MftIndexV1::from_entries(std::collections::BTreeMap::new());
        let checkpoint =
            mft_journal::MftCheckpointV2::new(volume, journal.journal_id, journal.next_usn, 0);
        (
            mft_runtime::VolumeMemoryRuntimeV1::rebuild_required(empty, cursor),
            checkpoint,
            StartupStoreV1::InvalidCanonicalQuarantineRequired,
        )
    };
    if orphan_temporary {
        return Ok(invalid_state());
    }
    if persisted_incomplete_path(cache, letter).is_file() && !replacement_backup.is_file() {
        let cursor = mft_persistence::JournalCursorV1 {
            journal_id: journal.journal_id,
            next_usn: journal.next_usn,
            generation: 0,
        };
        return Ok((
            mft_runtime::VolumeMemoryRuntimeV1::rebuild_required(
                mft_size_map::MftIndexV1::from_entries(std::collections::BTreeMap::new()),
                cursor,
            ),
            mft_journal::MftCheckpointV2::new(volume, journal.journal_id, journal.next_usn, 0),
            StartupStoreV1::LiveBudgetLimited,
        ));
    }
    let members = mft_sqlite::MftSqliteStoreV1::canonical_members(&canonical);
    let any_canonical = members.iter().any(|path| path.exists());
    if any_canonical {
        return match mft_sqlite::MftSqliteStoreV1::load_read_only_bounded(
            &canonical,
            cache,
            volume,
            journal.journal_id,
            volume_limit_bytes,
            file_limit_bytes,
        ) {
            Ok((identity, index, budget_complete)) => {
                let checkpoint = mft_journal::MftCheckpointV2::new(
                    volume,
                    identity.cursor.journal_id,
                    identity.cursor.next_usn,
                    identity.cursor.generation,
                );
                let cleanup_pending = replacement_backup.exists()
                    || !mft_migration::inventory_legacy(cache, letter)?.is_empty();
                Ok((
                    mft_runtime::VolumeMemoryRuntimeV1::rebuild_required(index, identity.cursor),
                    checkpoint,
                    if !budget_complete && cleanup_pending {
                        StartupStoreV1::LiveBudgetLimitedCleanupPending
                    } else if !budget_complete {
                        StartupStoreV1::CanonicalLiveBudgetLimited
                    } else if cleanup_pending {
                        StartupStoreV1::ExistingCanonicalCleanupCatchupPending
                    } else {
                        StartupStoreV1::ExistingCanonicalCatchupPending
                    },
                ))
            }
            Err(_) if replacement_backup.is_file() => {
                match mft_sqlite::MftSqliteStoreV1::load_replacement_backup_read_only_bounded(
                    &replacement_backup,
                    &canonical,
                    cache,
                    volume,
                    journal.journal_id,
                    volume_limit_bytes,
                    file_limit_bytes,
                ) {
                    Ok((identity, index, budget_complete)) => Ok((
                        mft_runtime::VolumeMemoryRuntimeV1::rebuild_required(
                            index,
                            identity.cursor,
                        ),
                        mft_journal::MftCheckpointV2::new(
                            volume,
                            identity.cursor.journal_id,
                            identity.cursor.next_usn,
                            identity.cursor.generation,
                        ),
                        if budget_complete {
                            StartupStoreV1::ReplacementRecoveryCatchupPending
                        } else {
                            StartupStoreV1::LiveBudgetLimited
                        },
                    )),
                    Err(_) => Ok(invalid_state()),
                }
            }
            Err(_) => Ok(invalid_state()),
        };
    }
    if replacement_backup.is_file() {
        return match mft_sqlite::MftSqliteStoreV1::load_replacement_backup_read_only_bounded(
            &replacement_backup,
            &canonical,
            cache,
            volume,
            journal.journal_id,
            volume_limit_bytes,
            file_limit_bytes,
        ) {
            Ok((identity, index, budget_complete)) => Ok((
                mft_runtime::VolumeMemoryRuntimeV1::rebuild_required(index, identity.cursor),
                mft_journal::MftCheckpointV2::new(
                    volume,
                    identity.cursor.journal_id,
                    identity.cursor.next_usn,
                    identity.cursor.generation,
                ),
                if budget_complete {
                    StartupStoreV1::ReplacementRecoveryCatchupPending
                } else {
                    StartupStoreV1::LiveBudgetLimited
                },
            )),
            Err(_) => Ok(invalid_state()),
        };
    }
    if !persisted_incomplete_path(cache, letter).exists()
        && let Some(checkpoint) = mft_journal::latest_checkpoint(cache, letter)?
        && checkpoint.compatible_with(volume, journal)
        && let Ok((index, budget_complete)) = load_legacy_memory_index(
            cache,
            letter,
            checkpoint,
            volume_limit_bytes,
            file_limit_bytes,
        )
    {
        let cursor = mft_persistence::JournalCursorV1 {
            journal_id: checkpoint.journal_id,
            next_usn: checkpoint.next_usn,
            generation: checkpoint.generation,
        };
        return Ok((
            mft_runtime::VolumeMemoryRuntimeV1::rebuild_required(index, cursor),
            checkpoint,
            if budget_complete {
                StartupStoreV1::LegacyCatchupPending
            } else {
                StartupStoreV1::LiveBudgetLimited
            },
        ));
    }
    let cursor = mft_persistence::JournalCursorV1 {
        journal_id: journal.journal_id,
        next_usn: journal.next_usn,
        generation: 0,
    };
    let empty = mft_size_map::MftIndexV1::from_entries(std::collections::BTreeMap::new());
    let checkpoint =
        mft_journal::MftCheckpointV2::new(volume, journal.journal_id, journal.next_usn, 0);
    Ok((
        mft_runtime::VolumeMemoryRuntimeV1::rebuild_required(empty, cursor),
        checkpoint,
        StartupStoreV1::FreshRebuildRequired,
    ))
}

fn run_pending_legacy_cleanup(
    startup_store: &mut StartupStoreV1,
    schedule: &mut mft_persistence::PersistenceScheduleV1,
    focus_leases: &Arc<Mutex<mft_persistence::FocusLeaseRegistryV1>>,
    cache: &std::path::Path,
    sqlite_path: &std::path::Path,
    letter: char,
) -> bool {
    if !matches!(
        *startup_store,
        StartupStoreV1::ExistingCanonicalCleanupPending
            | StartupStoreV1::LiveBudgetLimitedCleanupPending
    ) {
        return false;
    }
    let now = monotonic_now();
    let focused = focus_leases
        .lock()
        .is_ok_and(|mut leases| leases.any_focused(now));
    if schedule.decision(now, true, focused) != mft_persistence::PersistenceDecisionV1::BeginAttempt
    {
        return false;
    }
    let was_budget_limited = *startup_store == StartupStoreV1::LiveBudgetLimitedCleanupPending;
    let audit_root = cache.parent().unwrap_or(cache).join("MftMaintenanceAudit");
    if begin_persistence_attempt(schedule, focus_leases).is_err() {
        return true;
    }
    let legacy_cleanup = mft_migration::cleanup_legacy_after_promotion_linearized(
        cache,
        &audit_root,
        letter,
        &LIFECYCLE_BARRIER,
        || {
            LIFECYCLE_BARRIER.is_open()
                && !STOPPED.load(Ordering::Acquire)
                && focus_leases
                    .lock()
                    .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
        },
    );
    let backup = mft_sqlite::MftSqliteStoreV1::replacement_backup_path(sqlite_path);
    let marker = persisted_incomplete_path(cache, letter);
    let marker_cleanup = if legacy_cleanup.is_ok() && marker.exists() {
        LIFECYCLE_BARRIER.invoke(|| {
            if !focus_leases
                .lock()
                .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
            {
                return Err("MFT focus lease expired before marker cleanup".to_owned());
            }
            std::fs::remove_file(&marker).map_err(|error| error.to_string())
        })
    } else if legacy_cleanup.is_ok() {
        Ok(())
    } else {
        Err("legacy cleanup must complete before marker cleanup".to_owned())
    };
    let backup_cleanup = if legacy_cleanup.is_ok() && marker_cleanup.is_ok() {
        persisted_write_guard().and_then(|_persisted_guard| {
            mft_sqlite::MftSqliteStoreV1::cleanup_replacement_backup_focused_linearized(
                &backup,
                sqlite_path,
                cache,
                &LIFECYCLE_BARRIER,
                || {
                    focus_leases
                        .lock()
                        .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                },
            )
        })
    } else {
        Err("legacy cleanup must complete before backup cleanup".to_owned())
    };
    if legacy_cleanup.is_ok() && marker_cleanup.is_ok() && backup_cleanup.is_ok() {
        *startup_store = if was_budget_limited {
            StartupStoreV1::CanonicalLiveBudgetLimited
        } else {
            StartupStoreV1::ExistingCanonical
        };
        schedule.record_success(monotonic_record_time());
    }
    true
}

fn run_event_driven_service() {
    let cache = cache_root();
    if std::fs::create_dir_all(&cache).is_err() {
        return;
    }
    if initialize_protected_focus_image().is_err() {
        return;
    }
    let live_volumes = Arc::new(Mutex::new(HashMap::<
        char,
        mft_runtime::VolumeMemoryRuntimeV1,
    >::new()));
    let mut initial_budgets = LiveBudgetStateV1::default();
    initial_budgets.persisted_prune_pending = persisted_cache_bytes(&cache)
        > u64::from(initial_budgets.limits.persisted_index_mb) * 1024 * 1024;
    let live_budgets = Arc::new(Mutex::new(initial_budgets));
    let focus_leases = Arc::new(Mutex::new(mft_persistence::FocusLeaseRegistryV1::default()));
    let query_activity = Arc::new(AtomicUsize::new(0));
    let query_checkpoint_gate = Arc::new(RwLock::new(()));
    let volume_diagnostics = Arc::new(Mutex::new(
        HashMap::<char, mft_query::MftVolumeDiagnosticsV1>::new(),
    ));
    let focus_server_leases = Arc::clone(&focus_leases);
    let focus_worker = std::thread::spawn(move || {
        mft_focus::serve_focus_leases(
            || STOPPED.load(Ordering::Acquire),
            authorize_focus_pipe,
            focus_server_leases,
            monotonic_now,
        );
    });
    let session_leases = Arc::clone(&focus_leases);
    let session_worker = std::thread::spawn(move || {
        let mut active_session = unsafe { WTSGetActiveConsoleSessionId() };
        while !STOPPED.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(250));
            let current_session = unsafe { WTSGetActiveConsoleSessionId() };
            if current_session != active_session {
                if let Ok(mut leases) = session_leases.lock() {
                    leases.clear();
                }
                active_session = current_session;
            }
        }
    });
    let mut volumes = Vec::new();
    for letter in b'A'..=b'Z' {
        if STOPPED.load(Ordering::Acquire) {
            return;
        }
        let root = PathBuf::from(format!("{}:\\", char::from(letter)));
        if !root.exists() {
            continue;
        }
        let letter = char::from(letter);
        let (volume_remaining, file_remaining) =
            if let (Ok(budgets), Ok(live)) = (live_budgets.lock(), live_volumes.lock()) {
                let used = live.values().fold(
                    mft_size_map::MftIndexMemoryBreakdownV1::default(),
                    |mut total, runtime| {
                        let memory = runtime.index.memory_breakdown();
                        total.volume_index_bytes = total
                            .volume_index_bytes
                            .saturating_add(memory.volume_index_bytes);
                        total.file_data_bytes =
                            total.file_data_bytes.saturating_add(memory.file_data_bytes);
                        total
                    },
                );
                (
                    (usize::from(budgets.limits.volume_index_mb) * 1024 * 1024)
                        .saturating_sub(used.volume_index_bytes),
                    (usize::from(budgets.limits.file_data_mb) * 1024 * 1024)
                        .saturating_sub(used.file_data_bytes),
                )
            } else {
                continue;
            };
        if let Ok((runtime, checkpoint, startup_store)) =
            admit_volume_read_only(&cache, letter, &root, volume_remaining, file_remaining)
        {
            if let (Ok(mut budgets), Ok(mut live)) = (live_budgets.lock(), live_volumes.lock()) {
                live.insert(letter, runtime);
                if startup_store.is_live_budget_limited() {
                    budgets.blocked_volumes.insert(letter);
                }
                volumes.push((letter, root, checkpoint, startup_store));
            }
        }
    }
    if let (Ok(mut budgets), Ok(mut live)) = (live_budgets.lock(), live_volumes.lock()) {
        let trimmed = enforce_live_budgets_locked(&mut live, &budgets);
        if !trimmed.is_empty() {
            budgets.blocked_volumes.extend(trimmed.iter().copied());
            for (letter, _, _, startup_store) in &mut volumes {
                if trimmed.contains(letter) {
                    *startup_store = startup_store.live_budget_limited();
                }
            }
        }
    }
    let workers = volumes
        .into_iter()
        .map(|(letter, root, checkpoint, startup_store)| {
            let cache = cache.clone();
            let live_volumes = Arc::clone(&live_volumes);
            let live_budgets = Arc::clone(&live_budgets);
            let focus_leases = Arc::clone(&focus_leases);
            let query_activity = Arc::clone(&query_activity);
            let query_checkpoint_gate = Arc::clone(&query_checkpoint_gate);
            let volume_diagnostics = Arc::clone(&volume_diagnostics);
            std::thread::spawn(move || {
                watch_volume_memory(
                    cache,
                    letter,
                    root,
                    checkpoint,
                    live_volumes,
                    live_budgets,
                    focus_leases,
                    query_activity,
                    query_checkpoint_gate,
                    volume_diagnostics,
                    startup_store,
                )
            })
        })
        .collect::<Vec<_>>();
    // Multiple named-pipe instances keep Explorer-style metadata requests
    // responsive. Their result cache is shared so a durable SQLite aggregate
    // is computed only once per folder/generation instead of once per pipe
    // worker; all other volume state and hard budgets are shared as well.
    let shared_query_cache = Arc::new(Mutex::new(ServiceFolderAggregateCacheV1::default()));
    let query_workers = (0..4)
        .map(|_| {
            let query_cache_worker = Arc::clone(&shared_query_cache);
            let query_cache_diagnostics = Arc::clone(&shared_query_cache);
            let query_live_volumes = Arc::clone(&live_volumes);
            let query_live_budgets = Arc::clone(&live_budgets);
            let query_activity_worker = Arc::clone(&query_activity);
            let query_checkpoint_gate_worker = Arc::clone(&query_checkpoint_gate);
            let query_volume_diagnostics = Arc::clone(&volume_diagnostics);
            let folder_query_focus_leases = Arc::clone(&focus_leases);
            let subtree_query_focus_leases = Arc::clone(&focus_leases);
            let query_root = cache.clone();
            std::thread::spawn(move || {
                mft_query::serve_queries(
                    || STOPPED.load(Ordering::Acquire),
                    |letter, reference, cache_memory_mb, requested_path| {
                        renew_query_demand_lease(&folder_query_focus_leases);
                        let _checkpoint_guard = query_checkpoint_gate_worker
                            .read()
                            .map_err(|_| "MFT query/checkpoint gate is unavailable".to_owned())?;
                        let _activity = QueryActivityGuardV1::enter(&query_activity_worker);
                        query_cache_diagnostics
                            .lock()
                            .map_err(|_| "MFT Service query cache is unavailable".to_owned())?
                            .query_live(
                                &query_live_volumes,
                                &query_live_budgets,
                                &query_root,
                                &PathBuf::from(format!("{letter}:\\")),
                                requested_path.as_deref(),
                                letter,
                                reference,
                                cache_memory_mb,
                            )
                    },
                    |letter, reference, requested_path| {
                        renew_query_demand_lease(&subtree_query_focus_leases);
                        let _checkpoint_guard = query_checkpoint_gate_worker
                            .read()
                            .map_err(|_| "MFT query/checkpoint gate is unavailable".to_owned())?;
                        let _activity = QueryActivityGuardV1::enter(&query_activity_worker);
                        let (index, exact, durable) = {
                            let live = query_live_volumes
                                .lock()
                                .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
                            let volume = live
                                .get(&letter)
                                .ok_or_else(|| "MFT live volume is unavailable".to_owned())?;
                            (Arc::clone(&volume.index), volume.is_exact(), volume.durable)
                        };
                        if exact {
                            return index.project_subtree(reference, 100_000, || {
                                STOPPED.load(Ordering::Acquire)
                            });
                        }
                        let _ = (durable, requested_path);
                        // Never recursively enumerate the filesystem from an
                        // Explorer metadata request. When the retained MFT
                        // hierarchy is partial, the UI waits for the service
                        // rebuild instead of causing Defender to rescan a
                        // whole visible subtree.
                        Err("MFT hierarchy is partial while rebuild is pending".to_owned())
                    },
                    || {
                        query_cache_worker
                            .lock()
                            .map_err(|_| "MFT Service query cache is unavailable".to_owned())
                            .and_then(|cache| {
                                cache.diagnostics(
                                    persisted_cache_bytes(&query_root),
                                    &query_live_volumes,
                                    &query_live_budgets,
                                )
                            })
                    },
                    || {
                        query_volume_diagnostics
                            .lock()
                            .map_err(|_| "MFT durability diagnostics are unavailable".to_owned())
                            .map(|volumes| volumes.values().copied().collect())
                    },
                    |value| {
                        query_cache_worker
                            .lock()
                            .map_err(|_| "MFT Service query cache is unavailable".to_owned())
                            .and_then(|mut cache| {
                                cache.set_limits(
                                    &query_root,
                                    &query_live_volumes,
                                    &query_live_budgets,
                                    value,
                                )
                            })
                    },
                );
            })
        })
        .collect::<Vec<_>>();
    while !STOPPED.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(100));
    }
    for worker in workers {
        let _ = worker.join();
    }
    for query_worker in query_workers {
        let _ = query_worker.join();
    }
    let _ = focus_worker.join();
    let _ = session_worker.join();
}

struct CachedFolderAggregateVolumeV1 {
    index: mft_size_map::MftIndexV1,
    aggregates: mft_size_map::MftAggregateIndexV1,
    checkpoint: mft_journal::MftCheckpointV2,
    estimated_bytes: usize,
    volume_index_bytes: usize,
    file_data_bytes: usize,
    aggregate_bytes: usize,
    last_use: u64,
    volume_index_incomplete: bool,
    file_data_incomplete: bool,
    aggregate_incomplete: bool,
}

struct ServiceFolderAggregateCacheV1 {
    volumes: HashMap<char, CachedFolderAggregateVolumeV1>,
    results: HashMap<(char, u64), (mft_query::FolderAggregateQueryV1, u64)>,
    live_aggregates: HashMap<char, (u64, Arc<mft_size_map::MftAggregateIndexV1>, usize, u64)>,
    clock: u64,
    estimated_bytes: usize,
    limit_bytes: usize,
    hits: u64,
    misses: u64,
    generation: u64,
    limits: mft_query::MftCacheBudgetLimitsV1,
    limits_configured: bool,
}

impl Default for ServiceFolderAggregateCacheV1 {
    fn default() -> Self {
        Self {
            volumes: HashMap::new(),
            results: HashMap::new(),
            live_aggregates: HashMap::new(),
            clock: 0,
            estimated_bytes: 0,
            limit_bytes: 512 * 1024 * 1024,
            hits: 0,
            misses: 0,
            generation: 0,
            limits: default_cache_budget_limits(),
            limits_configured: false,
        }
    }
}

fn query_fallback_scan_exhausted(deadline: Instant, visited_entries: usize) -> bool {
    STOPPED.load(Ordering::Acquire)
        || visited_entries > QUERY_FALLBACK_MAX_ENTRIES
        || Instant::now() >= deadline
}

fn durable_snapshot_matches_current_journal(
    volume_root: &std::path::Path,
    durable: mft_persistence::JournalCursorV1,
) -> bool {
    mft_journal::query_journal(volume_root)
        .is_ok_and(|journal| journal_metadata_matches_durable(journal, durable))
}

fn journal_metadata_matches_durable(
    journal: mft_journal::JournalMetadataV2,
    durable: mft_persistence::JournalCursorV1,
) -> bool {
    journal.journal_id == durable.journal_id && journal.next_usn == durable.next_usn
}

impl ServiceFolderAggregateCacheV1 {
    fn query_live(
        &mut self,
        live_volumes: &Arc<Mutex<HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>>>,
        live_budgets: &Arc<Mutex<LiveBudgetStateV1>>,
        _cache_root: &std::path::Path,
        _volume_root: &std::path::Path,
        requested_path: Option<&std::path::Path>,
        letter: char,
        reference: u64,
        cache_memory_mb: u16,
    ) -> Result<mft_query::FolderAggregateQueryV1, String> {
        if !self.limits_configured {
            self.set_limit(cache_memory_mb);
        }
        prefer_live_volume(live_budgets, live_volumes, letter)?;
        self.clock = self.clock.wrapping_add(1).max(1);
        let (mut observed, durable, exact) = {
            let live = live_volumes
                .lock()
                .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
            let volume = live
                .get(&letter)
                .ok_or_else(|| "MFT live volume is unavailable".to_owned())?;
            (volume.observed, volume.durable, volume.is_exact())
        };
        // Prefer the durable recursive aggregate whenever it describes the
        // exact current journal. This keeps large folders exact without
        // forcing an interactive pipe request to walk millions of live MFT
        // nodes (or returning a misleading `Partial: 0 KB` after that walk
        // reaches its latency bound).
        let durable_read_candidate = requested_path.is_some() && observed == durable;
        let durable_read_available = durable_read_candidate
            && durable_snapshot_matches_current_journal(_volume_root, durable);
        if let Some((cached, last_use)) = self.results.get_mut(&(letter, reference))
            && cached.generation == observed.generation
            && if cached.partial {
                !exact && !durable_read_available
            } else {
                exact || durable_read_available
            }
        {
            *last_use = self.clock;
            self.hits = self.hits.saturating_add(1);
            return Ok(*cached);
        }
        self.misses = self.misses.saturating_add(1);
        // A live topology can be intentionally trimmed to obey the memory
        // budget, and even a complete topology may be too large for the
        // interactive subtree-walk bound. When no journal change exists after
        // the durable cursor, answer Folder Size exactly from SQLite metadata.
        // This reads only the MFT index, never user-file contents, and the
        // result cache makes it a one-time cost per folder/generation.
        if durable_read_available {
            let sqlite_path = _cache_root.join(format!("{letter}.mft.sqlite3"));
            let expected_volume = mft_journal::VolumeIdentityV2 {
                serial: mft_size_map::volume_serial_number(_volume_root)?,
            };
            if let Ok(aggregate) = mft_sqlite::MftSqliteStoreV1::query_folder_aggregate_read_only(
                &sqlite_path,
                _cache_root,
                expected_volume,
                durable,
                reference,
                &HashSet::new(),
            ) && durable_snapshot_matches_current_journal(_volume_root, durable)
            {
                let result = mft_query::FolderAggregateQueryV1 {
                    generation: observed.generation,
                    logical_bytes: aggregate.logical_bytes,
                    allocated_bytes: aggregate.allocated_bytes,
                    file_count: aggregate.file_count,
                    directory_count: aggregate.directory_count,
                    partial: false,
                };
                self.results
                    .insert((letter, reference), (result, self.clock));
                self.recount_result_bytes();
                self.evict_for(0);
                return Ok(result);
            }
        }
        if exact {
            let index = {
                let live = live_volumes
                    .lock()
                    .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
                let volume = live
                    .get(&letter)
                    .ok_or_else(|| "MFT live volume is unavailable".to_owned())?;
                Arc::clone(&volume.index)
            };
            if let Some((generation, aggregates, _, last_use)) =
                self.live_aggregates.get_mut(&letter)
                && *generation == observed.generation
                && let Some(aggregate) = aggregates.get(reference)
            {
                *last_use = self.clock;
                let result = mft_query::FolderAggregateQueryV1 {
                    generation: observed.generation,
                    logical_bytes: aggregate.logical_bytes,
                    allocated_bytes: aggregate.allocated_bytes,
                    file_count: aggregate.file_count,
                    directory_count: aggregate.directory_count,
                    partial: false,
                };
                self.results
                    .insert((letter, reference), (result, self.clock));
                self.recount_result_bytes();
                self.evict_for(0);
                return Ok(result);
            }
            let deadline = Instant::now() + QUERY_FALLBACK_SCAN_BUDGET;
            match index.aggregate_subtree_bounded(reference, QUERY_FALLBACK_MAX_ENTRIES, || {
                STOPPED.load(Ordering::Acquire) || Instant::now() >= deadline
            }) {
                Ok(aggregate) => {
                    let result = mft_query::FolderAggregateQueryV1 {
                        generation: observed.generation,
                        logical_bytes: aggregate.logical_bytes,
                        allocated_bytes: aggregate.allocated_bytes,
                        file_count: aggregate.file_count,
                        directory_count: aggregate.directory_count,
                        partial: false,
                    };
                    self.results
                        .insert((letter, reference), (result, self.clock));
                    self.recount_result_bytes();
                    self.evict_for(0);
                    return Ok(result);
                }
                Err(error) if error.contains("interactive bound") => {
                    // Large requested subtrees should not become `Partial: 0
                    // KB` merely because the low-latency walk reached its
                    // bound. Build the volume aggregate once in memory, then
                    // cache O(1) answers for every visible folder. No user-file
                    // contents are opened, so this cannot wake Defender's
                    // recursive file-content scanner.
                    let aggregate_limit = usize::from(self.limits.aggregate_mb) * 1024 * 1024;
                    if index.projected_aggregate_bytes() > aggregate_limit {
                        let result = mft_query::FolderAggregateQueryV1 {
                            generation: observed.generation,
                            partial: true,
                            ..Default::default()
                        };
                        self.results
                            .insert((letter, reference), (result, self.clock));
                        self.recount_result_bytes();
                        self.evict_for(0);
                        return Ok(result);
                    }
                    // Do not retain an old volume aggregate while allocating
                    // its replacement; the configured aggregate budget is a
                    // hard peak limit, not just an eviction target.
                    self.live_aggregates.clear();
                    let aggregates = Arc::new(mft_size_map::MftAggregateIndexV1::build_cancelled(
                        &index, 8, &STOPPED,
                    )?);
                    let aggregate = aggregates
                        .get(reference)
                        .ok_or_else(|| "folder aggregate is unavailable".to_owned())?;
                    let aggregate_bytes = aggregates.estimated_resident_bytes();
                    if aggregate_bytes <= aggregate_limit {
                        self.live_aggregates.insert(
                            letter,
                            (
                                observed.generation,
                                Arc::clone(&aggregates),
                                aggregate_bytes,
                                self.clock,
                            ),
                        );
                    }
                    let result = mft_query::FolderAggregateQueryV1 {
                        generation: observed.generation,
                        logical_bytes: aggregate.logical_bytes,
                        allocated_bytes: aggregate.allocated_bytes,
                        file_count: aggregate.file_count,
                        directory_count: aggregate.directory_count,
                        partial: false,
                    };
                    self.results
                        .insert((letter, reference), (result, self.clock));
                    self.recount_result_bytes();
                    self.evict_for(0);
                    return Ok(result);
                }
                Err(error) => return Err(error),
            }
        }
        // Inexact state must remain interactive. Building a whole-volume
        // aggregate here lets a few Explorer rows occupy every pipe worker for
        // minutes while a foreground catch-up is still running. Aggregate only
        // the requested retained subtree; it is a known lower bound and remains
        // typed partial until the volume becomes exact. Missing/trimmed roots
        // deliberately return an unknown partial result, never an exact 0 KB.
        let index = {
            let live = live_volumes
                .lock()
                .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
            let volume = live
                .get(&letter)
                .ok_or_else(|| "MFT live volume is unavailable".to_owned())?;
            observed = volume.observed;
            Arc::clone(&volume.index)
        };
        let deadline = Instant::now() + QUERY_FALLBACK_SCAN_BUDGET;
        let aggregate = index
            .aggregate_subtree_bounded(reference, QUERY_FALLBACK_MAX_ENTRIES, || {
                STOPPED.load(Ordering::Acquire) || Instant::now() >= deadline
            })
            .ok();
        let result = aggregate.map_or_else(
            || mft_query::FolderAggregateQueryV1 {
                generation: observed.generation,
                partial: true,
                ..Default::default()
            },
            |aggregate| mft_query::FolderAggregateQueryV1 {
                generation: observed.generation,
                logical_bytes: aggregate.logical_bytes,
                allocated_bytes: aggregate.allocated_bytes,
                file_count: aggregate.file_count,
                directory_count: aggregate.directory_count,
                partial: true,
            },
        );
        self.generation = self.generation.max(observed.generation);
        self.results
            .insert((letter, reference), (result, self.clock));
        self.recount_result_bytes();
        self.evict_for(0);
        Ok(result)
    }

    fn set_limits(
        &mut self,
        cache_root: &std::path::Path,
        live_volumes: &Arc<Mutex<HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>>>,
        live_budgets: &Arc<Mutex<LiveBudgetStateV1>>,
        limits: mft_query::MftCacheBudgetLimitsV1,
    ) -> Result<mft_query::MftCacheBudgetLimitsV1, String> {
        let limits = limits.normalized();
        // Budget acknowledgement must linearize against every SQLite/file-set
        // mutation. Never wait behind a potentially long rebuild: report the
        // request as pending so the UI can retry after the active mutation.
        let _persisted_guard = PERSISTED_WRITE_LOCK
            .get_or_init(|| Mutex::new(()))
            .try_lock()
            .map_err(|_| {
                "MFT budget change is pending until persisted maintenance finishes".to_owned()
            })?;
        let mut budgets = live_budgets
            .lock()
            .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
        if budgets.reserved_volume_bytes > usize::from(limits.volume_index_mb) * 1024 * 1024
            || budgets.reserved_file_bytes > usize::from(limits.file_data_mb) * 1024 * 1024
            || (limits.persisted_index_mb < budgets.limits.persisted_index_mb
                && budgets.reserved_persisted_bytes != 0)
        {
            return Err("MFT budget change is pending until active snapshots finish".to_owned());
        }
        let raised = limits.volume_index_mb > self.limits.volume_index_mb
            || limits.file_data_mb > self.limits.file_data_mb
            || limits.aggregate_mb > self.limits.aggregate_mb;
        self.limits = limits;
        self.limits_configured = true;
        self.set_limit(limits.lru_mb);
        // Budget state is always locked before live volume state. Keeping the
        // budget lock from validation through acknowledgement prevents a new
        // scratch snapshot from racing a lower hard maximum.
        {
            let mut live = live_volumes
                .lock()
                .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
            let live_raised = limits.volume_index_mb > budgets.limits.volume_index_mb
                || limits.file_data_mb > budgets.limits.file_data_mb;
            let persisted_raised = limits.persisted_index_mb > budgets.limits.persisted_index_mb;
            if limits != budgets.limits {
                budgets.epoch = budgets.epoch.wrapping_add(1).max(1);
            }
            budgets.limits = limits;
            budgets.persisted_prune_pending = persisted_cache_bytes(cache_root)
                > u64::from(limits.persisted_index_mb) * 1024 * 1024;
            if live_raised || persisted_raised {
                // Previously trimmed volumes get one new proof attempt under
                // the larger allowance. Any that still do not fit are added
                // back below and remain typed partial.
                budgets.blocked_volumes.clear();
            }
            let trimmed = enforce_live_budgets_locked(&mut live, &budgets);
            budgets.blocked_volumes.extend(trimmed);
        }
        drop(budgets);
        // A rebuild is the only proof that records removed under an older
        // limit are complete again.
        if raised
            && self.volumes.values().any(|volume| {
                volume.volume_index_incomplete
                    || volume.file_data_incomplete
                    || volume.aggregate_incomplete
            })
        {
            self.volumes.clear();
            self.results.clear();
            self.live_aggregates.clear();
            self.estimated_bytes = 0;
        } else {
            self.enforce_structure_limits();
        }
        // Budget IPC is memory-only in the SQLite service. Legacy durable
        // files are read-only migration inputs and must never be trimmed by
        // an unfocused interactive request.
        if std::fs::read_dir(cache_root).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                entry.path().extension().and_then(|value| value.to_str())
                    == Some("persisted-partial")
            })
        }) {
            self.volumes.clear();
            self.results.clear();
            self.live_aggregates.clear();
            self.recount_result_bytes();
        }
        Ok(self.limits)
    }

    fn set_limit(&mut self, cache_memory_mb: u16) -> u16 {
        let effective = explorer_model::normalized_mft_folder_cache_memory_mb(cache_memory_mb);
        self.limits.lru_mb = effective;
        self.limit_bytes = usize::from(effective).saturating_mul(1024 * 1024);
        self.evict_for(0);
        effective
    }

    fn query(
        &mut self,
        cache: &std::path::Path,
        letter: char,
        reference: u64,
        cache_memory_mb: u16,
    ) -> Result<mft_query::FolderAggregateQueryV1, String> {
        let result = self.query_inner(cache, letter, reference, cache_memory_mb);
        if result.is_ok() {
            self.hits = self.hits.saturating_add(1);
        } else {
            self.misses = self.misses.saturating_add(1);
        }
        result
    }

    fn query_inner(
        &mut self,
        cache: &std::path::Path,
        letter: char,
        reference: u64,
        cache_memory_mb: u16,
    ) -> Result<mft_query::FolderAggregateQueryV1, String> {
        let _ = cache_memory_mb;
        self.clock = self.clock.wrapping_add(1).max(1);
        let latest = mft_journal::latest_checkpoint(cache, letter)?
            .ok_or_else(|| "MFT Service checkpoint is unavailable".to_owned())?;
        if let Some((cached, last_use)) = self.results.get_mut(&(letter, reference))
            && cached.generation == latest.generation
        {
            *last_use = self.clock;
            return Ok(*cached);
        }
        let needs_refresh = self
            .volumes
            .get(&letter)
            .is_none_or(|volume| volume.checkpoint != latest);
        if needs_refresh {
            self.refresh_volume(cache, letter, latest)?;
        }
        let volume = self
            .volumes
            .get_mut(&letter)
            .ok_or_else(|| "MFT Service aggregate cache is unavailable".to_owned())?;
        volume.last_use = self.clock;
        let aggregate = match volume.aggregates.get(reference) {
            Some(aggregate) => aggregate,
            None if volume.aggregate_incomplete => mft_size_map::MftAggregateV1::default(),
            None => return Err("folder aggregate is unavailable".to_owned()),
        };
        self.generation = self.generation.max(volume.checkpoint.generation);
        let result = mft_query::FolderAggregateQueryV1 {
            generation: volume.checkpoint.generation,
            logical_bytes: aggregate.logical_bytes,
            allocated_bytes: aggregate.allocated_bytes,
            file_count: aggregate.file_count,
            directory_count: aggregate.directory_count,
            partial: volume.volume_index_incomplete
                || volume.file_data_incomplete
                || volume.aggregate_incomplete,
        };
        self.results
            .insert((letter, reference), (result, self.clock));
        self.recount_result_bytes();
        self.evict_for(0);
        Ok(result)
    }

    fn diagnostics(
        &self,
        persisted_index_bytes: u64,
        live_volumes: &Arc<Mutex<HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>>>,
        live_budgets: &Arc<Mutex<LiveBudgetStateV1>>,
    ) -> Result<mft_query::MftCacheDiagnosticsV1, String> {
        let budgets = live_budgets
            .lock()
            .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
        let live = live_volumes
            .lock()
            .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
        let (volume_index_bytes, file_data_bytes) =
            live.values()
                .fold((0_usize, 0_usize), |(volume_total, file_total), runtime| {
                    let memory = runtime.index.memory_breakdown();
                    (
                        volume_total.saturating_add(memory.volume_index_bytes),
                        file_total.saturating_add(memory.file_data_bytes),
                    )
                });
        let aggregate_bytes = self
            .volumes
            .values()
            .map(|volume| volume.aggregate_bytes)
            .sum::<usize>()
            .saturating_add(self.live_aggregate_bytes());
        Ok(mft_query::MftCacheDiagnosticsV1 {
            generation: self.generation,
            lru_bytes: self.estimated_bytes.try_into().unwrap_or(u64::MAX),
            limit_bytes: self.limit_bytes.try_into().unwrap_or(u64::MAX),
            entry_count: self.results.len().try_into().unwrap_or(u64::MAX),
            persisted_index_bytes: persisted_index_bytes
                .saturating_add(budgets.reserved_persisted_bytes),
            hits: self.hits,
            misses: self.misses,
            volume_index_bytes: Some(
                volume_index_bytes
                    .saturating_add(budgets.reserved_volume_bytes)
                    .try_into()
                    .unwrap_or(u64::MAX),
            ),
            file_data_bytes: Some(
                file_data_bytes
                    .saturating_add(budgets.reserved_file_bytes)
                    .try_into()
                    .unwrap_or(u64::MAX),
            ),
            aggregate_bytes: Some(aggregate_bytes.try_into().unwrap_or(u64::MAX)),
            persisted_index_limit_bytes: Some(
                u64::from(self.limits.persisted_index_mb) * 1024 * 1024,
            ),
            volume_index_limit_bytes: Some(u64::from(self.limits.volume_index_mb) * 1024 * 1024),
            file_data_limit_bytes: Some(u64::from(self.limits.file_data_mb) * 1024 * 1024),
            aggregate_limit_bytes: Some(u64::from(self.limits.aggregate_mb) * 1024 * 1024),
        })
    }

    fn refresh_volume(
        &mut self,
        cache: &std::path::Path,
        letter: char,
        latest: mft_journal::MftCheckpointV2,
    ) -> Result<(), String> {
        let previous = self.volumes.remove(&letter);
        self.results.retain(|(volume, _), _| *volume != letter);
        self.recount_result_bytes();
        let (mut index, mut cursor) = if let Some(previous) = previous
            && previous.checkpoint.volume == latest.volume
            && previous.checkpoint.journal_id == latest.journal_id
        {
            (previous.index, previous.checkpoint)
        } else {
            let base = cache.join(format!("{letter}.semftidx"));
            let index = mft_size_map::read_index(&base)?;
            let checkpoint =
                mft_journal::read_checkpoint(&mft_journal::checkpoint_path(cache, letter, 0))?;
            (index, checkpoint)
        };
        if cursor.generation < latest.generation {
            for delta in
                mft_journal::deltas_after(cache, letter, cursor.generation, latest.generation)?
            {
                if delta.volume != cursor.volume
                    || delta.journal_id != cursor.journal_id
                    || delta.generation != cursor.generation.saturating_add(1)
                    || delta.start_usn != cursor.next_usn
                {
                    return Err("MFT Service aggregate delta chain is not contiguous".to_owned());
                }
                for change in &delta.changes {
                    index.apply_change(change)?;
                }
                cursor = mft_journal::MftCheckpointV2::new(
                    cursor.volume,
                    cursor.journal_id,
                    delta.next_usn,
                    delta.generation,
                );
            }
        }
        if cursor != latest {
            return Err("MFT Service aggregate generation is stale".to_owned());
        }
        let mut aggregates = mft_size_map::MftAggregateIndexV1::build(&index, 8)?;
        let volume_limit = usize::from(self.limits.volume_index_mb) * 1024 * 1024;
        let file_limit = usize::from(self.limits.file_data_mb) * 1024 * 1024;
        let aggregate_limit = usize::from(self.limits.aggregate_mb) * 1024 * 1024;
        let persisted_incomplete = persisted_incomplete_path(cache, letter).is_file();
        let aggregate_incomplete =
            aggregates.trim_to_bytes(aggregate_limit) || persisted_incomplete;
        let file_data_incomplete =
            index.trim_file_data_to_bytes(file_limit) || persisted_incomplete;
        let volume_index_incomplete =
            index.trim_volume_index_to_bytes(volume_limit) || persisted_incomplete;
        let index_memory = index.memory_breakdown();
        let aggregate_bytes = aggregates.estimated_resident_bytes();
        let estimated_bytes = index_memory
            .volume_index_bytes
            .saturating_add(index_memory.file_data_bytes)
            .saturating_add(aggregate_bytes);
        self.volumes.insert(
            letter,
            CachedFolderAggregateVolumeV1 {
                index,
                aggregates,
                checkpoint: latest,
                estimated_bytes,
                volume_index_bytes: index_memory.volume_index_bytes,
                file_data_bytes: index_memory.file_data_bytes,
                aggregate_bytes,
                last_use: self.clock,
                volume_index_incomplete,
                file_data_incomplete,
                aggregate_incomplete,
            },
        );
        self.enforce_structure_limits();
        Ok(())
    }

    fn enforce_structure_limits(&mut self) {
        let volume_limit = usize::from(self.limits.volume_index_mb) * 1024 * 1024;
        let file_limit = usize::from(self.limits.file_data_mb) * 1024 * 1024;
        let aggregate_limit = usize::from(self.limits.aggregate_mb) * 1024 * 1024;
        while self.live_aggregate_bytes() > aggregate_limit {
            let Some(oldest) = self
                .live_aggregates
                .iter()
                .min_by_key(|(_, (_, _, _, last_use))| *last_use)
                .map(|(letter, _)| *letter)
            else {
                break;
            };
            self.live_aggregates.remove(&oldest);
            self.results.retain(|(volume, _), _| *volume != oldest);
        }
        // Oldest volume first. Each structure is trimmed independently, so a
        // small aggregate budget cannot evict file names or topology.
        let mut letters = self
            .volumes
            .iter()
            .map(|(letter, volume)| (*letter, volume.last_use))
            .collect::<Vec<_>>();
        letters.sort_by_key(|(_, last_use)| *last_use);
        for (letter, _) in letters {
            let volume_used = self
                .volumes
                .values()
                .map(|v| v.volume_index_bytes)
                .sum::<usize>();
            let file_used = self
                .volumes
                .values()
                .map(|v| v.file_data_bytes)
                .sum::<usize>();
            let aggregate_used = self
                .volumes
                .values()
                .map(|v| v.aggregate_bytes)
                .sum::<usize>();
            let Some(volume) = self.volumes.get_mut(&letter) else {
                continue;
            };
            if volume_used > volume_limit {
                volume.volume_index_incomplete |= volume.index.trim_volume_index_to_bytes(
                    volume_limit
                        .saturating_sub(volume_used.saturating_sub(volume.volume_index_bytes)),
                );
            }
            if file_used > file_limit {
                volume.file_data_incomplete |= volume.index.trim_file_data_to_bytes(
                    file_limit.saturating_sub(file_used.saturating_sub(volume.file_data_bytes)),
                );
            }
            if aggregate_used > aggregate_limit {
                volume.aggregate_incomplete |= volume.aggregates.trim_to_bytes(
                    aggregate_limit
                        .saturating_sub(aggregate_used.saturating_sub(volume.aggregate_bytes)),
                );
            }
            let memory = volume.index.memory_breakdown();
            volume.volume_index_bytes = memory.volume_index_bytes;
            volume.file_data_bytes = memory.file_data_bytes;
            volume.aggregate_bytes = volume.aggregates.estimated_resident_bytes();
            volume.estimated_bytes = volume
                .volume_index_bytes
                .saturating_add(volume.file_data_bytes)
                .saturating_add(volume.aggregate_bytes);
        }
        self.recount_result_bytes();
    }

    fn evict_for(&mut self, incoming: usize) {
        while self.estimated_bytes.saturating_add(incoming) > self.limit_bytes {
            let Some(key) = self
                .results
                .iter()
                .min_by_key(|(_, (_, last_use))| *last_use)
                .map(|(key, _)| *key)
            else {
                break;
            };
            self.results.remove(&key);
            self.recount_result_bytes();
        }
    }

    fn recount_result_bytes(&mut self) {
        self.estimated_bytes = self.results.len().saturating_mul(std::mem::size_of::<(
            (char, u64),
            (mft_query::FolderAggregateQueryV1, u64),
        )>());
    }

    fn live_aggregate_bytes(&self) -> usize {
        self.live_aggregates
            .values()
            .map(|(_, _, bytes, _)| *bytes)
            .sum()
    }
}

fn persisted_incomplete_path(root: &std::path::Path, letter: char) -> PathBuf {
    root.join(format!("{letter}.persisted-partial"))
}

fn enforce_persisted_limit(root: &std::path::Path, limit: usize) -> Result<(), String> {
    let mut indices = std::fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|value| value.to_str()) == Some("semftidx")).then_some(path)
        })
        .collect::<Vec<_>>();
    indices.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });
    let mut total = indices
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len() as usize)
        .sum::<usize>();
    for path in indices {
        if total <= limit {
            break;
        }
        let old_bytes = std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len() as usize;
        let allowance = limit.saturating_sub(total.saturating_sub(old_bytes));
        let mut index = mft_size_map::read_index(&path)?;
        if !index.trim_persisted_to_bytes(allowance) {
            continue;
        }
        let letter = path
            .file_stem()
            .and_then(|value| value.to_str())
            .and_then(|value| value.chars().next())
            .filter(char::is_ascii_alphabetic)
            .ok_or_else(|| "invalid persisted MFT index name".to_owned())?
            .to_ascii_uppercase();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = root.join(format!("{letter}.prune-{stamp}.tmp"));
        mft_size_map::write_service_index(&temporary, &index)?;
        publish_base_index(&temporary, &path)?;
        // Marker publication follows the atomic replacement. A crash before
        // this point leaves the previous complete index; a crash afterward
        // leaves an explicitly partial generation.
        std::fs::write(persisted_incomplete_path(root, letter), b"SEMFTPARTIAL1")
            .map_err(|error| error.to_string())?;
        let new_bytes = std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len() as usize;
        total = total.saturating_sub(old_bytes).saturating_add(new_bytes);
    }
    // Unique prune temporaries are never query inputs. Remove interrupted
    // replacements after resolving their fixed root and suffix.
    for entry in std::fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or_default();
        if name.contains(".prune-") && name.ends_with(".tmp") {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}

fn persisted_cache_bytes(root: &std::path::Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(root) else {
        return 0;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return 0;
    }
    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    let mut visited = 0_usize;
    while let Some(directory) = pending.pop() {
        visited = visited.saturating_add(1);
        if visited > 100_000 {
            return total;
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    total
}

fn persisted_sqlite_prune_target(root: &std::path::Path, limit: u64) -> Option<char> {
    if persisted_cache_bytes(root) <= limit {
        return None;
    }
    let entries = std::fs::read_dir(root).ok()?;
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            let stem = name.strip_suffix(".mft.sqlite3")?;
            if stem.len() != 1 {
                return None;
            }
            let letter = stem.chars().next()?.to_ascii_uppercase();
            if !letter.is_ascii_alphabetic() {
                return None;
            }
            let modified = std::fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some((modified, letter))
        })
        .min_by_key(|(modified, letter)| (*modified, *letter))
        .map(|(_, letter)| letter)
}

fn persisted_candidate_allowance(
    root: &std::path::Path,
    canonical: &std::path::Path,
    limit: u64,
    replace_existing: bool,
) -> u64 {
    let current = persisted_cache_bytes(root);
    let replaced_members = replace_existing
        .then(|| {
            mft_sqlite::MftSqliteStoreV1::canonical_members(canonical)
                .iter()
                .map(|member| std::fs::metadata(member).map_or(0, |metadata| metadata.len()))
                .sum::<u64>()
        })
        .unwrap_or(0);
    // This is the retained post-replacement allowance. Temporary rollback
    // journal and safety-backup bytes are separately bounded by the focused
    // snapshot builder and never become admitted cache state.
    limit.saturating_sub(current.saturating_sub(replaced_members))
}

fn replacement_recovery_projected_bytes(
    root: &std::path::Path,
    canonical: &std::path::Path,
) -> u64 {
    let canonical_bytes = mft_sqlite::MftSqliteStoreV1::canonical_members(canonical)
        .iter()
        .map(|member| std::fs::metadata(member).map_or(0, |metadata| metadata.len()))
        .sum::<u64>();
    // The verified backup is already counted in current usage. Promotion
    // consumes that path while replacing the invalid canonical set, then a
    // writer reopen may retain bounded WAL/SHM companions.
    persisted_cache_bytes(root)
        .saturating_sub(canonical_bytes)
        .saturating_add(SQLITE_WRITER_OPEN_RESERVATION_BYTES)
}

fn prepare_volume(
    cache: &std::path::Path,
    letter: char,
    root: &std::path::Path,
    force_rebuild: bool,
) -> Result<mft_journal::MftCheckpointV2, String> {
    let force_rebuild = force_rebuild || persisted_incomplete_path(cache, letter).is_file();
    let journal = mft_journal::query_journal(root)?;
    let volume = mft_journal::VolumeIdentityV2 {
        serial: mft_size_map::volume_serial_number(root)?,
    };
    let destination = cache.join(format!("{letter}.semftidx"));
    if !force_rebuild
        && destination.is_file()
        && let Some(checkpoint) = mft_journal::latest_checkpoint(cache, letter)?
        && checkpoint.compatible_with(volume, journal)
    {
        write_status(
            cache,
            letter,
            mft_journal::MftServiceModeV2::Journal,
            checkpoint,
            0,
            0,
            "",
        )?;
        return Ok(checkpoint);
    }

    // A complete volume index is intentionally memory-heavy. Journal readers remain
    // per-volume and parallel, but recoveries are serialized so two large HashMaps do
    // not coexist and multiply the service working set.
    let _recovery = RECOVERY_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "MFT recovery lock is poisoned".to_owned())?;
    if !force_rebuild
        && destination.is_file()
        && let Some(checkpoint) = mft_journal::latest_checkpoint(cache, letter)?
        && checkpoint.compatible_with(volume, mft_journal::query_journal(root)?)
    {
        write_status(
            cache,
            letter,
            mft_journal::MftServiceModeV2::Journal,
            checkpoint,
            0,
            0,
            "",
        )?;
        return Ok(checkpoint);
    }

    let mode = if destination.is_file() {
        mft_journal::MftServiceModeV2::Recovering
    } else {
        mft_journal::MftServiceModeV2::Initializing
    };
    let initial =
        mft_journal::MftCheckpointV2::new(volume, journal.journal_id, journal.next_usn, 0);
    write_status(
        cache,
        letter,
        mode,
        initial,
        0,
        0,
        if force_rebuild {
            "journal-recovery"
        } else {
            "initial-snapshot"
        },
    )?;
    let index = mft_size_map::read_volume_index(root, || STOPPED.load(Ordering::Acquire))?;
    if STOPPED.load(Ordering::Acquire) {
        return Err("MFT service stopping".to_owned());
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = cache.join(format!("{letter}.{stamp}.tmp"));
    mft_size_map::write_service_index(&temporary, &index)?;
    drop(index);
    let committed_journal = mft_journal::query_journal(root)?;
    if committed_journal.journal_id != journal.journal_id {
        let _ = std::fs::remove_file(&temporary);
        return Err("USN journal changed while the base snapshot was built".to_owned());
    }
    // The complete MFT snapshot already reflects the volume state observed during the scan.
    // Commit at the post-scan cursor so scan-time events (including our own cache writes on C:)
    // do not immediately force another complete recovery. Later events remain journal-driven.
    let committed = mft_journal::MftCheckpointV2::new(
        volume,
        committed_journal.journal_id,
        committed_journal.next_usn,
        0,
    );
    publish_base_index(&temporary, &destination)?;
    let _ = std::fs::remove_file(persisted_incomplete_path(cache, letter));
    mft_journal::remove_volume_sidecars(cache, letter)?;
    mft_journal::publish_initial_checkpoint(cache, letter, &committed)?;
    write_status(
        cache,
        letter,
        mft_journal::MftServiceModeV2::Journal,
        committed,
        0,
        0,
        "",
    )?;
    Ok(committed)
}

fn publish_base_index(
    temporary: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), String> {
    if !destination.exists() {
        return std::fs::rename(temporary, destination).map_err(|error| error.to_string());
    }
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    let temporary_wide = temporary
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    // SAFETY: both paths are terminated, valid for the call, and replacement is constrained to the cache.
    unsafe {
        ReplaceFileW(
            PCWSTR(destination_wide.as_ptr()),
            PCWSTR(temporary_wide.as_ptr()),
            PCWSTR::null(),
            REPLACE_FILE_FLAGS(0),
            None,
            None,
        )
    }
    .map_err(|error| error.to_string())
}

fn load_legacy_memory_index(
    cache: &std::path::Path,
    letter: char,
    checkpoint: mft_journal::MftCheckpointV2,
    volume_limit_bytes: usize,
    file_limit_bytes: usize,
) -> Result<(mft_size_map::MftIndexV1, bool), String> {
    let (mut index, mut complete) = mft_size_map::read_index_bounded(
        &cache.join(format!("{letter}.semftidx")),
        volume_limit_bytes,
        file_limit_bytes,
    )?;
    if !complete {
        return Ok((index, false));
    }
    let mut cursor = mft_journal::read_checkpoint(&mft_journal::checkpoint_path(cache, letter, 0))?;
    for delta in mft_journal::deltas_after(cache, letter, cursor.generation, checkpoint.generation)?
    {
        if delta.volume != cursor.volume
            || delta.journal_id != cursor.journal_id
            || delta.generation != cursor.generation.saturating_add(1)
            || delta.start_usn != cursor.next_usn
        {
            return Err("legacy MFT delta chain is not contiguous".to_owned());
        }
        for change in &delta.changes {
            let memory = index.memory_breakdown();
            if memory.volume_index_bytes.saturating_add(1_024) > volume_limit_bytes {
                complete = false;
                break;
            }
            let mut bounded = change.clone();
            if memory
                .file_data_bytes
                .saturating_add(bounded.name.len().saturating_mul(2))
                > file_limit_bytes
            {
                bounded.name.clear();
                complete = false;
            }
            index.apply_change(&bounded)?;
        }
        if !complete {
            break;
        }
        cursor = mft_journal::MftCheckpointV2::new(
            cursor.volume,
            cursor.journal_id,
            delta.next_usn,
            delta.generation,
        );
    }
    if complete && cursor != checkpoint {
        return Err("legacy MFT memory state is stale".to_owned());
    }
    Ok((index, complete))
}

fn event_change(
    root: &std::path::Path,
    event: &mft_journal::UsnEventV2,
) -> Result<mft_journal::MftChangeV2, String> {
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
    let kind = mft_journal::normalize_event(event);
    let mut change = mft_journal::MftChangeV2 {
        kind,
        reference: mft_journal::normalize_reference(event.reference),
        parent_reference: mft_journal::normalize_reference(event.parent_reference),
        name: event.name.clone(),
        logical_bytes: 0,
        allocated_bytes: 0,
        is_directory: event.attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
        reason: event.reason,
    };
    if kind == mft_journal::MftChangeKindV2::Upsert {
        match mft_size_map::current_entry(
            root,
            event.reference,
            event.parent_reference,
            event.name.clone(),
            change.is_directory,
        ) {
            Ok(entry) => {
                change.logical_bytes = entry.logical_bytes;
                change.allocated_bytes = entry.allocated_bytes;
            }
            // USN records are historical. A file can be renamed/deleted again
            // before catch-up reaches its earlier upsert event, so querying the
            // old file reference may legitimately fail. Treat that coalesced
            // terminal state as a delete instead of rejecting the entire
            // canonical catch-up and leaving Folder Size partial for another
            // ten-minute rebuild window.
            Err(_) => change.kind = mft_journal::MftChangeKindV2::Delete,
        }
    }
    Ok(change)
}

const fn normalize_journal_replay_cursor(next_usn: i64) -> i64 {
    if next_usn & 7 == 0 {
        next_usn
    } else {
        next_usn.saturating_sub(1)
    }
}

fn catch_up_memory_index(
    root: &std::path::Path,
    mut index: mft_size_map::MftIndexV1,
    cursor: mft_persistence::JournalCursorV1,
    volume_limit_bytes: usize,
    file_limit_bytes: usize,
) -> Result<
    (
        mft_size_map::MftIndexV1,
        mft_persistence::JournalCursorV1,
        Vec<mft_journal::MftChangeV2>,
    ),
    String,
> {
    let target = mft_journal::query_journal(root)?;
    // Older candidates persisted `last event USN + 1` instead of the cursor
    // returned by FSCTL_READ_USN_JOURNAL. That value points inside the record
    // boundary and Windows rejects it with E_INVALIDARG. The preceding value
    // is the actual event USN that produced the legacy cursor; replaying that
    // one event is safe because catch-up coalesces by file reference. Newly
    // persisted cursors come directly from the journal response and are used
    // unchanged.
    let cursor_next_usn = normalize_journal_replay_cursor(cursor.next_usn);
    if target.journal_id != cursor.journal_id
        || cursor_next_usn < target.first_usn.max(target.lowest_valid_usn)
    {
        return Err("startup journal catch-up range is unavailable".to_owned());
    }
    let volume = mft_journal::VolumeIdentityV2 {
        serial: mft_size_map::volume_serial_number(root)?,
    };
    let mut next_usn = cursor_next_usn;
    let mut generation = cursor.generation;
    let mut pending_events = HashMap::<u64, mft_journal::UsnEventV2>::new();
    let mut pending_bytes = 0_usize;
    while next_usn < target.next_usn {
        if STOPPED.load(Ordering::Acquire) {
            return Err("MFT service stopping during journal catch-up".to_owned());
        }
        let checkpoint =
            mft_journal::MftCheckpointV2::new(volume, cursor.journal_id, next_usn, generation);
        let (read_next, events) = mft_journal::read_journal_once(root, checkpoint)
            .map_err(|error| format!("startup journal read at USN {next_usn} failed: {error}"))?;
        if read_next <= next_usn {
            return Err("startup journal catch-up made no progress".to_owned());
        }
        for event in events {
            if STOPPED.load(Ordering::Acquire) {
                return Err("MFT service stopping during journal catch-up".to_owned());
            }
            if let Some(previous) = pending_events.get_mut(&event.reference) {
                pending_bytes =
                    pending_bytes.saturating_sub(64_usize.saturating_add(previous.name.len()));
                previous.reason |= event.reason;
                previous.parent_reference = event.parent_reference;
                previous.usn = previous.usn.max(event.usn);
                previous.attributes = event.attributes;
                if !event.name.is_empty() {
                    previous.name = event.name;
                }
                pending_bytes =
                    pending_bytes.saturating_add(64_usize.saturating_add(previous.name.len()));
            } else {
                pending_bytes =
                    pending_bytes.saturating_add(64_usize.saturating_add(event.name.len()));
                pending_events.insert(event.reference, event);
            }
            if pending_events.len() > mft_journal::PENDING_CHANGE_LIMIT
                || pending_bytes > mft_journal::PENDING_BYTE_LIMIT
            {
                return Err("startup journal catch-up exceeds pending memory bounds".to_owned());
            }
        }
        next_usn = read_next;
        generation = generation.saturating_add(1);
    }
    let mut events = pending_events.into_values().collect::<Vec<_>>();
    events.sort_by_key(|event| event.usn);
    let base_memory = index.memory_breakdown();
    let projected_volume = base_memory
        .volume_index_bytes
        .saturating_add(events.len().saturating_mul(1_024));
    let projected_file = events
        .iter()
        .fold(base_memory.file_data_bytes, |total, event| {
            total.saturating_add(event.name.len().saturating_mul(2))
        });
    if projected_volume > volume_limit_bytes || projected_file > file_limit_bytes {
        return Err("startup journal catch-up exceeds the configured live budget".to_owned());
    }
    let mut changes = Vec::with_capacity(events.len());
    for event in events {
        if STOPPED.load(Ordering::Acquire) {
            return Err("MFT service stopping during journal catch-up".to_owned());
        }
        let change = event_change(root, &event).map_err(|error| {
            format!(
                "startup event {} conversion failed: {error}",
                event.reference
            )
        })?;
        index
            .apply_change(&change)
            .map_err(|error| format!("startup event {} apply failed: {error}", event.reference))?;
        changes.push(change);
    }
    Ok((
        index,
        mft_persistence::JournalCursorV1 {
            journal_id: cursor.journal_id,
            next_usn,
            generation,
        },
        changes,
    ))
}

fn watch_volume_memory(
    cache: PathBuf,
    letter: char,
    root: PathBuf,
    mut checkpoint: mft_journal::MftCheckpointV2,
    live_volumes: Arc<Mutex<HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>>>,
    live_budgets: Arc<Mutex<LiveBudgetStateV1>>,
    focus_leases: Arc<Mutex<mft_persistence::FocusLeaseRegistryV1>>,
    query_activity: Arc<AtomicUsize>,
    query_checkpoint_gate: Arc<RwLock<()>>,
    volume_diagnostics: Arc<Mutex<HashMap<char, mft_query::MftVolumeDiagnosticsV1>>>,
    mut startup_store: StartupStoreV1,
) {
    let ignored_cache_parent = if root == std::path::Path::new(r"C:\") {
        mft_size_map::file_reference_number(&cache).ok()
    } else {
        None
    };
    let mut next_usn = checkpoint.next_usn;
    let mut schedule = mft_persistence::PersistenceScheduleV1::new(monotonic_now());
    let sqlite_path = cache.join(format!("{letter}.mft.sqlite3"));
    // Opening the sole-writer connection can create/update WAL bookkeeping, so
    // startup admission deliberately leaves it closed until both gates open.
    let mut store = None::<mft_sqlite::MftSqliteStoreV1>;
    let mut retired_telemetry = mft_sqlite::StoreTelemetryV1::default();
    let mut budget_blocked_epoch = None::<u64>;
    let mut read_only_reload_epoch = None::<u64>;
    while !STOPPED.load(Ordering::Acquire) {
        let now = monotonic_now();
        let runtime_snapshot = live_volumes.lock().ok().and_then(|live| {
            live.get(&letter).map(|runtime| {
                (
                    runtime.observed,
                    runtime.durable,
                    runtime.pending_count(),
                    runtime.pending_bytes(),
                    runtime.is_exact(),
                )
            })
        });
        if let Some((observed, durable, pending_count, pending_bytes, exact)) = runtime_snapshot {
            let (focus_lease_count, focus_expiry_remaining_ms) = focus_leases
                .lock()
                .map(|mut leases| {
                    (
                        leases.active_count(now) as u64,
                        leases
                            .expiry_remaining(now)
                            .as_millis()
                            .try_into()
                            .unwrap_or(u64::MAX),
                    )
                })
                .unwrap_or_default();
            let current_telemetry = store
                .as_mut()
                .map(mft_sqlite::MftSqliteStoreV1::telemetry)
                .unwrap_or_default();
            let (main_bytes, wal_bytes) =
                mft_sqlite::MftSqliteStoreV1::file_bytes_for_path(&sqlite_path);
            let (migration_state, recovery_reason) = match startup_store {
                StartupStoreV1::LegacyCatchupPending | StartupStoreV1::LegacyMigrationPending => {
                    (1, 0)
                }
                StartupStoreV1::ReplacementRecoveryCatchupPending
                | StartupStoreV1::ReplacementRecoveryPending => (5, 6),
                StartupStoreV1::InvalidCanonicalQuarantineRequired => (2, 5),
                StartupStoreV1::FreshRebuildRequired => (3, 1),
                StartupStoreV1::LiveBudgetLimited => (0, 7),
                StartupStoreV1::CanonicalLiveBudgetLimited => (0, 7),
                StartupStoreV1::LiveBudgetLimitedCleanupPending => (4, 7),
                StartupStoreV1::ExistingCanonicalCleanupCatchupPending
                | StartupStoreV1::ExistingCanonicalCleanupPending => (4, 0),
                _ => (0, 0),
            };
            let diagnostics = mft_query::MftVolumeDiagnosticsV1 {
                volume: letter as u8,
                mode: if exact {
                    if pending_count == 0 { 0 } else { 1 }
                } else {
                    2
                },
                schema: if matches!(
                    startup_store,
                    StartupStoreV1::ExistingCanonicalCatchupPending
                        | StartupStoreV1::ExistingCanonicalCleanupCatchupPending
                        | StartupStoreV1::ExistingCanonical
                        | StartupStoreV1::ExistingCanonicalCleanupPending
                        | StartupStoreV1::CanonicalLiveBudgetLimited
                        | StartupStoreV1::LiveBudgetLimitedCleanupPending
                ) {
                    mft_sqlite::MftSqliteStoreV1::schema_version()
                } else {
                    0
                },
                migration_state,
                recovery_reason,
                transaction_last_outcome: if current_telemetry.transaction_last_outcome == 0 {
                    retired_telemetry.transaction_last_outcome
                } else {
                    current_telemetry.transaction_last_outcome
                },
                checkpoint_last_outcome: if current_telemetry.checkpoint_last_outcome == 0 {
                    retired_telemetry.checkpoint_last_outcome
                } else {
                    current_telemetry.checkpoint_last_outcome
                },
                exact,
                observed_journal_id: observed.journal_id,
                observed_next_usn: observed.next_usn,
                observed_generation: observed.generation,
                durable_journal_id: durable.journal_id,
                durable_next_usn: durable.next_usn,
                durable_generation: durable.generation,
                pending_count: pending_count.try_into().unwrap_or(u64::MAX),
                pending_bytes: pending_bytes.try_into().unwrap_or(u64::MAX),
                last_successful_commit_ms: schedule.last_success().map_or(0, |value| value.0),
                focus_lease_count,
                focus_expiry_remaining_ms,
                main_bytes,
                wal_bytes,
                transaction_attempts: retired_telemetry
                    .transaction_attempts
                    .saturating_add(current_telemetry.transaction_attempts),
                transaction_failures: retired_telemetry
                    .transaction_failures
                    .saturating_add(current_telemetry.transaction_failures),
                checkpoint_attempts: retired_telemetry
                    .checkpoint_attempts
                    .saturating_add(current_telemetry.checkpoint_attempts),
                checkpoint_failures: retired_telemetry
                    .checkpoint_failures
                    .saturating_add(current_telemetry.checkpoint_failures),
            };
            if let Ok(mut volumes) = volume_diagnostics.lock() {
                volumes.insert(letter, diagnostics);
            }
        }
        let runtime_exact = live_volumes
            .lock()
            .ok()
            .and_then(|live| {
                live.get(&letter)
                    .map(mft_runtime::VolumeMemoryRuntimeV1::is_exact)
            })
            .unwrap_or(false);
        if run_pending_legacy_cleanup(
            &mut startup_store,
            &mut schedule,
            &focus_leases,
            &cache,
            &sqlite_path,
            letter,
        ) {
            continue;
        }
        let persisted_limit = live_budgets.lock().ok().and_then(|budgets| {
            budgets
                .persisted_prune_pending
                .then_some(u64::from(budgets.limits.persisted_index_mb) * 1024 * 1024)
        });
        if startup_store == StartupStoreV1::ExistingCanonical
            && persisted_limit
                .is_some_and(|limit| persisted_sqlite_prune_target(&cache, limit) == Some(letter))
        {
            let prune_focused = focus_leases
                .lock()
                .is_ok_and(|mut leases| leases.any_focused(now));
            if schedule.decision(now, true, prune_focused)
                == mft_persistence::PersistenceDecisionV1::BeginAttempt
            {
                let prune_snapshot = live_budgets
                    .lock()
                    .map_err(|_| "MFT live budget state is unavailable".to_owned())
                    .and_then(|mut budgets| {
                        let live = live_volumes
                            .lock()
                            .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
                        let runtime = live
                            .get(&letter)
                            .ok_or_else(|| "MFT live volume is unavailable".to_owned())?;
                        let memory = runtime.index.memory_breakdown();
                        let total = live.values().fold((0_usize, 0_usize), |total, runtime| {
                            let current = runtime.index.memory_breakdown();
                            (
                                total.0.saturating_add(current.volume_index_bytes),
                                total.1.saturating_add(current.file_data_bytes),
                            )
                        });
                        if total
                            .0
                            .saturating_add(budgets.reserved_volume_bytes)
                            .saturating_add(memory.volume_index_bytes)
                            > usize::from(budgets.limits.volume_index_mb) * 1024 * 1024
                            || total
                                .1
                                .saturating_add(budgets.reserved_file_bytes)
                                .saturating_add(memory.file_data_bytes)
                                > usize::from(budgets.limits.file_data_mb) * 1024 * 1024
                        {
                            return Err(
                                "MFT persisted prune snapshot exceeds the live budget".to_owned()
                            );
                        }
                        let reservation = reserve_live_scratch_locked(
                            &mut budgets,
                            &live_budgets,
                            memory.volume_index_bytes,
                            memory.file_data_bytes,
                        )?;
                        Ok((
                            Arc::clone(&runtime.index),
                            runtime.observed,
                            budgets.epoch,
                            u64::from(budgets.limits.persisted_index_mb) * 1024 * 1024,
                            reservation,
                        ))
                    });
                let Ok((snapshot, cursor, source_epoch, limit, reservation)) = prune_snapshot
                else {
                    continue;
                };
                if let Some(mut current) = store.take() {
                    let telemetry = current.telemetry();
                    retired_telemetry.transaction_attempts = retired_telemetry
                        .transaction_attempts
                        .saturating_add(telemetry.transaction_attempts);
                    retired_telemetry.transaction_failures = retired_telemetry
                        .transaction_failures
                        .saturating_add(telemetry.transaction_failures);
                    retired_telemetry.checkpoint_attempts = retired_telemetry
                        .checkpoint_attempts
                        .saturating_add(telemetry.checkpoint_attempts);
                    retired_telemetry.checkpoint_failures = retired_telemetry
                        .checkpoint_failures
                        .saturating_add(telemetry.checkpoint_failures);
                    drop(current);
                }
                let pruned = RECOVERY_LOCK
                    .get_or_init(|| Mutex::new(()))
                    .lock()
                    .map_err(|_| "MFT recovery lock is poisoned".to_owned())
                    .and_then(|_guard| {
                        let _persisted_guard = persisted_write_guard()?;
                        if persisted_sqlite_prune_target(&cache, limit) != Some(letter) {
                            return Err("MFT persisted prune target changed".to_owned());
                        }
                        let candidate_allowance =
                            persisted_candidate_allowance(&cache, &sqlite_path, limit, true)
                                .saturating_sub(4096);
                        if candidate_allowance == 0 {
                            return Err("MFT persisted prune has no atomic allowance".to_owned());
                        }
                        let mut partial = (*snapshot).clone();
                        partial.trim_persisted_to_bytes(
                            (candidate_allowance / 4).try_into().unwrap_or(usize::MAX),
                        );
                        begin_persistence_attempt(&mut schedule, &focus_leases)?;
                        mft_sqlite::MftSqliteStoreV1::prune_persisted_store_focused_linearized(
                            &sqlite_path,
                            &cache,
                            &persisted_incomplete_path(&cache, letter),
                            mft_sqlite::StoreIdentityV1 {
                                volume: checkpoint.volume,
                                cursor,
                                complete: false,
                            },
                            &partial,
                            candidate_allowance,
                            &LIFECYCLE_BARRIER,
                            || {
                                LIFECYCLE_BARRIER.is_open()
                                    && !STOPPED.load(Ordering::Acquire)
                                    && live_budgets
                                        .lock()
                                        .is_ok_and(|budgets| budgets.epoch == source_epoch)
                                    && focus_leases
                                        .lock()
                                        .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                            },
                        )
                        .map(|candidate| (candidate, reservation))
                    });
                let prune_succeeded = pruned.is_ok();
                if let Ok((candidate, reservation)) = pruned {
                    drop(candidate);
                    let _ = update_runtime_and_release_reservation(
                        &live_budgets,
                        &live_volumes,
                        letter,
                        reservation,
                        |runtime| {
                            runtime.mark_inexact();
                            Ok(())
                        },
                    );
                }
                if let Ok(mut budgets) = live_budgets.lock() {
                    budgets.persisted_prune_pending = persisted_cache_bytes(&cache)
                        > u64::from(budgets.limits.persisted_index_mb) * 1024 * 1024;
                    if prune_succeeded || persisted_incomplete_path(&cache, letter).exists() {
                        budgets.blocked_volumes.insert(letter);
                        budget_blocked_epoch = Some(budgets.epoch);
                    }
                }
                if prune_succeeded || persisted_incomplete_path(&cache, letter).exists() {
                    if !prune_succeeded {
                        let _ = update_runtime_under_live_budget(
                            &live_budgets,
                            &live_volumes,
                            letter,
                            |runtime| {
                                runtime.mark_inexact();
                                Ok(())
                            },
                        );
                    }
                    startup_store = StartupStoreV1::LiveBudgetLimited;
                }
                continue;
            }
        }
        let budget_snapshot = live_budgets
            .lock()
            .ok()
            .map(|budgets| (budgets.epoch, budgets.blocked_volumes.contains(&letter)));
        if !runtime_exact && budget_snapshot.is_some_and(|(_, blocked)| blocked) {
            startup_store = startup_store.live_budget_limited();
            budget_blocked_epoch = budget_snapshot.map(|(epoch, _)| epoch);
        } else if matches!(
            startup_store,
            StartupStoreV1::LiveBudgetLimited | StartupStoreV1::CanonicalLiveBudgetLimited
        ) && budget_snapshot
            .is_some_and(|(epoch, blocked)| !blocked || budget_blocked_epoch != Some(epoch))
        {
            startup_store = StartupStoreV1::FreshRebuildRequired;
            budget_blocked_epoch = None;
        }
        if !runtime_exact
            && matches!(
                startup_store,
                StartupStoreV1::LiveBudgetLimited | StartupStoreV1::CanonicalLiveBudgetLimited
            )
        {
            // A complete index cannot fit the configured live-memory budget.
            // Keep the observed cursor current even though entry mutations
            // cannot be retained. This prevents a stale durable SQLite
            // snapshot from being mistaken for a current exact result. No
            // pending entry detail is stored and no periodic disk work occurs
            // until a budget change grants one new proof attempt.
            match mft_journal::query_journal(&root) {
                Ok(journal) if journal.journal_id == checkpoint.journal_id => {
                    if let Ok(mut live) = live_volumes.lock()
                        && let Some(runtime) = live.get_mut(&letter)
                    {
                        let _ = runtime.advance_inexact_observed(journal.next_usn);
                    }
                    next_usn = next_usn.max(journal.next_usn);
                }
                Ok(_) | Err(_) => {
                    startup_store = StartupStoreV1::FreshRebuildRequired;
                    budget_blocked_epoch = None;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        if !runtime_exact
            && matches!(
                startup_store,
                StartupStoreV1::ExistingCanonical | StartupStoreV1::ExistingCanonicalCleanupPending
            )
        {
            // Memory-budget trims and later ambiguity invalidate exactness but
            // do not alter the durable store identity. Convert that state into
            // the serialized, foreground-gated rebuild path instead of
            // sleeping forever in an inexact normal state.
            startup_store = StartupStoreV1::FreshRebuildRequired;
        }
        if !runtime_exact
            && matches!(
                startup_store,
                StartupStoreV1::ExistingCanonicalCatchupPending
                    | StartupStoreV1::ExistingCanonicalCleanupCatchupPending
                    | StartupStoreV1::LegacyCatchupPending
                    | StartupStoreV1::ReplacementRecoveryCatchupPending
            )
        {
            let candidate = (|| {
                let mut budgets = live_budgets.lock().ok()?;
                let mut live = live_volumes.lock().ok()?;
                let other = live.iter().filter(|(key, _)| **key != letter).fold(
                    (0_usize, 0_usize),
                    |(volume_total, file_total), (_, runtime)| {
                        let memory = runtime.index.memory_breakdown();
                        (
                            volume_total.saturating_add(memory.volume_index_bytes),
                            file_total.saturating_add(memory.file_data_bytes),
                        )
                    },
                );
                let volume_remaining = (usize::from(budgets.limits.volume_index_mb) * 1024 * 1024)
                    .saturating_sub(budgets.reserved_volume_bytes)
                    .saturating_sub(other.0);
                let file_remaining = (usize::from(budgets.limits.file_data_mb) * 1024 * 1024)
                    .saturating_sub(budgets.reserved_file_bytes)
                    .saturating_sub(other.1);
                let runtime = live.get_mut(&letter)?;
                let index = std::mem::replace(
                    &mut runtime.index,
                    Arc::new(mft_size_map::MftIndexV1::from_entries(
                        std::collections::BTreeMap::new(),
                    )),
                );
                let index = Arc::try_unwrap(index).unwrap_or_else(|index| (*index).clone());
                let cursor = runtime.durable;
                let memory = index.memory_breakdown();
                let scratch_volume_limit = volume_remaining.min(
                    memory
                        .volume_index_bytes
                        .saturating_add(mft_journal::PENDING_CHANGE_LIMIT.saturating_mul(1_024)),
                );
                let scratch_file_limit = file_remaining.min(
                    memory
                        .file_data_bytes
                        .saturating_add(mft_journal::PENDING_BYTE_LIMIT.saturating_mul(2)),
                );
                let reservation = reserve_live_scratch_locked(
                    &mut budgets,
                    &live_budgets,
                    scratch_volume_limit,
                    scratch_file_limit,
                )
                .ok()?;
                let trimmed = enforce_live_budgets_locked(&mut live, &budgets);
                budgets.blocked_volumes.extend(trimmed);
                Some((
                    index,
                    cursor,
                    scratch_volume_limit,
                    scratch_file_limit,
                    reservation,
                ))
            })();
            let Some((index, cursor, volume_limit, file_limit, reservation)) = candidate else {
                break;
            };
            match catch_up_memory_index(&root, index, cursor, volume_limit, file_limit) {
                Ok((index, observed, changes)) => {
                    let current_budget = live_budgets.lock().ok().and_then(|budgets| {
                        live_volumes.lock().ok().map(|live| {
                            let other = live.iter().filter(|(key, _)| **key != letter).fold(
                                (0_usize, 0_usize),
                                |total, (_, runtime)| {
                                    let current = runtime.index.memory_breakdown();
                                    (
                                        total.0.saturating_add(current.volume_index_bytes),
                                        total.1.saturating_add(current.file_data_bytes),
                                    )
                                },
                            );
                            let memory = index.memory_breakdown();
                            other.0.saturating_add(memory.volume_index_bytes)
                                <= usize::from(budgets.limits.volume_index_mb) * 1024 * 1024
                                && other.1.saturating_add(memory.file_data_bytes)
                                    <= usize::from(budgets.limits.file_data_mb) * 1024 * 1024
                        })
                    });
                    if current_budget != Some(true) {
                        if let Ok(mut budgets) = live_budgets.lock() {
                            budgets.blocked_volumes.insert(letter);
                        }
                        startup_store = StartupStoreV1::LiveBudgetLimited;
                        continue;
                    }
                    let installed = update_runtime_and_release_reservation(
                        &live_budgets,
                        &live_volumes,
                        letter,
                        reservation,
                        move |runtime| {
                            runtime.replace_with_caught_up(index, cursor, observed, changes)
                        },
                    );
                    let Ok((exact, budget_epoch)) = installed else {
                        startup_store = StartupStoreV1::FreshRebuildRequired;
                        continue;
                    };
                    next_usn = observed.next_usn;
                    if exact {
                        startup_store = match startup_store {
                            StartupStoreV1::ExistingCanonicalCatchupPending => {
                                StartupStoreV1::ExistingCanonical
                            }
                            StartupStoreV1::ExistingCanonicalCleanupCatchupPending => {
                                StartupStoreV1::ExistingCanonicalCleanupPending
                            }
                            StartupStoreV1::LegacyCatchupPending => {
                                StartupStoreV1::LegacyMigrationPending
                            }
                            StartupStoreV1::ReplacementRecoveryCatchupPending => {
                                StartupStoreV1::ReplacementRecoveryPending
                            }
                            _ => unreachable!(),
                        };
                    } else {
                        startup_store = StartupStoreV1::LiveBudgetLimited;
                        budget_blocked_epoch = Some(budget_epoch);
                    }
                }
                Err(error) => {
                    // A journal gap, overload, or live-record lookup failure does
                    // not make the previously admitted durable SQLite store
                    // corrupt. Preserve it until a complete foreground rebuild
                    // candidate has been verified and atomically replaces it.
                    if error.contains("configured live budget") {
                        if let Ok(mut budgets) = live_budgets.lock() {
                            budgets.blocked_volumes.insert(letter);
                            budget_blocked_epoch = Some(budgets.epoch);
                        }
                        startup_store = StartupStoreV1::LiveBudgetLimited;
                    } else {
                        startup_store = StartupStoreV1::FreshRebuildRequired;
                    }
                }
            }
            continue;
        }
        if !runtime_exact && startup_store == StartupStoreV1::InvalidCanonicalQuarantineRequired {
            let now = monotonic_now();
            let focused = focus_leases
                .lock()
                .is_ok_and(|mut leases| leases.any_focused(now));
            if schedule.decision(now, true, focused)
                == mft_persistence::PersistenceDecisionV1::BeginAttempt
            {
                let quarantine_parent = cache.parent().unwrap_or(&cache).join("MftIndexQuarantine");
                let quarantined = persisted_write_guard().and_then(|_persisted_guard| {
                    begin_persistence_attempt(&mut schedule, &focus_leases)?;
                    mft_migration::quarantine_canonical_linearized(
                        &cache,
                        &quarantine_parent,
                        letter,
                        now.0,
                        &LIFECYCLE_BARRIER,
                        || {
                            LIFECYCLE_BARRIER.is_open()
                                && !STOPPED.load(Ordering::Acquire)
                                && focus_leases
                                    .lock()
                                    .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                        },
                    )
                });
                if quarantined.is_ok() {
                    startup_store = StartupStoreV1::FreshRebuildRequired;
                    schedule.record_success(monotonic_record_time());
                }
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        if !runtime_exact && startup_store == StartupStoreV1::FreshRebuildRequired {
            let now = monotonic_now();
            let (focused, query_demand) = focus_leases
                .lock()
                .map(|mut leases| {
                    let query_demand = leases.contains_active(QUERY_DEMAND_LEASE_ID_V1, now);
                    (leases.any_focused(now), query_demand)
                })
                .unwrap_or_default();
            if query_demand {
                schedule.expedite_initial_recovery(now);
            }
            let current_budget_epoch = live_budgets.lock().ok().map(|budgets| budgets.epoch);
            if focused && current_budget_epoch != read_only_reload_epoch {
                // A failed bounded load must not be retried every 100 ms. Retry
                // only after an actual budget/preference epoch change.
                read_only_reload_epoch = current_budget_epoch;
                // A low startup budget can admit an otherwise complete
                // canonical SQLite store as a typed partial runtime. When a
                // focused client later raises the budget, reload that durable
                // store and catch up through USN before considering a full MFT
                // scan or any SQLite replacement. This path is read-only, so it
                // must not wait for the ten-minute persistence cadence. Doing
                // so would leave a newly preferred foreground volume partial
                // even though its canonical store can be admitted immediately.
                let reloaded = RECOVERY_LOCK
                    .get_or_init(|| Mutex::new(()))
                    .lock()
                    .map_err(|_| "MFT recovery lock is poisoned".to_owned())
                    .and_then(|_guard| {
                        reload_canonical_into_live_memory(
                            &cache,
                            &sqlite_path,
                            &root,
                            letter,
                            checkpoint.volume,
                            &live_budgets,
                            &live_volumes,
                        )
                    });
                if let Ok(Some((index, durable, observed, changes, reservation))) = reloaded {
                    let installed = update_runtime_and_release_reservation(
                        &live_budgets,
                        &live_volumes,
                        letter,
                        reservation,
                        move |runtime| {
                            runtime.replace_with_caught_up(index, durable, observed, changes)
                        },
                    );
                    if let Ok((true, _)) = installed {
                        checkpoint = mft_journal::MftCheckpointV2::new(
                            checkpoint.volume,
                            durable.journal_id,
                            durable.next_usn,
                            durable.generation,
                        );
                        next_usn = observed.next_usn;
                        let cleanup_pending =
                            mft_sqlite::MftSqliteStoreV1::replacement_backup_path(&sqlite_path)
                                .exists()
                                || mft_migration::inventory_legacy(&cache, letter)
                                    .is_ok_and(|members| !members.is_empty());
                        startup_store = if cleanup_pending {
                            StartupStoreV1::ExistingCanonicalCleanupPending
                        } else {
                            StartupStoreV1::ExistingCanonical
                        };
                        budget_blocked_epoch = None;
                        continue;
                    }
                }
            }
            if schedule.decision(now, true, focused)
                == mft_persistence::PersistenceDecisionV1::BeginAttempt
            {
                if sqlite_path.exists() {
                    // Close without checkpoint. The verified rebuild path
                    // creates a self-contained rollback-mode safety copy from
                    // the logical DB (including WAL) only after the candidate
                    // itself is complete.
                    if let Some(mut previous) = store.take() {
                        let telemetry = previous.telemetry();
                        retired_telemetry.transaction_attempts = retired_telemetry
                            .transaction_attempts
                            .saturating_add(telemetry.transaction_attempts);
                        retired_telemetry.transaction_failures = retired_telemetry
                            .transaction_failures
                            .saturating_add(telemetry.transaction_failures);
                        retired_telemetry.checkpoint_attempts = retired_telemetry
                            .checkpoint_attempts
                            .saturating_add(telemetry.checkpoint_attempts);
                        retired_telemetry.checkpoint_failures = retired_telemetry
                            .checkpoint_failures
                            .saturating_add(telemetry.checkpoint_failures);
                        if telemetry.transaction_last_outcome != 0 {
                            retired_telemetry.transaction_last_outcome =
                                telemetry.transaction_last_outcome;
                        }
                        if telemetry.checkpoint_last_outcome != 0 {
                            retired_telemetry.checkpoint_last_outcome =
                                telemetry.checkpoint_last_outcome;
                        }
                    }
                }
                let rebuilt = RECOVERY_LOCK
                    .get_or_init(|| Mutex::new(()))
                    .lock()
                    .map_err(|_| "MFT recovery lock is poisoned".to_owned())
                    .and_then(|_guard| {
                        if STOPPED.load(Ordering::Acquire) {
                            return Err("MFT service stopping".to_owned());
                        }
                        let journal_before = mft_journal::query_journal(&root)?;
                        let (volume_remaining, file_remaining, source_epoch, reservation) = {
                            let mut budgets = live_budgets
                                .lock()
                                .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
                            let mut live = live_volumes
                                .lock()
                                .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
                            let other = live.iter().filter(|(key, _)| **key != letter).fold(
                                (0_usize, 0_usize),
                                |(volume_total, file_total), (_, runtime)| {
                                    let memory = runtime.index.memory_breakdown();
                                    (
                                        volume_total.saturating_add(memory.volume_index_bytes),
                                        file_total.saturating_add(memory.file_data_bytes),
                                    )
                                },
                            );
                            let volume_remaining =
                                (usize::from(budgets.limits.volume_index_mb) * 1024 * 1024)
                                    .saturating_sub(budgets.reserved_volume_bytes)
                                    .saturating_sub(other.0);
                            let file_remaining =
                                (usize::from(budgets.limits.file_data_mb) * 1024 * 1024)
                                    .saturating_sub(budgets.reserved_file_bytes)
                                    .saturating_sub(other.1);
                            let reservation = reserve_live_scratch_locked(
                                &mut budgets,
                                &live_budgets,
                                volume_remaining,
                                file_remaining,
                            )?;
                            let trimmed = enforce_live_budgets_locked(&mut live, &budgets);
                            budgets.blocked_volumes.extend(trimmed);
                            (volume_remaining, file_remaining, budgets.epoch, reservation)
                        };
                        let (index, budget_complete) = mft_size_map::read_volume_index_bounded(
                            &root,
                            volume_remaining,
                            file_remaining,
                            || STOPPED.load(Ordering::Acquire),
                        )?;
                        if !budget_complete {
                            let partial_cursor = mft_persistence::JournalCursorV1 {
                                journal_id: journal_before.journal_id,
                                next_usn: journal_before.next_usn,
                                generation: 0,
                            };
                            let _ = update_runtime_and_release_reservation(
                                &live_budgets,
                                &live_volumes,
                                letter,
                                reservation,
                                move |runtime| {
                                    runtime.replace_with_partial(index, partial_cursor);
                                    Ok(())
                                },
                            );
                            if let Ok(mut budgets) = live_budgets.lock() {
                                budgets.blocked_volumes.insert(letter);
                            }
                            return Err(
                                "MFT full index exceeds the configured live budget".to_owned()
                            );
                        }
                        let initial_cursor = mft_persistence::JournalCursorV1 {
                            journal_id: journal_before.journal_id,
                            next_usn: journal_before.next_usn,
                            generation: 0,
                        };
                        let (index, durable, _) = catch_up_memory_index(
                            &root,
                            index,
                            initial_cursor,
                            volume_remaining,
                            file_remaining,
                        )?;
                        if STOPPED.load(Ordering::Acquire) {
                            return Err("MFT service stopping before rebuild promotion".to_owned());
                        }
                        // Every SQLite/file-set mutation shares this gate. It is
                        // intentionally acquired after the read-only MFT scan,
                        // then held through allowance measurement and atomic
                        // promotion so another volume cannot consume the same
                        // persisted-cache slack.
                        let _persisted_guard = persisted_write_guard()?;
                        let current_limits = live_budgets
                            .lock()
                            .map_err(|_| "MFT live budget state is unavailable".to_owned())?;
                        let live = live_volumes
                            .lock()
                            .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
                        let other = live.iter().filter(|(key, _)| **key != letter).fold(
                            (0_usize, 0_usize),
                            |total, (_, runtime)| {
                                let current = runtime.index.memory_breakdown();
                                (
                                    total.0.saturating_add(current.volume_index_bytes),
                                    total.1.saturating_add(current.file_data_bytes),
                                )
                            },
                        );
                        let memory = index.memory_breakdown();
                        if current_limits.epoch != source_epoch
                            || memory.volume_index_bytes > volume_remaining
                            || memory.file_data_bytes > file_remaining
                            || other.0.saturating_add(memory.volume_index_bytes)
                                > usize::from(current_limits.limits.volume_index_mb) * 1024 * 1024
                            || other.1.saturating_add(memory.file_data_bytes)
                                > usize::from(current_limits.limits.file_data_mb) * 1024 * 1024
                        {
                            drop(live);
                            drop(current_limits);
                            if let Ok(mut budgets) = live_budgets.lock() {
                                budgets.blocked_volumes.insert(letter);
                            }
                            return Err("MFT rebuild budget changed before first write".to_owned());
                        }
                        let persisted_limit =
                            u64::from(current_limits.limits.persisted_index_mb) * 1024 * 1024;
                        drop(live);
                        drop(current_limits);
                        let replace_existing = sqlite_path.exists();
                        let candidate_allowance = persisted_candidate_allowance(
                            &cache,
                            &sqlite_path,
                            persisted_limit,
                            replace_existing,
                        );
                        if candidate_allowance == 0 {
                            return Err("MFT rebuild has no persisted-budget allowance".to_owned());
                        }
                        // A full MFT scan can take longer than the persistence
                        // interval. Advance the attempt clock only at the first
                        // SQLite write boundary, not before the read-only scan.
                        begin_persistence_attempt(&mut schedule, &focus_leases)?;
                        let temporary = cache.join(format!("{letter}.mft.sqlite3.migration-tmp"));
                        let candidate =
                            mft_sqlite::MftSqliteStoreV1::snapshot_focused_bounded_linearized(
                                &temporary,
                                &sqlite_path,
                                &cache,
                                mft_sqlite::StoreIdentityV1 {
                                    volume: checkpoint.volume,
                                    cursor: durable,
                                    complete: true,
                                },
                                &index,
                                replace_existing,
                                candidate_allowance,
                                &LIFECYCLE_BARRIER,
                                || {
                                    live_budgets
                                        .lock()
                                        .is_ok_and(|budgets| budgets.epoch == source_epoch)
                                        && focus_leases.lock().is_ok_and(|mut leases| {
                                            leases.any_focused(monotonic_now())
                                        })
                                },
                            )?;
                        Ok((index, durable, candidate, reservation))
                    });
                if let Ok((index, durable, candidate, reservation)) = rebuilt {
                    let marker = persisted_incomplete_path(&cache, letter);
                    let marker_cleanup = if marker.exists() {
                        LIFECYCLE_BARRIER.invoke(|| {
                            if !focus_leases
                                .lock()
                                .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                            {
                                return Err("MFT focus lease expired before prune marker cleanup"
                                    .to_owned());
                            }
                            std::fs::remove_file(&marker).map_err(|error| error.to_string())
                        })
                    } else {
                        Ok(())
                    };
                    if marker_cleanup.is_err() {
                        store = Some(candidate);
                        startup_store = StartupStoreV1::LiveBudgetLimited;
                        continue;
                    }
                    let installed = update_runtime_and_release_reservation(
                        &live_budgets,
                        &live_volumes,
                        letter,
                        reservation,
                        move |runtime| {
                            runtime.replace_with_exact(index, durable);
                            Ok(())
                        },
                    );
                    checkpoint = mft_journal::MftCheckpointV2::new(
                        checkpoint.volume,
                        durable.journal_id,
                        durable.next_usn,
                        durable.generation,
                    );
                    next_usn = durable.next_usn;
                    store = Some(candidate);
                    match installed {
                        Ok((true, _)) => {
                            startup_store = StartupStoreV1::ExistingCanonical;
                            budget_blocked_epoch = None;
                        }
                        Ok((false, epoch)) => {
                            startup_store = StartupStoreV1::LiveBudgetLimited;
                            budget_blocked_epoch = Some(epoch);
                        }
                        Err(_) => startup_store = StartupStoreV1::FreshRebuildRequired,
                    }
                    schedule.record_success(monotonic_record_time());
                }
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        if runtime_exact && startup_store == StartupStoreV1::ReplacementRecoveryPending {
            let now = monotonic_now();
            let focused = focus_leases
                .lock()
                .is_ok_and(|mut leases| leases.any_focused(now));
            if schedule.decision(now, true, focused)
                == mft_persistence::PersistenceDecisionV1::BeginAttempt
            {
                let backup = mft_sqlite::MftSqliteStoreV1::replacement_backup_path(&sqlite_path);
                let recovered = persisted_write_guard().and_then(|_persisted_guard| {
                    let persisted_limit = live_budgets
                        .lock()
                        .map_err(|_| "MFT live budget state is unavailable".to_owned())
                        .map(|budgets| {
                            u64::from(budgets.limits.persisted_index_mb) * 1024 * 1024
                        })?;
                    if replacement_recovery_projected_bytes(&cache, &sqlite_path) > persisted_limit
                    {
                        return Err(
                            "MFT replacement recovery exceeds the persisted budget".to_owned()
                        );
                    }
                    begin_persistence_attempt(&mut schedule, &focus_leases)?;
                    let recovered =
                        mft_sqlite::MftSqliteStoreV1::restore_replacement_backup_focused_linearized(
                        &backup,
                        &sqlite_path,
                        &cache,
                        checkpoint.volume,
                        checkpoint.journal_id,
                        &LIFECYCLE_BARRIER,
                        || {
                            focus_leases
                                .lock()
                                .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                        },
                    )?;
                    let marker = persisted_incomplete_path(&cache, letter);
                    let marker_clean = if marker.exists() {
                        LIFECYCLE_BARRIER
                            .invoke(|| {
                                if !focus_leases
                                    .lock()
                                    .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                                {
                                    return Err(
                                        "MFT focus lease expired before prune marker cleanup"
                                            .to_owned(),
                                    );
                                }
                                std::fs::remove_file(&marker).map_err(|error| error.to_string())
                            })
                            .is_ok()
                    } else {
                        true
                    };
                    Ok((recovered, marker_clean))
                });
                if let Ok(mut budgets) = live_budgets.lock() {
                    let limit = u64::from(budgets.limits.persisted_index_mb) * 1024 * 1024;
                    budgets.persisted_prune_pending = persisted_cache_bytes(&cache) > limit
                        || (backup.is_file()
                            && replacement_recovery_projected_bytes(&cache, &sqlite_path) > limit);
                }
                if let Ok((recovered, marker_clean)) = recovered {
                    store = Some(recovered);
                    startup_store = if !marker_clean
                        || mft_migration::inventory_legacy(&cache, letter)
                            .is_ok_and(|entries| !entries.is_empty())
                    {
                        StartupStoreV1::ExistingCanonicalCleanupPending
                    } else {
                        StartupStoreV1::ExistingCanonical
                    };
                    schedule.record_success(monotonic_record_time());
                } else if sqlite_path.is_file() && !backup.is_file() {
                    // The atomic backup promotion may have succeeded before a
                    // later focus/lifecycle-gated writer reopen lost its gate.
                    // Adopt the exact canonical state read-only so this worker
                    // cannot loop forever waiting for a consumed backup.
                    let limits = live_budgets.lock().ok().map(|budgets| budgets.limits);
                    if let Some(limits) = limits
                        && persisted_cache_bytes(&cache)
                            <= u64::from(limits.persisted_index_mb) * 1024 * 1024
                        && let Ok((identity, index, budget_complete)) =
                            mft_sqlite::MftSqliteStoreV1::load_read_only_bounded(
                                &sqlite_path,
                                &cache,
                                checkpoint.volume,
                                checkpoint.journal_id,
                                usize::from(limits.volume_index_mb) * 1024 * 1024,
                                usize::from(limits.file_data_mb) * 1024 * 1024,
                            )
                        && budget_complete
                        && update_runtime_under_live_budget(
                            &live_budgets,
                            &live_volumes,
                            letter,
                            move |runtime| {
                                runtime.replace_with_exact(index, identity.cursor);
                                Ok(())
                            },
                        )
                        .is_ok_and(|(exact, _)| exact)
                    {
                        checkpoint = mft_journal::MftCheckpointV2::new(
                            checkpoint.volume,
                            identity.cursor.journal_id,
                            identity.cursor.next_usn,
                            identity.cursor.generation,
                        );
                        next_usn = identity.cursor.next_usn;
                        startup_store = StartupStoreV1::ExistingCanonicalCleanupPending;
                        schedule.record_success(monotonic_record_time());
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        if !runtime_exact {
            // Invalid canonical sets remain quarantined and inadmissible until
            // the foreground maintenance state machine disposes them.
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        let read_checkpoint = mft_journal::MftCheckpointV2::new(
            checkpoint.volume,
            checkpoint.journal_id,
            next_usn,
            checkpoint.generation,
        );
        let (read_next_usn, events) = match mft_journal::read_journal_once(&root, read_checkpoint) {
            Ok(result) => result,
            Err(_) => {
                if let Ok(mut live) = live_volumes.lock()
                    && let Some(runtime) = live.get_mut(&letter)
                {
                    runtime.mark_inexact();
                }
                startup_store = StartupStoreV1::FreshRebuildRequired;
                continue;
            }
        };
        let mut rebuild_required = false;
        let mut prepared_changes = Vec::new();
        for event in events {
            if ignored_cache_parent.is_some_and(|reference| {
                event.reference == reference || event.parent_reference == reference
            }) {
                continue;
            }
            let Ok(change) = event_change(&root, &event) else {
                if let Ok(mut live) = live_volumes.lock()
                    && let Some(runtime) = live.get_mut(&letter)
                {
                    runtime.mark_inexact();
                }
                rebuild_required = true;
                break;
            };
            // The response cursor, not an individual record's USN, is the
            // only valid restart position for the coalesced batch.
            prepared_changes.push((change, read_next_usn));
        }
        let mut budget_limited = None;
        if !rebuild_required && !prepared_changes.is_empty() {
            // Hold the shared budget before the coherent live-state lock. A
            // query cannot observe newly applied USN entries as exact until
            // the same critical section has enforced the current hard limits.
            let applied = observe_batch_under_live_budget(
                &live_budgets,
                &live_volumes,
                letter,
                prepared_changes,
            );
            match applied {
                Ok(epoch) => budget_limited = epoch,
                Err(_) => rebuild_required = true,
            }
        }
        if let Some(epoch) = budget_limited {
            startup_store = startup_store.live_budget_limited();
            budget_blocked_epoch = Some(epoch);
            continue;
        }
        if rebuild_required {
            startup_store = StartupStoreV1::FreshRebuildRequired;
            continue;
        }
        next_usn = next_usn.max(read_next_usn);

        let now = monotonic_now();
        let focused = focus_leases
            .lock()
            .is_ok_and(|mut leases| leases.any_focused(now));
        let has_pending = live_volumes
            .lock()
            .ok()
            .and_then(|live| {
                live.get(&letter)
                    .map(mft_runtime::VolumeMemoryRuntimeV1::has_pending)
            })
            .unwrap_or(false);
        if startup_store == StartupStoreV1::LegacyMigrationPending
            && schedule.decision(now, true, focused)
                == mft_persistence::PersistenceDecisionV1::BeginAttempt
        {
            let migration_snapshot = live_budgets
                .lock()
                .map_err(|_| "MFT live budget state is unavailable".to_owned())
                .and_then(|mut budgets| {
                    let live = live_volumes
                        .lock()
                        .map_err(|_| "MFT live volume state is unavailable".to_owned())?;
                    let runtime = live
                        .get(&letter)
                        .ok_or_else(|| "MFT live volume is unavailable".to_owned())?;
                    let memory = runtime.index.memory_breakdown();
                    let total = live.values().fold((0_usize, 0_usize), |total, runtime| {
                        let current = runtime.index.memory_breakdown();
                        (
                            total.0.saturating_add(current.volume_index_bytes),
                            total.1.saturating_add(current.file_data_bytes),
                        )
                    });
                    if total
                        .0
                        .saturating_add(budgets.reserved_volume_bytes)
                        .saturating_add(memory.volume_index_bytes)
                        > usize::from(budgets.limits.volume_index_mb) * 1024 * 1024
                        || total
                            .1
                            .saturating_add(budgets.reserved_file_bytes)
                            .saturating_add(memory.file_data_bytes)
                            > usize::from(budgets.limits.file_data_mb) * 1024 * 1024
                    {
                        return Err(
                            "MFT migration snapshot exceeds the configured live budget".to_owned()
                        );
                    }
                    let reservation = reserve_live_scratch_locked(
                        &mut budgets,
                        &live_budgets,
                        memory.volume_index_bytes,
                        memory.file_data_bytes,
                    )?;
                    Ok((
                        Arc::clone(&runtime.index),
                        runtime.observed,
                        u64::from(budgets.limits.persisted_index_mb) * 1024 * 1024,
                        budgets.epoch,
                        reservation,
                    ))
                });
            let migrated = migration_snapshot.and_then(
                |(index, cursor, persisted_limit, source_epoch, _reservation)| {
                    let _persisted_guard = persisted_write_guard()?;
                    let candidate_allowance =
                        persisted_candidate_allowance(&cache, &sqlite_path, persisted_limit, false);
                    if candidate_allowance == 0 {
                        return Err("MFT migration has no persisted-budget allowance".to_owned());
                    }
                    begin_persistence_attempt(&mut schedule, &focus_leases)?;
                    let temporary = cache.join(format!("{letter}.mft.sqlite3.migration-tmp"));
                    let candidate =
                        mft_sqlite::MftSqliteStoreV1::snapshot_focused_bounded_linearized(
                            &temporary,
                            &sqlite_path,
                            &cache,
                            mft_sqlite::StoreIdentityV1 {
                                volume: checkpoint.volume,
                                cursor,
                                complete: true,
                            },
                            &index,
                            false,
                            candidate_allowance,
                            &LIFECYCLE_BARRIER,
                            || {
                                live_budgets
                                    .lock()
                                    .is_ok_and(|budgets| budgets.epoch == source_epoch)
                                    && focus_leases
                                        .lock()
                                        .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                            },
                        )?;
                    Ok((candidate, cursor))
                },
            );
            if let Ok((candidate, cursor)) = migrated {
                checkpoint = mft_journal::MftCheckpointV2::new(
                    checkpoint.volume,
                    cursor.journal_id,
                    cursor.next_usn,
                    cursor.generation,
                );
                next_usn = cursor.next_usn;
                store = Some(candidate);
                let audit_root = cache.parent().unwrap_or(&cache).join("MftMaintenanceAudit");
                startup_store = if mft_migration::cleanup_legacy_after_promotion_linearized(
                    &cache,
                    &audit_root,
                    letter,
                    &LIFECYCLE_BARRIER,
                    || {
                        LIFECYCLE_BARRIER.is_open()
                            && !STOPPED.load(Ordering::Acquire)
                            && focus_leases
                                .lock()
                                .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                    },
                )
                .is_ok()
                {
                    StartupStoreV1::ExistingCanonical
                } else {
                    StartupStoreV1::ExistingCanonicalCleanupPending
                };
                schedule.record_success(monotonic_record_time());
            } else if sqlite_path.is_file() {
                // Promotion is the linearization point. A later writer reopen,
                // post-verification, or cleanup failure can return an error
                // even though a complete canonical main file is already
                // durable. Probe and adopt that exact state instead of
                // retrying legacy migration forever against an existing file.
                let limits = live_budgets.lock().ok().map(|budgets| budgets.limits);
                if let Some(limits) = limits
                    && persisted_cache_bytes(&cache)
                        <= u64::from(limits.persisted_index_mb) * 1024 * 1024
                    && let Ok((identity, index, budget_complete)) =
                        mft_sqlite::MftSqliteStoreV1::load_read_only_bounded(
                            &sqlite_path,
                            &cache,
                            checkpoint.volume,
                            checkpoint.journal_id,
                            usize::from(limits.volume_index_mb) * 1024 * 1024,
                            usize::from(limits.file_data_mb) * 1024 * 1024,
                        )
                    && budget_complete
                    && update_runtime_under_live_budget(
                        &live_budgets,
                        &live_volumes,
                        letter,
                        move |runtime| {
                            runtime.replace_with_exact(index, identity.cursor);
                            Ok(())
                        },
                    )
                    .is_ok_and(|(exact, _)| exact)
                {
                    checkpoint = mft_journal::MftCheckpointV2::new(
                        checkpoint.volume,
                        identity.cursor.journal_id,
                        identity.cursor.next_usn,
                        identity.cursor.generation,
                    );
                    next_usn = identity.cursor.next_usn;
                    startup_store = StartupStoreV1::ExistingCanonicalCleanupPending;
                    schedule.record_success(monotonic_record_time());
                }
            }
            continue;
        }
        if startup_store == StartupStoreV1::ExistingCanonical
            && schedule.decision(now, has_pending, focused)
                == mft_persistence::PersistenceDecisionV1::BeginAttempt
        {
            let mut attempt_started = false;
            if store.is_none() {
                store = (|| {
                    let _persisted_guard = persisted_write_guard()?;
                    let persisted_reservation = reserve_persisted_commit(
                        &live_budgets,
                        &cache,
                        SQLITE_WRITER_OPEN_RESERVATION_BYTES,
                    )?;
                    begin_persistence_attempt(&mut schedule, &focus_leases)?;
                    attempt_started = true;
                    let opened = LIFECYCLE_BARRIER.invoke(|| {
                        if !focus_leases
                            .lock()
                            .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                        {
                            return Err("MFT focus lease expired before writer open".to_owned());
                        }
                        mft_sqlite::MftSqliteStoreV1::open(
                            &sqlite_path,
                            &cache,
                            checkpoint.volume,
                            checkpoint.journal_id,
                        )
                    });
                    persisted_reservation.finish(&cache);
                    opened
                })()
                .ok();
            }
            if store.is_none() {
                continue;
            }
            if store
                .as_mut()
                .is_some_and(|candidate| candidate.wal_checkpoint_eligible(focused, false))
            {
                let checkpoint_guard = query_checkpoint_gate.write();
                let Ok(_checkpoint_guard) = checkpoint_guard else {
                    continue;
                };
                let Ok(_persisted_guard) = persisted_write_guard() else {
                    continue;
                };
                if !attempt_started
                    && begin_persistence_attempt(&mut schedule, &focus_leases).is_err()
                {
                    continue;
                }
                let _ = store
                    .as_mut()
                    .expect("writer admitted above")
                    .truncate_wal_ready_linearized(
                        &LIFECYCLE_BARRIER,
                        || {
                            focus_leases
                                .lock()
                                .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                        },
                        || query_activity.load(Ordering::Acquire) != 0,
                    );
                // A maintenance attempt consumes this interval. Pending memory
                // remains intact for the next focused opportunity.
                continue;
            }
            let maximum_commit_growth = mft_sqlite::maximum_wal_batch_growth_bytes(
                mft_journal::PENDING_BYTE_LIMIT
                    .try_into()
                    .unwrap_or(u64::MAX),
            );
            let Ok(persisted_guard) = persisted_write_guard() else {
                continue;
            };
            let Ok(persisted_reservation) =
                reserve_persisted_commit(&live_budgets, &cache, maximum_commit_growth)
            else {
                // Other volume workers reserve from the same global counter,
                // so two commits cannot independently consume the same slack.
                continue;
            };
            let batch = live_volumes
                .lock()
                .map_err(|_| ())
                .and_then(|mut live| {
                    live.get_mut(&letter)
                        .ok_or(())
                        .and_then(|v| v.capture().map_err(|_| ()))
                })
                .ok();
            if let Some(batch) = batch {
                if !attempt_started
                    && begin_persistence_attempt(&mut schedule, &focus_leases).is_err()
                {
                    if let Ok(mut live) = live_volumes.lock()
                        && let Some(runtime) = live.get_mut(&letter)
                    {
                        runtime.commit_failed(batch);
                    }
                    continue;
                }
                let next = batch.observed;
                let result = store
                    .as_mut()
                    .expect("writer admitted above")
                    .commit_changes_focused_linearized(
                        &batch.changes,
                        next,
                        &LIFECYCLE_BARRIER,
                        || {
                            focus_leases
                                .lock()
                                .is_ok_and(|mut leases| leases.any_focused(monotonic_now()))
                        },
                    );
                persisted_reservation.finish(&cache);
                drop(persisted_guard);
                if let Ok(mut live) = live_volumes.lock()
                    && let Some(runtime) = live.get_mut(&letter)
                {
                    if result.is_ok() {
                        runtime.commit_succeeded(&batch);
                        schedule.record_success(monotonic_record_time());
                    } else {
                        runtime.commit_failed(batch);
                    }
                }
            }
        }
    }
    schedule.inhibit_for_stop();
}

fn watch_volume(
    cache: PathBuf,
    letter: char,
    root: PathBuf,
    mut checkpoint: mft_journal::MftCheckpointV2,
) {
    let ignored_cache_parent = if root == std::path::Path::new(r"C:\") {
        mft_size_map::file_reference_number(&cache).ok()
    } else {
        None
    };
    let mut pending = HashMap::<u64, mft_journal::UsnEventV2>::new();
    let mut pending_bytes = 0_usize;
    let mut first_pending = None::<Instant>;
    let mut last_pending = None::<Instant>;
    let mut next_usn = checkpoint.next_usn;
    let mut high_water = 0_usize;
    while !STOPPED.load(Ordering::Acquire) {
        let mut read_checkpoint = checkpoint;
        read_checkpoint.next_usn = next_usn;
        let read = mft_journal::read_journal_once(&root, read_checkpoint);
        let (read_next_usn, events) = match read {
            Ok(result) => result,
            Err(error) => {
                let recover = mft_journal::query_journal(&root)
                    .map(|journal| !checkpoint.compatible_with(checkpoint.volume, journal))
                    .unwrap_or(true);
                if recover {
                    match prepare_volume(&cache, letter, &root, true) {
                        Ok(rebuilt) => {
                            checkpoint = rebuilt;
                            next_usn = rebuilt.next_usn;
                            pending.clear();
                            pending_bytes = 0;
                            first_pending = None;
                            last_pending = None;
                            continue;
                        }
                        Err(rebuild_error) => {
                            let _ = write_status(
                                &cache,
                                letter,
                                mft_journal::MftServiceModeV2::Error,
                                checkpoint,
                                pending.len(),
                                pending_bytes,
                                &format!("journal-recovery-failed:{rebuild_error}"),
                            );
                        }
                    }
                } else {
                    let _ = write_status(
                        &cache,
                        letter,
                        mft_journal::MftServiceModeV2::Error,
                        checkpoint,
                        pending.len(),
                        pending_bytes,
                        &format!("journal-read:{error}"),
                    );
                }
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
        };
        next_usn = next_usn.max(read_next_usn);
        for event in events {
            if ignored_cache_parent.is_some_and(|reference| {
                event.reference == reference || event.parent_reference == reference
            }) {
                continue;
            }
            let observed = Instant::now();
            if first_pending.is_none() {
                first_pending = Some(observed);
            }
            last_pending = Some(observed);
            pending
                .entry(event.reference)
                .and_modify(|current| {
                    current.reason |= event.reason;
                    current.parent_reference = event.parent_reference;
                    current.usn = current.usn.max(event.usn);
                    current.attributes = event.attributes;
                    if !event.name.is_empty() {
                        current.name.clone_from(&event.name);
                    }
                })
                .or_insert(event);
        }
        // Bound the coalesced queue's resident representation, not the cumulative
        // number of journal observations for the same hot file reference.
        pending_bytes = mft_journal::coalesced_bytes(&pending);
        high_water = high_water.max(pending.len());
        if pending.len() > mft_journal::PENDING_CHANGE_LIMIT
            || pending_bytes > mft_journal::PENDING_BYTE_LIMIT
        {
            let _ = write_status(
                &cache,
                letter,
                mft_journal::MftServiceModeV2::Recovering,
                checkpoint,
                pending.len(),
                pending_bytes,
                "pending-overflow",
            );
            match prepare_volume(&cache, letter, &root, true) {
                Ok(rebuilt) => checkpoint = rebuilt,
                Err(_) => break,
            }
            next_usn = checkpoint.next_usn;
            pending.clear();
            pending_bytes = 0;
            first_pending = None;
            last_pending = None;
            continue;
        }
        let publication_due = first_pending
            .zip(last_pending)
            .is_some_and(|(first, last)| {
                mft_journal::publication_due(first.elapsed(), last.elapsed())
            });
        if publication_due {
            let ambiguous = pending.values().any(|event| {
                mft_journal::normalize_event(event) == mft_journal::MftChangeKindV2::Invalidate
            });
            let published = if ambiguous {
                Err("ambiguous-link-or-reparse-change".to_owned())
            } else {
                publish_pending(&cache, letter, &root, &mut checkpoint, next_usn, &pending)
            };
            match published {
                Ok(()) => {
                    pending.clear();
                    pending_bytes = 0;
                    first_pending = None;
                    last_pending = None;
                    let _ = write_status_with_high_water(
                        &cache,
                        letter,
                        mft_journal::MftServiceModeV2::Journal,
                        checkpoint,
                        0,
                        0,
                        high_water,
                        "",
                    );
                }
                Err(error) => {
                    let _ = write_status(
                        &cache,
                        letter,
                        mft_journal::MftServiceModeV2::Recovering,
                        checkpoint,
                        pending.len(),
                        pending_bytes,
                        &error,
                    );
                    match prepare_volume(&cache, letter, &root, true) {
                        Ok(rebuilt) => checkpoint = rebuilt,
                        Err(rebuild_error) => {
                            let _ = write_status(
                                &cache,
                                letter,
                                mft_journal::MftServiceModeV2::Error,
                                checkpoint,
                                pending.len(),
                                pending_bytes,
                                &format!("delta-recovery-failed:{rebuild_error}"),
                            );
                            break;
                        }
                    }
                    next_usn = checkpoint.next_usn;
                    pending.clear();
                    pending_bytes = 0;
                    first_pending = None;
                    last_pending = None;
                }
            }
        }
    }
}

fn publish_pending(
    cache: &std::path::Path,
    letter: char,
    root: &std::path::Path,
    checkpoint: &mut mft_journal::MftCheckpointV2,
    next_usn: i64,
    pending: &HashMap<u64, mft_journal::UsnEventV2>,
) -> Result<(), String> {
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
    let mut changes = Vec::with_capacity(pending.len());
    for event in pending.values() {
        let kind = mft_journal::normalize_event(event);
        let mut change = mft_journal::MftChangeV2 {
            kind,
            reference: event.reference,
            parent_reference: event.parent_reference,
            name: event.name.clone(),
            logical_bytes: 0,
            allocated_bytes: 0,
            is_directory: event.attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
            reason: event.reason,
        };
        if kind == mft_journal::MftChangeKindV2::Upsert {
            match mft_size_map::current_entry(
                root,
                event.reference,
                event.parent_reference,
                event.name.clone(),
                change.is_directory,
            ) {
                Ok(entry) => {
                    change.logical_bytes = entry.logical_bytes;
                    change.allocated_bytes = entry.allocated_bytes;
                }
                Err(_) => change.kind = mft_journal::MftChangeKindV2::Delete,
            }
        }
        changes.push(change);
    }
    let generation = checkpoint.generation.saturating_add(1);
    let delta = mft_journal::MftDeltaV2 {
        schema: 2,
        volume: checkpoint.volume,
        journal_id: checkpoint.journal_id,
        generation,
        start_usn: checkpoint.next_usn,
        next_usn,
        changes,
    };
    let next = mft_journal::MftCheckpointV2::new(
        checkpoint.volume,
        checkpoint.journal_id,
        next_usn,
        generation,
    );
    mft_journal::publish_delta_and_checkpoint(cache, letter, &delta, &next)?;
    *checkpoint = next;
    Ok(())
}

fn write_status(
    cache: &std::path::Path,
    letter: char,
    mode: mft_journal::MftServiceModeV2,
    checkpoint: mft_journal::MftCheckpointV2,
    pending_count: usize,
    pending_bytes: usize,
    reason: &str,
) -> Result<(), String> {
    write_status_with_high_water(
        cache,
        letter,
        mode,
        checkpoint,
        pending_count,
        pending_bytes,
        pending_count,
        reason,
    )
}

fn write_status_with_high_water(
    cache: &std::path::Path,
    letter: char,
    mode: mft_journal::MftServiceModeV2,
    checkpoint: mft_journal::MftCheckpointV2,
    pending_count: usize,
    pending_bytes: usize,
    high_water: usize,
    reason: &str,
) -> Result<(), String> {
    let published_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    mft_journal::write_status(
        cache,
        letter,
        &mft_journal::MftServiceStatusV2 {
            mode,
            generation: checkpoint.generation,
            journal_id: checkpoint.journal_id,
            committed_usn: checkpoint.next_usn,
            pending_count: pending_count as u64,
            pending_bytes: pending_bytes as u64,
            queue_high_water: high_water as u64,
            published_unix_ms,
            reason: reason.to_owned(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn metadata_query_renews_short_service_local_demand_lease() {
        let leases = Arc::new(Mutex::new(mft_persistence::FocusLeaseRegistryV1::default()));
        renew_query_demand_lease(&leases);
        let now = monotonic_now();
        let mut leases = leases.lock().unwrap();
        assert_eq!(leases.active_count(now), 1);
        assert!(leases.expiry_remaining(now) <= QUERY_DEMAND_LEASE_TTL_V1);
        assert!(leases.expiry_remaining(now) > Duration::ZERO);
    }

    #[test]
    fn durable_sqlite_fallback_rejects_a_newer_live_journal() {
        let durable = mft_persistence::JournalCursorV1 {
            journal_id: 9,
            next_usn: 100,
            generation: 4,
        };
        let current = mft_journal::JournalMetadataV2 {
            journal_id: 9,
            first_usn: 1,
            next_usn: 101,
            lowest_valid_usn: 1,
        };

        assert!(!journal_metadata_matches_durable(current, durable));
        assert!(journal_metadata_matches_durable(
            mft_journal::JournalMetadataV2 {
                next_usn: 100,
                ..current
            },
            durable,
        ));
    }

    #[test]
    fn legacy_event_plus_one_cursor_replays_the_actual_event_boundary() {
        assert_eq!(normalize_journal_replay_cursor(0x16e8eb191), 0x16e8eb190);
        assert_eq!(normalize_journal_replay_cursor(0x16e8eb198), 0x16e8eb198);
    }

    fn set_limits_eventually(
        cache: &mut ServiceFolderAggregateCacheV1,
        root: &std::path::Path,
        live: &Arc<Mutex<HashMap<char, mft_runtime::VolumeMemoryRuntimeV1>>>,
        budgets: &Arc<Mutex<LiveBudgetStateV1>>,
        limits: mft_query::MftCacheBudgetLimitsV1,
    ) -> mft_query::MftCacheBudgetLimitsV1 {
        for _ in 0..1_000 {
            match cache.set_limits(root, live, budgets, limits) {
                Ok(effective) => return effective,
                Err(error) if error.contains("persisted maintenance finishes") => {
                    std::thread::yield_now();
                }
                Err(error) => panic!("failed to set cache limits: {error}"),
            }
        }
        panic!("timed out waiting for the persisted maintenance test gate")
    }

    fn temporary_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("superexplorer-{name}-{unique}"));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn interactive_memory_subtree_has_time_and_entry_bounds() {
        STOPPED.store(false, Ordering::Release);
        assert!(query_fallback_scan_exhausted(
            Instant::now(),
            QUERY_FALLBACK_MAX_ENTRIES
        ));
        assert!(query_fallback_scan_exhausted(
            Instant::now() + Duration::from_secs(30),
            QUERY_FALLBACK_MAX_ENTRIES + 1
        ));
        assert!(!query_fallback_scan_exhausted(
            Instant::now() + Duration::from_secs(30),
            QUERY_FALLBACK_MAX_ENTRIES
        ));
    }

    fn fixture(entries: u64) -> mft_size_map::MftIndexV1 {
        let mut values = BTreeMap::new();
        values.insert(
            1,
            mft_size_map::MftEntryV1 {
                reference: 1,
                parent_reference: 1,
                name: "root".to_owned(),
                logical_bytes: 0,
                allocated_bytes: 0,
                is_directory: true,
            },
        );
        for reference in 2..=entries {
            values.insert(
                reference,
                mft_size_map::MftEntryV1 {
                    reference,
                    parent_reference: 1,
                    name: format!("persisted-record-{reference:08}-payload"),
                    logical_bytes: reference,
                    allocated_bytes: reference,
                    is_directory: false,
                },
            );
        }
        mft_size_map::MftIndexV1::from_entries(values)
    }

    fn oversized_file_data_fixture() -> mft_size_map::MftIndexV1 {
        large_name_fixture(1_050)
    }

    fn large_name_fixture(entries: u64) -> mft_size_map::MftIndexV1 {
        let mut values = BTreeMap::new();
        values.insert(
            1,
            mft_size_map::MftEntryV1 {
                reference: 1,
                parent_reference: 1,
                name: "root".to_owned(),
                logical_bytes: 0,
                allocated_bytes: 0,
                is_directory: true,
            },
        );
        // The normalized hard minimum is 64 MiB. A modest number of large
        // names keeps this regression deterministic without millions of MFT
        // records.
        for reference in 2..=entries {
            values.insert(
                reference,
                mft_size_map::MftEntryV1 {
                    reference,
                    parent_reference: 1,
                    name: "x".repeat(65_000),
                    logical_bytes: 1,
                    allocated_bytes: 4_096,
                    is_directory: false,
                },
            );
        }
        mft_size_map::MftIndexV1::from_entries(values)
    }

    #[test]
    fn live_usn_growth_is_trimmed_before_queries_can_observe_exact_over_budget_state() {
        let cursor = mft_persistence::JournalCursorV1 {
            journal_id: 2,
            next_usn: 3,
            generation: 4,
        };
        let live = Arc::new(Mutex::new(HashMap::from([(
            'C',
            mft_runtime::VolumeMemoryRuntimeV1::new(large_name_fixture(1_033), cursor),
        )])));
        let budgets = Arc::new(Mutex::new(LiveBudgetStateV1 {
            limits: mft_query::MftCacheBudgetLimitsV1 {
                file_data_mb: 64,
                ..default_cache_budget_limits()
            },
            ..LiveBudgetStateV1::default()
        }));
        assert!(live.lock().unwrap()[&'C'].is_exact());
        let change = mft_journal::MftChangeV2 {
            kind: mft_journal::MftChangeKindV2::Upsert,
            reference: 2_000,
            parent_reference: 1,
            name: "y".repeat(65_000),
            logical_bytes: 1,
            allocated_bytes: 4_096,
            reason: 1,
            is_directory: false,
        };
        let limited = observe_batch_under_live_budget(
            &budgets,
            &live,
            'C',
            vec![(change, cursor.next_usn + 1)],
        )
        .unwrap();
        assert_eq!(limited, Some(0));
        assert!(!live.lock().unwrap()[&'C'].is_exact());
        assert!(budgets.lock().unwrap().blocked_volumes.contains(&'C'));
    }

    #[test]
    fn queried_volume_gets_first_claim_on_the_existing_live_budget() {
        let cursor = mft_persistence::JournalCursorV1 {
            journal_id: 2,
            next_usn: 3,
            generation: 4,
        };
        let live = Arc::new(Mutex::new(HashMap::from([
            (
                'C',
                mft_runtime::VolumeMemoryRuntimeV1::new(large_name_fixture(600), cursor),
            ),
            (
                'D',
                mft_runtime::VolumeMemoryRuntimeV1::new(large_name_fixture(600), cursor),
            ),
        ])));
        let budgets = Arc::new(Mutex::new(LiveBudgetStateV1 {
            limits: mft_query::MftCacheBudgetLimitsV1 {
                file_data_mb: 64,
                ..default_cache_budget_limits()
            },
            ..LiveBudgetStateV1::default()
        }));

        prefer_live_volume(&budgets, &live, 'D').unwrap();

        let live = live.lock().unwrap();
        assert!(live[&'D'].is_exact());
        assert!(!live[&'C'].is_exact());
        drop(live);
        let budgets = budgets.lock().unwrap();
        assert_eq!(budgets.preferred_volume, Some('D'));
        assert!(budgets.blocked_volumes.contains(&'C'));
        assert!(!budgets.blocked_volumes.contains(&'D'));
    }

    #[test]
    fn live_budget_survives_rebuild_and_raise_allows_exact_repopulation() {
        let root = temporary_root("live-budget-rebuild");
        let cursor = mft_persistence::JournalCursorV1 {
            journal_id: 2,
            next_usn: 3,
            generation: 4,
        };
        let live = Arc::new(Mutex::new(HashMap::from([(
            'C',
            mft_runtime::VolumeMemoryRuntimeV1::new(oversized_file_data_fixture(), cursor),
        )])));
        let budgets = Arc::new(Mutex::new(LiveBudgetStateV1::default()));
        let mut cache = ServiceFolderAggregateCacheV1::default();
        let low = mft_query::MftCacheBudgetLimitsV1 {
            file_data_mb: 64,
            ..default_cache_budget_limits()
        };
        set_limits_eventually(&mut cache, &root, &live, &budgets, low);
        assert!(!live.lock().unwrap()[&'C'].is_exact());
        assert!(budgets.lock().unwrap().blocked_volumes.contains(&'C'));

        let (exact, blocked_epoch) =
            update_runtime_under_live_budget(&budgets, &live, 'C', move |runtime| {
                runtime.replace_with_exact(oversized_file_data_fixture(), cursor);
                Ok(())
            })
            .unwrap();
        assert!(!exact, "an oversized rebuilt index must remain partial");
        assert_eq!(blocked_epoch, budgets.lock().unwrap().epoch);
        assert!(budgets.lock().unwrap().blocked_volumes.contains(&'C'));

        let high = mft_query::MftCacheBudgetLimitsV1 {
            file_data_mb: 128,
            ..default_cache_budget_limits()
        };
        set_limits_eventually(&mut cache, &root, &live, &budgets, high);
        assert!(!budgets.lock().unwrap().blocked_volumes.contains(&'C'));
        let (exact, _) = update_runtime_under_live_budget(&budgets, &live, 'C', move |runtime| {
            runtime.replace_with_exact(oversized_file_data_fixture(), cursor);
            Ok(())
        })
        .unwrap();
        assert!(exact, "a raised budget permits exact repopulation");
        assert!(live.lock().unwrap()[&'C'].is_exact());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn persisted_limit_is_acknowledged_without_dynamic_floor_and_raise_releases_retry() {
        let root = temporary_root("persisted-hard-limit");
        let file = std::fs::File::create(root.join("C.mft.sqlite3")).unwrap();
        file.set_len(300 * 1024 * 1024).unwrap();
        drop(file);
        let live = Arc::new(Mutex::new(HashMap::new()));
        let budgets = Arc::new(Mutex::new(LiveBudgetStateV1::default()));
        budgets.lock().unwrap().blocked_volumes.insert('C');
        let mut cache = ServiceFolderAggregateCacheV1::default();

        let low = mft_query::MftCacheBudgetLimitsV1 {
            persisted_index_mb: 256,
            ..default_cache_budget_limits()
        };
        assert!(
            persisted_candidate_allowance(
                &root,
                &root.join("C.mft.sqlite3"),
                256 * 1024 * 1024,
                true,
            ) > 0,
            "an over-limit replaced store must retain a nonzero final allowance"
        );
        let effective = set_limits_eventually(&mut cache, &root, &live, &budgets, low);
        assert_eq!(effective.persisted_index_mb, 256);
        assert!(budgets.lock().unwrap().persisted_prune_pending);

        let high = mft_query::MftCacheBudgetLimitsV1 {
            persisted_index_mb: 512,
            ..default_cache_budget_limits()
        };
        let effective = set_limits_eventually(&mut cache, &root, &live, &budgets, high);
        assert_eq!(effective.persisted_index_mb, 512);
        let state = budgets.lock().unwrap();
        assert!(!state.persisted_prune_pending);
        assert!(!state.blocked_volumes.contains(&'C'));
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn persisted_budget_change_returns_pending_during_atomic_mutation() {
        let root = temporary_root("persisted-budget-linearization");
        let live = Arc::new(Mutex::new(HashMap::new()));
        let budgets = Arc::new(Mutex::new(LiveBudgetStateV1::default()));
        let mut cache = ServiceFolderAggregateCacheV1::default();
        let mutation = persisted_write_guard().unwrap();
        let result = cache.set_limits(
            &root,
            &live,
            &budgets,
            mft_query::MftCacheBudgetLimitsV1 {
                persisted_index_mb: 256,
                ..default_cache_budget_limits()
            },
        );
        assert!(result.is_err());
        assert_eq!(
            budgets.lock().unwrap().limits,
            default_cache_budget_limits()
        );
        drop(mutation);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn replacement_recovery_preflight_accounts_for_consumed_paths_and_companions() {
        let root = temporary_root("replacement-recovery-budget");
        let canonical = root.join("C.mft.sqlite3");
        std::fs::File::create(root.join("other.mft.sqlite3"))
            .unwrap()
            .set_len(252 * 1024 * 1024)
            .unwrap();
        std::fs::File::create(&canonical)
            .unwrap()
            .set_len(1024)
            .unwrap();
        std::fs::File::create(mft_sqlite::MftSqliteStoreV1::replacement_backup_path(
            &canonical,
        ))
        .unwrap()
        .set_len(7 * 512 * 1024)
        .unwrap();
        assert!(persisted_cache_bytes(&root) <= 256 * 1024 * 1024);
        assert!(replacement_recovery_projected_bytes(&root, &canonical) > 256 * 1024 * 1024);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn persisted_commit_reservations_are_global_and_remeasured_on_finish() {
        let root = temporary_root("persisted-commit-reservation");
        let budgets = Arc::new(Mutex::new(LiveBudgetStateV1 {
            limits: mft_query::MftCacheBudgetLimitsV1 {
                persisted_index_mb: 256,
                ..default_cache_budget_limits()
            },
            ..LiveBudgetStateV1::default()
        }));
        let first = reserve_persisted_commit(&budgets, &root, 200 * 1024 * 1024).unwrap();
        assert!(reserve_persisted_commit(&budgets, &root, 100 * 1024 * 1024).is_err());
        assert_eq!(
            budgets.lock().unwrap().reserved_persisted_bytes,
            200 * 1024 * 1024
        );
        first.finish(&root);
        let state = budgets.lock().unwrap();
        assert_eq!(state.reserved_persisted_bytes, 0);
        assert!(!state.persisted_prune_pending);
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn persisted_pruning_atomically_replaces_index_and_preserves_unrelated_files() {
        let root = temporary_root("persisted-prune");
        let destination = root.join("C.semftidx");
        let initial = root.join("C.initial.tmp");
        mft_size_map::write_service_index(&initial, &fixture(4_096)).unwrap();
        std::fs::rename(&initial, &destination).unwrap();
        let unrelated = root.join("keep.txt");
        std::fs::write(&unrelated, b"keep").unwrap();
        std::fs::write(root.join("C.prune-interrupted.tmp"), b"incomplete").unwrap();

        enforce_persisted_limit(&root, 32 * 1024).unwrap();

        let metadata = std::fs::metadata(&destination).unwrap();
        assert!(metadata.len() <= 32 * 1024);
        assert!(persisted_incomplete_path(&root, 'C').is_file());
        assert_eq!(std::fs::read(&unrelated).unwrap(), b"keep");
        assert!(!root.join("C.prune-interrupted.tmp").exists());
        assert!(
            !mft_size_map::read_index(&destination)
                .unwrap()
                .entries
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn one_oversized_persisted_record_is_retained_and_marked_partial() {
        let root = temporary_root("persisted-oversized");
        let destination = root.join("D.semftidx");
        let initial = root.join("D.initial.tmp");
        mft_size_map::write_service_index(&initial, &fixture(2)).unwrap();
        std::fs::rename(&initial, &destination).unwrap();
        enforce_persisted_limit(&root, 1).unwrap();
        let recovered = mft_size_map::read_index(&destination).unwrap();
        assert_eq!(recovered.entries.len(), 1);
        assert!(persisted_incomplete_path(&root, 'D').is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn service_limits_are_normalized_and_raising_a_trimmed_structure_requires_repopulation() {
        let root = temporary_root("limit-transition");
        let index = fixture(32);
        let aggregates = mft_size_map::MftAggregateIndexV1::build(&index, 64).unwrap();
        let memory = index.memory_breakdown();
        let aggregate_bytes = aggregates.estimated_resident_bytes();
        let mut cache = ServiceFolderAggregateCacheV1::default();
        cache.volumes.insert(
            'C',
            CachedFolderAggregateVolumeV1 {
                index,
                aggregates,
                checkpoint: mft_journal::MftCheckpointV2::new(
                    mft_journal::VolumeIdentityV2 { serial: 1 },
                    2,
                    3,
                    4,
                ),
                estimated_bytes: memory
                    .volume_index_bytes
                    .saturating_add(memory.file_data_bytes)
                    .saturating_add(aggregate_bytes),
                volume_index_bytes: memory.volume_index_bytes,
                file_data_bytes: memory.file_data_bytes,
                aggregate_bytes,
                last_use: 1,
                volume_index_incomplete: false,
                file_data_incomplete: false,
                aggregate_incomplete: true,
            },
        );
        cache.results.insert(
            ('C', 1),
            (
                mft_query::FolderAggregateQueryV1 {
                    generation: 4,
                    logical_bytes: 1,
                    partial: true,
                    ..Default::default()
                },
                1,
            ),
        );
        cache.recount_result_bytes();

        let effective = cache
            .set_limits(
                &root,
                &Arc::new(Mutex::new(HashMap::new())),
                &Arc::new(Mutex::new(LiveBudgetStateV1::default())),
                mft_query::MftCacheBudgetLimitsV1 {
                    persisted_index_mb: 1,
                    volume_index_mb: u16::MAX,
                    file_data_mb: 1,
                    aggregate_mb: 16_384,
                    lru_mb: 1,
                },
            )
            .unwrap();
        assert_eq!(effective.persisted_index_mb, 256);
        assert_eq!(effective.volume_index_mb, 16_384);
        assert_eq!(effective.file_data_mb, 64);
        assert_eq!(effective.lru_mb, 128);
        assert!(cache.volumes.is_empty());
        assert!(
            cache.results.is_empty(),
            "stale partial results must be discarded"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn result_lru_evicts_oldest_without_discarding_volume_indexes() {
        let mut cache = ServiceFolderAggregateCacheV1::default();
        for reference in 1..=3 {
            cache.results.insert(
                ('C', reference),
                (mft_query::FolderAggregateQueryV1::default(), reference),
            );
        }
        cache.recount_result_bytes();
        let one = std::mem::size_of::<((char, u64), (mft_query::FolderAggregateQueryV1, u64))>();
        cache.limit_bytes = one * 2;
        cache.evict_for(0);
        assert_eq!(cache.results.len(), 2);
        assert!(!cache.results.contains_key(&('C', 1)));
        assert!(cache.estimated_bytes <= cache.limit_bytes);
        assert!(cache.volumes.is_empty());
    }

    #[test]
    fn live_query_uses_observed_generation_and_returns_typed_partial_for_rebuild() {
        let cursor = mft_persistence::JournalCursorV1 {
            journal_id: 2,
            next_usn: 3,
            generation: 4,
        };
        let live = Arc::new(Mutex::new(HashMap::from([(
            'C',
            mft_runtime::VolumeMemoryRuntimeV1::new(fixture(4), cursor),
        )])));
        let budgets = Arc::new(Mutex::new(LiveBudgetStateV1::default()));
        let mut cache = ServiceFolderAggregateCacheV1::default();
        let exact = cache
            .query_live(
                &live,
                &budgets,
                std::path::Path::new("."),
                std::path::Path::new("Z:\\"),
                None,
                'C',
                1,
                128,
            )
            .unwrap();
        assert_eq!(exact.generation, 4);
        assert!(!exact.partial);
        assert!(cache.live_aggregates.is_empty());
        let second = cache
            .query_live(
                &live,
                &budgets,
                std::path::Path::new("."),
                std::path::Path::new("Z:\\"),
                None,
                'C',
                2,
                128,
            )
            .unwrap();
        assert!(!second.partial);
        assert!(cache.live_aggregates.is_empty());
        assert_eq!(cache.results.len(), 2);
        live.lock().unwrap().get_mut(&'C').unwrap().mark_inexact();
        let partial = cache
            .query_live(
                &live,
                &budgets,
                std::path::Path::new("."),
                std::path::Path::new("Z:\\"),
                None,
                'C',
                1,
                128,
            )
            .unwrap();
        assert_eq!(partial.generation, 4);
        assert!(partial.partial);
        assert_eq!(partial.logical_bytes, 9);
        assert_eq!(partial.file_count, 3);
        assert!(cache.live_aggregates.is_empty());
        assert_eq!(cache.limit_bytes, 128 * 1024 * 1024);
    }

    #[test]
    fn exact_large_folder_builds_one_memory_aggregate_instead_of_partial_zero() {
        STOPPED.store(false, Ordering::Release);
        let cursor = mft_persistence::JournalCursorV1 {
            journal_id: 2,
            next_usn: 3,
            generation: 4,
        };
        let entries = QUERY_FALLBACK_MAX_ENTRIES as u64 + 2;
        let live = Arc::new(Mutex::new(HashMap::from([(
            'C',
            mft_runtime::VolumeMemoryRuntimeV1::new(fixture(entries), cursor),
        )])));
        let budgets = Arc::new(Mutex::new(LiveBudgetStateV1::default()));
        let mut cache = ServiceFolderAggregateCacheV1::default();

        let root = cache
            .query_live(
                &live,
                &budgets,
                std::path::Path::new("."),
                std::path::Path::new("Z:\\"),
                None,
                'C',
                1,
                128,
            )
            .unwrap();

        assert!(!root.partial);
        assert_eq!(root.logical_bytes, entries * (entries + 1) / 2 - 1);
        assert_eq!(root.file_count, entries - 1);
        assert_eq!(root.directory_count, 1);
        assert!(cache.live_aggregates.contains_key(&'C'));

        let child = cache
            .query_live(
                &live,
                &budgets,
                std::path::Path::new("."),
                std::path::Path::new("Z:\\"),
                None,
                'C',
                entries,
                128,
            )
            .unwrap();
        assert!(!child.partial);
        assert_eq!(child.logical_bytes, entries);
        assert_eq!(child.file_count, 1);
    }

    #[test]
    #[ignore = "requires an elevated real NTFS volume and installed canonical cache"]
    fn real_canonical_store_fits_live_budgets_and_catches_up() {
        let root = std::path::PathBuf::from(
            std::env::var_os("SUPEREXPLORER_REAL_MFT_VOLUME")
                .unwrap_or_else(|| std::ffi::OsString::from(r"D:\")),
        );
        let letter = root
            .to_string_lossy()
            .chars()
            .next()
            .unwrap()
            .to_ascii_uppercase();
        let cache = std::path::PathBuf::from(
            std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into()),
        )
        .join("SuperExplorer")
        .join("MftIndex");
        let sqlite = cache.join(format!("{letter}.mft.sqlite3"));
        let journal = mft_journal::query_journal(&root).unwrap();
        let volume = mft_journal::VolumeIdentityV2 {
            serial: mft_size_map::volume_serial_number(&root).unwrap(),
        };
        let limits = default_cache_budget_limits();
        let (identity, index, complete) =
            mft_sqlite::MftSqliteStoreV1::load_read_only_bounded_cancelled(
                &sqlite,
                &cache,
                volume,
                journal.journal_id,
                usize::from(limits.volume_index_mb) * 1024 * 1024,
                usize::from(limits.file_data_mb) * 1024 * 1024,
                || false,
            )
            .unwrap();
        assert!(complete);
        let (index, observed, changes) = catch_up_memory_index(
            &root,
            index,
            identity.cursor,
            usize::from(limits.volume_index_mb) * 1024 * 1024,
            usize::from(limits.file_data_mb) * 1024 * 1024,
        )
        .unwrap();
        let memory = index.memory_breakdown();
        println!(
            "canonical reload: observed={observed:?} changes={} topology={} names={}",
            changes.len(),
            memory.volume_index_bytes,
            memory.file_data_bytes
        );
    }
}

fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    if let Some(index) = arguments
        .iter()
        .position(|argument| argument == "--query-hierarchy")
    {
        let Some(path) = arguments.get(index.saturating_add(1)) else {
            eprintln!("--query-hierarchy requires a local directory path");
            std::process::exit(2);
        };
        match mft_query::query_hierarchy(
            std::path::Path::new(path),
            explorer_model::DEFAULT_MFT_FOLDER_CACHE_MEMORY_MB,
        ) {
            Ok(nodes) => {
                println!("nodes={}", nodes.len());
                for node in nodes.iter().take(16) {
                    println!(
                        "reference={} parent={:?} directory={} logical_bytes={} allocated_bytes={} name={}",
                        node.reference,
                        node.parent_reference,
                        node.is_directory,
                        node.logical_bytes,
                        node.allocated_bytes,
                        node.name,
                    );
                }
            }
            Err(error) => {
                eprintln!("MFT hierarchy query failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(index) = arguments
        .iter()
        .position(|argument| argument == "--query-folder")
    {
        let Some(path) = arguments.get(index.saturating_add(1)) else {
            eprintln!("--query-folder requires a local directory path");
            std::process::exit(2);
        };
        match mft_query::query_folder(
            std::path::Path::new(path),
            explorer_model::DEFAULT_MFT_FOLDER_CACHE_MEMORY_MB,
        ) {
            Ok(aggregate) => println!(
                "generation={} logical_bytes={} allocated_bytes={} files={} directories={} partial={}",
                aggregate.generation,
                aggregate.logical_bytes,
                aggregate.allocated_bytes,
                aggregate.file_count,
                aggregate.directory_count,
                aggregate.partial,
            ),
            Err(error) => {
                eprintln!("MFT folder query failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if arguments.iter().any(|argument| argument == "--diagnostics") {
        match mft_query::query_diagnostics() {
            Ok(diagnostics) => {
                println!(
                    "generation={} cache_bytes={} cache_limit_bytes={} entries={} persisted_index_bytes={} hits={} misses={} volume_index_bytes={} file_data_bytes={} aggregate_bytes={} persisted_limit_bytes={} volume_index_limit_bytes={} file_data_limit_bytes={} aggregate_limit_bytes={}",
                    diagnostics.generation,
                    diagnostics.lru_bytes,
                    diagnostics.limit_bytes,
                    diagnostics.entry_count,
                    diagnostics.persisted_index_bytes,
                    diagnostics.hits,
                    diagnostics.misses,
                    diagnostics.volume_index_bytes.unwrap_or_default(),
                    diagnostics.file_data_bytes.unwrap_or_default(),
                    diagnostics.aggregate_bytes.unwrap_or_default(),
                    diagnostics.persisted_index_limit_bytes.unwrap_or_default(),
                    diagnostics.volume_index_limit_bytes.unwrap_or_default(),
                    diagnostics.file_data_limit_bytes.unwrap_or_default(),
                    diagnostics.aggregate_limit_bytes.unwrap_or_default(),
                );
                match mft_query::query_durability_diagnostics() {
                    Ok(volumes) => {
                        for volume in volumes {
                            println!(
                                "volume={} mode={} schema={} exact={} observed={}:{}:{} durable={}:{}:{} pending_count={} pending_bytes={} last_success_ms={} focus_leases={} focus_expiry_ms={} main_bytes={} wal_bytes={} tx_attempts={} tx_failures={} tx_outcome={} checkpoint_attempts={} checkpoint_failures={} checkpoint_outcome={} migration={} recovery={}",
                                char::from(volume.volume),
                                volume.mode,
                                volume.schema,
                                volume.exact,
                                volume.observed_journal_id,
                                volume.observed_next_usn,
                                volume.observed_generation,
                                volume.durable_journal_id,
                                volume.durable_next_usn,
                                volume.durable_generation,
                                volume.pending_count,
                                volume.pending_bytes,
                                volume.last_successful_commit_ms,
                                volume.focus_lease_count,
                                volume.focus_expiry_remaining_ms,
                                volume.main_bytes,
                                volume.wal_bytes,
                                volume.transaction_attempts,
                                volume.transaction_failures,
                                volume.transaction_last_outcome,
                                volume.checkpoint_attempts,
                                volume.checkpoint_failures,
                                volume.checkpoint_last_outcome,
                                volume.migration_state,
                                volume.recovery_reason,
                            );
                        }
                    }
                    Err(error) => {
                        eprintln!("MFT durability diagnostics query failed: {error}");
                        std::process::exit(1);
                    }
                }
            }
            Err(error) => {
                eprintln!("MFT diagnostics query failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if arguments.iter().any(|argument| argument == "--serve") {
        // Console serve mode for headful tests outside SCM. The event-driven
        // service loop owns the MFT query pipe and blocks until the process
        // is terminated.
        run_event_driven_service();
        return;
    }
    let mut name = wide(SERVICE_NAME);
    let table = [
        ServiceTableEntryW {
            name: name.as_mut_ptr(),
            main: Some(service_main),
        },
        ServiceTableEntryW {
            name: std::ptr::null_mut(),
            main: None,
        },
    ];
    // SAFETY: table is terminated and remains alive until the dispatcher returns.
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        // Developer smoke mode: initialize/reuse each volume once when launched outside SCM.
        let cache = cache_root();
        if std::fs::create_dir_all(&cache).is_ok() {
            for letter in b'A'..=b'Z' {
                let root = PathBuf::from(format!("{}:\\", char::from(letter)));
                if root.exists() {
                    let _ = prepare_volume(&cache, char::from(letter), &root, false);
                }
            }
        }
    }
}
