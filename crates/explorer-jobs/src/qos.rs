//! Reusable interaction-first admission, bounded frame draining, and pressure policy.

use std::{collections::VecDeque, time::Duration};

/// The reason another result cannot be integrated in the current frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameDrainLimit {
    /// The configured maximum number of results has been integrated.
    ItemLimit,
    /// The configured per-frame integration time has elapsed.
    TimeLimit,
}

/// Limits applied while integrating asynchronous results on the UI thread.
///
/// The default limit is 64 items or 16 ms, whichever comes first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameDrainBudget {
    item_limit: usize,
    time_limit: Duration,
}

impl FrameDrainBudget {
    /// The default number of results admitted in a frame.
    pub const DEFAULT_ITEM_LIMIT: usize = 64;

    /// The default amount of UI-thread time spent integrating results in a frame.
    pub const DEFAULT_TIME_LIMIT: Duration = Duration::from_millis(16);

    /// Creates a dual item/time frame-integration budget.
    ///
    /// An item limit of zero is promoted to one so a live result queue can always make
    /// progress. A zero time limit intentionally admits no result.
    pub const fn new(item_limit: usize, time_limit: Duration) -> Self {
        Self {
            item_limit: if item_limit == 0 { 1 } else { item_limit },
            time_limit,
        }
    }

    /// Returns the maximum number of results that may be integrated in one frame.
    pub const fn item_limit(self) -> usize {
        self.item_limit
    }

    /// Returns the maximum UI-thread time spent integrating results in one frame.
    pub const fn time_limit(self) -> Duration {
        self.time_limit
    }

    /// Checks whether another result may be integrated without exceeding either bound.
    ///
    /// Callers that use a channel or another queue type can call this before removing
    /// each result. `elapsed` must be measured from the start of the current frame's
    /// integration pass.
    ///
    /// # Errors
    ///
    /// Returns the item or time limit that prevents another result from being admitted.
    pub fn admit_next(
        self,
        integrated_items: usize,
        elapsed: Duration,
    ) -> Result<(), FrameDrainLimit> {
        if integrated_items >= self.item_limit {
            Err(FrameDrainLimit::ItemLimit)
        } else if elapsed >= self.time_limit {
            Err(FrameDrainLimit::TimeLimit)
        } else {
            Ok(())
        }
    }

    /// Drains queued results while both frame bounds permit it.
    ///
    /// `elapsed` is injected so callers can use a monotonic clock while unit tests can
    /// use deterministic elapsed values. Items that do not fit remain in `queue` in
    /// their original order for the next frame.
    pub fn drain<T, F>(self, queue: &mut VecDeque<T>, mut elapsed: F) -> FrameDrain<T>
    where
        F: FnMut() -> Duration,
    {
        let mut items = Vec::new();
        loop {
            let Some(_) = queue.front() else {
                return FrameDrain { items, limit: None };
            };
            match self.admit_next(items.len(), elapsed()) {
                Ok(()) => {
                    // A front item was observed above, so this cannot fail unless a
                    // caller mutates the same queue reentrantly, which `&mut` forbids.
                    if let Some(item) = queue.pop_front() {
                        items.push(item);
                    }
                }
                Err(limit) => {
                    return FrameDrain {
                        items,
                        limit: Some(limit),
                    };
                }
            }
        }
    }
}

impl Default for FrameDrainBudget {
    fn default() -> Self {
        Self::new(Self::DEFAULT_ITEM_LIMIT, Self::DEFAULT_TIME_LIMIT)
    }
}

/// Results admitted from a queue during a frame-integration pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameDrain<T> {
    /// Results that fit in the current frame budget, retained in queue order.
    pub items: Vec<T>,
    /// The bound that stopped the pass, or `None` when the queue was exhausted.
    pub limit: Option<FrameDrainLimit>,
}

