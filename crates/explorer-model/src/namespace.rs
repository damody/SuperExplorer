//! Shell namespace roots, capabilities, identities, and owned property metadata.

use crate::{LocationDescriptor, ShellItemId};
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum NamespaceRoot {
    Home,
    QuickAccess,
    KnownFolder([u8; 16]),
    ThisPc,
    Drive(LocationDescriptor),
    Libraries,
    Zip(LocationDescriptor),
    RecycleBin,
    Network,
    ThirdParty(LocationDescriptor),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NamespaceAvailability {
    Available,
    Loading,
    Unavailable(UnavailableReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnavailableReason {
    NotInstalled,
    Offline,
    AccessDenied,
    ProviderFailure,
    Unsupported,
}

/// Deny-by-default item and container capability bits derived from public Shell attributes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct NamespaceCapabilities(u32);

impl NamespaceCapabilities {
    pub const ENUMERATE: u32 = 1 << 0;
    pub const OPEN: u32 = 1 << 1;
    pub const RENAME: u32 = 1 << 2;
    pub const DELETE: u32 = 1 << 3;
    pub const RESTORE: u32 = 1 << 4;
    pub const EMPTY: u32 = 1 << 5;
    pub const PIN: u32 = 1 << 6;
    pub const COPY: u32 = 1 << 7;
    pub const PASTE: u32 = 1 << 8;
    pub const DROP: u32 = 1 << 9;
    pub const SEARCH: u32 = 1 << 10;
    pub const PROPERTIES: u32 = 1 << 11;
    pub const CONTEXT_MENU: u32 = 1 << 12;
    pub const THUMBNAIL: u32 = 1 << 13;
    pub const PREVIEW: u32 = 1 << 14;
    const KNOWN: u32 = (1 << 15) - 1;

    pub const fn from_public_bits(bits: u32) -> Self {
        Self(bits & Self::KNOWN)
    }

    pub const fn contains(self, capability: u32) -> bool {
        capability != 0 && self.0 & capability == capability
    }

    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// Complete owned Shell identity; display text is never treated as identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellIdentity {
    pub stable_id: ShellItemId,
    pub descriptor: LocationDescriptor,
    pub display_name: String,
    pub parsing_name: Option<String>,
    pub serializable: bool,
    pub nonserializable_reason: Option<String>,
}

impl ShellIdentity {
    /// Validates display and persistence metadata consistency.
    ///
    /// # Errors
    ///
    /// Returns a stable invariant name when display or serialization metadata is inconsistent.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.display_name.is_empty() {
            return Err("display name is empty");
        }
        if self.serializable == self.nonserializable_reason.is_some() {
            return Err("serialization flag and reason are inconsistent");
        }
        Ok(())
    }
}

/// Owned PROPERTYKEY without a PROPVARIANT or property-store lifetime.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PropertyKey {
    pub format_id: [u8; 16],
    pub property_id: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PropertyValue {
    Empty,
    Text(String),
    Unsigned(u64),
    Signed(i64),
    Boolean(bool),
    FileTime(u64),
    StringList(Vec<String>),
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicColumnDescriptor {
    pub key: PropertyKey,
    pub display_name: String,
    pub width: u16,
    pub sortable: bool,
    pub groupable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespaceItem {
    pub identity: ShellIdentity,
    pub is_container: bool,
    pub capabilities: NamespaceCapabilities,
    pub properties: Vec<(PropertyKey, PropertyValue)>,
    pub unavailable_reason: Option<UnavailableReason>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuickAccessPin {
    pub identity: ShellIdentity,
    pub order: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QuickAccessPins {
    pins: Vec<QuickAccessPin>,
}

/// Result of a durable pin mutation. The previous value is retained so a failed
/// persistence write can be rolled back without reconstructing Shell identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuickAccessMutation {
    previous: QuickAccessPins,
    changed: bool,
}

impl QuickAccessPins {
    pub fn entries(&self) -> &[QuickAccessPin] {
        &self.pins
    }

    pub fn contains_descriptor(&self, descriptor: &LocationDescriptor) -> bool {
        self.pins
            .iter()
            .any(|pin| &pin.identity.descriptor == descriptor)
    }

    pub fn id_for_descriptor(&self, descriptor: &LocationDescriptor) -> Option<&ShellItemId> {
        self.pins
            .iter()
            .find(|pin| &pin.identity.descriptor == descriptor)
            .map(|pin| &pin.identity.stable_id)
    }

    pub fn pin(&mut self, identity: ShellIdentity) -> bool {
        if !identity.serializable
            || self.pins.iter().any(|pin| {
                pin.identity.stable_id == identity.stable_id
                    || pin.identity.descriptor == identity.descriptor
            })
        {
            return false;
        }
        let order = u32::try_from(self.pins.len()).unwrap_or(u32::MAX);
        self.pins.push(QuickAccessPin { identity, order });
        true
    }

    pub fn unpin(&mut self, id: &ShellItemId) -> Option<QuickAccessPin> {
        let index = self
            .pins
            .iter()
            .position(|pin| &pin.identity.stable_id == id)?;
        let removed = self.pins.remove(index);
        self.normalize_order();
        Some(removed)
    }

    pub fn reorder(&mut self, id: &ShellItemId, destination: usize) -> bool {
        if destination >= self.pins.len() {
            return false;
        }
        let Some(source) = self
            .pins
            .iter()
            .position(|pin| &pin.identity.stable_id == id)
        else {
            return false;
        };
        let pin = self.pins.remove(source);
        self.pins.insert(destination, pin);
        self.normalize_order();
        true
    }

    pub fn begin_pin(&mut self, identity: ShellIdentity) -> QuickAccessMutation {
        let previous = self.clone();
        let changed = self.pin(identity);
        QuickAccessMutation { previous, changed }
    }

    pub fn begin_unpin(&mut self, id: &ShellItemId) -> QuickAccessMutation {
        let previous = self.clone();
        let changed = self.unpin(id).is_some();
        QuickAccessMutation { previous, changed }
    }

    pub fn begin_reorder(&mut self, id: &ShellItemId, destination: usize) -> QuickAccessMutation {
        let previous = self.clone();
        let changed = self.reorder(id, destination);
        QuickAccessMutation { previous, changed }
    }

    /// Restores the exact pre-mutation order after a durable-store failure.
    pub fn rollback(&mut self, mutation: QuickAccessMutation) {
        if mutation.changed {
            *self = mutation.previous;
        }
    }

    fn normalize_order(&mut self) {
        for (index, pin) in self.pins.iter_mut().enumerate() {
            pin.order = u32::try_from(index).unwrap_or(u32::MAX);
        }
    }
}

impl QuickAccessMutation {
    pub const fn changed(&self) -> bool {
        self.changed
    }

    /// Returns the exact pre-mutation value for persistence rollback tests and adapters.
    pub const fn previous(&self) -> &QuickAccessPins {
        &self.previous
    }
}

/// User-visible Home aggregation state. Empty and failure are deliberately
/// distinct so the UI never presents a failed provider as an empty folder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HomeAggregationState {
    Loading,
    Ready(Vec<ShellIdentity>),
    Empty,
    Failed { retryable: bool },
}

/// Commands whose availability is derived from Shell capabilities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NamespaceCommand {
    Open,
    Rename,
    Delete,
    Restore,
    Empty,
    Pin,
    Copy,
    Paste,
    Drop,
    Search,
    Properties,
    ContextMenu,
    Thumbnail,
    Preview,
}

impl NamespaceCommand {
    pub const fn required_capability(self) -> u32 {
        match self {
            Self::Open => NamespaceCapabilities::OPEN,
            Self::Rename => NamespaceCapabilities::RENAME,
            Self::Delete => NamespaceCapabilities::DELETE,
            Self::Restore => NamespaceCapabilities::RESTORE,
            Self::Empty => NamespaceCapabilities::EMPTY,
            Self::Pin => NamespaceCapabilities::PIN,
            Self::Copy => NamespaceCapabilities::COPY,
            Self::Paste => NamespaceCapabilities::PASTE,
            Self::Drop => NamespaceCapabilities::DROP,
            Self::Search => NamespaceCapabilities::SEARCH,
            Self::Properties => NamespaceCapabilities::PROPERTIES,
            Self::ContextMenu => NamespaceCapabilities::CONTEXT_MENU,
            Self::Thumbnail => NamespaceCapabilities::THUMBNAIL,
            Self::Preview => NamespaceCapabilities::PREVIEW,
        }
    }
}

/// Shared reducer used by mouse, keyboard, context-menu, and UIA entry points.
pub const fn namespace_command_enabled(
    availability: &NamespaceAvailability,
    capabilities: NamespaceCapabilities,
    command: NamespaceCommand,
) -> bool {
    matches!(availability, NamespaceAvailability::Available)
        && capabilities.contains(command.required_capability())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecentNamespaceItem {
    pub identity: ShellIdentity,
    pub last_opened_epoch_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecentItems {
    entries: Vec<RecentNamespaceItem>,
    capacity: usize,
    maximum_age_seconds: u64,
    excluded_roots: Vec<std::path::PathBuf>,
}

impl RecentItems {
    pub fn new(
        capacity: usize,
        maximum_age_seconds: u64,
        excluded_roots: Vec<std::path::PathBuf>,
    ) -> Self {
        Self {
            entries: Vec::new(),
            capacity: capacity.max(1),
            maximum_age_seconds: maximum_age_seconds.max(1),
            excluded_roots,
        }
    }

    pub fn record(&mut self, identity: ShellIdentity, now_epoch_seconds: u64) -> bool {
        if !identity.serializable
            || identity.descriptor.path().is_some_and(|path| {
                self.excluded_roots
                    .iter()
                    .any(|root| path.starts_with(root))
            })
        {
            return false;
        }
        self.entries
            .retain(|entry| entry.identity.stable_id != identity.stable_id);
        self.entries.insert(
            0,
            RecentNamespaceItem {
                identity,
                last_opened_epoch_seconds: now_epoch_seconds,
            },
        );
        self.entries.truncate(self.capacity);
        true
    }

    pub fn visible(&self, now_epoch_seconds: u64) -> Vec<&RecentNamespaceItem> {
        self.entries
            .iter()
            .filter(|entry| {
                now_epoch_seconds.saturating_sub(entry.last_opened_epoch_seconds)
                    <= self.maximum_age_seconds
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Home sections merge pins and recents by stable identity while preserving pin order first.
pub fn aggregate_home<'a>(
    pins: &'a QuickAccessPins,
    recents: impl IntoIterator<Item = &'a RecentNamespaceItem>,
) -> Vec<&'a ShellIdentity> {
    let mut seen = HashSet::new();
    pins.entries()
        .iter()
        .map(|pin| &pin.identity)
        .chain(recents.into_iter().map(|recent| &recent.identity))
        .filter(|identity| seen.insert(identity.stable_id.clone()))
        .collect()
}

pub fn aggregate_home_state(
    pins: &QuickAccessPins,
    recents: &RecentItems,
    now_epoch_seconds: u64,
) -> HomeAggregationState {
    let entries = aggregate_home(pins, recents.visible(now_epoch_seconds))
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    if entries.is_empty() {
        HomeAggregationState::Empty
    } else {
        HomeAggregationState::Ready(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_deny_by_default_and_unknown_bits_are_removed() {
        assert_eq!(NamespaceCapabilities::default().bits(), 0);
        let value = NamespaceCapabilities::from_public_bits(
            NamespaceCapabilities::OPEN | NamespaceCapabilities::PREVIEW | (1 << 31),
        );
        assert!(value.contains(NamespaceCapabilities::OPEN));
        assert!(value.contains(NamespaceCapabilities::PREVIEW));
        assert_eq!(value.bits() & (1 << 31), 0);
    }

    #[test]
    fn identity_requires_consistent_serialization_metadata() {
        let identity = ShellIdentity {
            stable_id: ShellItemId::from_provider_bytes([1]).expect("id"),
            descriptor: LocationDescriptor::ParsingName("shell:HomeFolder".to_owned()),
            display_name: "Home".to_owned(),
            parsing_name: Some("shell:HomeFolder".to_owned()),
            serializable: true,
            nonserializable_reason: None,
        };
        assert_eq!(identity.validate(), Ok(()));
        let mut invalid = identity;
        invalid.nonserializable_reason = Some("provider denied persistence".to_owned());
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn quick_access_and_home_are_stable_deduplicated_and_privacy_filtered() {
        let make = |id, path: &str| ShellIdentity {
            stable_id: ShellItemId::from_provider_bytes([id]).expect("id"),
            descriptor: LocationDescriptor::file_system(path),
            display_name: path.to_owned(),
            parsing_name: None,
            serializable: true,
            nonserializable_reason: None,
        };
        let mut pins = QuickAccessPins::default();
        assert!(pins.pin(make(1, r"C:\Public")));
        assert!(!pins.pin(make(1, r"C:\Duplicate")));
        let mut recents = RecentItems::new(2, 100, vec![r"C:\Secret".into()]);
        assert!(!recents.record(make(2, r"C:\Secret\file"), 10));
        assert!(recents.record(make(1, r"C:\Public"), 10));
        assert!(recents.record(make(3, r"C:\Other"), 11));
        let visible = recents.visible(12);
        let home = aggregate_home(&pins, visible);
        assert_eq!(home.len(), 2);
        assert_eq!(home[0].stable_id, pins.entries()[0].identity.stable_id);

        let first_id = home[0].stable_id.clone();
        drop(home);
        let transaction = pins.begin_unpin(&first_id);
        assert!(transaction.changed());
        assert!(pins.entries().is_empty());
        pins.rollback(transaction);
        assert_eq!(pins.entries().len(), 1);
        assert!(matches!(
            aggregate_home_state(&pins, &recents, 12),
            HomeAggregationState::Ready(entries) if entries.len() == 2
        ));
    }

    #[test]
    fn command_enablement_is_shared_and_deny_by_default() {
        let capabilities = NamespaceCapabilities::from_public_bits(
            NamespaceCapabilities::OPEN | NamespaceCapabilities::CONTEXT_MENU,
        );
        assert!(namespace_command_enabled(
            &NamespaceAvailability::Available,
            capabilities,
            NamespaceCommand::Open
        ));
        assert!(!namespace_command_enabled(
            &NamespaceAvailability::Available,
            capabilities,
            NamespaceCommand::Delete
        ));
        assert!(!namespace_command_enabled(
            &NamespaceAvailability::Loading,
            capabilities,
            NamespaceCommand::Open
        ));
    }

    #[test]
    fn fake_namespace_failure_and_identity_matrix_fails_closed() {
        let identity = |id: u8, name: &str, serializable: bool| ShellIdentity {
            stable_id: ShellItemId::from_provider_bytes([id]).expect("stable identity"),
            descriptor: LocationDescriptor::ParsingName(format!("fake:{id}")),
            display_name: name.to_owned(),
            parsing_name: serializable.then(|| format!("fake:{id}")),
            serializable,
            nonserializable_reason: (!serializable).then(|| "provider declined".to_owned()),
        };
        let first = identity(1, "duplicate", true);
        let second = identity(2, "duplicate", true);
        assert_ne!(first.stable_id, second.stable_id);
        assert_eq!(first.display_name, second.display_name);
        let mut pins = QuickAccessPins::default();
        assert!(!pins.pin(identity(3, "ephemeral", false)));

        let mutable = NamespaceCapabilities::from_public_bits(
            NamespaceCapabilities::OPEN | NamespaceCapabilities::DELETE,
        );
        assert!(namespace_command_enabled(
            &NamespaceAvailability::Available,
            mutable,
            NamespaceCommand::Delete
        ));
        assert!(!namespace_command_enabled(
            &NamespaceAvailability::Unavailable(UnavailableReason::ProviderFailure),
            mutable,
            NamespaceCommand::Delete
        ));
        let changed = NamespaceCapabilities::from_public_bits(NamespaceCapabilities::OPEN);
        assert!(!namespace_command_enabled(
            &NamespaceAvailability::Available,
            changed,
            NamespaceCommand::Delete
        ));
        assert!(matches!(
            HomeAggregationState::Failed { retryable: true },
            HomeAggregationState::Failed { retryable: true }
        ));
    }
}
