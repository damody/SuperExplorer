//! Pure preview-pane eligibility and lifecycle contracts.

use std::fmt;
use std::time::Duration;

use crate::{Generation, LocationDescriptor, ShellItemId, TabId};

/// Why the current selection can or cannot be previewed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewEligibility {
    None,
    SingleEligible(PreviewSelection),
    Folder,
    Multiple { count: usize },
    Unsupported,
    Offline,
    Quarantined { handler_digest: String },
    Error { retryable: bool },
}

/// Owned, reconstructible data for one preview candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewSelection {
    pub item_id: ShellItemId,
    pub location: LocationDescriptor,
    pub display_name: String,
}

/// Public initialization surfaces offered by Windows Preview Handlers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewInitializationMode {
    File,
    Stream,
    ShellItem,
}

/// Privacy-safe handler identity; paths and document content are deliberately absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewHandlerIdentity {
    pub clsid: [u8; 16],
    pub registration_source: PreviewRegistrationSource,
    pub initialization_modes: Vec<PreviewInitializationMode>,
    pub diagnostic_digest: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewRegistrationSource {
    PerUser,
    Machine,
    System,
}

/// Lifecycle visible to the app. Native COM/HWND state remains broker-owned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewLifecycle {
    Closed,
    Idle,
    Debouncing {
        generation: Generation,
    },
    Loading {
        generation: Generation,
    },
    Visible {
        generation: Generation,
    },
    Fallback {
        generation: Generation,
        reason: PreviewFallback,
    },
    Unloading {
        generation: Generation,
    },
    Failed {
        generation: Generation,
        retryable: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewFallback {
    NoSelection,
    MultipleSelection,
    Folder,
    Unsupported,
    Offline,
    Quarantined,
    HostUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewOperation {
    Lookup,
    Initialize,
    Render,
    Resize,
    Input,
    Unload,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewDeadlinePolicy {
    pub lookup: Duration,
    pub initialize: Duration,
    pub render: Duration,
    pub resize: Duration,
    pub input: Duration,
    pub unload: Duration,
}

impl PreviewDeadlinePolicy {
    pub fn from_limits(limits: explorer_common::RoadmapLimits) -> Self {
        Self {
            lookup: Duration::from_millis(limits.preview_lookup_timeout_ms),
            initialize: Duration::from_millis(limits.preview_initialize_timeout_ms),
            render: Duration::from_millis(limits.preview_render_timeout_ms),
            resize: Duration::from_millis(limits.preview_resize_timeout_ms),
            input: Duration::from_millis(limits.preview_input_timeout_ms),
            unload: Duration::from_millis(limits.preview_unload_timeout_ms),
        }
    }

    pub const fn for_operation(self, operation: PreviewOperation) -> Duration {
        match operation {
            PreviewOperation::Lookup => self.lookup,
            PreviewOperation::Initialize => self.initialize,
            PreviewOperation::Render => self.render,
            PreviewOperation::Resize => self.resize,
            PreviewOperation::Input => self.input,
            PreviewOperation::Unload => self.unload,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewHostError {
    Unsupported,
    Registration,
    Initialization,
    InvalidWindow,
    DpiMismatch,
    Timeout(PreviewOperation),
    Crash,
    Disconnected,
    Quarantined,
    StaleGeneration,
}

/// Generation-bound preview operation identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewRequestIdentity {
    pub tab_id: TabId,
    pub generation: Generation,
    pub request_id: u64,
}

/// App-owned host geometry sent to the broker. A stale generation must be ignored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewHostBounds {
    pub generation: Generation,
    pub left_physical: i32,
    pub top_physical: i32,
    pub width_physical: u32,
    pub height_physical: u32,
    pub dpi: u32,
}

/// Owned commands crossing the app/service boundary for one broker-hosted preview session.
/// Native COM objects and HWND ownership never enter the model.
#[derive(Clone, Debug, PartialEq)]
pub enum PreviewHostCommand {
    Start {
        selection: PreviewSelection,
        parent_window: u64,
        bounds: PreviewHostBounds,
    },
    SetBounds(PreviewHostBounds),
    SetFocus {
        generation: Generation,
    },
    Accelerator {
        generation: Generation,
        virtual_key: u32,
        modifiers: u8,
    },
    Unload {
        generation: Generation,
    },
}

impl PreviewHostCommand {
    pub const fn generation(&self) -> Generation {
        match self {
            Self::Start { bounds, .. } | Self::SetBounds(bounds) => bounds.generation,
            Self::SetFocus { generation }
            | Self::Accelerator { generation, .. }
            | Self::Unload { generation } => *generation,
        }
    }
}

/// Exactly-one result for a preview host command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewHostTerminal {
    Ready {
        generation: Generation,
        mode: PreviewInitializationMode,
    },
    Updated {
        generation: Generation,
    },
    Unloaded {
        generation: Generation,
    },
    Failed {
        generation: Generation,
        error: PreviewHostError,
    },
}

impl PreviewHostTerminal {
    pub const fn generation(&self) -> Generation {
        match self {
            Self::Ready { generation, .. }
            | Self::Updated { generation }
            | Self::Unloaded { generation }
            | Self::Failed { generation, .. } => *generation,
        }
    }
}

impl PreviewHostBounds {
    /// Validates size and DPI limits before crossing IPC.
    pub const fn is_valid(self) -> bool {
        self.width_physical > 0
            && self.height_physical > 0
            && self.width_physical <= 16_384
            && self.height_physical <= 16_384
            && self.dpi >= 48
            && self.dpi <= 960
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewTransitionError {
    Invalid,
    StaleGeneration,
}

impl fmt::Display for PreviewTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => formatter.write_str("invalid preview lifecycle transition"),
            Self::StaleGeneration => formatter.write_str("stale preview generation"),
        }
    }
}

impl std::error::Error for PreviewTransitionError {}

impl PreviewLifecycle {
    /// Applies one transition while enforcing generation and unload ownership.
    ///
    /// # Errors
    /// Returns `Invalid` for a disallowed edge or `StaleGeneration` for late host work.
    pub fn transition(&mut self, next: Self) -> Result<(), PreviewTransitionError> {
        if let (Some(current), Some(candidate)) = (self.generation(), next.generation())
            && candidate < current
        {
            return Err(PreviewTransitionError::StaleGeneration);
        }
        let valid = matches!(
            (&*self, &next),
            (Self::Closed, Self::Idle)
                | (
                    Self::Idle,
                    Self::Closed | Self::Debouncing { .. } | Self::Fallback { .. }
                )
                | (
                    Self::Debouncing { .. },
                    Self::Debouncing { .. }
                        | Self::Loading { .. }
                        | Self::Idle
                        | Self::Fallback { .. }
                )
                | (
                    Self::Loading { .. },
                    Self::Visible { .. }
                        | Self::Failed { .. }
                        | Self::Fallback { .. }
                        | Self::Unloading { .. }
                )
                | (Self::Visible { .. }, Self::Unloading { .. })
                | (
                    Self::Fallback { .. } | Self::Failed { .. },
                    Self::Idle | Self::Debouncing { .. } | Self::Closed
                )
                | (
                    Self::Unloading { .. },
                    Self::Idle | Self::Closed | Self::Debouncing { .. }
                )
        );
        if !valid {
            return Err(PreviewTransitionError::Invalid);
        }
        *self = next;
        Ok(())
    }

    pub const fn generation(&self) -> Option<Generation> {
        match self {
            Self::Closed | Self::Idle => None,
            Self::Debouncing { generation }
            | Self::Loading { generation }
            | Self::Visible { generation }
            | Self::Fallback { generation, .. }
            | Self::Unloading { generation }
            | Self::Failed { generation, .. } => Some(*generation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_rejects_stale_and_skipped_activation() {
        let mut state = PreviewLifecycle::Closed;
        assert_eq!(state.transition(PreviewLifecycle::Idle), Ok(()));
        assert_eq!(
            state.transition(PreviewLifecycle::Visible {
                generation: Generation::new(1)
            }),
            Err(PreviewTransitionError::Invalid)
        );
        state
            .transition(PreviewLifecycle::Debouncing {
                generation: Generation::new(2),
            })
            .expect("debounce");
        state
            .transition(PreviewLifecycle::Loading {
                generation: Generation::new(2),
            })
            .expect("load");
        state
            .transition(PreviewLifecycle::Visible {
                generation: Generation::new(2),
            })
            .expect("visible");
        assert_eq!(
            state.transition(PreviewLifecycle::Unloading {
                generation: Generation::new(1)
            }),
            Err(PreviewTransitionError::StaleGeneration)
        );
    }

    #[test]
    fn host_bounds_are_bounded() {
        let valid = PreviewHostBounds {
            generation: Generation::new(1),
            left_physical: -400,
            top_physical: 20,
            width_physical: 800,
            height_physical: 600,
            dpi: 144,
        };
        assert!(valid.is_valid());
        assert!(
            !PreviewHostBounds {
                width_physical: 0,
                ..valid
            }
            .is_valid()
        );
        assert!(!PreviewHostBounds { dpi: 1, ..valid }.is_valid());
    }

    #[test]
    fn every_preview_stage_has_an_independent_nonzero_deadline() {
        let policy = PreviewDeadlinePolicy::from_limits(explorer_common::RoadmapLimits::default());
        for operation in [
            PreviewOperation::Lookup,
            PreviewOperation::Initialize,
            PreviewOperation::Render,
            PreviewOperation::Resize,
            PreviewOperation::Input,
            PreviewOperation::Unload,
        ] {
            assert!(!policy.for_operation(operation).is_zero());
        }
        assert_ne!(
            policy.for_operation(PreviewOperation::Input),
            policy.for_operation(PreviewOperation::Render)
        );
    }
}