/// Classes of work considered by the degradation policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QosWorkClass {
    /// Direct pointer, keyboard, selection, or tab interaction.
    DirectInteraction,
    /// Work needed to present the currently requested directory.
    Navigation,
    /// User-visible file-operation state such as progress, errors, and cancellation.
    FileOperationProgress,
    /// Lowest-value periodic or cleanup work.
    Maintenance,
    /// Background-tab or predicted-content work.
    Prefetch,
    /// Enrichment for content outside the visible viewport.
    OffscreenEnrichment,
    /// Nonessential visual quality improvements.
    VisualRefinement,
    /// Decorative animation that has no interaction or correctness role.
    OptionalAnimation,
}

impl QosWorkClass {
    const fn shedding_rank(self) -> Option<u8> {
        match self {
            Self::Maintenance => Some(1),
            Self::Prefetch => Some(2),
            Self::OffscreenEnrichment => Some(3),
            Self::VisualRefinement => Some(4),
            Self::OptionalAnimation => Some(5),
            Self::DirectInteraction | Self::Navigation | Self::FileOperationProgress => None,
        }
    }
}

/// The current ordered level of optional-work shedding.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum DegradationLevel {
    /// All work classes are eligible for normal scheduling.
    #[default]
    Normal,
    /// Maintenance work is deferred or rejected.
    ShedMaintenance,
    /// Maintenance and prefetch work are deferred or rejected.
    ShedPrefetch,
    /// Maintenance, prefetch, and off-screen enrichment are deferred or rejected.
    ShedOffscreenEnrichment,
    /// Visual refinement is additionally deferred or rejected.
    ShedVisualRefinement,
    /// Optional animation is additionally deferred or rejected.
    ShedOptionalAnimation,
}

impl DegradationLevel {
    const MAX: Self = Self::ShedOptionalAnimation;

    const fn rank(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::ShedMaintenance => 1,
            Self::ShedPrefetch => 2,
            Self::ShedOffscreenEnrichment => 3,
            Self::ShedVisualRefinement => 4,
            Self::ShedOptionalAnimation => 5,
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Normal => Self::ShedMaintenance,
            Self::ShedMaintenance => Self::ShedPrefetch,
            Self::ShedPrefetch => Self::ShedOffscreenEnrichment,
            Self::ShedOffscreenEnrichment => Self::ShedVisualRefinement,
            Self::ShedVisualRefinement | Self::ShedOptionalAnimation => Self::ShedOptionalAnimation,
        }
    }

    const fn previous(self) -> Self {
        match self {
            Self::Normal | Self::ShedMaintenance => Self::Normal,
            Self::ShedPrefetch => Self::ShedMaintenance,
            Self::ShedOffscreenEnrichment => Self::ShedPrefetch,
            Self::ShedVisualRefinement => Self::ShedOffscreenEnrichment,
            Self::ShedOptionalAnimation => Self::ShedVisualRefinement,
        }
    }

    /// Returns whether this level defers the supplied work class.
    pub const fn sheds(self, work: QosWorkClass) -> bool {
        match work.shedding_rank() {
            Some(work_rank) => work_rank <= self.rank(),
            None => false,
        }
    }
}

/// One deterministic scheduler-pressure observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PressureSample {
    /// The current number of queued work items.
    pub queue_depth: usize,
    /// The bounded capacity of the observed queue.
    pub queue_capacity: usize,
    /// Whether a frame's result integration consumed its configured budget.
    pub frame_budget_exhausted: bool,
}

impl PressureSample {
    /// Creates a pressure observation from bounded queue state and frame drain state.
    pub const fn new(
        queue_depth: usize,
        queue_capacity: usize,
        frame_budget_exhausted: bool,
    ) -> Self {
        Self {
            queue_depth,
            queue_capacity,
            frame_budget_exhausted,
        }
    }

    /// Returns whether the observed bounded queue is saturated.
    pub const fn is_saturated(self) -> bool {
        self.queue_capacity != 0 && self.queue_depth >= self.queue_capacity
    }
}

