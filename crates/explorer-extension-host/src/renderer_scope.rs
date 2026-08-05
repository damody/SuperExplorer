//! Host-owned scoped action and invalidation handles for extension renderers.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use explorer_extension_api::StableIdV1;

pub const MAX_RENDERER_SCOPED_ACTIONS_V1: usize = 256;
pub const MAX_RENDERER_SCOPED_INVALIDATIONS_V1: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendererScopedActionV1 {
    pub package_id: String,
    pub render_generation: u64,
    pub request_generation: u64,
    pub item_id: StableIdV1,
    pub action_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendererScopedInvalidationV1 {
    pub package_id: String,
    pub render_generation: u64,
    pub request_generation: u64,
    pub item_id: StableIdV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RendererScopeSubmitErrorV1 {
    Closed,
    CrossPackage,
    Stale,
    Invalid,
    Capacity,
}

#[derive(Debug)]
struct RendererScopeStateV1 {
    package_id: String,
    render_generation: u64,
    request_generation: u64,
    open: bool,
    actions: VecDeque<RendererScopedActionV1>,
    invalidations: VecDeque<RendererScopedInvalidationV1>,
}

#[derive(Clone, Debug)]
pub struct RendererScopedActionSinkV1(Arc<Mutex<RendererScopeStateV1>>);

#[derive(Clone, Debug)]
pub struct RendererScopedInvalidationHandleV1(Arc<Mutex<RendererScopeStateV1>>);

#[derive(Debug)]
pub struct RendererScopeV1(Arc<Mutex<RendererScopeStateV1>>);

impl RendererScopeV1 {
    pub fn open(
        package_id: impl Into<String>,
        render_generation: u64,
        request_generation: u64,
    ) -> Result<Self, RendererScopeSubmitErrorV1> {
        let package_id = package_id.into();
        if package_id.is_empty() || render_generation == 0 || request_generation == 0 {
            return Err(RendererScopeSubmitErrorV1::Invalid);
        }
        Ok(Self(Arc::new(Mutex::new(RendererScopeStateV1 {
            package_id,
            render_generation,
            request_generation,
            open: true,
            actions: VecDeque::new(),
            invalidations: VecDeque::new(),
        }))))
    }

    #[must_use]
    pub fn action_sink(&self) -> RendererScopedActionSinkV1 {
        RendererScopedActionSinkV1(Arc::clone(&self.0))
    }

    #[must_use]
    pub fn invalidation_handle(&self) -> RendererScopedInvalidationHandleV1 {
        RendererScopedInvalidationHandleV1(Arc::clone(&self.0))
    }

    pub fn close(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.open = false;
            state.actions.clear();
            state.invalidations.clear();
        }
    }

    pub fn drain_actions(&self) -> Vec<RendererScopedActionV1> {
        self.0
            .lock()
            .map(|mut state| state.actions.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn drain_invalidations(&self) -> Vec<RendererScopedInvalidationV1> {
        self.0
            .lock()
            .map(|mut state| state.invalidations.drain(..).collect())
            .unwrap_or_default()
    }
}

fn validate_scope(
    state: &RendererScopeStateV1,
    package_id: &str,
    render_generation: u64,
    request_generation: u64,
) -> Result<(), RendererScopeSubmitErrorV1> {
    if !state.open {
        return Err(RendererScopeSubmitErrorV1::Closed);
    }
    if state.package_id != package_id {
        return Err(RendererScopeSubmitErrorV1::CrossPackage);
    }
    if state.render_generation != render_generation
        || state.request_generation != request_generation
    {
        return Err(RendererScopeSubmitErrorV1::Stale);
    }
    Ok(())
}

impl RendererScopedActionSinkV1 {
    pub fn submit(&self, action: RendererScopedActionV1) -> Result<(), RendererScopeSubmitErrorV1> {
        if !action.item_id.is_valid() || action.action_id.is_empty() || action.action_id.len() > 128
        {
            return Err(RendererScopeSubmitErrorV1::Invalid);
        }
        let mut state = self
            .0
            .lock()
            .map_err(|_| RendererScopeSubmitErrorV1::Closed)?;
        validate_scope(
            &state,
            &action.package_id,
            action.render_generation,
            action.request_generation,
        )?;
        if state.actions.len() >= MAX_RENDERER_SCOPED_ACTIONS_V1 {
            return Err(RendererScopeSubmitErrorV1::Capacity);
        }
        state.actions.push_back(action);
        Ok(())
    }
}

impl RendererScopedInvalidationHandleV1 {
    pub fn invalidate(
        &self,
        invalidation: RendererScopedInvalidationV1,
    ) -> Result<(), RendererScopeSubmitErrorV1> {
        if !invalidation.item_id.is_valid() {
            return Err(RendererScopeSubmitErrorV1::Invalid);
        }
        let mut state = self
            .0
            .lock()
            .map_err(|_| RendererScopeSubmitErrorV1::Closed)?;
        validate_scope(
            &state,
            &invalidation.package_id,
            invalidation.render_generation,
            invalidation.request_generation,
        )?;
        if state.invalidations.len() >= MAX_RENDERER_SCOPED_INVALIDATIONS_V1 {
            return Err(RendererScopeSubmitErrorV1::Capacity);
        }
        state.invalidations.push_back(invalidation);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> StableIdV1 {
        StableIdV1::new(explorer_extension_api::EXTENSION_ID_NAMESPACE_V1, 1)
    }

    #[test]
    fn scoped_handles_reject_cross_package_stale_and_retained_after_close() {
        let scope = RendererScopeV1::open("package", 3, 5).unwrap();
        let actions = scope.action_sink();
        let invalidations = scope.invalidation_handle();
        let action =
            |package: &str, render_generation, request_generation| RendererScopedActionV1 {
                package_id: package.into(),
                render_generation,
                request_generation,
                item_id: item(),
                action_id: "open".into(),
            };
        assert_eq!(actions.submit(action("package", 3, 5)), Ok(()));
        assert_eq!(
            actions.submit(action("other", 3, 5)),
            Err(RendererScopeSubmitErrorV1::CrossPackage)
        );
        assert_eq!(
            actions.submit(action("package", 4, 5)),
            Err(RendererScopeSubmitErrorV1::Stale)
        );
        assert_eq!(scope.drain_actions().len(), 1);
        assert_eq!(
            invalidations.invalidate(RendererScopedInvalidationV1 {
                package_id: "package".into(),
                render_generation: 3,
                request_generation: 5,
                item_id: item(),
            }),
            Ok(())
        );
        scope.close();
        assert!(scope.drain_invalidations().is_empty());
        assert_eq!(
            actions.submit(action("package", 3, 5)),
            Err(RendererScopeSubmitErrorV1::Closed)
        );
    }
}
