//! Background session persistence coordination and monitor-safe restore geometry.

use std::{
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use explorer_common::RoadmapLimits;
use explorer_model::{
    ExplorerWindowState, PersistedQuickAccessPin, PersistedRect, PersistedSessionEnvelope,
    PersistedWindowPlacement, SessionProvenance, SessionStore, SessionStoreError,
};

/// Accepted durable reducer transitions. Pointer motion and render state cannot enter this API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurableTransition {
    NavigationCommitted,
    TabOpened,
    TabClosed,
    TabReordered,
    ViewSettingsChanged,
    WindowPlacementChanged,
    QuickAccessChanged,
    RestorePreferenceChanged,
}

/// Observable, privacy-safe persistence health.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PersistenceHealth {
    pub accepted_transitions: u64,
    pub successful_writes: u64,
    pub failed_writes: u64,
    pub write_in_progress: bool,
    pub dirty: bool,
    pub last_error: Option<String>,
}

struct CoordinatorState {
    latest: Option<PendingSnapshot>,
    pending_reset: Option<explorer_model::SessionResetScope>,
    due: Option<Instant>,
    shutdown: bool,
    health: PersistenceHealth,
}

#[derive(Clone)]
enum PendingSnapshot {
    Envelope(PersistedSessionEnvelope),
    Runtime(Box<RuntimeSessionSnapshot>),
}

impl PendingSnapshot {
    fn project(&self) -> Result<PersistedSessionEnvelope, SessionStoreError> {
        match self {
            Self::Envelope(envelope) => Ok(envelope.clone()),
            Self::Runtime(runtime) => PersistedSessionEnvelope::project(
                &runtime.window,
                runtime.placement,
                &runtime.quick_access,
                runtime.restore_enabled,
                runtime.write_generation,
                runtime.provenance.clone(),
                runtime.limits,
            )
            .map_err(|error| SessionStoreError::InvalidSnapshot(error.to_string())),
        }
    }
}

/// Owned runtime inputs projected and serialized only by the persistence worker.
#[derive(Clone, Debug)]
pub struct RuntimeSessionSnapshot {
    pub window: ExplorerWindowState,
    pub placement: PersistedWindowPlacement,
    pub quick_access: Vec<PersistedQuickAccessPin>,
    pub restore_enabled: bool,
    pub write_generation: u64,
    pub provenance: SessionProvenance,
    pub limits: RoadmapLimits,
}

/// Cloneable, non-blocking producer side used by UI observers.
#[derive(Clone)]
pub struct PersistenceHandle {
    shared: Arc<(Mutex<CoordinatorState>, Condvar)>,
    debounce: Duration,
}

impl PersistenceHandle {
    /// Replaces any older pending runtime state with the latest accepted transition.
    pub fn accepted_runtime(
        &self,
        _transition: DurableTransition,
        snapshot: RuntimeSessionSnapshot,
    ) -> bool {
        self.accept(PendingSnapshot::Runtime(Box::new(snapshot)))
    }

    /// Queues one exact reset scope on the persistence worker and discards older pending saves.
    pub fn request_reset(&self, scope: explorer_model::SessionResetScope) -> bool {
        let (mutex, ready) = &*self.shared;
        let mut state = mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.shutdown {
            return false;
        }
        state.latest = None;
        state.due = Some(Instant::now());
        state.pending_reset = Some(scope);
        state.health.dirty = true;
        ready.notify_one();
        true
    }

    fn accept(&self, snapshot: PendingSnapshot) -> bool {
        let (mutex, ready) = &*self.shared;
        let mut state = mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.shutdown {
            return false;
        }
        state.latest = Some(snapshot);
        state.due = Some(Instant::now() + self.debounce);
        state.health.accepted_transitions = state.health.accepted_transitions.saturating_add(1);
        state.health.dirty = true;
        ready.notify_one();
        true
    }
}

/// Coalesces durable state changes and performs serialization/filesystem work off the UI thread.
pub struct PersistenceCoordinator {
    shared: Arc<(Mutex<CoordinatorState>, Condvar)>,
    worker: Option<JoinHandle<()>>,
    debounce: Duration,
}

