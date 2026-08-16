//! Bounded background BC7 conversion and persistence runtime.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    time::{Duration, Instant},
};

use explorer_model::{CancellationToken, ShellIconKey, ShellIconPayload};

use crate::{
    bc7_codec::{self, Bc7ContentKind},
    icon_disk_cache::ShellIconDiskCache,
};

const FORMAT_SCHEMA: u16 = 1;
const MAX_QUEUED_JOBS: usize = 32;
const MAX_CONCURRENT_JOBS: usize = 2;
const MAX_JOB_AGE: Duration = Duration::from_secs(10);

static QUEUED_JOBS: AtomicU64 = AtomicU64::new(0);
static PEAK_QUEUED_JOBS: AtomicU64 = AtomicU64::new(0);
static ACTIVE_JOBS: AtomicU64 = AtomicU64::new(0);
static PEAK_ACTIVE_JOBS: AtomicU64 = AtomicU64::new(0);
static RESERVED_STAGING_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_RESERVED_STAGING_BYTES: AtomicU64 = AtomicU64::new(0);
static SUBMITTED_JOBS: AtomicU64 = AtomicU64::new(0);
static COMPLETED_JOBS: AtomicU64 = AtomicU64::new(0);
static DUPLICATE_JOBS: AtomicU64 = AtomicU64::new(0);
static OVERLOAD_REJECTIONS: AtomicU64 = AtomicU64::new(0);
static OVERSIZED_REJECTIONS: AtomicU64 = AtomicU64::new(0);
static CANCELLED_JOBS: AtomicU64 = AtomicU64::new(0);
static STALE_JOBS: AtomicU64 = AtomicU64::new(0);
static PERSIST_ERRORS: AtomicU64 = AtomicU64::new(0);
static FALLBACKS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Bc7JobStatsV1 {
    pub queued_jobs: u64,
    pub queue_limit: u64,
    pub peak_queued_jobs: u64,
    pub active_jobs: u64,
    pub concurrency_limit: u64,
    pub peak_active_jobs: u64,
    pub reserved_staging_bytes: u64,
    pub staging_limit_bytes: u64,
    pub peak_reserved_staging_bytes: u64,
    pub submitted_jobs: u64,
    pub completed_jobs: u64,
    pub duplicate_jobs: u64,
    pub overload_rejections: u64,
    pub oversized_rejections: u64,
    pub cancelled_jobs: u64,
    pub stale_jobs: u64,
    pub persist_errors: u64,
    pub fallbacks: u64,
}

