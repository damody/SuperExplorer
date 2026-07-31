//! Lightweight callback-duration diagnostics for latency-sensitive UI actions.

use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

pub const CALLBACK_BUDGET: Duration = Duration::from_millis(4);
const FILE_VIEW_SAMPLE_CAPACITY: usize = 2_048;

/// Aggregate file-view counters that contain no paths, names, or item identities.
#[derive(Debug, Default)]
pub struct FileViewPerformanceCounters {
    directory_revision: AtomicU64,
    presentation_rebuilds: AtomicU64,
    realized_items: AtomicUsize,
    maximum_realized_items: AtomicUsize,
    complete_snapshot_clones: AtomicU64,
    render_samples: Mutex<VecDeque<Duration>>,
    scroll_samples: Mutex<VecDeque<Duration>>,
    input_samples: Mutex<VecDeque<Duration>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileViewPerformanceSnapshot {
    pub directory_revision: u64,
    pub presentation_rebuilds: u64,
    pub realized_items: usize,
    pub maximum_realized_items: usize,
    pub complete_snapshot_clones: u64,
    pub render: Option<CallbackDistribution>,
    pub scroll: Option<CallbackDistribution>,
    pub input: Option<CallbackDistribution>,
}

impl FileViewPerformanceCounters {
    pub fn record_directory_revision(&self, revision: u64) {
        self.directory_revision.store(revision, Ordering::Relaxed);
    }

