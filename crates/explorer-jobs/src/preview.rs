//! Deterministic selection debounce and stale-terminal suppression for Preview Pane.

use std::time::Duration;

use explorer_model::{
    Generation, PreviewEligibility, PreviewFallback, PreviewLifecycle, PreviewSelection,
    PreviewTransitionError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewCoordinatorAction {
    Start {
        generation: Generation,
        selection: PreviewSelection,
    },
    Unload {
        generation: Generation,
    },
}

/// Pure coordinator driven by a monotonic elapsed duration, making rapid-selection tests stable.
pub struct PreviewCoordinator {
    lifecycle: PreviewLifecycle,
    generation: Generation,
    debounce: Duration,
    due: Option<Duration>,
    selection: Option<PreviewSelection>,
    pending_after_unload: Option<(PreviewEligibility, Duration)>,
    closing: bool,
}

impl PreviewCoordinator {
    pub fn new(debounce: Duration) -> Self {
        Self {
            lifecycle: PreviewLifecycle::Closed,
            generation: Generation::default(),
            debounce,
            due: None,
            selection: None,
            pending_after_unload: None,
            closing: false,
        }
    }

    pub const fn lifecycle(&self) -> &PreviewLifecycle {
        &self.lifecycle
    }

    /// Opens the pane without loading content.
    ///
    /// # Errors
    /// Returns a transition error if called from a state that still owns a handler.
    pub fn open(&mut self) -> Result<(), PreviewTransitionError> {
        self.closing = false;
        self.lifecycle.transition(PreviewLifecycle::Idle)
    }

    /// Closes the pane and requests idempotent unload when native content is active.
    ///
    /// # Errors
    /// Returns a transition error only for an invalid internal lifecycle edge.
    pub fn close(&mut self) -> Result<Option<PreviewCoordinatorAction>, PreviewTransitionError> {
        self.due = None;
        self.selection = None;
        self.pending_after_unload = None;
        if matches!(
            self.lifecycle,
            PreviewLifecycle::Closed | PreviewLifecycle::Unloading { .. }
        ) {
            self.closing = true;
            return Ok(None);
        }
        let active = matches!(
            self.lifecycle,
            PreviewLifecycle::Loading { .. } | PreviewLifecycle::Visible { .. }
        );
        if active {
            self.closing = true;
            self.lifecycle.transition(PreviewLifecycle::Unloading {
                generation: self.generation,
            })?;
            return Ok(Some(PreviewCoordinatorAction::Unload {
                generation: self.generation,
            }));
        }
        self.lifecycle.transition(PreviewLifecycle::Closed)?;
        self.closing = true;
        Ok(None)
    }

    /// Replaces any pending selection and schedules only the last eligible generation.
    ///
    /// # Errors
    /// Returns a transition error for an invalid internal lifecycle edge.
    pub fn select(
        &mut self,
        eligibility: &PreviewEligibility,
        now: Duration,
    ) -> Result<Option<PreviewCoordinatorAction>, PreviewTransitionError> {
        self.generation = self.generation.checked_next().unwrap_or(self.generation);
        self.due = None;
        self.closing = false;
        if matches!(self.lifecycle, PreviewLifecycle::Unloading { .. }) {
            self.pending_after_unload = Some((eligibility.clone(), now));
            return Ok(None);
        }
        let active_generation = self.lifecycle.generation();
        let unload = matches!(
            self.lifecycle,
            PreviewLifecycle::Loading { .. } | PreviewLifecycle::Visible { .. }
        )
        .then_some(PreviewCoordinatorAction::Unload {
            generation: active_generation.unwrap_or(self.generation),
        });
        if unload.is_some() {
            self.lifecycle.transition(PreviewLifecycle::Unloading {
                generation: active_generation.unwrap_or(self.generation),
            })?;
            self.pending_after_unload = Some((eligibility.clone(), now));
            return Ok(unload);
        } else if !matches!(
            self.lifecycle,
            PreviewLifecycle::Idle
                | PreviewLifecycle::Fallback { .. }
                | PreviewLifecycle::Failed { .. }
                | PreviewLifecycle::Debouncing { .. }
        ) {
            return Err(PreviewTransitionError::Invalid);
        }
        self.schedule_selection(eligibility, now)?;
        Ok(unload)
    }

    fn schedule_selection(
        &mut self,
        eligibility: &PreviewEligibility,
        now: Duration,
    ) -> Result<(), PreviewTransitionError> {
        if matches!(eligibility, PreviewEligibility::SingleEligible(_)) {
            self.selection = match eligibility {
                PreviewEligibility::SingleEligible(selection) => Some(selection.clone()),
                _ => None,
            };
            self.lifecycle.transition(PreviewLifecycle::Debouncing {
                generation: self.generation,
            })?;
            self.due = Some(now.saturating_add(self.debounce));
        } else {
            self.selection = None;
            let reason = match eligibility {
                PreviewEligibility::None => PreviewFallback::NoSelection,
                PreviewEligibility::Folder => PreviewFallback::Folder,
                PreviewEligibility::Multiple { .. } => PreviewFallback::MultipleSelection,
                PreviewEligibility::Offline => PreviewFallback::Offline,
                PreviewEligibility::Quarantined { .. } => PreviewFallback::Quarantined,
                PreviewEligibility::Unsupported | PreviewEligibility::Error { .. } => {
                    PreviewFallback::Unsupported
                }
                PreviewEligibility::SingleEligible(_) => {
                    unreachable!("eligible case handled above")
                }
            };
            self.lifecycle.transition(PreviewLifecycle::Fallback {
                generation: self.generation,
                reason,
            })?;
        }
        Ok(())
    }

    /// Completes the one outstanding unload. Stale and duplicate acknowledgements are ignored.
    pub fn unloaded(&mut self, generation: Generation) -> bool {
        if !matches!(
            self.lifecycle,
            PreviewLifecycle::Unloading {
                generation: current
            } if current == generation
        ) {
            return false;
        }
        if self.closing {
            self.pending_after_unload = None;
            return self.lifecycle.transition(PreviewLifecycle::Closed).is_ok();
        }
        if self.lifecycle.transition(PreviewLifecycle::Idle).is_err() {
            return false;
        }
        if let Some((eligibility, now)) = self.pending_after_unload.take() {
            self.schedule_selection(&eligibility, now).is_ok()
        } else {
            true
        }
    }

    /// Starts the latest selection once its debounce expires.
    ///
    /// # Errors
    /// Returns a transition error for an inconsistent lifecycle.
    pub fn poll(
        &mut self,
        now: Duration,
    ) -> Result<Option<PreviewCoordinatorAction>, PreviewTransitionError> {
        if self.due.is_none_or(|due| now < due) {
            return Ok(None);
        }
        self.due = None;
        let selection = self
            .selection
            .clone()
            .ok_or(PreviewTransitionError::Invalid)?;
        self.lifecycle.transition(PreviewLifecycle::Loading {
            generation: self.generation,
        })?;
        Ok(Some(PreviewCoordinatorAction::Start {
            generation: self.generation,
            selection,
        }))
    }

    /// Applies exactly one current-generation terminal; late and duplicate terminals are ignored.
    pub fn finish(&mut self, generation: Generation, success: bool, retryable: bool) -> bool {
        if generation != self.generation
            || !matches!(self.lifecycle, PreviewLifecycle::Loading { .. })
        {
            return false;
        }
        let next = if success {
            PreviewLifecycle::Visible { generation }
        } else {
            PreviewLifecycle::Failed {
                generation,
                retryable,
            }
        };
        self.lifecycle.transition(next).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_model::{LocationDescriptor, PreviewSelection, ShellItemId};

    fn eligible(id: u8) -> PreviewEligibility {
        PreviewEligibility::SingleEligible(PreviewSelection {
            item_id: ShellItemId::from_provider_bytes([id]).expect("id"),
            location: LocationDescriptor::ParsingName(format!("test:{id}")),
            display_name: format!("item {id}"),
        })
    }

    #[test]
    fn rapid_selection_activates_only_last_and_suppresses_late_terminal() {
        let mut coordinator = PreviewCoordinator::new(Duration::from_millis(100));
        coordinator.open().expect("open");
        coordinator
            .select(&eligible(1), Duration::ZERO)
            .expect("first");
        coordinator
            .select(&eligible(2), Duration::from_millis(50))
            .expect("second");
        assert_eq!(coordinator.poll(Duration::from_millis(100)), Ok(None));
        let action = coordinator
            .poll(Duration::from_millis(150))
            .expect("poll")
            .expect("start");
        let PreviewCoordinatorAction::Start {
            generation,
            selection,
        } = action
        else {
            panic!("start")
        };
        assert_eq!(selection.display_name, "item 2");
        assert!(!coordinator.finish(
            Generation::new(generation.value().saturating_sub(1)),
            true,
            false
        ));
        assert!(coordinator.finish(generation, true, false));
        assert!(!coordinator.finish(generation, true, false));
    }

    #[test]
    fn unsupported_offline_and_multiple_never_start() {
        for eligibility in [
            PreviewEligibility::Unsupported,
            PreviewEligibility::Offline,
            PreviewEligibility::Multiple { count: 2 },
        ] {
            let mut coordinator = PreviewCoordinator::new(Duration::from_millis(1));
            coordinator.open().expect("open");
            coordinator
                .select(&eligibility, Duration::ZERO)
                .expect("fallback");
            assert_eq!(coordinator.poll(Duration::from_secs(1)), Ok(None));
        }
    }

    #[test]
    fn failed_handler_does_not_poison_the_next_safe_selection() {
        let mut coordinator = PreviewCoordinator::new(Duration::from_millis(1));
        coordinator.open().expect("open");
        coordinator
            .select(&eligible(1), Duration::ZERO)
            .expect("first selection");
        let PreviewCoordinatorAction::Start { generation, .. } = coordinator
            .poll(Duration::from_millis(1))
            .expect("poll first")
            .expect("start first")
        else {
            panic!("expected first start")
        };
        assert!(coordinator.finish(generation, false, true));
        coordinator
            .select(&eligible(2), Duration::from_millis(2))
            .expect("safe replacement");
        let PreviewCoordinatorAction::Start {
            generation: replacement,
            ..
        } = coordinator
            .poll(Duration::from_millis(3))
            .expect("poll replacement")
            .expect("start replacement")
        else {
            panic!("expected replacement start")
        };
        assert_ne!(replacement, generation);
        assert!(coordinator.finish(replacement, true, false));
    }

    #[test]
    fn active_replacement_waits_for_exact_unload_and_close_is_idempotent() {
        let mut coordinator = PreviewCoordinator::new(Duration::from_millis(1));
        coordinator.open().expect("open");
        coordinator
            .select(&eligible(1), Duration::ZERO)
            .expect("first selection");
        let PreviewCoordinatorAction::Start {
            generation: first, ..
        } = coordinator
            .poll(Duration::from_millis(1))
            .expect("first poll")
            .expect("first start")
        else {
            panic!("first start")
        };
        assert!(coordinator.finish(first, true, false));
        assert_eq!(
            coordinator
                .select(&eligible(2), Duration::from_millis(2))
                .expect("replacement"),
            Some(PreviewCoordinatorAction::Unload { generation: first })
        );
        assert_eq!(coordinator.poll(Duration::from_secs(1)), Ok(None));
        assert!(!coordinator.unloaded(Generation::new(first.value() + 99)));
        assert!(coordinator.unloaded(first));
        let PreviewCoordinatorAction::Start {
            generation: second,
            selection,
        } = coordinator
            .poll(Duration::from_millis(3))
            .expect("replacement poll")
            .expect("replacement start")
        else {
            panic!("replacement start")
        };
        assert_eq!(selection.display_name, "item 2");
        assert!(coordinator.finish(second, true, false));
        assert_eq!(
            coordinator.close().expect("close"),
            Some(PreviewCoordinatorAction::Unload { generation: second })
        );
        assert_eq!(coordinator.close(), Ok(None));
        assert!(coordinator.unloaded(second));
        assert_eq!(coordinator.close(), Ok(None));
    }
}