impl PersistenceCoordinator {
    /// Starts one bounded worker. Only the latest full snapshot is retained.
    pub fn start(store: Arc<dyn SessionStore>, debounce: Duration, retry: Duration) -> Self {
        let shared = Arc::new((
            Mutex::new(CoordinatorState {
                latest: None,
                pending_reset: None,
                due: None,
                shutdown: false,
                health: PersistenceHealth::default(),
            }),
            Condvar::new(),
        ));
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("explorer-session-store".to_owned())
            .spawn(move || worker_loop(store.as_ref(), &worker_shared, retry))
            .ok();
        Self {
            shared,
            worker,
            debounce,
        }
    }

    /// Records one accepted durable transition without serializing or touching the filesystem.
    pub fn accepted(&self, _transition: DurableTransition, snapshot: PersistedSessionEnvelope) {
        let _ = self.handle().accept(PendingSnapshot::Envelope(snapshot));
    }

    /// Returns the cloneable producer side for an app/UI persistence bridge.
    pub fn handle(&self) -> PersistenceHandle {
        PersistenceHandle {
            shared: Arc::clone(&self.shared),
            debounce: self.debounce,
        }
    }

    /// Forces pending data to become immediately eligible and waits for bounded completion.
    pub fn flush(&self, timeout: Duration) -> bool {
        let (mutex, ready) = &*self.shared;
        let mut state = mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.due = state.latest.as_ref().map(|_| Instant::now());
        ready.notify_all();
        let deadline = Instant::now() + timeout;
        while (state.latest.is_some()
            || state.pending_reset.is_some()
            || state.health.write_in_progress)
            && Instant::now() < deadline
        {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (next, _) = ready
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next;
        }
        state.latest.is_none() && state.pending_reset.is_none() && !state.health.write_in_progress
    }

    /// Returns counters and the latest privacy-safe failure.
    pub fn health(&self) -> PersistenceHealth {
        self.shared
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .health
            .clone()
    }

    /// Performs a final flush and joins the worker. Repeated calls are harmless.
    pub fn shutdown(&mut self, timeout: Duration) -> bool {
        let (mutex, ready) = &*self.shared;
        {
            let mut state = mutex
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.shutdown = true;
            state.due = state.latest.as_ref().map(|_| Instant::now());
            ready.notify_all();
        }
        let flushed = self.flush(timeout);
        self.worker
            .take()
            .is_none_or(|worker| worker.join().is_ok())
            && flushed
    }
}

impl Drop for PersistenceCoordinator {
    fn drop(&mut self) {
        let _ = self.shutdown(Duration::from_secs(5));
    }
}

fn worker_loop(
    store: &dyn SessionStore,
    shared: &Arc<(Mutex<CoordinatorState>, Condvar)>,
    retry: Duration,
) {
    let (mutex, ready) = &**shared;
    loop {
        let mut state = mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while state.latest.is_none() && state.pending_reset.is_none() && !state.shutdown {
            state = ready
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if state.shutdown && state.latest.is_none() && state.pending_reset.is_none() {
            ready.notify_all();
            return;
        }
        if let Some(due) = state.due {
            let now = Instant::now();
            if due > now && !state.shutdown {
                let (next, _) = ready
                    .wait_timeout(state, due - now)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                drop(next);
                continue;
            }
        }
        let reset = state.pending_reset.take();
        let snapshot = reset.is_none().then(|| state.latest.take()).flatten();
        if reset.is_none() && snapshot.is_none() {
            continue;
        }
        state.due = None;
        state.health.write_in_progress = true;
        drop(state);

        let result = if let Some(scope) = reset {
            store.reset(scope)
        } else if let Some(snapshot) = &snapshot {
            snapshot
                .project()
                .and_then(|envelope| store.save(&envelope))
        } else {
            Ok(())
        };
        let mut state = mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.health.write_in_progress = false;
        match result {
            Ok(()) => {
                state.health.successful_writes = state.health.successful_writes.saturating_add(1);
                state.health.last_error = None;
            }
            Err(error) => {
                state.health.failed_writes = state.health.failed_writes.saturating_add(1);
                state.health.last_error = Some(error.to_string());
                if !state.shutdown && state.latest.is_none() && reset.is_none() {
                    state.latest = snapshot;
                    state.due = Some(Instant::now() + retry);
                } else if !state.shutdown && reset.is_some() {
                    state.pending_reset = reset;
                    state.due = Some(Instant::now() + retry);
                }
            }
        }
        state.health.dirty = state.latest.is_some() || state.pending_reset.is_some();
        ready.notify_all();
        if state.shutdown && state.latest.is_none() && state.pending_reset.is_none() {
            return;
        }
    }
}

/// Current monitor work area and effective DPI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorWorkArea {
    pub bounds: PersistedRect,
    pub dpi: u32,
    pub primary: bool,
}