/// Hysteresis configuration for ordered optional-work shedding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DegradationPolicyConfig {
    /// Consecutive saturated or frame-exhausted samples required to shed one level.
    pub pressure_samples_per_step: u32,
    /// Consecutive below-recovery samples required to restore one level.
    pub recovery_samples_per_step: u32,
    /// A queue depth at or below which a non-exhausted sample counts as recovery.
    pub recovery_queue_depth: usize,
}

impl Default for DegradationPolicyConfig {
    fn default() -> Self {
        Self {
            pressure_samples_per_step: 2,
            recovery_samples_per_step: 3,
            recovery_queue_depth: 0,
        }
    }
}

impl DegradationPolicyConfig {
    const fn pressure_samples_per_step(self) -> u32 {
        if self.pressure_samples_per_step == 0 {
            1
        } else {
            self.pressure_samples_per_step
        }
    }

    const fn recovery_samples_per_step(self) -> u32 {
        if self.recovery_samples_per_step == 0 {
            1
        } else {
            self.recovery_samples_per_step
        }
    }
}

/// A change to the current degradation state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DegradationTransition {
    /// Pressure did not change the current level.
    Unchanged(DegradationLevel),
    /// Sustained pressure shed the next optional-work class.
    Degraded {
        /// Level before this pressure observation.
        from: DegradationLevel,
        /// Level after this pressure observation.
        to: DegradationLevel,
    },
    /// Sustained recovery restored one optional-work class for future submissions.
    Recovered {
        /// Level before this recovery observation.
        from: DegradationLevel,
        /// Level after this recovery observation.
        to: DegradationLevel,
    },
}

/// Side-effect-free interaction-first overload policy with hysteresis.
#[derive(Clone, Debug)]
pub struct InteractionFirstPolicy {
    config: DegradationPolicyConfig,
    level: DegradationLevel,
    consecutive_pressure: u32,
    consecutive_recovery: u32,
}

impl InteractionFirstPolicy {
    /// Creates a policy at normal degradation.
    pub const fn new(config: DegradationPolicyConfig) -> Self {
        Self {
            config,
            level: DegradationLevel::Normal,
            consecutive_pressure: 0,
            consecutive_recovery: 0,
        }
    }

    /// Returns the current optional-work shedding level.
    pub const fn degradation_level(&self) -> DegradationLevel {
        self.level
    }

    /// Returns whether work should be deferred or rejected at the current level.
    ///
    /// This only makes an admission decision for new work. It never recreates cancelled
    /// or superseded work when the level later recovers.
    pub const fn should_shed(&self, work: QosWorkClass) -> bool {
        self.level.sheds(work)
    }

    /// Applies one pressure observation and returns the deterministic state transition.
    pub fn observe_pressure(&mut self, sample: PressureSample) -> DegradationTransition {
        if sample.is_saturated() || sample.frame_budget_exhausted {
            self.consecutive_pressure = self.consecutive_pressure.saturating_add(1);
            self.consecutive_recovery = 0;
            if self.consecutive_pressure >= self.config.pressure_samples_per_step()
                && self.level != DegradationLevel::MAX
            {
                let from = self.level;
                self.level = self.level.next();
                self.consecutive_pressure = 0;
                return DegradationTransition::Degraded {
                    from,
                    to: self.level,
                };
            }
        } else if sample.queue_depth <= self.config.recovery_queue_depth {
            self.consecutive_recovery = self.consecutive_recovery.saturating_add(1);
            self.consecutive_pressure = 0;
            if self.consecutive_recovery >= self.config.recovery_samples_per_step()
                && self.level != DegradationLevel::Normal
            {
                let from = self.level;
                self.level = self.level.previous();
                self.consecutive_recovery = 0;
                return DegradationTransition::Recovered {
                    from,
                    to: self.level,
                };
            }
        } else {
            self.consecutive_pressure = 0;
            self.consecutive_recovery = 0;
        }
        DegradationTransition::Unchanged(self.level)
    }
}