pub fn job_stats() -> Bc7JobStatsV1 {
    Bc7JobStatsV1 {
        queued_jobs: QUEUED_JOBS.load(Ordering::Acquire),
        queue_limit: MAX_QUEUED_JOBS as u64,
        peak_queued_jobs: PEAK_QUEUED_JOBS.load(Ordering::Acquire),
        active_jobs: ACTIVE_JOBS.load(Ordering::Acquire),
        concurrency_limit: MAX_CONCURRENT_JOBS as u64,
        peak_active_jobs: PEAK_ACTIVE_JOBS.load(Ordering::Acquire),
        reserved_staging_bytes: RESERVED_STAGING_BYTES.load(Ordering::Acquire),
        staging_limit_bytes: bc7_codec::MAX_BC7_STAGING_BYTES as u64,
        peak_reserved_staging_bytes: PEAK_RESERVED_STAGING_BYTES.load(Ordering::Acquire),
        submitted_jobs: SUBMITTED_JOBS.load(Ordering::Acquire),
        completed_jobs: COMPLETED_JOBS.load(Ordering::Acquire),
        duplicate_jobs: DUPLICATE_JOBS.load(Ordering::Acquire),
        overload_rejections: OVERLOAD_REJECTIONS.load(Ordering::Acquire),
        oversized_rejections: OVERSIZED_REJECTIONS.load(Ordering::Acquire),
        cancelled_jobs: CANCELLED_JOBS.load(Ordering::Acquire),
        stale_jobs: STALE_JOBS.load(Ordering::Acquire),
        persist_errors: PERSIST_ERRORS.load(Ordering::Acquire),
        fallbacks: FALLBACKS.load(Ordering::Acquire),
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ConversionKey {
    kind: Bc7ContentKind,
    source: ShellIconKey,
    width: u16,
    height: u16,
    schema: u16,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SupersessionKey {
    kind: Bc7ContentKind,
    item_id: Option<explorer_model::ShellItemId>,
    location: explorer_model::LocationDescriptor,
    size_bucket: u16,
    dpi: u16,
    theme: explorer_model::ShellIconTheme,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GenerationStamp {
    association: u64,
    overlay: u64,
}

impl ConversionKey {
    fn new(kind: Bc7ContentKind, payload: &ShellIconPayload) -> Self {
        Self {
            kind,
            source: payload.key.clone(),
            width: payload.width,
            height: payload.height,
            schema: FORMAT_SCHEMA,
        }
    }

    fn supersession_key(&self) -> SupersessionKey {
        SupersessionKey {
            kind: self.kind,
            item_id: self.source.item_id.clone(),
            location: self.source.location.clone(),
            size_bucket: self.source.size_bucket,
            dpi: self.source.dpi,
            theme: self.source.theme,
        }
    }

    const fn generation(&self) -> GenerationStamp {
        GenerationStamp {
            association: self.source.association_generation,
            overlay: self.source.overlay_generation,
        }
    }
}

#[derive(Default)]
struct Registry {
    in_flight: HashSet<ConversionKey>,
    latest: HashMap<SupersessionKey, GenerationStamp>,
}

struct Job {
    key: ConversionKey,
    payload: ShellIconPayload,
    disk: ShellIconDiskCache,
    cancellation: Option<CancellationToken>,
    deadline: Instant,
    reserved_staging_bytes: u64,
}

struct Runtime {
    sender: SyncSender<Job>,
    registry: Arc<Mutex<Registry>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScheduleOutcome {
    Submitted,
    Duplicate,
    RejectedOversized,
    RejectedOverload,
    Cancelled,
}

pub(crate) fn schedule(
    kind: Bc7ContentKind,
    payload: ShellIconPayload,
    disk: ShellIconDiskCache,
    cancellation: Option<CancellationToken>,
    deadline: Option<Instant>,
) -> ScheduleOutcome {
    if cancellation
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
    {
        CANCELLED_JOBS.fetch_add(1, Ordering::Relaxed);
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
        return ScheduleOutcome::Cancelled;
    }
    let Ok(reserved_staging_bytes) = estimated_staging_bytes(&payload) else {
        OVERSIZED_REJECTIONS.fetch_add(1, Ordering::Relaxed);
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
        return ScheduleOutcome::RejectedOversized;
    };
    let key = ConversionKey::new(kind, &payload);
    let runtime = runtime();
    {
        let mut registry = runtime
            .registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !registry.in_flight.insert(key.clone()) {
            DUPLICATE_JOBS.fetch_add(1, Ordering::Relaxed);
            return ScheduleOutcome::Duplicate;
        }
        registry
            .latest
            .insert(key.supersession_key(), key.generation());
    }
    if !reserve_staging(reserved_staging_bytes) {
        runtime
            .registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .in_flight
            .remove(&key);
        OVERLOAD_REJECTIONS.fetch_add(1, Ordering::Relaxed);
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
        return ScheduleOutcome::RejectedOverload;
    }
    let job = Job {
        key: key.clone(),
        payload,
        disk,
        cancellation,
        deadline: deadline.unwrap_or_else(|| Instant::now() + MAX_JOB_AGE),
        reserved_staging_bytes,
    };
    QUEUED_JOBS.fetch_add(1, Ordering::AcqRel);
    update_peak(&PEAK_QUEUED_JOBS, QUEUED_JOBS.load(Ordering::Acquire));
    match runtime.sender.try_send(job) {
        Ok(()) => {
            SUBMITTED_JOBS.fetch_add(1, Ordering::Relaxed);
            ScheduleOutcome::Submitted
        }
        Err(TrySendError::Full(job) | TrySendError::Disconnected(job)) => {
            QUEUED_JOBS.fetch_sub(1, Ordering::AcqRel);
            release_staging(job.reserved_staging_bytes);
            runtime
                .registry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .in_flight
                .remove(&key);
            OVERLOAD_REJECTIONS.fetch_add(1, Ordering::Relaxed);
            FALLBACKS.fetch_add(1, Ordering::Relaxed);
            ScheduleOutcome::RejectedOverload
        }
    }
}

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<Job>(MAX_QUEUED_JOBS);
        let receiver = Arc::new(Mutex::new(receiver));
        let registry = Arc::new(Mutex::new(Registry::default()));
        for worker in 0..MAX_CONCURRENT_JOBS {
            let receiver = Arc::clone(&receiver);
            let registry = Arc::clone(&registry);
            let _ = std::thread::Builder::new()
                .name(format!("bc7-converter-{worker}"))
                .spawn(move || worker_loop(&receiver, &registry));
        }
        Runtime { sender, registry }
    })
}

fn worker_loop(receiver: &Mutex<mpsc::Receiver<Job>>, registry: &Mutex<Registry>) {
    loop {
        let job = receiver
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv();
        let Ok(job) = job else { return };
        QUEUED_JOBS.fetch_sub(1, Ordering::AcqRel);
        ACTIVE_JOBS.fetch_add(1, Ordering::AcqRel);
        update_peak(&PEAK_ACTIVE_JOBS, ACTIVE_JOBS.load(Ordering::Acquire));
        run_job(&job, registry);
        ACTIVE_JOBS.fetch_sub(1, Ordering::AcqRel);
        release_staging(job.reserved_staging_bytes);
        registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .in_flight
            .remove(&job.key);
    }
}

fn run_job(job: &Job, registry: &Mutex<Registry>) {
    if job
        .cancellation
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
        || Instant::now() >= job.deadline
    {
        CANCELLED_JOBS.fetch_add(1, Ordering::Relaxed);
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
        return;
    }
    if !kind_enabled(job.key.kind) || !is_current(registry, &job.key) {
        STALE_JOBS.fetch_add(1, Ordering::Relaxed);
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let published = job.disk.store_if(&job.payload, || {
        job.cancellation
            .as_ref()
            .is_none_or(|token| !token.is_cancelled())
            && Instant::now() < job.deadline
            && kind_enabled(job.key.kind)
            && is_current(registry, &job.key)
    });
    if published {
        COMPLETED_JOBS.fetch_add(1, Ordering::Relaxed);
    } else if job
        .cancellation
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
        || Instant::now() >= job.deadline
    {
        CANCELLED_JOBS.fetch_add(1, Ordering::Relaxed);
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
    } else if !kind_enabled(job.key.kind) || !is_current(registry, &job.key) {
        STALE_JOBS.fetch_add(1, Ordering::Relaxed);
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
    } else {
        PERSIST_ERRORS.fetch_add(1, Ordering::Relaxed);
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
    }
}

fn kind_enabled(kind: Bc7ContentKind) -> bool {
    match kind {
        Bc7ContentKind::Icon => crate::icon_disk_cache::icon_bc7_enabled(),
        Bc7ContentKind::Thumbnail => crate::icon_disk_cache::thumbnail_bc7_enabled(),
    }
}

fn is_current(registry: &Mutex<Registry>, key: &ConversionKey) -> bool {
    registry
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .latest
        .get(&key.supersession_key())
        .is_some_and(|generation| *generation == key.generation())
}

fn estimated_staging_bytes(payload: &ShellIconPayload) -> Result<u64, ()> {
    let layout = bc7_codec::checked_layout(u32::from(payload.width), u32::from(payload.height))
        .map_err(|_| ())?;
    let source_bytes = usize::try_from(payload.stride)
        .ok()
        .and_then(|stride| stride.checked_mul(usize::from(payload.height)))
        .filter(|bytes| *bytes == payload.rgba.len())
        .ok_or(())?;
    let padded_rgba = usize::try_from(layout.padded_width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .and_then(|stride| stride.checked_mul(layout.padded_height as usize))
        .ok_or(())?;
    let total = source_bytes
        .checked_add(padded_rgba)
        .and_then(|bytes| bytes.checked_add(layout.payload_bytes))
        .ok_or(())?;
    if total > bc7_codec::MAX_BC7_STAGING_BYTES {
        return Err(());
    }
    u64::try_from(total).map_err(|_| ())
}

fn reserve_staging(bytes: u64) -> bool {
    let limit = bc7_codec::MAX_BC7_STAGING_BYTES as u64;
    let mut current = RESERVED_STAGING_BYTES.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(bytes).filter(|next| *next <= limit) else {
            return false;
        };
        match RESERVED_STAGING_BYTES.compare_exchange_weak(
            current,
            next,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                update_peak(&PEAK_RESERVED_STAGING_BYTES, next);
                return true;
            }
            Err(observed) => current = observed,
        }
    }
}

fn release_staging(bytes: u64) {
    RESERVED_STAGING_BYTES.fetch_sub(bytes, Ordering::AcqRel);
}

fn update_peak(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::Acquire);
    while value > current {
        match target.compare_exchange_weak(current, value, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use explorer_model::{LocationDescriptor, ShellIconTheme};

    use super::*;

    fn payload(generation: u64, width: u16, height: u16) -> ShellIconPayload {
        payload_at(r"C:\bc7-pipeline-fixture", generation, width, height)
    }

    fn payload_at(
        path: impl AsRef<Path>,
        generation: u64,
        width: u16,
        height: u16,
    ) -> ShellIconPayload {
        let stride = u32::from(width) * 4;
        ShellIconPayload::new(
            ShellIconKey {
                item_id: None,
                location: LocationDescriptor::file_system(path.as_ref()),
                size_bucket: width,
                dpi: 96,
                theme: ShellIconTheme::Light,
                association_generation: generation,
                overlay_generation: generation,
            },
            width,
            height,
            stride,
            vec![0x7f; stride as usize * usize::from(height)],
            None,
        )
        .expect("valid fixture")
    }

    #[test]
    fn conversion_identity_separates_kind_size_and_generation() {
        let first = payload(1, 16, 16);
        let same = ConversionKey::new(Bc7ContentKind::Icon, &first);
        assert_eq!(same, ConversionKey::new(Bc7ContentKind::Icon, &first));
        assert_ne!(same, ConversionKey::new(Bc7ContentKind::Thumbnail, &first));
        assert_ne!(
            same,
            ConversionKey::new(Bc7ContentKind::Icon, &payload(2, 16, 16))
        );
        assert_ne!(
            same,
            ConversionKey::new(Bc7ContentKind::Icon, &payload(1, 20, 20))
        );
    }

    #[test]
    fn admission_rejects_oversized_and_aggregate_staging_without_leaking_reservations() {
        assert!(estimated_staging_bytes(&payload(1, 16, 16)).is_ok());
        let invalid = ShellIconPayload {
            rgba: Vec::new(),
            ..payload(1, 16, 16)
        };
        assert!(estimated_staging_bytes(&invalid).is_err());
        let starting = RESERVED_STAGING_BYTES.load(Ordering::Acquire);
        let available = bc7_codec::MAX_BC7_STAGING_BYTES as u64 - starting;
        assert!(reserve_staging(available));
        assert!(!reserve_staging(1));
        release_staging(available);
        assert_eq!(RESERVED_STAGING_BYTES.load(Ordering::Acquire), starting);
    }

    #[test]
    fn registry_deduplicates_exact_jobs_and_supersedes_old_generations() {
        let first = ConversionKey::new(Bc7ContentKind::Icon, &payload(1, 16, 16));
        let second = ConversionKey::new(Bc7ContentKind::Icon, &payload(2, 16, 16));
        let registry = Mutex::new(Registry::default());
        {
            let mut state = registry.lock().unwrap();
            assert!(state.in_flight.insert(first.clone()));
            assert!(!state.in_flight.insert(first.clone()));
            state
                .latest
                .insert(first.supersession_key(), first.generation());
        }
        assert!(is_current(&registry, &first));
        registry
            .lock()
            .unwrap()
            .latest
            .insert(second.supersession_key(), second.generation());
        assert!(!is_current(&registry, &first));
        assert!(is_current(&registry, &second));
    }

    fn wait_for(predicate: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !predicate() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(predicate(), "background BC7 condition timed out");
    }

    #[test]
    fn background_runtime_single_flights_and_releases_all_reserved_buffers() {
        let _gate = crate::icon_disk_cache::BC7_GATE_TEST_LOCK
            .lock()
            .expect("gate lock");
        crate::icon_disk_cache::set_shell_bc7_runtime_gates(true, false);
        let root = tempfile::tempdir().expect("cache root");
        let disk = ShellIconDiskCache::with_root(root.path().to_path_buf());
        let payload = payload_at(root.path().join("source.png"), 11, 64, 64);
        let starting = job_stats();
        assert_eq!(
            schedule(
                Bc7ContentKind::Icon,
                payload.clone(),
                disk.clone(),
                None,
                Some(Instant::now() + Duration::from_secs(5)),
            ),
            ScheduleOutcome::Submitted
        );
        assert_eq!(
            schedule(
                Bc7ContentKind::Icon,
                payload.clone(),
                disk.clone(),
                None,
                Some(Instant::now() + Duration::from_secs(5)),
            ),
            ScheduleOutcome::Duplicate
        );
        wait_for(|| disk.load(&payload.key).is_some());
        wait_for(|| {
            let stats = job_stats();
            stats.active_jobs == 0 && stats.queued_jobs == 0 && stats.reserved_staging_bytes == 0
        });
        let loaded = disk.load(&payload.key).expect("persisted BC7 payload");
        assert!(loaded.rgba.is_empty());
        assert!(loaded.bc7.is_some());
        let finished = job_stats();
        assert_eq!(finished.submitted_jobs, starting.submitted_jobs + 1);
        assert_eq!(finished.completed_jobs, starting.completed_jobs + 1);
        assert_eq!(finished.duplicate_jobs, starting.duplicate_jobs + 1);
        crate::icon_disk_cache::set_shell_bc7_runtime_gates(false, false);
    }

    #[test]
    fn cancelled_request_is_rejected_before_queue_admission() {
        let token = CancellationToken::new();
        token.cancel();
        let root = tempfile::tempdir().expect("cache root");
        let outcome = schedule(
            Bc7ContentKind::Icon,
            payload(21, 16, 16),
            ShellIconDiskCache::with_root(root.path().to_path_buf()),
            Some(token),
            None,
        );
        assert_eq!(outcome, ScheduleOutcome::Cancelled);
    }

    #[test]
    fn expired_and_oversized_jobs_fall_back_without_publication() {
        let _gate = crate::icon_disk_cache::BC7_GATE_TEST_LOCK
            .lock()
            .expect("gate lock");
        crate::icon_disk_cache::set_shell_bc7_runtime_gates(true, false);
        let root = tempfile::tempdir().expect("cache root");
        let disk = ShellIconDiskCache::with_root(root.path().to_path_buf());
        let expired = payload_at(root.path().join("expired.png"), 22, 16, 16);
        let key = ConversionKey::new(Bc7ContentKind::Icon, &expired);
        let registry = Mutex::new(Registry::default());
        {
            let mut state = registry.lock().unwrap();
            state
                .latest
                .insert(key.supersession_key(), key.generation());
        }
        run_job(
            &Job {
                key,
                payload: expired.clone(),
                disk: disk.clone(),
                cancellation: None,
                deadline: Instant::now() - Duration::from_millis(1),
                reserved_staging_bytes: 0,
            },
            &registry,
        );
        assert!(disk.load(&expired.key).is_none());

        let oversized = ShellIconPayload {
            rgba: Vec::new(),
            ..payload_at(root.path().join("oversized.png"), 23, 16, 16)
        };
        assert_eq!(
            schedule(Bc7ContentKind::Icon, oversized, disk, None, None),
            ScheduleOutcome::RejectedOversized
        );
        crate::icon_disk_cache::set_shell_bc7_runtime_gates(false, false);
    }

    #[test]
    fn stale_generation_and_disabled_sibling_cannot_publish() {
        let _gate = crate::icon_disk_cache::BC7_GATE_TEST_LOCK
            .lock()
            .expect("gate lock");
        crate::icon_disk_cache::set_shell_bc7_runtime_gates(false, true);
        let root = tempfile::tempdir().expect("cache root");
        let icon_disk = ShellIconDiskCache::with_root(root.path().join("icons"));
        let thumbnail_disk =
            ShellIconDiskCache::with_root_lossy_thumbnail(root.path().join("thumbnails"));
        let old_icon = payload_at(root.path().join("same-source.png"), 30, 16, 16);
        let new_icon = payload_at(root.path().join("same-source.png"), 31, 16, 16);
        let thumbnail = payload_at(root.path().join("thumbnail.png"), 40, 20, 20);
        let old_key = ConversionKey::new(Bc7ContentKind::Icon, &old_icon);
        let new_key = ConversionKey::new(Bc7ContentKind::Icon, &new_icon);
        let thumbnail_key = ConversionKey::new(Bc7ContentKind::Thumbnail, &thumbnail);
        let registry = Mutex::new(Registry::default());
        {
            let mut state = registry.lock().unwrap();
            state
                .latest
                .insert(new_key.supersession_key(), new_key.generation());
            state
                .latest
                .insert(thumbnail_key.supersession_key(), thumbnail_key.generation());
        }
        let old_job = Job {
            key: old_key,
            payload: old_icon.clone(),
            disk: icon_disk.clone(),
            cancellation: None,
            deadline: Instant::now() + Duration::from_secs(2),
            reserved_staging_bytes: 0,
        };
        run_job(&old_job, &registry);
        assert!(icon_disk.load(&old_icon.key).is_none());

        let thumbnail_job = Job {
            key: thumbnail_key,
            payload: thumbnail.clone(),
            disk: thumbnail_disk.clone(),
            cancellation: None,
            deadline: Instant::now() + Duration::from_secs(2),
            reserved_staging_bytes: 0,
        };
        run_job(&thumbnail_job, &registry);
        assert!(thumbnail_disk.load(&thumbnail.key).is_some());
        crate::icon_disk_cache::set_shell_bc7_runtime_gates(false, false);
    }
}