/// Fits saved normal bounds to the best current monitor work area and current DPI.
pub fn fit_window_placement(
    saved: PersistedWindowPlacement,
    monitors: &[MonitorWorkArea],
    minimum_width: i32,
    minimum_height: i32,
) -> PersistedWindowPlacement {
    let target = monitors
        .iter()
        .max_by_key(|monitor| intersection_area(saved.normal_bounds, monitor.bounds))
        .filter(|monitor| intersection_area(saved.normal_bounds, monitor.bounds) > 0)
        .or_else(|| monitors.iter().find(|monitor| monitor.primary))
        .or_else(|| monitors.first());
    let Some(target) = target else {
        return saved;
    };
    let source_dpi = saved.source_dpi.max(1);
    let scale = |value: i32| {
        let scaled = i64::from(value)
            .saturating_mul(i64::from(target.dpi))
            .saturating_div(i64::from(source_dpi))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX));
        i32::try_from(scaled).unwrap_or(if scaled.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        })
    };
    let width = scale(saved.normal_bounds.width)
        .max(minimum_width)
        .min(target.bounds.width);
    let height = scale(saved.normal_bounds.height)
        .max(minimum_height)
        .min(target.bounds.height);
    let left = saved.normal_bounds.left.clamp(
        target.bounds.left,
        target.bounds.left + target.bounds.width - width,
    );
    let top = saved.normal_bounds.top.clamp(
        target.bounds.top,
        target.bounds.top + target.bounds.height - height,
    );
    PersistedWindowPlacement {
        normal_bounds: PersistedRect {
            left,
            top,
            width,
            height,
        },
        source_work_area: target.bounds,
        source_dpi: target.dpi,
        maximized: saved.maximized,
    }
}

fn intersection_area(left: PersistedRect, right: PersistedRect) -> i64 {
    let width = left
        .left
        .saturating_add(left.width)
        .min(right.left.saturating_add(right.width))
        - left.left.max(right.left);
    let height = left
        .top
        .saturating_add(left.height)
        .min(right.top.saturating_add(right.height))
        - left.top.max(right.top);
    i64::from(width.max(0)) * i64::from(height.max(0))
}