/// Aggregate, privacy-safe `QoS` measurements with bounded latency retention.
#[derive(Clone, Debug)]
pub struct QosObservations {
    latency_capacity: usize,
    latency_samples: VecDeque<Duration>,
    queue_depth: usize,
    queue_high_water: usize,
    overloads: u64,
    cancellations: u64,
    stale_results: u64,
    saturations: u64,
    degradation_transitions: u64,
}

impl QosObservations {
    /// Creates aggregate counters with a bounded latency sample window.
    pub fn new(latency_capacity: usize) -> Self {
        Self {
            latency_capacity: latency_capacity.max(1),
            latency_samples: VecDeque::with_capacity(latency_capacity.max(1)),
            queue_depth: 0,
            queue_high_water: 0,
            overloads: 0,
            cancellations: 0,
            stale_results: 0,
            saturations: 0,
            degradation_transitions: 0,
        }
    }

    /// Records an aggregate queue depth and updates its high-water mark.
    pub fn observe_queue_depth(&mut self, depth: usize) {
        self.queue_depth = depth;
        self.queue_high_water = self.queue_high_water.max(depth);
    }

    /// Records a latency duration without associating it with an item or path.
    pub fn record_latency(&mut self, latency: Duration) {
        if self.latency_samples.len() == self.latency_capacity {
            let _ = self.latency_samples.pop_front();
        }
        self.latency_samples.push_back(latency);
    }

    /// Records one overload rejection.
    pub fn record_overload(&mut self) {
        self.overloads = self.overloads.saturating_add(1);
    }

    /// Records one cancellation outcome.
    pub fn record_cancellation(&mut self) {
        self.cancellations = self.cancellations.saturating_add(1);
    }

    /// Records one stale result rejected before presentation.
    pub fn record_stale_result(&mut self) {
        self.stale_results = self.stale_results.saturating_add(1);
    }

    /// Records one queue or worker saturation observation.
    pub fn record_saturation(&mut self) {
        self.saturations = self.saturations.saturating_add(1);
    }

    /// Records one transition into or out of an optional-work shedding level.
    pub fn record_degradation_transition(&mut self) {
        self.degradation_transitions = self.degradation_transitions.saturating_add(1);
    }

    /// Returns a bounded, aggregate-only diagnostic snapshot.
    pub fn snapshot(&self) -> QosObservationSnapshot {
        let mut sorted_latencies = self.latency_samples.iter().copied().collect::<Vec<_>>();
        sorted_latencies.sort_unstable();
        QosObservationSnapshot {
            latency_sample_capacity: self.latency_capacity,
            latency_sample_count: sorted_latencies.len(),
            latency_p50: percentile(&sorted_latencies, 50),
            latency_p95: percentile(&sorted_latencies, 95),
            latency_max: sorted_latencies.last().copied(),
            queue_depth: self.queue_depth,
            queue_high_water: self.queue_high_water,
            overloads: self.overloads,
            cancellations: self.cancellations,
            stale_results: self.stale_results,
            saturations: self.saturations,
            degradation_transitions: self.degradation_transitions,
        }
    }
}

impl Default for QosObservations {
    fn default() -> Self {
        Self::new(128)
    }
}