    pub fn record_presentation_rebuild(&self) {
        self.presentation_rebuilds.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_realized_items(&self, count: usize) {
        self.realized_items.store(count, Ordering::Relaxed);
        self.maximum_realized_items
            .fetch_max(count, Ordering::Relaxed);
    }

    pub fn record_complete_snapshot_clone(&self) {
        self.complete_snapshot_clones
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_render(&self, elapsed: Duration) {
        record_bounded(&self.render_samples, elapsed);
    }

    pub fn record_scroll(&self, elapsed: Duration) {
        record_bounded(&self.scroll_samples, elapsed);
    }

    pub fn record_input(&self, elapsed: Duration) {
        record_bounded(&self.input_samples, elapsed);
    }

    pub fn snapshot(&self) -> FileViewPerformanceSnapshot {
        FileViewPerformanceSnapshot {
            directory_revision: self.directory_revision.load(Ordering::Relaxed),
            presentation_rebuilds: self.presentation_rebuilds.load(Ordering::Relaxed),
            realized_items: self.realized_items.load(Ordering::Relaxed),
            maximum_realized_items: self.maximum_realized_items.load(Ordering::Relaxed),
            complete_snapshot_clones: self.complete_snapshot_clones.load(Ordering::Relaxed),
            render: distribution(&self.render_samples, "file_view_render"),
            scroll: distribution(&self.scroll_samples, "file_view_scroll"),
            input: distribution(&self.input_samples, "file_view_input"),
        }
    }
}

fn record_bounded(samples: &Mutex<VecDeque<Duration>>, elapsed: Duration) {
    let Ok(mut samples) = samples.lock() else {
        return;
    };
    if samples.len() == FILE_VIEW_SAMPLE_CAPACITY {
        samples.pop_front();
    }
    samples.push_back(elapsed);
}

fn distribution(
    samples: &Mutex<VecDeque<Duration>>,
    action_name: &'static str,
) -> Option<CallbackDistribution> {
    let Ok(samples) = samples.lock() else {
        return None;
    };
    let measurements = samples
        .iter()
        .copied()
        .map(|elapsed| CallbackMeasurement {
            action_name,
            elapsed,
        })
        .collect::<Vec<_>>();
    CallbackDistribution::from_measurements(&measurements)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallbackDistribution {
    pub samples: usize,
    pub median: Duration,
    pub p95: Duration,
    pub maximum: Duration,
    pub over_budget: usize,
}

impl CallbackDistribution {
    pub fn from_measurements(measurements: &[CallbackMeasurement]) -> Option<Self> {
        if measurements.is_empty() {
            return None;
        }
        let mut elapsed = measurements
            .iter()
            .map(|measurement| measurement.elapsed)
            .collect::<Vec<_>>();
        elapsed.sort_unstable();
        let percentile = |percent: usize| {
            let rank = measurements.len().saturating_mul(percent).div_ceil(100);
            elapsed[rank.saturating_sub(1).min(elapsed.len() - 1)]
        };
        let maximum = elapsed.iter().copied().max().unwrap_or_default();
        Some(Self {
            samples: measurements.len(),
            median: percentile(50),
            p95: percentile(95),
            maximum,
            over_budget: measurements
                .iter()
                .filter(|measurement| measurement.exceeds_budget())
                .count(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallbackMeasurement {
    pub action_name: &'static str,
    pub elapsed: Duration,
}

impl CallbackMeasurement {
    pub const fn exceeds_budget(self) -> bool {
        self.elapsed.as_nanos() > CALLBACK_BUDGET.as_nanos()
    }

    pub fn record(self) {
        if self.exceeds_budget() {
            tracing::warn!(
                action = self.action_name,
                elapsed_micros = self.elapsed.as_micros(),
                budget_micros = CALLBACK_BUDGET.as_micros(),
                "Explorer UI callback exceeded latency budget"
            );
        } else {
            tracing::trace!(
                action = self.action_name,
                elapsed_micros = self.elapsed.as_micros(),
                "Explorer UI callback duration"
            );
        }
    }
}

pub fn measure_callback<T>(
    action_name: &'static str,
    callback: impl FnOnce() -> T,
) -> (T, CallbackMeasurement) {
    let started = Instant::now();
    let value = callback();
    (
        value,
        CallbackMeasurement {
            action_name,
            elapsed: started.elapsed(),
        },
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        CALLBACK_BUDGET, CallbackDistribution, CallbackMeasurement, FileViewPerformanceCounters,
        measure_callback,
    };

    #[test]
    fn measurement_reports_budget_regressions_without_failing_on_machine_speed() {
        let (value, measured) = measure_callback("fixture", || 42);
        assert_eq!(value, 42);
        assert_eq!(measured.action_name, "fixture");

        let within = CallbackMeasurement {
            action_name: "within",
            elapsed: CALLBACK_BUDGET,
        };
        let regression = CallbackMeasurement {
            action_name: "regression",
            elapsed: CALLBACK_BUDGET + Duration::from_nanos(1),
        };
        assert!(!within.exceeds_budget());
        assert!(regression.exceeds_budget());
        within.record();
        regression.record();
    }

    #[test]
    fn percentile_summary_uses_nearest_rank_and_counts_budget_regressions() {
        let measurements = (1_u64..=100)
            .map(|micros| CallbackMeasurement {
                action_name: "fixture",
                elapsed: Duration::from_micros(micros),
            })
            .collect::<Vec<_>>();
        let summary = CallbackDistribution::from_measurements(&measurements).unwrap();
        assert_eq!(summary.samples, 100);
        assert_eq!(summary.median, Duration::from_micros(50));
        assert_eq!(summary.p95, Duration::from_micros(95));
        assert_eq!(summary.maximum, Duration::from_micros(100));
        assert_eq!(summary.over_budget, 0);

        let over_budget = [CallbackMeasurement {
            action_name: "fixture",
            elapsed: CALLBACK_BUDGET + Duration::from_nanos(1),
        }];
        assert_eq!(
            CallbackDistribution::from_measurements(&over_budget)
                .unwrap()
                .over_budget,
            1
        );
        assert!(CallbackDistribution::from_measurements(&[]).is_none());
    }

    #[test]
    fn file_view_counters_are_bounded_and_privacy_safe() {
        let counters = FileViewPerformanceCounters::default();
        counters.record_directory_revision(42);
        counters.record_presentation_rebuild();
        counters.record_realized_items(123);
        counters.record_realized_items(64);
        counters.record_render(Duration::from_millis(2));
        counters.record_scroll(Duration::from_millis(3));
        counters.record_input(Duration::from_millis(4));
        let snapshot = counters.snapshot();
        assert_eq!(snapshot.directory_revision, 42);
        assert_eq!(snapshot.presentation_rebuilds, 1);
        assert_eq!(snapshot.realized_items, 64);
        assert_eq!(snapshot.maximum_realized_items, 123);
        assert_eq!(snapshot.complete_snapshot_clones, 0);
        assert_eq!(snapshot.render.unwrap().p95, Duration::from_millis(2));
        assert_eq!(snapshot.scroll.unwrap().p95, Duration::from_millis(3));
        assert_eq!(snapshot.input.unwrap().p95, Duration::from_millis(4));
    }

    #[test]
    #[ignore = "explicit machine-speed benchmark; run with --ignored --nocapture"]
    fn measures_production_dispatcher_callback_percentiles() {
        use crate::{
            actions::{ActionSource, ExplorerAction, dispatch_action},
            layout::LogicalPx,
            state::AppViewState,
        };

        const SAMPLES: usize = 20_000;
        let mut state = AppViewState::default();
        let mut run = |name: &'static str, mut callback: Box<dyn FnMut(&mut AppViewState)>| {
            let measurements = (0..SAMPLES)
                .map(|_| {
                    let ((), measurement) = measure_callback(name, || callback(&mut state));
                    measurement
                })
                .collect::<Vec<_>>();
            let summary = CallbackDistribution::from_measurements(&measurements).unwrap();
            println!(
                "{name}: samples={}, median_ns={}, p95_ns={}, max_ns={}, over_4ms={}",
                summary.samples,
                summary.median.as_nanos(),
                summary.p95.as_nanos(),
                summary.maximum.as_nanos(),
                summary.over_budget
            );
        };

        let mut expanded = false;
        run(
            "ResizeNavigationPane",
            Box::new(move |state| {
                expanded = !expanded;
                let width = if expanded { 360.0 } else { 220.0 };
                let _ = dispatch_action(
                    state,
                    ExplorerAction::ResizeNavigationPane {
                        width: LogicalPx::new(width),
                    },
                    ActionSource::Programmatic,
                );
            }),
        );
        run(
            "ToggleTheme",
            Box::new(|state| {
                let _ = dispatch_action(
                    state,
                    ExplorerAction::ToggleTheme,
                    ActionSource::Programmatic,
                );
            }),
        );
        let mut search = true;
        run(
            "FocusTraversal",
            Box::new(move |state| {
                let action = if search {
                    ExplorerAction::FocusSearch
                } else {
                    ExplorerAction::RestorePreviousFocus
                };
                search = !search;
                let _ = dispatch_action(state, action, ActionSource::Keyboard);
            }),
        );
    }

    #[test]
    #[ignore = "explicit release large-directory benchmark; run with --ignored --nocapture"]
    #[allow(
        clippy::cast_precision_loss,
        reason = "deterministic benchmark offsets stay within the exact practical scroll range"
    )]
    fn measures_large_directory_virtual_scroll_percentiles() {
        use crate::file_view::{
            DirectoryPresentationCache, MAX_STANDARD_REALIZED_ITEMS, fixed_virtual_range,
        };
        use explorer_model::{DirectorySnapshot, SortDescriptor};

        let mut snapshot = DirectorySnapshot::default();
        for entry in explorer_test_support::synthetic_directory_entries(100_000) {
            snapshot.upsert(entry);
        }
        let mut cache = DirectoryPresentationCache::default();
        let presentation = cache.resolve(&snapshot, false, SortDescriptor::default());
        let mut frame_measurements = Vec::with_capacity(10_000);
        let mut input_measurements = Vec::with_capacity(10_000);
        for sample in 0..10_000 {
            let offset = (sample * 997 % 2_300_000) as f32;
            let (range, frame) = measure_callback("virtual_scroll_frame", || {
                fixed_virtual_range(presentation.len(), 24.0, 720.0, offset, 2)
            });
            assert!(range.items.len() <= MAX_STANDARD_REALIZED_ITEMS);
            frame_measurements.push(frame);
            let (_, input) = measure_callback("virtual_scroll_input", || {
                presentation.entry(range.items.start)
            });
            input_measurements.push(input);
        }
        let frame = CallbackDistribution::from_measurements(&frame_measurements).unwrap();
        let input = CallbackDistribution::from_measurements(&input_measurements).unwrap();
        assert!(frame.p95 <= Duration::from_micros(16_700));
        assert!(frame.maximum <= Duration::from_millis(100));
        assert!(input.p95 <= Duration::from_millis(50));
        println!(
            "virtual-scroll: samples={}, p50_ns={}, p95_ns={}, max_ns={}, input_p95_ns={}",
            frame.samples,
            frame.median.as_nanos(),
            frame.p95.as_nanos(),
            frame.maximum.as_nanos(),
            input.p95.as_nanos()
        );
        assert_eq!(cache.rebuilds(), 1);
    }
}
