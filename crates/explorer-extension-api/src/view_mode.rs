//! Frozen, data-only dynamic view registration contract.

use abi_stable::{
    StableAbi,
    std_types::{RString, RVec},
};

use std::collections::HashSet;

use crate::StableIdV1;

macro_rules! wire_enum {
    ($name:ident { $($constant:ident = $value:expr),+ $(,)? }) => {
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
        pub struct $name(u32);
        impl $name {
            $(pub const $constant: Self = Self($value);)+
            #[must_use] pub const fn into_raw(self) -> u32 { self.0 }
            #[must_use] pub const fn is_known(self) -> bool { matches!(self.0, $($value)|+) }
        }
    };
}

wire_enum!(ViewIconV1 { GRID = 1, TREE_MAP = 2, LIST = 3 });
wire_enum!(ViewSelectionCapabilityV1 { NONE = 1, SINGLE = 2, MULTIPLE = 3 });
wire_enum!(ViewSelectionOperationV1 { REPLACE = 1, ADD = 2, REMOVE = 3, TOGGLE = 4 });
wire_enum!(ViewNavigationOperationV1 { OPEN = 1, ENTER = 2, OPEN_NEW_TAB = 3, REVEAL = 4 });

/// Host-minted identity for one immutable extension-view snapshot.
///
/// Location and refresh generations are deliberately separate on the public
/// wire even when a host currently advances them together. Extensions must
/// echo all three values; none of them grants access to a live tab model.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct ViewSnapshotIdentityV1 {
    pub location_generation: u64,
    pub refresh_generation: u64,
    pub render_revision: u64,
}

impl ViewSnapshotIdentityV1 {
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.location_generation != 0 && self.refresh_generation != 0 && self.render_revision != 0
    }
}

/// Data-only selection request delivered to the host-owned selection bridge.
/// Node IDs are meaningful only for the echoed immutable snapshot.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct ViewSelectionRequestV1 {
    pub snapshot: ViewSnapshotIdentityV1,
    pub operation: ViewSelectionOperationV1,
    pub node_ids: RVec<StableIdV1>,
}

impl ViewSelectionRequestV1 {
    /// Host-side authorization against the one immutable snapshot that minted
    /// the opaque node IDs. Unknown, duplicate, and stale IDs are rejected.
    pub fn validate_for_snapshot(
        &self,
        current: ViewSnapshotIdentityV1,
        known_node_ids: &HashSet<StableIdV1>,
    ) -> bool {
        if self.snapshot != current
            || !current.is_valid()
            || !self.operation.is_known()
            || self.node_ids.is_empty()
        {
            return false;
        }
        let requested = self.node_ids.iter().copied().collect::<HashSet<_>>();
        requested.len() == self.node_ids.len()
            && requested
                .iter()
                .all(|node_id| known_node_ids.contains(node_id))
    }
}

/// Data-only navigation request delivered to the host-owned navigation
/// adapter. The adapter resolves and authorizes the opaque node ID before
/// dispatching through the normal tab model/open policy.
#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct NavigationRequestV1 {
    pub snapshot: ViewSnapshotIdentityV1,
    pub operation: ViewNavigationOperationV1,
    pub node_id: StableIdV1,
}

impl NavigationRequestV1 {
    #[must_use]
    pub const fn is_well_formed(&self) -> bool {
        self.snapshot.is_valid() && self.operation.is_known() && self.node_id.is_valid()
    }

    /// Host-side authorization for a node minted by the current snapshot.
    #[must_use]
    pub fn is_authorized_for(
        &self,
        current: ViewSnapshotIdentityV1,
        known_node_ids: &HashSet<StableIdV1>,
    ) -> bool {
        self.is_well_formed() && self.snapshot == current && known_node_ids.contains(&self.node_id)
    }
}