/// Reads the current primary monitor work area, excluding the taskbar.
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Win32 exposes the primary work area through an output RECT pointer"
)]
pub fn primary_monitor_work_area() -> Option<MonitorWorkArea> {
    use windows::Win32::{
        Foundation::RECT,
        UI::{
            HiDpi::GetDpiForSystem,
            WindowsAndMessaging::{
                SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
            },
        },
    };

    let mut rect = RECT::default();
    // SAFETY: `rect` is a live writable RECT for the duration of the synchronous call.
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(std::ptr::from_mut(&mut rect).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()?;
    let width = rect.right.checked_sub(rect.left)?;
    let height = rect.bottom.checked_sub(rect.top)?;
    (width > 0 && height > 0).then(|| MonitorWorkArea {
        bounds: PersistedRect {
            left: rect.left,
            top: rect.top,
            width,
            height,
        },
        // SAFETY: `GetDpiForSystem` has no pointer or ownership preconditions.
        dpi: unsafe { GetDpiForSystem() }.max(96),
        primary: true,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use explorer_common::RoadmapLimits;
    use explorer_model::{
        ExplorerWindowState, HistoryEntry, LocationDescriptor, PersistedSessionEnvelope,
        PersistedWindowPlacement, SessionLoadOutcome, SessionLoadSource, SessionProvenance,
        SessionResetScope, SessionStoreError, SyntheticRoot,
    };

    use super::*;

    struct RecordingStore {
        generations: Mutex<Vec<u64>>,
        failures_remaining: AtomicUsize,
        delay: Duration,
    }

    impl RecordingStore {
        fn new(delay: Duration, failures: usize) -> Self {
            Self {
                generations: Mutex::new(Vec::new()),
                failures_remaining: AtomicUsize::new(failures),
                delay,
            }
        }
    }

    impl SessionStore for RecordingStore {
        fn load(&self) -> Result<SessionLoadOutcome, SessionStoreError> {
            Ok(SessionLoadOutcome {
                source: SessionLoadSource::Defaults,
                envelope: None,
                rejected_artifacts: 0,
                migration_performed: false,
            })
        }

        fn save(&self, envelope: &PersistedSessionEnvelope) -> Result<(), SessionStoreError> {
            thread::sleep(self.delay);
            if self
                .failures_remaining
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    value.checked_sub(1)
                })
                .is_ok()
            {
                return Err(SessionStoreError::StorageFull);
            }
            self.generations
                .lock()
                .expect("generation log")
                .push(envelope.write_generation);
            Ok(())
        }

        fn reset(&self, _scope: SessionResetScope) -> Result<(), SessionStoreError> {
            Ok(())
        }
    }

    fn snapshot(generation: u64) -> PersistedSessionEnvelope {
        let window = ExplorerWindowState::new(HistoryEntry::new(
            LocationDescriptor::synthetic(SyntheticRoot::Home),
            "Home",
        ));
        PersistedSessionEnvelope::project(
            &window,
            PersistedWindowPlacement {
                normal_bounds: PersistedRect {
                    left: 10,
                    top: 10,
                    width: 1000,
                    height: 700,
                },
                source_work_area: PersistedRect {
                    left: 0,
                    top: 0,
                    width: 1920,
                    height: 1080,
                },
                source_dpi: 96,
                maximized: false,
            },
            &[],
            true,
            generation,
            SessionProvenance {
                app_version: "test".to_owned(),
                app_revision: "test".to_owned(),
                windows_build: "test".to_owned(),
            },
            RoadmapLimits::default(),
        )
        .expect("snapshot")
    }

    #[test]
    fn rapid_changes_coalesce_to_the_latest_snapshot() {
        let store = Arc::new(RecordingStore::new(Duration::ZERO, 0));
        let mut coordinator = PersistenceCoordinator::start(
            store.clone(),
            Duration::from_millis(30),
            Duration::from_millis(10),
        );
        coordinator.accepted(DurableTransition::TabOpened, snapshot(1));
        coordinator.accepted(DurableTransition::ViewSettingsChanged, snapshot(2));
        assert!(coordinator.flush(Duration::from_secs(1)));
        assert!(coordinator.shutdown(Duration::from_secs(1)));
        assert_eq!(*store.generations.lock().expect("writes"), vec![2]);
    }

    #[test]
    fn dirty_during_write_and_failure_retry_are_not_lost() {
        let store = Arc::new(RecordingStore::new(Duration::from_millis(30), 1));
        let mut coordinator =
            PersistenceCoordinator::start(store.clone(), Duration::ZERO, Duration::from_millis(5));
        coordinator.accepted(DurableTransition::NavigationCommitted, snapshot(1));
        thread::sleep(Duration::from_millis(5));
        coordinator.accepted(DurableTransition::TabReordered, snapshot(2));
        assert!(coordinator.flush(Duration::from_secs(2)));
        assert!(coordinator.shutdown(Duration::from_secs(1)));
        assert_eq!(*store.generations.lock().expect("writes"), vec![2]);
        let health = coordinator.health();
        assert_eq!(health.failed_writes, 1);
        assert_eq!(health.successful_writes, 1);
        assert!(!health.dirty);
    }

    #[test]
    fn placement_scales_clamps_and_preserves_maximized_normal_bounds() {
        let saved = PersistedWindowPlacement {
            normal_bounds: PersistedRect {
                left: -5000,
                top: 100,
                width: 1200,
                height: 900,
            },
            source_work_area: PersistedRect {
                left: -1920,
                top: 0,
                width: 1920,
                height: 1040,
            },
            source_dpi: 96,
            maximized: true,
        };
        let fitted = fit_window_placement(
            saved,
            &[MonitorWorkArea {
                bounds: PersistedRect {
                    left: 0,
                    top: 0,
                    width: 1600,
                    height: 860,
                },
                dpi: 144,
                primary: true,
            }],
            640,
            480,
        );
        assert_eq!(fitted.normal_bounds.left, 0);
        assert_eq!(fitted.normal_bounds.top, 0);
        assert_eq!(fitted.normal_bounds.width, 1600);
        assert_eq!(fitted.normal_bounds.height, 860);
        assert_eq!(fitted.source_dpi, 144);
        assert!(fitted.maximized);
    }
}