/// A bounded, aggregate-only `QoS` diagnostic snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QosObservationSnapshot {
    /// Maximum number of retained latency samples.
    pub latency_sample_capacity: usize,
    /// Number of retained latency samples currently represented by this snapshot.
    pub latency_sample_count: usize,
    /// Median retained latency, if any samples have been recorded.
    pub latency_p50: Option<Duration>,
    /// 95th-percentile retained latency, if any samples have been recorded.
    pub latency_p95: Option<Duration>,
    /// Maximum retained latency, if any samples have been recorded.
    pub latency_max: Option<Duration>,
    /// Most recently observed queue depth.
    pub queue_depth: usize,
    /// Largest observed queue depth since creation.
    pub queue_high_water: usize,
    /// Number of non-blocking overload rejections.
    pub overloads: u64,
    /// Number of cancellation outcomes.
    pub cancellations: u64,
    /// Number of presentation-boundary stale-result rejections.
    pub stale_results: u64,
    /// Number of queue or worker saturation observations.
    pub saturations: u64,
    /// Number of degradation-level transitions.
    pub degradation_transitions: u64,
}

fn percentile(sorted: &[Duration], percentile: usize) -> Option<Duration> {
    if sorted.is_empty() {
        return None;
    }
    let index = sorted
        .len()
        .saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1);
    sorted.get(index).copied()
}

/// Configuration for the reusable interaction-first `QoS` coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractionFirstQosConfig {
    /// Result integration bounds supplied to UI-facing work loops.
    pub frame_drain_budget: FrameDrainBudget,
    /// Optional-work shedding hysteresis configuration.
    pub degradation: DegradationPolicyConfig,
    /// Maximum retained aggregate latency samples.
    pub latency_sample_capacity: usize,
}

impl Default for InteractionFirstQosConfig {
    fn default() -> Self {
        Self {
            frame_drain_budget: FrameDrainBudget::default(),
            degradation: DegradationPolicyConfig::default(),
            latency_sample_capacity: 128,
        }
    }
}

/// Coordinates reusable pressure policy with aggregate-only observations.
#[derive(Clone, Debug)]
pub struct InteractionFirstQos {
    frame_drain_budget: FrameDrainBudget,
    policy: InteractionFirstPolicy,
    observations: QosObservations,
}

impl InteractionFirstQos {
    /// Creates a coordinator with normal degradation and empty observations.
    pub fn new(config: InteractionFirstQosConfig) -> Self {
        Self {
            frame_drain_budget: config.frame_drain_budget,
            policy: InteractionFirstPolicy::new(config.degradation),
            observations: QosObservations::new(config.latency_sample_capacity),
        }
    }

    /// Returns the frame result-integration budget.
    pub const fn frame_drain_budget(&self) -> FrameDrainBudget {
        self.frame_drain_budget
    }

    /// Returns the current optional-work shedding level.
    pub const fn degradation_level(&self) -> DegradationLevel {
        self.policy.degradation_level()
    }

    /// Returns whether a new work item should be shed under current pressure.
    pub const fn should_shed(&self, work: QosWorkClass) -> bool {
        self.policy.should_shed(work)
    }

    /// Applies pressure, recording only aggregate queue, saturation, and transition data.
    pub fn observe_pressure(&mut self, sample: PressureSample) -> DegradationTransition {
        self.observations.observe_queue_depth(sample.queue_depth);
        if sample.is_saturated() {
            self.observations.record_saturation();
        }
        let transition = self.policy.observe_pressure(sample);
        if !matches!(transition, DegradationTransition::Unchanged(_)) {
            self.observations.record_degradation_transition();
        }
        transition
    }

    /// Returns mutable aggregate observations for recording terminal outcomes.
    pub fn observations_mut(&mut self) -> &mut QosObservations {
        &mut self.observations
    }

    /// Returns a bounded, aggregate-only diagnostic snapshot.
    pub fn observation_snapshot(&self) -> QosObservationSnapshot {
        self.observations.snapshot()
    }
}