/// Host-owned lifecycle states. These are not sent to a renderer callback;
/// they make ordered create/focus/suspend/close handling explicit in adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewLifecycleStateV1 {
    Created,
    Active,
    Focused,
    Suspended,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewLifecycleEventV1 {
    Activate,
    Render,
    Focus,
    Blur,
    LocationChanged,
    SelectionChanged,
    Refresh,
    Suspend,
    Resume,
    Close,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewLifecycleV1 {
    state: ViewLifecycleStateV1,
}

impl Default for ViewLifecycleV1 {
    fn default() -> Self {
        Self {
            state: ViewLifecycleStateV1::Created,
        }
    }
}

impl ViewLifecycleV1 {
    #[must_use]
    pub const fn state(self) -> ViewLifecycleStateV1 {
        self.state
    }

    pub fn transition(&mut self, event: ViewLifecycleEventV1) -> bool {
        use ViewLifecycleEventV1 as Event;
        use ViewLifecycleStateV1 as State;
        let next = match (self.state, event) {
            (State::Created, Event::Activate) | (State::Suspended, Event::Resume) => State::Active,
            (State::Active, Event::Focus) => State::Focused,
            (State::Focused, Event::Blur) => State::Active,
            (State::Active | State::Focused, Event::Suspend) => State::Suspended,
            (State::Created | State::Active | State::Focused | State::Suspended, Event::Close) => {
                State::Closed
            }
            (
                State::Active | State::Focused,
                Event::Render | Event::LocationChanged | Event::SelectionChanged | Event::Refresh,
            ) => self.state,
            _ => return false,
        };
        self.state = next;
        true
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct ViewLocationKindsV1(u32);

impl ViewLocationKindsV1 {
    pub const FILESYSTEM: Self = Self(1);
    pub const VIRTUAL: Self = Self(2);
    pub const CONTAINER: Self = Self(4);
    pub const ALL: Self = Self(7);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.0 != 0 && self.0 & !Self::ALL.0 == 0
    }
}

#[repr(C)]
#[derive(Clone, Debug, StableAbi)]
pub struct ViewModeRegistrationV1 {
    pub id: RString,
    pub display_name: RString,
    pub icon: ViewIconV1,
    pub locations: ViewLocationKindsV1,
    pub priority: i16,
    pub selection: ViewSelectionCapabilityV1,
    pub factory_interface_id: StableIdV1,
    pub factory_contribution_id: RString,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewModeRegistrationErrorV1 {
    InvalidId,
    InvalidDisplayName,
    InvalidSemantic,
    InvalidFactory,
}

impl ViewModeRegistrationV1 {
    pub fn validate(&self) -> Result<(), ViewModeRegistrationErrorV1> {
        if !valid_id(self.id.as_str()) {
            return Err(ViewModeRegistrationErrorV1::InvalidId);
        }
        if self.display_name.trim().is_empty()
            || self.display_name.len() > 256
            || self.display_name.chars().any(char::is_control)
        {
            return Err(ViewModeRegistrationErrorV1::InvalidDisplayName);
        }
        if !self.icon.is_known() || !self.locations.is_valid() || !self.selection.is_known() {
            return Err(ViewModeRegistrationErrorV1::InvalidSemantic);
        }
        if !self.factory_interface_id.is_valid() || !valid_id(self.factory_contribution_id.as_str())
        {
            return Err(ViewModeRegistrationErrorV1::InvalidFactory);
        }
        Ok(())
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphanumeric() || (index > 0 && matches!(byte, b'.' | b'_' | b'-' | b':'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_registration_is_owned_data_without_author_callback_fields() {
        let registration = ViewModeRegistrationV1 {
            id: "size-map".into(),
            display_name: "Size Map".into(),
            icon: ViewIconV1::TREE_MAP,
            locations: ViewLocationKindsV1::FILESYSTEM,
            priority: 10,
            selection: ViewSelectionCapabilityV1::MULTIPLE,
            factory_interface_id: StableIdV1::new(crate::EXTENSION_ID_NAMESPACE_V1, 7),
            factory_contribution_id: "size-map".into(),
        };
        assert_eq!(registration.validate(), Ok(()));
        let source = include_str!("view_mode.rs");
        let contract = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(!contract.contains("extern \"C\""));
        assert!(!contract.contains("dyn Fn"));
        assert!(!contract.contains("gpui::"));
    }

    #[test]
    fn lifecycle_orders_focus_suspend_resume_and_close() {
        let mut lifecycle = ViewLifecycleV1::default();
        assert!(!lifecycle.transition(ViewLifecycleEventV1::Render));
        assert!(lifecycle.transition(ViewLifecycleEventV1::Activate));
        assert!(lifecycle.transition(ViewLifecycleEventV1::Render));
        assert!(lifecycle.transition(ViewLifecycleEventV1::Focus));
        assert!(lifecycle.transition(ViewLifecycleEventV1::SelectionChanged));
        assert!(lifecycle.transition(ViewLifecycleEventV1::Blur));
        assert!(lifecycle.transition(ViewLifecycleEventV1::Refresh));
        assert!(lifecycle.transition(ViewLifecycleEventV1::LocationChanged));
        assert!(lifecycle.transition(ViewLifecycleEventV1::Suspend));
        assert!(!lifecycle.transition(ViewLifecycleEventV1::Render));
        assert!(lifecycle.transition(ViewLifecycleEventV1::Resume));
        assert!(lifecycle.transition(ViewLifecycleEventV1::Close));
        assert_eq!(lifecycle.state(), ViewLifecycleStateV1::Closed);
        assert!(!lifecycle.transition(ViewLifecycleEventV1::Activate));
    }

    #[test]
    fn selection_and_navigation_reject_unknown_duplicate_and_stale_ids() {
        let current = ViewSnapshotIdentityV1 {
            location_generation: 3,
            refresh_generation: 5,
            render_revision: 8,
        };
        let known = StableIdV1::new(crate::EXTENSION_ID_NAMESPACE_V1, 1);
        let unknown = StableIdV1::new(crate::EXTENSION_ID_NAMESPACE_V1, 2);
        let known_ids = HashSet::from([known]);
        let selection = |snapshot, node_ids| ViewSelectionRequestV1 {
            snapshot,
            operation: ViewSelectionOperationV1::REPLACE,
            node_ids: RVec::from(node_ids),
        };
        assert!(selection(current, vec![known]).validate_for_snapshot(current, &known_ids));
        assert!(!selection(current, vec![unknown]).validate_for_snapshot(current, &known_ids));
        assert!(!selection(current, vec![known, known]).validate_for_snapshot(current, &known_ids));
        let stale = ViewSnapshotIdentityV1 {
            render_revision: 7,
            ..current
        };
        assert!(!selection(stale, vec![known]).validate_for_snapshot(current, &known_ids));

        let request = NavigationRequestV1 {
            snapshot: current,
            operation: ViewNavigationOperationV1::ENTER,
            node_id: known,
        };
        assert!(request.is_authorized_for(current, &known_ids));
        assert!(
            !NavigationRequestV1 {
                snapshot: stale,
                ..request.clone()
            }
            .is_authorized_for(current, &known_ids)
        );
        assert!(
            !NavigationRequestV1 {
                node_id: unknown,
                ..request
            }
            .is_authorized_for(current, &known_ids)
        );
    }
}
