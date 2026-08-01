//! UI-specific diagnostics layered on the shared interaction-first QoS policy.

/// Aggregate result-delivery counts that are specific to the GPUI presentation boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiQosSnapshot {
    pub integrated_results: u64,
    pub deferred_results: usize,
    pub frame_budget_exhaustions: u64,
    pub degradation: explorer_jobs::DegradationLevel,
    pub observations: explorer_jobs::QosObservationSnapshot,
}

#[derive(Debug, Default)]
pub struct UiDeliveryCounters {
    integrated_results: u64,
    deferred_results: usize,
    frame_budget_exhaustions: u64,
}

impl UiDeliveryCounters {
    pub fn record_drain(&mut self, integrated: usize, deferred: usize, exhausted: bool) {
        self.integrated_results = self.integrated_results.saturating_add(integrated as u64);
        self.deferred_results = deferred;
        if exhausted {
            self.frame_budget_exhaustions = self.frame_budget_exhaustions.saturating_add(1);
        }
    }

    pub const fn integrated_results(&self) -> u64 {
        self.integrated_results
    }

    pub const fn deferred_results(&self) -> usize {
        self.deferred_results
    }

    pub const fn frame_budget_exhaustions(&self) -> u64 {
        self.frame_budget_exhaustions
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, time::Duration};

    use explorer_jobs::{
        DegradationLevel, DegradationPolicyConfig, DegradationTransition, FrameDrainBudget,
        FrameDrainLimit, InteractionFirstQos, InteractionFirstQosConfig, PressureSample,
        QosWorkClass,
    };

    #[test]
    fn default_frame_budget_targets_sixty_frames_per_second() {
        let budget = FrameDrainBudget::default();
        assert_eq!(budget.item_limit(), 64);
        assert_eq!(budget.time_limit(), Duration::from_millis(16));
    }

    #[test]
    fn shared_budget_splits_completion_bursts_without_reordering() {
        let budget = FrameDrainBudget::new(2, Duration::from_millis(8));
        let mut pending = VecDeque::from([1, 2, 3]);
        let drained = budget.drain(&mut pending, || Duration::ZERO);

        assert_eq!(drained.items, [1, 2]);
        assert_eq!(drained.limit, Some(FrameDrainLimit::ItemLimit));
        assert_eq!(pending, VecDeque::from([3]));
    }

    #[test]
    fn shared_policy_sheds_every_optional_stage_and_recovers_hysteretically() {
        let mut qos = InteractionFirstQos::new(InteractionFirstQosConfig {
            degradation: DegradationPolicyConfig {
                pressure_samples_per_step: 1,
                recovery_samples_per_step: 2,
                recovery_queue_depth: 0,
            },
            ..InteractionFirstQosConfig::default()
        });
        let pressure = PressureSample::new(1, 1, true);
        for (level, newly_shed) in [
            (DegradationLevel::ShedMaintenance, QosWorkClass::Maintenance),
            (DegradationLevel::ShedPrefetch, QosWorkClass::Prefetch),
            (
                DegradationLevel::ShedOffscreenEnrichment,
                QosWorkClass::OffscreenEnrichment,
            ),
            (
                DegradationLevel::ShedVisualRefinement,
                QosWorkClass::VisualRefinement,
            ),
            (
                DegradationLevel::ShedOptionalAnimation,
                QosWorkClass::OptionalAnimation,
            ),
        ] {
            assert!(matches!(
                qos.observe_pressure(pressure),
                DegradationTransition::Degraded { to, .. } if to == level
            ));
            assert!(qos.should_shed(newly_shed));
            assert!(!qos.should_shed(QosWorkClass::DirectInteraction));
            assert!(!qos.should_shed(QosWorkClass::Navigation));
            assert!(!qos.should_shed(QosWorkClass::FileOperationProgress));
        }

        for expected in [
            DegradationLevel::ShedVisualRefinement,
            DegradationLevel::ShedOffscreenEnrichment,
            DegradationLevel::ShedPrefetch,
            DegradationLevel::ShedMaintenance,
            DegradationLevel::Normal,
        ] {
            assert!(matches!(
                qos.observe_pressure(PressureSample::new(0, 1, false)),
                DegradationTransition::Unchanged(_)
            ));
            assert!(matches!(
                qos.observe_pressure(PressureSample::new(0, 1, false)),
                DegradationTransition::Recovered { to, .. } if to == expected
            ));
        }
    }
}