impl Default for InteractionFirstQos {
    fn default() -> Self {
        Self::new(InteractionFirstQosConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DegradationLevel, DegradationPolicyConfig, DegradationTransition, FrameDrainBudget,
        FrameDrainLimit, InteractionFirstPolicy, InteractionFirstQos, InteractionFirstQosConfig,
        PressureSample, QosObservations, QosWorkClass,
    };
    use std::{collections::VecDeque, time::Duration};

    #[test]
    fn frame_drain_retains_remaining_items_in_order_at_item_limit() {
        let budget = FrameDrainBudget::new(2, Duration::from_millis(8));
        let mut queue = VecDeque::from(["first", "second", "third"]);

        let drained = budget.drain(&mut queue, || Duration::ZERO);

        assert_eq!(drained.items, ["first", "second"]);
        assert_eq!(drained.limit, Some(FrameDrainLimit::ItemLimit));
        assert_eq!(queue, VecDeque::from(["third"]));
    }

    #[test]
    fn frame_drain_stops_before_time_budget_and_incremental_check_matches() {
        let budget = FrameDrainBudget::new(4, Duration::from_millis(8));
        let mut queue = VecDeque::from([1, 2, 3]);
        let mut samples = [Duration::ZERO, Duration::from_millis(8)].into_iter();

        let drained = budget.drain(&mut queue, || samples.next().unwrap_or(Duration::ZERO));

        assert_eq!(drained.items, [1]);
        assert_eq!(drained.limit, Some(FrameDrainLimit::TimeLimit));
        assert_eq!(queue, VecDeque::from([2, 3]));
        assert_eq!(
            budget.admit_next(4, Duration::ZERO),
            Err(FrameDrainLimit::ItemLimit)
        );
        assert_eq!(
            budget.admit_next(0, Duration::from_millis(8)),
            Err(FrameDrainLimit::TimeLimit)
        );
    }

    #[test]
    fn degradation_sheds_optional_work_in_order_and_preserves_foreground() {
        let config = DegradationPolicyConfig {
            pressure_samples_per_step: 1,
            recovery_samples_per_step: 2,
            recovery_queue_depth: 0,
        };
        let mut policy = InteractionFirstPolicy::new(config);
        let pressure = PressureSample::new(4, 4, false);

        assert!(matches!(
            policy.observe_pressure(pressure),
            DegradationTransition::Degraded {
                to: DegradationLevel::ShedMaintenance,
                ..
            }
        ));
        assert!(policy.should_shed(QosWorkClass::Maintenance));
        assert!(!policy.should_shed(QosWorkClass::Prefetch));
        assert!(!policy.should_shed(QosWorkClass::DirectInteraction));
        assert!(!policy.should_shed(QosWorkClass::Navigation));
        assert!(!policy.should_shed(QosWorkClass::FileOperationProgress));

        let _ = policy.observe_pressure(pressure);
        assert_eq!(policy.degradation_level(), DegradationLevel::ShedPrefetch);
        assert!(policy.should_shed(QosWorkClass::Prefetch));
        assert!(!policy.should_shed(QosWorkClass::OffscreenEnrichment));

        let _ = policy.observe_pressure(pressure);
        assert!(policy.should_shed(QosWorkClass::OffscreenEnrichment));
        let _ = policy.observe_pressure(pressure);
        assert!(policy.should_shed(QosWorkClass::VisualRefinement));
        let _ = policy.observe_pressure(pressure);
        assert!(policy.should_shed(QosWorkClass::OptionalAnimation));
    }

    #[test]
    fn repeated_frame_budget_exhaustion_advances_degradation_without_queue_saturation() {
        let mut policy = InteractionFirstPolicy::new(DegradationPolicyConfig {
            pressure_samples_per_step: 2,
            recovery_samples_per_step: 2,
            recovery_queue_depth: 0,
        });
        let exhausted_frame = PressureSample::new(1, 4, true);

        assert_eq!(
            policy.observe_pressure(exhausted_frame),
            DegradationTransition::Unchanged(DegradationLevel::Normal)
        );
        assert_eq!(
            policy.observe_pressure(exhausted_frame),
            DegradationTransition::Degraded {
                from: DegradationLevel::Normal,
                to: DegradationLevel::ShedMaintenance,
            }
        );
    }

    #[test]
    fn degradation_requires_sustained_recovery_before_restoring_future_admission() {
        let config = DegradationPolicyConfig {
            pressure_samples_per_step: 1,
            recovery_samples_per_step: 2,
            recovery_queue_depth: 1,
        };
        let mut policy = InteractionFirstPolicy::new(config);
        let _ = policy.observe_pressure(PressureSample::new(2, 2, false));
        assert_eq!(
            policy.degradation_level(),
            DegradationLevel::ShedMaintenance
        );

        assert_eq!(
            policy.observe_pressure(PressureSample::new(1, 2, false)),
            DegradationTransition::Unchanged(DegradationLevel::ShedMaintenance)
        );
        assert!(policy.should_shed(QosWorkClass::Maintenance));
        assert_eq!(
            policy.observe_pressure(PressureSample::new(0, 2, false)),
            DegradationTransition::Recovered {
                from: DegradationLevel::ShedMaintenance,
                to: DegradationLevel::Normal,
            }
        );
        assert!(!policy.should_shed(QosWorkClass::Maintenance));
    }

    #[test]
    fn neutral_samples_break_hysteresis_streaks_to_prevent_oscillation() {
        let config = DegradationPolicyConfig {
            pressure_samples_per_step: 2,
            recovery_samples_per_step: 2,
            recovery_queue_depth: 0,
        };
        let mut policy = InteractionFirstPolicy::new(config);
        let _ = policy.observe_pressure(PressureSample::new(2, 2, false));
        let _ = policy.observe_pressure(PressureSample::new(1, 2, false));
        assert_eq!(policy.degradation_level(), DegradationLevel::Normal);
        let _ = policy.observe_pressure(PressureSample::new(2, 2, false));
        let _ = policy.observe_pressure(PressureSample::new(2, 2, false));
        assert_eq!(
            policy.degradation_level(),
            DegradationLevel::ShedMaintenance
        );
    }

    #[test]
    fn observations_are_bounded_and_do_not_retain_work_identity() {
        let mut observations = QosObservations::new(2);
        observations.observe_queue_depth(3);
        observations.observe_queue_depth(1);
        observations.record_latency(Duration::from_millis(30));
        observations.record_latency(Duration::from_millis(10));
        observations.record_latency(Duration::from_millis(20));
        observations.record_overload();
        observations.record_cancellation();
        observations.record_stale_result();
        observations.record_saturation();
        observations.record_degradation_transition();

        assert_eq!(
            observations.snapshot(),
            super::QosObservationSnapshot {
                latency_sample_capacity: 2,
                latency_sample_count: 2,
                latency_p50: Some(Duration::from_millis(10)),
                latency_p95: Some(Duration::from_millis(20)),
                latency_max: Some(Duration::from_millis(20)),
                queue_depth: 1,
                queue_high_water: 3,
                overloads: 1,
                cancellations: 1,
                stale_results: 1,
                saturations: 1,
                degradation_transitions: 1,
            }
        );
    }

    #[test]
    fn coordinator_records_pressure_and_transitions_without_identity_data() {
        let mut qos = InteractionFirstQos::new(InteractionFirstQosConfig {
            frame_drain_budget: FrameDrainBudget::new(64, Duration::from_millis(8)),
            degradation: DegradationPolicyConfig {
                pressure_samples_per_step: 1,
                recovery_samples_per_step: 1,
                recovery_queue_depth: 0,
            },
            latency_sample_capacity: 1,
        });

        let _ = qos.observe_pressure(PressureSample::new(1, 1, false));
        let snapshot = qos.observation_snapshot();
        assert_eq!(snapshot.queue_depth, 1);
        assert_eq!(snapshot.queue_high_water, 1);
        assert_eq!(snapshot.saturations, 1);
        assert_eq!(snapshot.degradation_transitions, 1);
        assert!(qos.should_shed(QosWorkClass::Maintenance));
    }
}
