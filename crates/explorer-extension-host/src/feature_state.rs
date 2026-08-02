//! Desired extension-feature state and pure effective-state resolution.

/// A persisted user preference for a scope.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DesiredStateV1 {
    /// The scope is requested to run when its other requirements are satisfied.
    #[default]
    Enabled,
    /// The scope is requested to remain disabled.
    Disabled,
}

/// The current effective state exposed to callers and the options UI.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EffectiveFeatureStateV1 {
    /// The feature may run.
    #[default]
    Enabled,
    /// Desired state disables the feature or one of its parents.
    Disabled,
    /// A runtime drain is in progress.
    Disabling,
    /// A restart is required before the requested state can take effect.
    PendingRestart,
    /// A dependency, compatibility, or diagnostic requirement prevents use.
    Blocked,
    /// A runtime failure prevents use.
    Faulted,
}

/// Desired state only. Effective runtime state is deliberately not persisted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FeatureStateStoreV1 {
    global_desired: DesiredStateV1,
}

impl FeatureStateStoreV1 {
    /// Creates the default enabled global desired state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            global_desired: DesiredStateV1::Enabled,
        }
    }

    /// Returns the persisted global desired state.
    #[must_use]
    pub const fn global_desired(&self) -> DesiredStateV1 {
        self.global_desired
    }

    /// Updates the persisted global desired state.
    pub fn set_global_desired(&mut self, desired: DesiredStateV1) {
        self.global_desired = desired;
    }
}

/// Pure resolver seam. Runtime integration remains a separate lifecycle task.
#[derive(Clone, Copy, Debug, Default)]
pub struct EffectiveFeatureResolverV1;

impl EffectiveFeatureResolverV1 {
    /// Resolves the global desired state without performing any runtime action.
    #[must_use]
    pub const fn resolve_global(store: &FeatureStateStoreV1) -> EffectiveFeatureStateV1 {
        match store.global_desired() {
            DesiredStateV1::Enabled => EffectiveFeatureStateV1::Enabled,
            DesiredStateV1::Disabled => EffectiveFeatureStateV1::Disabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DesiredStateV1, EffectiveFeatureResolverV1, EffectiveFeatureStateV1, FeatureStateStoreV1};

    #[test]
    fn global_desired_state_resolves_without_runtime_side_effects() {
        let mut store = FeatureStateStoreV1::new();
        assert_eq!(
            EffectiveFeatureResolverV1::resolve_global(&store),
            EffectiveFeatureStateV1::Enabled
        );
        store.set_global_desired(DesiredStateV1::Disabled);
        assert_eq!(
            EffectiveFeatureResolverV1::resolve_global(&store),
            EffectiveFeatureStateV1::Disabled
        );
    }
}
