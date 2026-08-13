//! Pure per-tab navigation, directory, presentation, and selection state.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use explorer_common::{ExplorerError, RequestId};

use crate::{Generation, LocationDescriptor, RequestContext, RequestRejection, ShellItemId, TabId};

/// Reconstructable viewport position stored with a history entry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ViewAnchor {
    pub item: Option<ShellItemId>,
    pub offset_logical_pixels: i32,
}

/// One committed navigation location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEntry {
    pub location: LocationDescriptor,
    pub display_title: String,
    pub view_anchor: ViewAnchor,
}

impl HistoryEntry {
    /// Creates an entry at the top of its viewport.
    pub fn new(location: LocationDescriptor, display_title: impl Into<String>) -> Self {
        Self {
            location,
            display_title: display_title.into(),
            view_anchor: ViewAnchor::default(),
        }
    }
}

/// Independent Back/Forward history owned by one tab.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NavigationHistory {
    back: Vec<HistoryEntry>,
    current: Option<HistoryEntry>,
    forward: Vec<HistoryEntry>,
}

impl NavigationHistory {
    /// Creates history with one already-resolved location.
    pub fn with_initial(entry: HistoryEntry) -> Self {
        Self {
            current: Some(entry),
            ..Self::default()
        }
    }

    /// Reconstructs bounded committed stacks whose entries were resolved during startup.
    pub fn from_resolved_parts(
        back: Vec<HistoryEntry>,
        current: HistoryEntry,
        forward: Vec<HistoryEntry>,
    ) -> Self {
        Self {
            back,
            current: Some(current),
            forward,
        }
    }

    /// Returns the currently committed location.
    pub const fn current(&self) -> Option<&HistoryEntry> {
        self.current.as_ref()
    }

    /// Returns committed Back entries from oldest to nearest destination.
    pub fn back_entries(&self) -> &[HistoryEntry] {
        &self.back
    }

    /// Returns committed Forward entries from oldest stack entry to nearest destination.
    pub fn forward_entries(&self) -> &[HistoryEntry] {
        &self.forward
    }

    /// Returns whether Back is available.
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    /// Returns whether Forward is available.
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Returns the next Back destination without changing committed history.
    pub fn back_destination(&self) -> Option<&HistoryEntry> {
        self.back.last()
    }

    /// Returns a Back destination by one-based distance from the current entry.
    pub fn back_destination_at(&self, steps: usize) -> Option<&HistoryEntry> {
        (steps > 0)
            .then(|| self.back.len().checked_sub(steps))
            .flatten()
            .and_then(|index| self.back.get(index))
    }

    /// Returns the next Forward destination without changing committed history.
    pub fn forward_destination(&self) -> Option<&HistoryEntry> {
        self.forward.last()
    }

    /// Returns a Forward destination by one-based distance from the current entry.
    pub fn forward_destination_at(&self, steps: usize) -> Option<&HistoryEntry> {
        (steps > 0)
            .then(|| self.forward.len().checked_sub(steps))
            .flatten()
            .and_then(|index| self.forward.get(index))
    }

    /// Commits a successfully resolved new location.
    ///
    /// Committing the same descriptor is a Refresh and does not add history.
    pub fn commit_navigation(&mut self, entry: HistoryEntry) {
        if self.current.as_ref().map(|value| &value.location) == Some(&entry.location) {
            self.current = Some(entry);
            return;
        }
        if let Some(current) = self.current.replace(entry) {
            self.back.push(current);
        }
        self.forward.clear();
    }

    /// Moves to the previous committed location.
    pub fn go_back(&mut self) -> Option<&HistoryEntry> {
        let destination = self.back.pop()?;
        if let Some(current) = self.current.replace(destination) {
            self.forward.push(current);
        }
        self.current.as_ref()
    }

    /// Moves to the next committed location.
    pub fn go_forward(&mut self) -> Option<&HistoryEntry> {
        let destination = self.forward.pop()?;
        if let Some(current) = self.current.replace(destination) {
            self.back.push(current);
        }
        self.current.as_ref()
    }

    fn commit_back_steps(&mut self, resolved: HistoryEntry, steps: usize) -> bool {
        let Some(destination_index) = self.back.len().checked_sub(steps) else {
            return false;
        };
        if steps == 0
            || self
                .back
                .get(destination_index)
                .map(|entry| &entry.location)
                != Some(&resolved.location)
        {
            return false;
        }
        let mut crossed = self.back.split_off(destination_index);
        let _destination = crossed.remove(0);
        if let Some(current) = self.current.replace(resolved) {
            self.forward.push(current);
            self.forward.extend(crossed.into_iter().rev());
        }
        true
    }

    fn commit_forward_steps(&mut self, resolved: HistoryEntry, steps: usize) -> bool {
        let Some(destination_index) = self.forward.len().checked_sub(steps) else {
            return false;
        };
        if steps == 0
            || self
                .forward
                .get(destination_index)
                .map(|entry| &entry.location)
                != Some(&resolved.location)
        {
            return false;
        }
        let mut crossed = self.forward.split_off(destination_index);
        let _destination = crossed.remove(0);
        if let Some(current) = self.current.replace(resolved) {
            self.back.push(current);
            self.back.extend(crossed.into_iter().rev());
        }
        true
    }
}

/// Owned directory row with stable identity independent from presentation order.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct FileEntry {
    pub id: ShellItemId,
    pub display_name: String,
    pub location: LocationDescriptor,
    pub is_container: bool,
    pub metadata: FileEntryMetadata,
}

/// Owned presentation metadata returned by the Shell boundary with a directory row.
///
/// Display strings are resolved on the Shell STA so they follow the user's Windows locale and
/// registered file associations without carrying apartment-affine property-store interfaces into
/// the model or UI layers.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct FileEntryMetadata {
    pub modified_display: Option<String>,
    pub modified_sort_key: Option<u64>,
    pub created_display: Option<String>,
    pub created_sort_key: Option<u64>,
    pub size_bytes: Option<u64>,
    pub type_display: Option<String>,
    pub authors_display: Option<String>,
    pub tags_display: Option<String>,
    pub title_display: Option<String>,
    pub drive: Option<DriveMetadata>,
    pub filesystem_attributes: u32,
    pub unavailable_reason: Option<String>,
    pub namespace_capabilities: crate::NamespaceCapabilities,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum DriveKind {
    Removable,
    Fixed,
    Network,
    Optical,
    RamDisk,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum DriveAvailability {
    Available,
    NoMedia,
    Disconnected,
    AccessDenied,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct DriveMetadata {
    pub kind: DriveKind,
    pub availability: DriveAvailability,
    pub volume_label: Option<String>,
    pub filesystem_name: Option<String>,
    pub total_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
}

impl DriveMetadata {
    #[allow(
        clippy::cast_precision_loss,
        reason = "capacity bars need only a bounded visual ratio, not byte-exact floating-point precision"
    )]
    pub fn used_fraction(&self) -> Option<f32> {
        let total = self.total_bytes?;
        let available = self.available_bytes?;
        if total == 0 || available > total {
            return None;
        }
        Some((total - available) as f32 / total as f32)
    }

    pub fn is_low_space(&self) -> bool {
        matches!((self.available_bytes, self.total_bytes), (Some(available), Some(total)) if total > 0 && available <= 10 * 1024 * 1024 * 1024 && available.saturating_mul(10) <= total)
    }
}

/// Allocation-free text keys maintained alongside one directory entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEntrySortKeys {
    display_name: Arc<str>,
    type_display: Option<Arc<str>>,
}

impl FileEntrySortKeys {
    fn for_entry(entry: &FileEntry) -> Self {
        Self {
            display_name: Arc::from(entry.display_name.to_lowercase()),
            type_display: entry
                .metadata
                .type_display
                .as_deref()
                .map(str::to_lowercase)
                .map(Arc::from),
        }
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn type_display(&self) -> Option<&str> {
        self.type_display.as_deref()
    }
}

/// Stable-ID mutation applied to a directory presentation store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresentationChange {
    Inserted(ShellItemId),
    Updated(ShellItemId),
    Removed(ShellItemId),
    Unchanged,
}

/// Incrementally merged directory entries in deterministic presentation order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DirectorySnapshot {
    revision: u64,
    entries: Arc<Vec<FileEntry>>,
    indices: Arc<HashMap<ShellItemId, usize>>,
    sort_keys: Arc<Vec<FileEntrySortKeys>>,
}

impl DirectorySnapshot {
    /// Monotonic content revision used by cached presentation projections.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the entries in their current presentation order.
    pub fn entries(&self) -> &[FileEntry] {
        self.entries.as_slice()
    }

    /// Shares the immutable entry allocation with render-only consumers.
    pub fn shared_entries(&self) -> Arc<Vec<FileEntry>> {
        Arc::clone(&self.entries)
    }

    /// Returns precomputed allocation-free sort keys for one entry index.
    pub fn sort_keys(&self, index: usize) -> Option<&FileEntrySortKeys> {
        self.sort_keys.get(index)
    }

    /// Finds an entry by stable identity.
    pub fn get(&self, id: &ShellItemId) -> Option<&FileEntry> {
        self.indices
            .get(id)
            .and_then(|index| self.entries.get(*index))
    }

    /// Inserts a new identity or updates the existing row without changing its index.
    pub fn upsert(&mut self, entry: FileEntry) -> PresentationChange {
        let change = self.upsert_without_revision(entry);
        if change != PresentationChange::Unchanged {
            self.advance_revision();
        }
        change
    }

    /// Applies a provider batch as one presentation revision.
    pub fn upsert_batch(
        &mut self,
        entries: impl IntoIterator<Item = FileEntry>,
    ) -> Vec<PresentationChange> {
        let changes = entries
            .into_iter()
            .map(|entry| self.upsert_without_revision(entry))
            .collect::<Vec<_>>();
        if changes
            .iter()
            .any(|change| *change != PresentationChange::Unchanged)
        {
            self.advance_revision();
        }
        changes
    }

    fn upsert_without_revision(&mut self, entry: FileEntry) -> PresentationChange {
        if let Some(index) = self.indices.get(&entry.id).copied() {
            let existing = &self.entries[index];
            if *existing == entry {
                PresentationChange::Unchanged
            } else {
                let id = entry.id.clone();
                let keys = FileEntrySortKeys::for_entry(&entry);
                Arc::make_mut(&mut self.entries)[index] = entry;
                Arc::make_mut(&mut self.sort_keys)[index] = keys;
                PresentationChange::Updated(id)
            }
        } else {
            let id = entry.id.clone();
            let keys = FileEntrySortKeys::for_entry(&entry);
            Arc::make_mut(&mut self.entries).push(entry);
            Arc::make_mut(&mut self.sort_keys).push(keys);
            let index = self.entries.len() - 1;
            Arc::make_mut(&mut self.indices).insert(id.clone(), index);
            PresentationChange::Inserted(id)
        }
    }

    /// Removes an entry by stable identity rather than row index.
    pub fn remove(&mut self, id: &ShellItemId) -> PresentationChange {
        let Some(index) = self.indices.get(id).copied() else {
            return PresentationChange::Unchanged;
        };
        Arc::make_mut(&mut self.indices).remove(id);
        let removed = Arc::make_mut(&mut self.entries).remove(index).id;
        Arc::make_mut(&mut self.sort_keys).remove(index);
        for (offset, entry) in self.entries[index..].iter().enumerate() {
            Arc::make_mut(&mut self.indices).insert(entry.id.clone(), index + offset);
        }
        self.advance_revision();
        PresentationChange::Removed(removed)
    }

    /// Retains rows accepted by a complete refresh oracle.
    pub fn retain(&mut self, mut predicate: impl FnMut(&FileEntry) -> bool) {
        let retained = self
            .entries
            .iter()
            .zip(self.sort_keys.iter())
            .filter(|(entry, _)| predicate(entry))
            .map(|(entry, keys)| (entry.clone(), keys.clone()))
            .collect::<Vec<_>>();
        if retained.len() == self.entries.len() {
            return;
        }
        let (entries, sort_keys): (Vec<_>, Vec<_>) = retained.into_iter().unzip();
        self.entries = Arc::new(entries);
        self.sort_keys = Arc::new(sort_keys);
        self.rebuild_indices();
        self.advance_revision();
    }

    fn rebuild_indices(&mut self) {
        let indices = Arc::make_mut(&mut self.indices);
        indices.clear();
        indices.extend(
            self.entries
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.id.clone(), index)),
        );
    }

    fn advance_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

/// Selection expressed only in stable item identities.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionModel {
    selected: Arc<HashSet<ShellItemId>>,
    focused: Option<ShellItemId>,
    anchor: Option<ShellItemId>,
}

impl SelectionModel {
    pub fn clear(&mut self) {
        Arc::make_mut(&mut self.selected).clear();
        self.focused = None;
        self.anchor = None;
    }

    /// Replaces selection with one item and focuses it.
    pub fn select_only(&mut self, id: ShellItemId) {
        let selected = Arc::make_mut(&mut self.selected);
        selected.clear();
        selected.insert(id.clone());
        self.focused = Some(id.clone());
        self.anchor = Some(id);
    }

    pub fn select_additive(&mut self, id: ShellItemId) {
        Arc::make_mut(&mut self.selected).insert(id.clone());
        self.focused = Some(id.clone());
        self.anchor.get_or_insert(id);
    }

    /// Moves keyboard focus without changing the selected set or range anchor.
    pub fn focus_only(&mut self, id: ShellItemId) {
        self.focused = Some(id);
    }

    /// Toggles one item without discarding the established range anchor.
    pub fn toggle(&mut self, id: ShellItemId) {
        let selected = Arc::make_mut(&mut self.selected);
        if !selected.remove(&id) {
            selected.insert(id.clone());
        }
        self.focused = Some(id.clone());
        self.anchor.get_or_insert(id);
    }

    /// Selects an inclusive stable-id range in the caller's presentation order.
    pub fn select_range(&mut self, order: &[ShellItemId], target: ShellItemId, additive: bool) {
        let anchor = self.anchor.as_ref().unwrap_or(&target);
        let Some(anchor_index) = order.iter().position(|id| id == anchor) else {
            self.select_only(target);
            return;
        };
        let Some(target_index) = order.iter().position(|id| id == &target) else {
            return;
        };
        if !additive {
            Arc::make_mut(&mut self.selected).clear();
        }
        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        Arc::make_mut(&mut self.selected).extend(order[start..=end].iter().cloned());
        self.focused = Some(target);
    }

    pub fn select_all(&mut self, order: &[ShellItemId]) {
        Arc::make_mut(&mut self.selected).extend(order.iter().cloned());
        if self.focused.is_none() {
            self.focused = order.first().cloned();
        }
        if self.anchor.is_none() {
            self.anchor = self.focused.clone();
        }
    }

    pub fn invert(&mut self, order: &[ShellItemId]) {
        let selected = Arc::make_mut(&mut self.selected);
        for id in order {
            if !selected.remove(id) {
                selected.insert(id.clone());
            }
        }
        self.focused = order.first().cloned();
        self.anchor = self.focused.clone();
    }

    /// Returns whether an identity is selected.
    pub fn contains(&self, id: &ShellItemId) -> bool {
        self.selected.contains(id)
    }

    /// Returns the number of selected identities.
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    /// Returns whether selection is empty.
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ShellItemId> {
        self.selected.iter()
    }

    /// Returns the stable identity that owns keyboard focus.
    pub fn focused(&self) -> Option<&ShellItemId> {
        self.focused.as_ref()
    }

    pub fn anchor(&self) -> Option<&ShellItemId> {
        self.anchor.as_ref()
    }

    /// Removes identities no longer present while preserving rename/update identities.
    pub fn reconcile(&mut self, snapshot: &DirectorySnapshot) {
        Arc::make_mut(&mut self.selected).retain(|id| snapshot.get(id).is_some());
        if self
            .focused
            .as_ref()
            .is_some_and(|id| snapshot.get(id).is_none())
        {
            self.focused = None;
        }
        if self
            .anchor
            .as_ref()
            .is_some_and(|id| snapshot.get(id).is_none())
        {
            self.anchor = self.focused.clone();
        }
    }
}

/// Per-tab directory loading terminal state.
#[derive(Clone, Debug, Default)]
pub enum DirectoryState {
    #[default]
    Idle,
    Loading {
        request: RequestContext,
        snapshot: DirectorySnapshot,
        seen: HashSet<ShellItemId>,
    },
    Ready(DirectorySnapshot),
    Error {
        error: ExplorerError,
        previous: DirectorySnapshot,
    },
}

impl DirectoryState {
    /// Validates an event against the sole currently loading request.
    ///
    /// # Errors
    ///
    /// Returns a correlation rejection when no request is loading or the event is stale.
    pub fn accepts(&self, event: &RequestContext) -> Result<(), RequestRejection> {
        let Self::Loading { request, .. } = self else {
            return Err(RequestRejection::RequestId);
        };
        request.validate_event(event)
    }

    /// Returns the currently presentable snapshot for loading, ready, or recoverable error states.
    pub const fn snapshot(&self) -> Option<&DirectorySnapshot> {
        match self {
            Self::Loading { snapshot, .. } | Self::Ready(snapshot) => Some(snapshot),
            Self::Error { previous, .. } => Some(previous),
            Self::Idle => None,
        }
    }

    /// Starts a new generation and cancels any previous loading request.
    pub fn begin(&mut self, request: RequestContext, preserve_snapshot: bool) {
        let previous_snapshot = match std::mem::replace(self, Self::Idle) {
            Self::Loading {
                request: previous,
                snapshot,
                ..
            } => {
                previous.cancellation.cancel();
                snapshot
            }
            Self::Ready(snapshot)
            | Self::Error {
                previous: snapshot, ..
            } => snapshot,
            Self::Idle => DirectorySnapshot::default(),
        };
        let snapshot = if preserve_snapshot {
            previous_snapshot
        } else {
            DirectorySnapshot::default()
        };
        *self = Self::Loading {
            request,
            snapshot,
            seen: HashSet::new(),
        };
    }

    /// Merges a bounded batch if it belongs to the current request.
    ///
    /// # Errors
    ///
    /// Returns a stale/cancelled rejection without mutating current state.
    pub fn merge_batch(
        &mut self,
        event: &RequestContext,
        entries: impl IntoIterator<Item = FileEntry>,
    ) -> Result<Vec<PresentationChange>, RequestRejection> {
        let Self::Loading {
            request,
            snapshot,
            seen,
        } = self
        else {
            return Err(RequestRejection::RequestId);
        };
        request.validate_event(event)?;
        let entries = entries
            .into_iter()
            .inspect(|entry| {
                seen.insert(entry.id.clone());
            })
            .collect::<Vec<_>>();
        Ok(snapshot.upsert_batch(entries))
    }

    /// Finishes the current request and publishes its snapshot.
    ///
    /// # Errors
    ///
    /// Returns a stale/cancelled rejection without changing the loading state.
    pub fn finish(&mut self, event: &RequestContext) -> Result<(), RequestRejection> {
        let Self::Loading { request, .. } = self else {
            return Err(RequestRejection::RequestId);
        };
        request.validate_event(event)?;
        let Self::Loading {
            mut snapshot, seen, ..
        } = std::mem::replace(self, Self::Idle)
        else {
            unreachable!("state checked above")
        };
        snapshot.retain(|entry| seen.contains(&entry.id));
        *self = Self::Ready(snapshot);
        Ok(())
    }

    /// Ends the current request with a recoverable error while retaining its previous snapshot.
    ///
    /// # Errors
    ///
    /// Returns a stale/cancelled rejection without changing the loading state.
    pub fn fail(
        &mut self,
        event: &RequestContext,
        error: ExplorerError,
    ) -> Result<(), RequestRejection> {
        let Self::Loading { request, .. } = self else {
            return Err(RequestRejection::RequestId);
        };
        request.validate_event(event)?;
        let Self::Loading {
            snapshot: previous, ..
        } = std::mem::replace(self, Self::Idle)
        else {
            unreachable!("state checked above")
        };
        *self = Self::Error { error, previous };
        Ok(())
    }
}

/// Independent state owned by one Explorer tab.
#[derive(Clone, Debug)]
pub struct TabState {
    pub id: TabId,
    pub generation: Generation,
    pub history: NavigationHistory,
    pub directory: DirectoryState,
    pub selection: SelectionModel,
    pub view: TabViewState,
    pub search: TabSearchState,
    pub search_sources: Vec<crate::SearchSourceStatus>,
    pub search_attribution: HashMap<ShellItemId, Vec<crate::SearchBackend>>,
    pub search_history: Vec<String>,
    pub requests: TabRequestScopes,
    pub location_can_write: bool,
    pending_history: Option<PendingHistoryNavigation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HistoryDirection {
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingHistoryNavigation {
    request_id: RequestId,
    direction: HistoryDirection,
    steps: usize,
}

/// Cancellation ownership for every asynchronous scope attached to one tab.
#[derive(Clone, Debug, Default)]
pub struct TabRequestScopes {
    navigation: Option<crate::CancellationToken>,
    search: Option<crate::CancellationToken>,
    operation_view: Option<crate::CancellationToken>,
}

impl TabRequestScopes {
    /// Replaces the search request token, cancelling the prior search.
    pub fn replace_search(&mut self, token: crate::CancellationToken) {
        if let Some(previous) = self.search.replace(token) {
            previous.cancel();
        }
    }

    /// Replaces the operation-view subscription token, cancelling the prior subscription.
    pub fn replace_operation_view(&mut self, token: crate::CancellationToken) {
        if let Some(previous) = self.operation_view.replace(token) {
            previous.cancel();
        }
    }

    /// Cancels navigation, search, and operation-view work exactly once.
    pub fn cancel_all(&mut self) {
        for token in [
            self.navigation.take(),
            self.search.take(),
            self.operation_view.take(),
        ]
        .into_iter()
        .flatten()
        {
            token.cancel();
        }
    }
}

/// View-only state that must remain scoped to one tab.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TabViewState {
    pub anchor: ViewAnchor,
    pub address: AddressBarState,
    pub settings: ViewSettings,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ViewMode {
    ExtraLargeIcons,
    LargeIcons,
    MediumIcons,
    SmallIcons,
    List,
    #[default]
    Details,
    Tiles,
    Content,
}

impl ViewMode {
    pub const ALL: [Self; 8] = [
        Self::ExtraLargeIcons,
        Self::LargeIcons,
        Self::MediumIcons,
        Self::SmallIcons,
        Self::List,
        Self::Details,
        Self::Tiles,
        Self::Content,
    ];
}

/// Stable identity of a Details column.
///
/// Built-ins intentionally retain their short variants while extension IDs carry an explicit
/// publisher/package namespace.  The latter is validated at registration time rather than being
/// assigned an ordinal: an ordinal would recreate the fixed bitmask problem and make persisted
/// layouts depend on plugin load order.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ColumnId {
    Name,
    DateModified,
    Type,
    Size,
    DateCreated,
    Authors,
    Tags,
    Title,
    FileCount,
    FolderCount,
    Extension {
        package_id: String,
        column_id: String,
    },
}

impl ColumnId {
    pub const BUILT_INS: [Self; 10] = [
        Self::Name,
        Self::DateModified,
        Self::Type,
        Self::Size,
        Self::DateCreated,
        Self::Authors,
        Self::Tags,
        Self::Title,
        Self::FileCount,
        Self::FolderCount,
    ];

    pub const ALL: [Self; 10] = Self::BUILT_INS;

    /// Constructs a plugin-owned ID in the durable `package_id:column_id` namespace.
    ///
    /// # Errors
    ///
    /// Returns an error when either namespace component violates the stable ID grammar.
    pub fn extension(package_id: &str, column_id: &str) -> Result<Self, ColumnIdError> {
        validate_extension_owner(package_id)?;
        validate_column_id_component(column_id, "column")?;
        let stable_id = format!("{package_id}:{column_id}");
        if stable_id.len() > Self::MAX_STABLE_ID_BYTES {
            return Err(ColumnIdError::TooLong {
                length: stable_id.len(),
                maximum: Self::MAX_STABLE_ID_BYTES,
            });
        }
        let _ = stable_id;
        Ok(Self::Extension {
            package_id: package_id.to_owned(),
            column_id: column_id.to_owned(),
        })
    }

    /// Returns the one canonical durable representation. Never persist `Debug` or a Rust enum
    /// discriminant: those are implementation details and cannot name extension columns.
    pub fn stable_id(&self) -> String {
        match self {
            Self::Name => "builtin:name".to_owned(),
            Self::DateModified => "builtin:date_modified".to_owned(),
            Self::Type => "builtin:type".to_owned(),
            Self::Size => "builtin:size".to_owned(),
            Self::DateCreated => "builtin:date_created".to_owned(),
            Self::Authors => "builtin:authors".to_owned(),
            Self::Tags => "builtin:tags".to_owned(),
            Self::Title => "builtin:title".to_owned(),
            Self::FileCount => "builtin:file_count".to_owned(),
            Self::FolderCount => "builtin:folder_count".to_owned(),
            Self::Extension {
                package_id,
                column_id,
            } => format!("{package_id}:{column_id}"),
        }
    }

    /// Reconstructs a built-in or extension ID from its durable stable representation.
    ///
    /// # Errors
    ///
    /// Returns an error when the durable representation is not a known built-in or a valid
    /// extension namespace.
    pub fn parse(stable_id: impl Into<String>) -> Result<Self, ColumnIdError> {
        let stable_id = stable_id.into();
        match stable_id.as_str() {
            "builtin:name" => return Ok(Self::Name),
            "builtin:date_modified" => return Ok(Self::DateModified),
            "builtin:type" => return Ok(Self::Type),
            "builtin:size" => return Ok(Self::Size),
            "builtin:date_created" => return Ok(Self::DateCreated),
            "builtin:authors" => return Ok(Self::Authors),
            "builtin:tags" => return Ok(Self::Tags),
            "builtin:title" => return Ok(Self::Title),
            "builtin:file_count" => return Ok(Self::FileCount),
            "builtin:folder_count" => return Ok(Self::FolderCount),
            _ => {}
        }
        let Some((package_id, column_id)) = stable_id.split_once(':') else {
            return Err(ColumnIdError::MissingNamespace);
        };
        if package_id == "builtin" {
            return Err(ColumnIdError::ReservedNamespace);
        }
        if column_id.contains(':') {
            return Err(ColumnIdError::InvalidCharacter(':'));
        }
        let expected = Self::extension(package_id, column_id)?;
        if expected.stable_id() != stable_id {
            return Err(ColumnIdError::InvalidCharacter(':'));
        }
        Ok(expected)
    }

    pub const MAX_COMPONENT_BYTES: usize = 64;
    pub const MAX_STABLE_ID_BYTES: usize = Self::MAX_COMPONENT_BYTES * 2 + 1;

    pub const fn is_builtin(&self) -> bool {
        !matches!(self, Self::Extension { .. })
    }

    /// Revalidates an ID at every host boundary. The enum is public to keep built-ins ergonomic,
    /// therefore registry code must not assume an `Extension` value was made by `extension()`.
    ///
    /// # Errors
    ///
    /// Returns an error when an extension namespace component violates the stable ID grammar.
    pub fn validate(&self) -> Result<(), ColumnIdError> {
        match self {
            Self::Extension {
                package_id,
                column_id,
            } => {
                let parsed = Self::extension(package_id, column_id)?;
                if &parsed == self {
                    Ok(())
                } else {
                    Err(ColumnIdError::InvalidCharacter(':'))
                }
            }
            _ => Ok(()),
        }
    }

    pub fn extension_parts(&self) -> Option<(&str, &str)> {
        match self {
            Self::Extension {
                package_id,
                column_id,
            } => Some((package_id, column_id)),
            _ => None,
        }
    }
}

fn validate_column_id_component(value: &str, component: &'static str) -> Result<(), ColumnIdError> {
    if value.is_empty() {
        return Err(ColumnIdError::EmptyComponent(component));
    }
    if value.len() > ColumnId::MAX_COMPONENT_BYTES {
        return Err(ColumnIdError::TooLong {
            length: value.len(),
            maximum: ColumnId::MAX_COMPONENT_BYTES,
        });
    }
    let Some(first) = value.chars().next() else {
        return Err(ColumnIdError::EmptyComponent(component));
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(ColumnIdError::InvalidFirstCharacter(first));
    }
    if let Some(character) = value.chars().skip(1).find(|character| {
        !character.is_ascii_lowercase()
            && !character.is_ascii_digit()
            && !matches!(character, '.' | '_' | '-')
    }) {
        return Err(ColumnIdError::InvalidCharacter(character));
    }
    Ok(())
}

fn validate_extension_owner(package_id: &str) -> Result<(), ColumnIdError> {
    validate_column_id_component(package_id, "package")?;
    if package_id == "builtin" {
        return Err(ColumnIdError::ReservedNamespace);
    }
    Ok(())
}

/// A rejected plugin-provided stable column ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ColumnIdError {
    EmptyComponent(&'static str),
    MissingNamespace,
    ReservedNamespace,
    InvalidFirstCharacter(char),
    InvalidCharacter(char),
    TooLong { length: usize, maximum: usize },
}

impl std::fmt::Display for ColumnIdError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyComponent(component) => write!(formatter, "{component} component is empty"),
            Self::MissingNamespace => {
                write!(formatter, "extension column ID is missing ':' namespace")
            }
            Self::ReservedNamespace => {
                write!(formatter, "'builtin' is a reserved column namespace")
            }
            Self::InvalidFirstCharacter(character) => {
                write!(formatter, "invalid first column ID character {character:?}")
            }
            Self::InvalidCharacter(character) => {
                write!(formatter, "invalid column ID character {character:?}")
            }
            Self::TooLong { length, maximum } => {
                write!(formatter, "column ID length {length} exceeds {maximum}")
            }
        }
    }
}

impl std::error::Error for ColumnIdError {}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SortDescriptor {
    pub column: ColumnId,
    pub direction: SortDirection,
}

impl Default for SortDescriptor {
    fn default() -> Self {
        Self {
            column: ColumnId::Name,
            direction: SortDirection::Ascending,
        }
    }
}

/// How a column's value is produced and consumed by the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnValueType {
    Text,
    LocalizedText,
    Integer,
    Float,
    Bytes,
    Time,
    Duration,
    Boolean,
    Structured,
    Opaque,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnAlignment {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnApplicability {
    AllEntries,
    Files,
    Containers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnSortSemantics {
    /// A host-comparable UTF-8 stable sort key.
    Text,
    /// A host-comparable signed or unsigned integer stable sort key.
    Integer,
    /// A host-comparable finite floating-point stable sort key.
    Float,
    /// A host-comparable byte-sequence stable sort key.
    Bytes,
    /// A host-comparable Unix-nanoseconds stable sort key.
    Time,
    /// A host-comparable duration-nanoseconds stable sort key.
    Duration,
    /// A host-comparable Boolean stable sort key.
    Boolean,
    /// Rejected for V1 descriptors: plugins cannot supply comparator callbacks. A future semantic
    /// must name a host-comparable, copied stable-key domain before it can be registered.
    ProviderDefined,
    /// This column has no sort affordance and therefore requires no stable sort key.
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnCost {
    Immediate,
    BackgroundSingle,
    BackgroundBatch,
    BackgroundAggregate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnDescriptor {
    pub id: ColumnId,
    pub display_name: String,
    pub value_type: ColumnValueType,
    pub default_width: u16,
    pub minimum_width: u16,
    pub maximum_width: u16,
    pub alignment: ColumnAlignment,
    pub applicability: ColumnApplicability,
    pub sort_semantics: ColumnSortSemantics,
    pub cost: ColumnCost,
}

impl ColumnDescriptor {
    /// Validates the descriptor shape and its value/sort compatibility.
    ///
    /// # Errors
    ///
    /// Returns an error when the ID, display name, width range, or sort semantics are invalid.
    pub fn validate(&self) -> Result<(), ColumnRegistryError> {
        self.id
            .validate()
            .map_err(ColumnRegistryError::InvalidExtensionId)?;
        if self.display_name.trim().is_empty() {
            return Err(ColumnRegistryError::EmptyDisplayName);
        }
        if self.display_name.len() > 256 || self.display_name.chars().any(char::is_control) {
            return Err(ColumnRegistryError::InvalidDisplayName);
        }
        if self.minimum_width == 0
            || self.minimum_width < OrderedColumnLayout::MINIMUM_WIDTH
            || self.minimum_width > self.default_width
            || self.default_width > self.maximum_width
            || self.maximum_width > OrderedColumnLayout::MAXIMUM_WIDTH
        {
            return Err(ColumnRegistryError::InvalidWidthRange {
                minimum: self.minimum_width,
                default: self.default_width,
                maximum: self.maximum_width,
            });
        }
        // Display values and sort keys deliberately use separate contracts. For example, a
        // localized "900 MB" value carries a `Bytes` stable sort key so the host compares its
        // owned exact byte count without reparsing text or invoking plugin code. Every accepted
        // semantic below maps to one `StableSortValueV1` host-comparable domain; it does not
        // authorize a provider callback comparator.
        if matches!(self.sort_semantics, ColumnSortSemantics::ProviderDefined) {
            return Err(ColumnRegistryError::IncompatibleSortSemantics);
        }
        Ok(())
    }
}

/// Registered descriptors keyed by their stable identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnRegistry {
    generation: u64,
    descriptors: std::collections::BTreeMap<ColumnId, ColumnDescriptor>,
}

impl ColumnRegistry {
    /// Monotonic lifecycle generation. Consumers can discard a descriptor projection built
    /// before a package replacement or removal.
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    pub fn built_ins() -> Self {
        let descriptors = builtin_column_descriptors()
            .into_iter()
            .map(|descriptor| (descriptor.id.clone(), descriptor))
            .collect();
        Self {
            generation: 1,
            descriptors,
        }
    }

    /// Adds a host-owned built-in descriptor after validating its reserved identity.
    ///
    /// # Errors
    ///
    /// Returns an error if the ID is not built-in, is invalid, or is already registered.
    pub fn register_builtin(
        &mut self,
        descriptor: ColumnDescriptor,
    ) -> Result<(), ColumnRegistryError> {
        if !descriptor.id.is_builtin() {
            return Err(ColumnRegistryError::BuiltinMustUseBuiltinId);
        }
        self.register(descriptor)
    }

    pub fn get(&self, id: &ColumnId) -> Option<&ColumnDescriptor> {
        self.descriptors.get(id)
    }

    pub fn contains(&self, id: &ColumnId) -> bool {
        self.descriptors.contains_key(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ColumnDescriptor> {
        self.descriptors.values()
    }

    /// Atomically replaces every descriptor owned by one package. The registry copies descriptor
    /// data into host-owned `String`/value fields; it never keeps a plugin allocation or callback
    /// pointer. Existing layout entries are intentionally untouched so an uninstalled plugin's
    /// preferences can be restored by a later persistence migration.
    ///
    /// # Errors
    ///
    /// Returns an error when the package or any descriptor is invalid, owned by another package,
    /// or duplicates an existing ID.
    pub fn replace_package(
        &mut self,
        package_id: &str,
        descriptors: impl IntoIterator<Item = ColumnDescriptor>,
    ) -> Result<(), ColumnRegistryError> {
        validate_extension_owner(package_id).map_err(ColumnRegistryError::InvalidExtensionId)?;
        let descriptors = descriptors.into_iter().collect::<Vec<_>>();
        let mut replacement = std::collections::BTreeMap::new();
        for descriptor in descriptors {
            let Some((owner, _)) = descriptor.id.extension_parts() else {
                return Err(ColumnRegistryError::ExtensionMustUseNamespacedId);
            };
            if owner != package_id {
                return Err(ColumnRegistryError::OwnershipMismatch {
                    expected: package_id.to_owned(),
                    actual: owner.to_owned(),
                });
            }
            descriptor.validate()?;
            let id = descriptor.id.clone();
            if replacement.insert(id.clone(), descriptor).is_some() {
                return Err(ColumnRegistryError::DuplicateId(id));
            }
        }
        if let Some(conflict) = replacement
            .keys()
            .find(|id| self.descriptors.contains_key(*id) && !id_owned_by_package(id, package_id))
        {
            return Err(ColumnRegistryError::DuplicateId(conflict.clone()));
        }
        self.descriptors
            .retain(|id, _| !id_owned_by_package(id, package_id));
        self.descriptors.extend(replacement);
        self.generation = self.generation.saturating_add(1);
        Ok(())
    }

    /// Removes a package's current descriptors without deleting its layout entries.
    pub fn unregister_package(&mut self, package_id: &str) -> usize {
        let before = self.descriptors.len();
        self.descriptors
            .retain(|id, _| !id_owned_by_package(id, package_id));
        let removed = before - self.descriptors.len();
        if removed > 0 {
            self.generation = self.generation.saturating_add(1);
        }
        removed
    }

    fn register(&mut self, descriptor: ColumnDescriptor) -> Result<(), ColumnRegistryError> {
        descriptor.validate()?;
        let id = descriptor.id.clone();
        if self.descriptors.contains_key(&id) {
            return Err(ColumnRegistryError::DuplicateId(id));
        }
        self.descriptors.insert(id, descriptor);
        self.generation = self.generation.saturating_add(1);
        Ok(())
    }
}

impl Default for ColumnRegistry {
    fn default() -> Self {
        Self::built_ins()
    }
}

fn id_owned_by_package(id: &ColumnId, package_id: &str) -> bool {
    id.extension_parts()
        .is_some_and(|(owner, _)| owner == package_id)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ColumnRegistryError {
    EmptyDisplayName,
    InvalidDisplayName,
    InvalidWidthRange {
        minimum: u16,
        default: u16,
        maximum: u16,
    },
    DuplicateId(ColumnId),
    BuiltinMustUseBuiltinId,
    ExtensionMustUseNamespacedId,
    InvalidExtensionId(ColumnIdError),
    OwnershipMismatch {
        expected: String,
        actual: String,
    },
    IncompatibleSortSemantics,
}

impl std::fmt::Display for ColumnRegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyDisplayName => write!(formatter, "column display name is empty"),
            Self::InvalidDisplayName => write!(
                formatter,
                "column display name is oversized or contains a control character"
            ),
            Self::InvalidWidthRange {
                minimum,
                default,
                maximum,
            } => write!(
                formatter,
                "invalid column width range {minimum}/{default}/{maximum}"
            ),
            Self::DuplicateId(id) => write!(formatter, "duplicate column ID {id:?}"),
            Self::BuiltinMustUseBuiltinId => {
                write!(formatter, "built-in column must use a built-in ID")
            }
            Self::ExtensionMustUseNamespacedId => {
                write!(formatter, "extension column must use a namespaced ID")
            }
            Self::InvalidExtensionId(error) => error.fmt(formatter),
            Self::OwnershipMismatch { expected, actual } => {
                write!(formatter, "column is owned by {actual}, not {expected}")
            }
            Self::IncompatibleSortSemantics => write!(
                formatter,
                "column value type and sort semantics are incompatible"
            ),
        }
    }
}

impl std::error::Error for ColumnRegistryError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnLayoutEntry {
    pub id: ColumnId,
    pub width: u16,
    pub visible: bool,
}

/// Ordered, extensible details-column preferences. Unknown extension IDs remain entries here;
/// callers use the registry to decide whether an entry is currently renderable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderedColumnLayout {
    entries: Vec<ColumnLayoutEntry>,
}

impl OrderedColumnLayout {
    pub const MINIMUM_WIDTH: u16 = 48;
    pub const MAXIMUM_WIDTH: u16 = 1_200;

    pub fn entries(&self) -> &[ColumnLayoutEntry] {
        &self.entries
    }

    /// Restores a validated, canonical persisted layout, including currently
    /// unavailable extension IDs. The caller owns schema validation; this
    /// constructor still fail-closes on invalid IDs, duplicate IDs, or widths.
    ///
    /// # Errors
    ///
    /// Returns [`ColumnIdError`] for invalid or duplicate IDs, invalid widths,
    /// or a layout that does not contain the required Name column.
    pub fn restore_entries(
        entries: impl IntoIterator<Item = (String, u16, bool)>,
    ) -> Result<Self, ColumnIdError> {
        let mut restored = Vec::new();
        for (stable_id, width, visible) in entries {
            let id = ColumnId::parse(stable_id)?;
            if restored
                .iter()
                .any(|entry: &ColumnLayoutEntry| entry.id == id)
            {
                return Err(ColumnIdError::InvalidCharacter(':'));
            }
            if !(Self::MINIMUM_WIDTH..=Self::MAXIMUM_WIDTH).contains(&width) {
                return Err(ColumnIdError::TooLong {
                    length: usize::from(width),
                    maximum: usize::from(Self::MAXIMUM_WIDTH),
                });
            }
            restored.push(ColumnLayoutEntry {
                visible: visible || id == ColumnId::Name,
                id,
                width,
            });
        }
        let Some(name_index) = restored.iter().position(|entry| entry.id == ColumnId::Name) else {
            return Err(ColumnIdError::MissingNamespace);
        };
        let name = restored.remove(name_index);
        restored.insert(0, name);
        Ok(Self { entries: restored })
    }

    pub fn entry(&self, id: &ColumnId) -> Option<&ColumnLayoutEntry> {
        self.entries.iter().find(|entry| entry.id == *id)
    }

    pub fn width(&self, id: &ColumnId) -> Option<u16> {
        self.entry(id).map(|entry| entry.width)
    }

    pub fn visible(&self, id: &ColumnId) -> bool {
        self.entry(id).is_some_and(|entry| entry.visible)
    }

    pub fn set_width(&mut self, id: &ColumnId, width: u16) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == *id) else {
            return false;
        };
        entry.width = width.clamp(Self::MINIMUM_WIDTH, Self::MAXIMUM_WIDTH);
        true
    }

    /// Stores the user's desired width after applying the descriptor's own safe range. The
    /// descriptor is host-owned registry data, so a plugin cannot widen a persisted preference
    /// beyond the host's global cap.
    pub fn set_width_for(&mut self, descriptor: &ColumnDescriptor, width: u16) -> bool {
        if descriptor.validate().is_err() {
            return false;
        }
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.id == descriptor.id)
        else {
            return false;
        };
        entry.width = width.clamp(descriptor.minimum_width, descriptor.maximum_width);
        true
    }

    pub fn effective_width(&self, descriptor: &ColumnDescriptor) -> Option<u16> {
        if descriptor.validate().is_err() {
            return None;
        }
        self.width(&descriptor.id)
            .map(|width| width.clamp(descriptor.minimum_width, descriptor.maximum_width))
    }

    pub fn set_visible(&mut self, id: &ColumnId, visible: bool) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == *id) else {
            return false;
        };
        if entry.id == ColumnId::Name && !visible {
            return false;
        }
        entry.visible = visible;
        true
    }

    pub fn toggle_visible(&mut self, id: &ColumnId) -> bool {
        let visible = self.entry(id).is_some_and(|entry| entry.visible);
        self.set_visible(id, !visible)
    }

    /// Inserts a registered descriptor only once; re-registering a plugin never resets a user's
    /// width, visibility, or ordering preference.
    pub fn ensure_descriptor(&mut self, descriptor: &ColumnDescriptor, visible: bool) -> bool {
        if descriptor.validate().is_err() {
            return false;
        }
        if self.entry(&descriptor.id).is_none() {
            let entry = ColumnLayoutEntry {
                id: descriptor.id.clone(),
                width: descriptor
                    .default_width
                    .clamp(descriptor.minimum_width, descriptor.maximum_width),
                visible,
            };
            if descriptor.id == ColumnId::Name {
                self.entries.insert(0, entry);
            } else {
                self.entries.push(entry);
            }
        }
        true
    }

    /// Appends built-ins introduced after a persisted extensible layout was
    /// written. Existing entries are deliberately left byte-for-byte
    /// equivalent in order, width, and visibility; newly introduced columns
    /// start hidden so an upgrade never changes the visible Details layout.
    pub fn reconcile_current_built_ins(&mut self) {
        for descriptor in builtin_column_descriptors() {
            self.ensure_descriptor(&descriptor, false);
        }
    }

    pub fn move_before(&mut self, id: &ColumnId, before: Option<&ColumnId>) -> bool {
        if *id == ColumnId::Name {
            return false;
        }
        let Some(index) = self.entries.iter().position(|entry| entry.id == *id) else {
            return false;
        };
        let Some(before) = before else {
            let entry = self.entries.remove(index);
            self.entries.push(entry);
            return true;
        };
        if before == id {
            return false;
        }
        if *before == ColumnId::Name {
            let entry = self.entries.remove(index);
            let insertion = usize::from(!self.entries.is_empty());
            self.entries.insert(insertion, entry);
            return true;
        }
        let Some(target) = self.entries.iter().position(|entry| entry.id == *before) else {
            return false;
        };
        let entry = self.entries.remove(index);
        let insertion = if index < target { target - 1 } else { target };
        self.entries.insert(insertion, entry);
        true
    }

    /// Applies an ordered preference prefix once, then appends any unmentioned entries in their
    /// deterministic registry order. This avoids the sequential-move reversal trap during legacy
    /// four-column migrations.
    pub fn reorder_known(&mut self, order: impl IntoIterator<Item = ColumnId>) {
        let mut reordered = Vec::with_capacity(self.entries.len());
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.id == ColumnId::Name)
        {
            reordered.push(self.entries.remove(index));
        }
        for id in order {
            if id == ColumnId::Name {
                continue;
            }
            if let Some(index) = self.entries.iter().position(|entry| entry.id == id) {
                reordered.push(self.entries.remove(index));
            }
        }
        reordered.append(&mut self.entries);
        self.entries = reordered;
    }

    pub fn visible_registered<'a>(
        &'a self,
        registry: &'a ColumnRegistry,
    ) -> impl Iterator<Item = &'a ColumnLayoutEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.visible && registry.contains(&entry.id))
    }
}

impl Default for OrderedColumnLayout {
    fn default() -> Self {
        Self {
            entries: builtin_column_descriptors()
                .into_iter()
                .map(|descriptor| {
                    let visible = matches!(
                        descriptor.id,
                        ColumnId::Name | ColumnId::DateModified | ColumnId::Type | ColumnId::Size
                    );
                    ColumnLayoutEntry {
                        id: descriptor.id,
                        width: descriptor.default_width,
                        visible,
                    }
                })
                .collect(),
        }
    }
}

fn builtin_column_descriptors() -> [ColumnDescriptor; 10] {
    [
        builtin_descriptor(
            ColumnId::Name,
            "Name",
            ColumnValueType::Text,
            280,
            ColumnAlignment::Start,
            ColumnSortSemantics::Text,
        ),
        builtin_descriptor(
            ColumnId::DateModified,
            "Date modified",
            ColumnValueType::Time,
            150,
            ColumnAlignment::Start,
            ColumnSortSemantics::Time,
        ),
        builtin_descriptor(
            ColumnId::Type,
            "Type",
            ColumnValueType::Text,
            115,
            ColumnAlignment::Start,
            ColumnSortSemantics::Text,
        ),
        builtin_descriptor(
            ColumnId::Size,
            "Size",
            ColumnValueType::Bytes,
            90,
            ColumnAlignment::End,
            ColumnSortSemantics::Bytes,
        ),
        builtin_descriptor(
            ColumnId::DateCreated,
            "Date created",
            ColumnValueType::Time,
            150,
            ColumnAlignment::Start,
            ColumnSortSemantics::Time,
        ),
        builtin_descriptor(
            ColumnId::Authors,
            "Authors",
            ColumnValueType::Text,
            150,
            ColumnAlignment::Start,
            ColumnSortSemantics::Text,
        ),
        builtin_descriptor(
            ColumnId::Tags,
            "Tags",
            ColumnValueType::Text,
            150,
            ColumnAlignment::Start,
            ColumnSortSemantics::Text,
        ),
        builtin_descriptor(
            ColumnId::Title,
            "Title",
            ColumnValueType::Text,
            180,
            ColumnAlignment::Start,
            ColumnSortSemantics::Text,
        ),
        builtin_aggregate_descriptor(ColumnId::FileCount, "File Count"),
        builtin_aggregate_descriptor(ColumnId::FolderCount, "Folder Count"),
    ]
}

fn builtin_aggregate_descriptor(id: ColumnId, display_name: &str) -> ColumnDescriptor {
    ColumnDescriptor {
        id,
        display_name: display_name.to_owned(),
        value_type: ColumnValueType::Integer,
        default_width: 104,
        minimum_width: OrderedColumnLayout::MINIMUM_WIDTH,
        maximum_width: OrderedColumnLayout::MAXIMUM_WIDTH,
        alignment: ColumnAlignment::End,
        applicability: ColumnApplicability::Containers,
        sort_semantics: ColumnSortSemantics::Integer,
        cost: ColumnCost::BackgroundAggregate,
    }
}

fn builtin_descriptor(
    id: ColumnId,
    display_name: &str,
    value_type: ColumnValueType,
    default_width: u16,
    alignment: ColumnAlignment,
    sort_semantics: ColumnSortSemantics,
) -> ColumnDescriptor {
    ColumnDescriptor {
        id,
        display_name: display_name.to_owned(),
        value_type,
        default_width,
        minimum_width: OrderedColumnLayout::MINIMUM_WIDTH,
        maximum_width: OrderedColumnLayout::MAXIMUM_WIDTH,
        alignment,
        applicability: ColumnApplicability::AllEntries,
        sort_semantics,
        cost: ColumnCost::Immediate,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Explorer exposes these independent View menu toggles and each is persisted per tab"
)]
pub struct ViewSettings {
    pub mode: ViewMode,
    /// The last selected extension view identity for this tab. Built-in
    /// `ViewMode` remains closed so an unavailable extension can fall back
    /// without losing the user's recoverable choice.
    pub extension_view_id: Option<String>,
    /// The exact Explorer Ctrl+wheel icon-size notch for the active view.
    /// Non-icon views keep their presentation icon size here as well so the
    /// Shell request and rendered geometry always agree.
    pub icon_size: u16,
    pub details_pane: bool,
    pub preview_pane: bool,
    pub item_check_boxes: bool,
    pub file_name_extensions: bool,
    pub hidden_items: bool,
    pub compact_view: bool,
    pub always_show_icons: bool,
    /// In-process Shell icon presentation cache budget in MiB.
    pub icon_cache_memory_mb: u16,
    /// In-process decoded thumbnail cache budget in MiB.
    pub thumbnail_cache_memory_mb: u16,
    /// `SuperExplorer` MFT Service folder-aggregate LRU budget in MiB.
    pub mft_folder_cache_memory_mb: u16,
    pub cache_budgets: crate::CacheBudgetSettingsV1,
    pub sort: SortDescriptor,
    pub details_layout: OrderedColumnLayout,
    pub details_pane_width: u16,
    pub preview_pane_width: u16,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            mode: ViewMode::Details,
            extension_view_id: None,
            icon_size: default_icon_size_for_mode(ViewMode::Details),
            details_pane: false,
            preview_pane: false,
            item_check_boxes: false,
            file_name_extensions: true,
            hidden_items: false,
            compact_view: false,
            always_show_icons: false,
            icon_cache_memory_mb: DEFAULT_ICON_CACHE_MEMORY_MB,
            thumbnail_cache_memory_mb: DEFAULT_THUMBNAIL_CACHE_MEMORY_MB,
            mft_folder_cache_memory_mb: DEFAULT_MFT_FOLDER_CACHE_MEMORY_MB,
            cache_budgets: crate::CacheBudgetSettingsV1::default(),
            sort: SortDescriptor::default(),
            details_layout: OrderedColumnLayout::default(),
            details_pane_width: 293,
            preview_pane_width: 293,
        }
    }
}

pub const DEFAULT_ICON_CACHE_MEMORY_MB: u16 = 32;
pub const MAX_ICON_CACHE_MEMORY_MB: u16 = 1_024;
pub const DEFAULT_THUMBNAIL_CACHE_MEMORY_MB: u16 = 128;
pub const MAX_THUMBNAIL_CACHE_MEMORY_MB: u16 = 1_024;
pub const MIN_MFT_FOLDER_CACHE_MEMORY_MB: u16 = 128;
pub const DEFAULT_MFT_FOLDER_CACHE_MEMORY_MB: u16 = 512;
pub const MAX_MFT_FOLDER_CACHE_MEMORY_MB: u16 = 16_384;

pub const fn normalized_mft_folder_cache_memory_mb(value: u16) -> u16 {
    if value < MIN_MFT_FOLDER_CACHE_MEMORY_MB {
        MIN_MFT_FOLDER_CACHE_MEMORY_MB
    } else if value > MAX_MFT_FOLDER_CACHE_MEMORY_MB {
        MAX_MFT_FOLDER_CACHE_MEMORY_MB
    } else {
        value
    }
}

pub const fn normalized_icon_cache_memory_mb(value: u16) -> u16 {
    match value {
        0..=47 => 32,
        48..=95 => 64,
        96..=191 => 128,
        192..=383 => 256,
        384..=767 => 512,
        _ => MAX_ICON_CACHE_MEMORY_MB,
    }
}

pub const fn normalized_thumbnail_cache_memory_mb(value: u16) -> u16 {
    match value {
        0..=47 => 32,
        48..=95 => 64,
        96..=191 => 128,
        192..=383 => 256,
        384..=767 => 512,
        _ => MAX_THUMBNAIL_CACHE_MEMORY_MB,
    }
}

impl ViewSettings {
    /// Resolves an extension view preference without making the model depend
    /// on a plugin registry. Callers render the returned ID only while their
    /// host-owned runtime reports it available; otherwise they use `mode`.
    pub fn effective_extension_view_id(
        &self,
        mut available: impl FnMut(&str) -> bool,
    ) -> Option<&str> {
        self.extension_view_id
            .as_deref()
            .filter(|view_id| available(view_id))
    }

    pub fn details_column_width(&self, id: &ColumnId) -> u16 {
        self.details_layout
            .width(id)
            .unwrap_or(OrderedColumnLayout::MINIMUM_WIDTH)
    }

    pub fn details_column_visible(&self, id: &ColumnId) -> bool {
        self.details_layout.visible(id)
    }
}

/// Returns the middle Explorer notch used when a view is selected directly
/// from the View menu. Ctrl+wheel can subsequently move to the adjacent
/// smaller or larger notch without changing the named view mode.
pub const fn default_icon_size_for_mode(mode: ViewMode) -> u16 {
    match mode {
        ViewMode::ExtraLargeIcons => 384,
        ViewMode::LargeIcons => 108,
        ViewMode::MediumIcons => 72,
        ViewMode::SmallIcons | ViewMode::Content => 32,
        ViewMode::List | ViewMode::Details => 20,
        ViewMode::Tiles => 40,
    }
}

/// Normalizes restored or programmatically constructed settings to a valid
/// notch for their named view mode.
pub fn effective_icon_size(settings: &ViewSettings) -> u16 {
    let valid = match settings.mode {
        ViewMode::SmallIcons => matches!(settings.icon_size, 24 | 32 | 48),
        ViewMode::MediumIcons => matches!(settings.icon_size, 64 | 72 | 84),
        ViewMode::LargeIcons => matches!(settings.icon_size, 96 | 108 | 128),
        ViewMode::ExtraLargeIcons => matches!(settings.icon_size, 256 | 384 | 512),
        ViewMode::List | ViewMode::Details => settings.icon_size == 20,
        ViewMode::Tiles => settings.icon_size == 40,
        ViewMode::Content => settings.icon_size == 32,
    };
    if valid {
        settings.icon_size
    } else {
        default_icon_size_for_mode(settings.mode)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BreadcrumbSegmentId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreadcrumbIconHint {
    Computer,
    Drive,
    Folder,
    Archive,
    Namespace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BreadcrumbSegment {
    pub id: BreadcrumbSegmentId,
    pub display_name: String,
    pub location: LocationDescriptor,
    pub icon_hint: BreadcrumbIconHint,
    pub is_container: bool,
}

impl BreadcrumbSegment {
    /// Keeps local drive breadcrumbs stable while asynchronous Shell metadata arrives.
    /// Explorer may expose a volume label elsewhere, but the address ancestry uses the
    /// canonical drive designator so the row cannot change width during navigation.
    pub fn stabilize_display_name(&mut self) {
        if self.icon_hint != BreadcrumbIconHint::Drive {
            return;
        }
        let Some(path) = self.location.path() else {
            return;
        };
        let text = path.to_string_lossy();
        let root = text.trim_end_matches(['\\', '/']);
        let canonical_root = root
            .strip_prefix(r"\\?\")
            .or_else(|| root.strip_prefix(r"\\.\"))
            .unwrap_or(root);
        let bytes = canonical_root.as_bytes();
        if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            self.display_name = format!("{}:", char::from(bytes[0]).to_ascii_uppercase());
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BreadcrumbMenuItem {
    pub display_name: String,
    pub location: LocationDescriptor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AddressBarMode {
    Browsing,
    Editing,
    EnumeratingMenu {
        segment_id: BreadcrumbSegmentId,
        generation: u64,
    },
    NavigationError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressBarState {
    pub mode: AddressBarMode,
    pub draft: String,
    pub resolved_ancestry: Vec<BreadcrumbSegment>,
    pub error: Option<String>,
    pub menu_children: Vec<BreadcrumbMenuItem>,
    pub menu_error: Option<String>,
    pub menu_loading: bool,
    pub overflow_open: bool,
    /// Roving keyboard focus within the resolved breadcrumb ancestry.
    pub keyboard_segment_index: Option<usize>,
    /// Roving keyboard focus within the currently open child menu.
    pub keyboard_menu_index: Option<usize>,
    /// Case-folded incremental prefix used by Explorer-style menu type-ahead.
    pub menu_typeahead: String,
    menu_generation: u64,
}

impl AddressBarState {
    pub fn for_entry(entry: &HistoryEntry) -> Self {
        Self {
            mode: AddressBarMode::Browsing,
            draft: parsing_text(&entry.location),
            resolved_ancestry: location_breadcrumbs(&entry.location),
            error: None,
            menu_children: Vec::new(),
            menu_error: None,
            menu_loading: false,
            overflow_open: false,
            keyboard_segment_index: None,
            keyboard_menu_index: None,
            menu_typeahead: String::new(),
            menu_generation: 0,
        }
    }

    pub fn enter_editing(&mut self) {
        self.mode = AddressBarMode::Editing;
        self.error = None;
        self.overflow_open = false;
        self.keyboard_menu_index = None;
        self.menu_typeahead.clear();
    }

    pub fn update_draft(&mut self, draft: String) -> bool {
        if !matches!(
            self.mode,
            AddressBarMode::Editing | AddressBarMode::NavigationError
        ) {
            return false;
        }
        self.draft = draft;
        self.mode = AddressBarMode::Editing;
        self.error = None;
        self.overflow_open = false;
        true
    }

    pub fn cancel_editing(&mut self, current: &HistoryEntry) {
        *self = Self::for_entry(current);
    }

    pub fn begin_menu(&mut self, segment_id: BreadcrumbSegmentId) -> Option<u64> {
        if segment_id != BreadcrumbSegmentId(0) {
            self.resolved_ancestry
                .iter()
                .find(|segment| segment.id == segment_id && segment.is_container)?;
        }
        self.menu_generation = self.menu_generation.checked_add(1)?;
        self.mode = AddressBarMode::EnumeratingMenu {
            segment_id,
            generation: self.menu_generation,
        };
        self.menu_children.clear();
        self.menu_error = None;
        self.menu_loading = true;
        self.overflow_open = false;
        self.keyboard_segment_index = self
            .resolved_ancestry
            .iter()
            .position(|segment| segment.id == segment_id)
            .or_else(|| (!self.resolved_ancestry.is_empty()).then_some(0));
        self.keyboard_menu_index = None;
        self.menu_typeahead.clear();
        Some(self.menu_generation)
    }

    pub fn toggle_overflow(&mut self) -> bool {
        if !matches!(self.mode, AddressBarMode::Browsing) {
            return false;
        }
        self.overflow_open = !self.overflow_open;
        self.keyboard_menu_index = None;
        self.menu_typeahead.clear();
        self.overflow_open
    }

    pub fn finish_menu(
        &mut self,
        segment_id: BreadcrumbSegmentId,
        generation: u64,
        result: Result<Vec<BreadcrumbMenuItem>, String>,
    ) -> bool {
        if !matches!(
            self.mode,
            AddressBarMode::EnumeratingMenu {
                segment_id: active_segment,
                generation: active_generation,
            } if active_segment == segment_id && active_generation == generation
        ) {
            return false;
        }
        match result {
            Ok(children) => {
                self.menu_children = children;
                self.menu_error = None;
            }
            Err(error) => {
                self.menu_children.clear();
                self.menu_error = Some(error);
            }
        }
        self.menu_loading = false;
        self.keyboard_menu_index = (!self.menu_children.is_empty()).then_some(0);
        true
    }

    pub fn close_menu(&mut self) {
        if matches!(self.mode, AddressBarMode::EnumeratingMenu { .. }) {
            self.mode = AddressBarMode::Browsing;
        }
        self.overflow_open = false;
        self.keyboard_menu_index = None;
        self.menu_typeahead.clear();
    }

    pub fn navigation_failed(&mut self, message: String) {
        self.mode = AddressBarMode::NavigationError;
        self.error = Some(message);
        self.overflow_open = false;
    }

    pub fn resolve(&mut self, entry: &HistoryEntry) {
        *self = Self::for_entry(entry);
    }

    pub fn move_segment_focus(&mut self, direction: i8) -> bool {
        if self.resolved_ancestry.is_empty() || !matches!(self.mode, AddressBarMode::Browsing) {
            return false;
        }
        let current = self
            .keyboard_segment_index
            .unwrap_or(self.resolved_ancestry.len() - 1);
        let next = if direction <= -2 {
            0
        } else if direction >= 2 {
            self.resolved_ancestry.len() - 1
        } else if direction < 0 {
            current.saturating_sub(1)
        } else {
            (current + 1).min(self.resolved_ancestry.len() - 1)
        };
        self.keyboard_segment_index = Some(next);
        true
    }

    pub fn focused_segment(&self) -> Option<&BreadcrumbSegment> {
        let index = self
            .keyboard_segment_index
            .unwrap_or_else(|| self.resolved_ancestry.len().saturating_sub(1));
        self.resolved_ancestry.get(index)
    }

    pub fn move_menu_focus(&mut self, movement: MenuFocusMovement) -> bool {
        let len = self.menu_children.len();
        if len == 0 || !matches!(self.mode, AddressBarMode::EnumeratingMenu { .. }) {
            return false;
        }
        let current = self.keyboard_menu_index.unwrap_or(0).min(len - 1);
        let next = match movement {
            MenuFocusMovement::Previous => current.saturating_sub(1),
            MenuFocusMovement::Next => (current + 1).min(len - 1),
            MenuFocusMovement::First => 0,
            MenuFocusMovement::Last => len - 1,
            MenuFocusMovement::PagePrevious => current.saturating_sub(8),
            MenuFocusMovement::PageNext => (current + 8).min(len - 1),
        };
        self.keyboard_menu_index = Some(next);
        self.menu_typeahead.clear();
        true
    }

    pub fn set_menu_focus(&mut self, index: usize) -> bool {
        if !matches!(self.mode, AddressBarMode::EnumeratingMenu { .. })
            || index >= self.menu_children.len()
        {
            return false;
        }
        if self.keyboard_menu_index == Some(index) {
            return false;
        }
        self.keyboard_menu_index = Some(index);
        self.menu_typeahead.clear();
        true
    }

    pub fn typeahead_menu_focus(&mut self, text: &str) -> bool {
        if text.chars().any(char::is_control) || text.is_empty() || self.menu_children.is_empty() {
            return false;
        }
        self.menu_typeahead.push_str(&text.to_lowercase());
        let mut found = self.menu_children.iter().position(|item| {
            item.display_name
                .to_lowercase()
                .starts_with(&self.menu_typeahead)
        });
        if found.is_none() {
            self.menu_typeahead = text.to_lowercase();
            found = self.menu_children.iter().position(|item| {
                item.display_name
                    .to_lowercase()
                    .starts_with(&self.menu_typeahead)
            });
        }
        if let Some(index) = found {
            self.keyboard_menu_index = Some(index);
            true
        } else {
            false
        }
    }

    pub fn focused_menu_item(&self) -> Option<&BreadcrumbMenuItem> {
        self.keyboard_menu_index
            .and_then(|index| self.menu_children.get(index))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuFocusMovement {
    Previous,
    Next,
    First,
    Last,
    PagePrevious,
    PageNext,
}

impl Default for TabViewState {
    fn default() -> Self {
        Self {
            anchor: ViewAnchor::default(),
            address: AddressBarState {
                mode: AddressBarMode::Browsing,
                draft: String::new(),
                resolved_ancestry: Vec::new(),
                error: None,
                menu_children: Vec::new(),
                menu_error: None,
                menu_loading: false,
                overflow_open: false,
                keyboard_segment_index: None,
                keyboard_menu_index: None,
                menu_typeahead: String::new(),
                menu_generation: 0,
            },
            settings: ViewSettings::default(),
        }
    }
}

fn parsing_text(location: &LocationDescriptor) -> String {
    match location {
        LocationDescriptor::FileSystem(path) => path.to_string_lossy().into_owned(),
        LocationDescriptor::ParsingName(name) => name.clone(),
        LocationDescriptor::ShellNamespace(_) | LocationDescriptor::KnownFolder(_) => String::new(),
        LocationDescriptor::Virtual(location) => {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut identity = String::with_capacity(location.container_identity.len() * 2);
            for byte in location.container_identity {
                identity.push(char::from(HEX[usize::from(byte >> 4)]));
                identity.push(char::from(HEX[usize::from(byte & 0x0f)]));
            }
            let suffix = location.components.join("/");
            if suffix.is_empty() {
                format!(
                    "{}://{identity}/{}",
                    location.provider_id, location.container_generation
                )
            } else {
                format!(
                    "{}://{identity}/{}/{suffix}",
                    location.provider_id, location.container_generation
                )
            }
        }
    }
}

/// Builds deterministic host-owned breadcrumbs for filesystem and virtual locations.
#[must_use]
pub fn location_breadcrumbs(location: &LocationDescriptor) -> Vec<BreadcrumbSegment> {
    use std::hash::{Hash, Hasher};

    if let LocationDescriptor::Virtual(virtual_location) = location {
        let mut segments = Vec::with_capacity(virtual_location.components.len() + 1);
        for depth in 0..=virtual_location.components.len() {
            let mut descriptor = virtual_location.clone();
            descriptor.components.truncate(depth);
            descriptor.entry_id = None;
            let descriptor = LocationDescriptor::Virtual(descriptor);
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            descriptor.hash(&mut hasher);
            segments.push(BreadcrumbSegment {
                id: BreadcrumbSegmentId(hasher.finish()),
                display_name: if depth == 0 {
                    virtual_location.provider_id.clone()
                } else {
                    virtual_location.components[depth - 1].clone()
                },
                location: descriptor,
                icon_hint: if depth == 0 {
                    BreadcrumbIconHint::Archive
                } else {
                    BreadcrumbIconHint::Folder
                },
                is_container: true,
            });
        }
        return segments;
    }
    let Some(path) = location.path() else {
        return Vec::new();
    };
    let mut current = std::path::PathBuf::new();
    path.components()
        .filter_map(|component| {
            current.push(component.as_os_str());
            let display_name = match component {
                std::path::Component::Prefix(prefix) => {
                    prefix.as_os_str().to_string_lossy().into_owned()
                }
                std::path::Component::RootDir => return None,
                _ => component.as_os_str().to_string_lossy().into_owned(),
            };
            let descriptor = LocationDescriptor::file_system(current.clone());
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            descriptor.hash(&mut hasher);
            let mut segment = BreadcrumbSegment {
                id: BreadcrumbSegmentId(hasher.finish()),
                display_name,
                location: descriptor,
                icon_hint: if current.parent().is_none() {
                    BreadcrumbIconHint::Drive
                } else if current.extension().is_some_and(|extension| {
                    matches!(
                        extension.to_string_lossy().to_ascii_lowercase().as_str(),
                        "zip" | "rar" | "7z" | "tar" | "gz"
                    )
                }) {
                    BreadcrumbIconHint::Archive
                } else {
                    BreadcrumbIconHint::Folder
                },
                is_container: true,
            };
            segment.stabilize_display_name();
            Some(segment)
        })
        .collect()
}

/// Search state scoped to one tab and independent from its directory snapshot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum TabSearchState {
    #[default]
    Idle,
    Editing(String),
    Loading {
        request: RequestContext,
        input: String,
        results: DirectorySnapshot,
    },
    Ready {
        input: String,
        results: DirectorySnapshot,
    },
    Partial {
        input: String,
        results: DirectorySnapshot,
        error: ExplorerError,
    },
    Cancelled {
        input: String,
        results: DirectorySnapshot,
    },
    Error {
        input: String,
        results: DirectorySnapshot,
        error: ExplorerError,
    },
}

impl TabSearchState {
    pub fn input(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Editing(input)
            | Self::Loading { input, .. }
            | Self::Ready { input, .. }
            | Self::Partial { input, .. }
            | Self::Cancelled { input, .. }
            | Self::Error { input, .. } => Some(input),
        }
    }
}

impl TabState {
    /// Creates a tab at an already-resolved initial location.
    pub fn new(initial: HistoryEntry) -> Self {
        let address = AddressBarState::for_entry(&initial);
        Self {
            id: TabId::new(),
            generation: Generation::default(),
            history: NavigationHistory::with_initial(initial),
            directory: DirectoryState::Idle,
            selection: SelectionModel::default(),
            view: TabViewState {
                anchor: ViewAnchor::default(),
                address,
                settings: ViewSettings::default(),
            },
            search: TabSearchState::Idle,
            search_sources: Vec::new(),
            search_attribution: HashMap::new(),
            search_history: Vec::new(),
            requests: TabRequestScopes::default(),
            location_can_write: false,
            pending_history: None,
        }
    }

    /// Reconstructs durable tab state while resetting all transient request and interaction data.
    pub fn from_restored(
        id: TabId,
        history: NavigationHistory,
        settings: ViewSettings,
    ) -> Option<Self> {
        let current = history.current()?.clone();
        let mut tab = Self::new(current.clone());
        tab.id = id;
        tab.history = history;
        tab.view.address = AddressBarState::for_entry(&current);
        tab.view.anchor = current.view_anchor;
        tab.view.settings = settings;
        Some(tab)
    }

    /// Starts navigation to a different location and discards partial rows from the old location.
    pub fn begin_navigation_request(&mut self) -> Option<RequestContext> {
        self.pending_history = None;
        self.cancel_search();
        self.generation = self.generation.checked_next()?;
        let request = RequestContext::new(self.id, self.generation);
        self.directory.begin(request.clone(), false);
        self.requests.navigation = Some(request.cancellation.clone());
        Some(request)
    }

    /// Starts Refresh while retaining current rows until the new snapshot converges.
    pub fn begin_refresh_request(&mut self) -> Option<RequestContext> {
        self.pending_history = None;
        self.cancel_search();
        self.generation = self.generation.checked_next()?;
        let request = RequestContext::new(self.id, self.generation);
        self.directory.begin(request.clone(), true);
        self.requests.navigation = Some(request.cancellation.clone());
        Some(request)
    }

    /// Starts Back without mutating history until the destination resolves successfully.
    pub fn begin_back_request(&mut self) -> Option<(RequestContext, LocationDescriptor)> {
        self.begin_back_request_at(1)
    }

    /// Starts a multi-step Back jump without mutating committed history until resolution.
    pub fn begin_back_request_at(
        &mut self,
        steps: usize,
    ) -> Option<(RequestContext, LocationDescriptor)> {
        let destination = self.history.back_destination_at(steps)?.location.clone();
        let request = self.begin_navigation_request()?;
        self.pending_history = Some(PendingHistoryNavigation {
            request_id: request.request_id,
            direction: HistoryDirection::Back,
            steps,
        });
        Some((request, destination))
    }

    /// Starts Forward without mutating history until the destination resolves successfully.
    pub fn begin_forward_request(&mut self) -> Option<(RequestContext, LocationDescriptor)> {
        self.begin_forward_request_at(1)
    }

    /// Starts a multi-step Forward jump without mutating committed history until resolution.
    pub fn begin_forward_request_at(
        &mut self,
        steps: usize,
    ) -> Option<(RequestContext, LocationDescriptor)> {
        let destination = self.history.forward_destination_at(steps)?.location.clone();
        let request = self.begin_navigation_request()?;
        self.pending_history = Some(PendingHistoryNavigation {
            request_id: request.request_id,
            direction: HistoryDirection::Forward,
            steps,
        });
        Some((request, destination))
    }

    pub(crate) fn commit_resolved_location(
        &mut self,
        context: &RequestContext,
        entry: HistoryEntry,
    ) -> bool {
        let resolved_entry = entry.clone();
        let Some(pending) = self.pending_history else {
            self.history.commit_navigation(entry);
            self.view.address.resolve(&resolved_entry);
            return true;
        };
        if pending.request_id != context.request_id {
            return false;
        }
        self.pending_history = None;
        let committed = match pending.direction {
            HistoryDirection::Back => self.history.commit_back_steps(entry, pending.steps),
            HistoryDirection::Forward => self.history.commit_forward_steps(entry, pending.steps),
        };
        if committed {
            self.view.address.resolve(&resolved_entry);
        }
        committed
    }

    pub(crate) fn reject_history_navigation(&mut self, context: &RequestContext) {
        if self
            .pending_history
            .is_some_and(|pending| pending.request_id == context.request_id)
        {
            self.pending_history = None;
        }
    }

    /// Starts a new per-tab search generation while preserving the directory snapshot underneath.
    pub fn begin_search_request(&mut self, input: String) -> Option<RequestContext> {
        self.generation = self.generation.checked_next()?;
        let request = RequestContext::new(self.id, self.generation);
        self.requests.replace_search(request.cancellation.clone());
        self.search_sources.clear();
        self.search_attribution.clear();
        if self.search_history.last() != Some(&input) {
            self.search_history.push(input.clone());
            if self.search_history.len() > 32 {
                self.search_history.remove(0);
            }
        }
        self.search = TabSearchState::Loading {
            request: request.clone(),
            input,
            results: DirectorySnapshot::default(),
        };
        self.selection.clear();
        Some(request)
    }

    /// Returns the search result snapshot when a search surface owns the file view.
    pub fn search_results(&self) -> Option<&DirectorySnapshot> {
        match &self.search {
            TabSearchState::Loading { results, .. }
            | TabSearchState::Ready { results, .. }
            | TabSearchState::Partial { results, .. }
            | TabSearchState::Cancelled { results, .. }
            | TabSearchState::Error { results, .. } => Some(results),
            TabSearchState::Idle | TabSearchState::Editing(_) => None,
        }
    }

    /// The snapshot currently presented by `FileViewHost`.
    pub fn visible_snapshot(&self) -> Option<&DirectorySnapshot> {
        self.search_results().or_else(|| self.directory.snapshot())
    }

    /// Adapts search state to the existing file-view loading/ready/error presentation contract.
    pub fn visible_directory_state(&self) -> DirectoryState {
        match &self.search {
            TabSearchState::Loading {
                request, results, ..
            } => DirectoryState::Loading {
                request: request.clone(),
                snapshot: results.clone(),
                seen: results
                    .entries()
                    .iter()
                    .map(|entry| entry.id.clone())
                    .collect(),
            },
            TabSearchState::Ready { results, .. } | TabSearchState::Cancelled { results, .. } => {
                DirectoryState::Ready(results.clone())
            }
            TabSearchState::Partial { results, error, .. }
            | TabSearchState::Error { results, error, .. } => DirectoryState::Error {
                error: error.clone(),
                previous: results.clone(),
            },
            TabSearchState::Idle | TabSearchState::Editing(_) => self.directory.clone(),
        }
    }

    /// Merges a correlated batch and rejects late generations without mutating current results.
    ///
    /// # Errors
    ///
    /// Returns a correlation rejection for a cancelled, replaced, or otherwise stale request.
    pub fn merge_search_batch(
        &mut self,
        event: &RequestContext,
        source: crate::SearchBackend,
        entries: impl IntoIterator<Item = FileEntry>,
    ) -> Result<(), RequestRejection> {
        let TabSearchState::Loading {
            request, results, ..
        } = &mut self.search
        else {
            return Err(RequestRejection::RequestId);
        };
        request.validate_event(event)?;
        for entry in entries {
            let sources = self.search_attribution.entry(entry.id.clone()).or_default();
            if !sources.contains(&source) {
                sources.push(source);
            }
            results.upsert(entry);
        }
        Ok(())
    }

    /// Updates one backend status for the correlated active search.
    ///
    /// # Errors
    ///
    /// Returns a correlation rejection for a cancelled, replaced, or otherwise stale request.
    pub fn update_search_status(
        &mut self,
        event: &RequestContext,
        status: crate::SearchSourceStatus,
    ) -> Result<(), RequestRejection> {
        let TabSearchState::Loading { request, .. } = &self.search else {
            return Err(RequestRejection::RequestId);
        };
        request.validate_event(event)?;
        if let Some(existing) = self
            .search_sources
            .iter_mut()
            .find(|existing| existing.backend == status.backend)
        {
            *existing = status;
        } else {
            self.search_sources.push(status);
        }
        Ok(())
    }

    /// Publishes exactly one correlated terminal search state while retaining partial results.
    ///
    /// # Errors
    ///
    /// Returns a correlation rejection for a cancelled, replaced, or otherwise stale request.
    pub fn finish_search(
        &mut self,
        event: &RequestContext,
        outcome: crate::SearchTerminal,
    ) -> Result<(), RequestRejection> {
        let TabSearchState::Loading { request, .. } = &self.search else {
            return Err(RequestRejection::RequestId);
        };
        request.validate_event(event)?;
        let TabSearchState::Loading { input, results, .. } = std::mem::take(&mut self.search)
        else {
            unreachable!("search state was checked")
        };
        self.search = match outcome {
            crate::SearchTerminal::Finished => TabSearchState::Ready { input, results },
            crate::SearchTerminal::Cancelled => TabSearchState::Cancelled { input, results },
            crate::SearchTerminal::Failed(error) if results.entries().is_empty() => {
                TabSearchState::Error {
                    input,
                    results,
                    error,
                }
            }
            crate::SearchTerminal::Partial(error) | crate::SearchTerminal::Failed(error) => {
                TabSearchState::Partial {
                    input,
                    results,
                    error,
                }
            }
        };
        Ok(())
    }

    /// Leaves search and restores the unchanged directory/history presentation.
    pub fn cancel_search(&mut self) {
        if let TabSearchState::Loading { request, .. } = &self.search {
            request.cancellation.cancel();
        }
        self.search = TabSearchState::Idle;
        self.search_sources.clear();
        self.search_attribution.clear();
    }
}

#[cfg(test)]
mod tests {
    use explorer_common::{ExplorerError, ExplorerErrorKind};

    use super::*;

    fn location(name: &str) -> LocationDescriptor {
        LocationDescriptor::file_system(format!(r"C:\fixture\{name}"))
    }

    fn id(value: u8) -> ShellItemId {
        ShellItemId::from_provider_bytes([value]).expect("non-empty id")
    }

    fn entry(identity: u8, name: &str) -> FileEntry {
        FileEntry {
            id: id(identity),
            display_name: name.to_owned(),
            location: location(name),
            is_container: false,
            metadata: FileEntryMetadata::default(),
        }
    }

    #[test]
    fn virtual_location_has_address_breadcrumb_parent_and_history_semantics() {
        let root =
            LocationDescriptor::try_virtual("rust-7z", [9; 16], 5, None, vec![]).expect("root");
        let nested = LocationDescriptor::try_virtual(
            "rust-7z",
            [9; 16],
            5,
            Some(81),
            vec!["資料".to_owned(), "nested".to_owned()],
        )
        .expect("nested");
        let address = AddressBarState::for_entry(&HistoryEntry::new(nested.clone(), "nested"));
        assert_eq!(
            address.draft,
            "rust-7z://09090909090909090909090909090909/5/資料/nested"
        );
        assert_eq!(address.resolved_ancestry.len(), 3);
        assert_eq!(address.resolved_ancestry[0].location, root);
        assert_eq!(
            address.resolved_ancestry[0].icon_hint,
            BreadcrumbIconHint::Archive
        );
        assert_eq!(
            nested.virtual_parent(),
            Some(address.resolved_ancestry[1].location.clone())
        );

        let mut history = NavigationHistory::with_initial(HistoryEntry::new(root.clone(), "root"));
        history.commit_navigation(HistoryEntry::new(nested.clone(), "nested"));
        assert_eq!(history.go_back().map(|entry| &entry.location), Some(&root));
        assert_eq!(
            history.go_forward().map(|entry| &entry.location),
            Some(&nested)
        );
    }

    #[test]
    fn filesystem_breadcrumbs_distinguish_drive_folder_and_archive_icons() {
        let address = AddressBarState::for_entry(&HistoryEntry::new(
            LocationDescriptor::file_system(r"D:\fixture\bundle.zip"),
            "bundle.zip",
        ));
        assert_eq!(address.resolved_ancestry.len(), 3);
        assert_eq!(
            address.resolved_ancestry[0].icon_hint,
            BreadcrumbIconHint::Drive
        );
        assert_eq!(
            address.resolved_ancestry[1].icon_hint,
            BreadcrumbIconHint::Folder
        );
        assert_eq!(
            address.resolved_ancestry[2].icon_hint,
            BreadcrumbIconHint::Archive
        );
    }

    #[test]
    fn filesystem_drive_breadcrumb_name_is_stable_and_does_not_use_the_volume_label() {
        let mut drive = BreadcrumbSegment {
            id: BreadcrumbSegmentId(7),
            display_name: "新增磁碟區 (D:)".to_owned(),
            location: LocationDescriptor::file_system(r"d:\"),
            icon_hint: BreadcrumbIconHint::Drive,
            is_container: true,
        };
        drive.stabilize_display_name();
        assert_eq!(drive.display_name, "D:");

        drive.location = LocationDescriptor::file_system(r"\\?\d:\");
        drive.display_name = r"\\?\d:".to_owned();
        drive.stabilize_display_name();
        assert_eq!(drive.display_name, "D:");

        let mut folder = BreadcrumbSegment {
            icon_hint: BreadcrumbIconHint::Folder,
            display_name: "新增磁碟區 (D:)".to_owned(),
            ..drive
        };
        folder.stabilize_display_name();
        assert_eq!(folder.display_name, "新增磁碟區 (D:)");
    }

    #[test]
    fn breadcrumb_roving_focus_menu_pages_and_typeahead_are_tab_local_state() {
        let initial = HistoryEntry::new(location("Current"), "Current");
        let mut address = AddressBarState::for_entry(&initial);
        assert_eq!(
            address
                .focused_segment()
                .map(|segment| segment.display_name.as_str()),
            Some("Current")
        );
        assert!(address.move_segment_focus(-127));
        assert_eq!(address.keyboard_segment_index, Some(0));
        assert!(address.move_segment_focus(127));
        let segment = address.focused_segment().expect("current segment").id;
        let generation = address.begin_menu(segment).expect("open menu");
        let children = (0..20)
            .map(|index| BreadcrumbMenuItem {
                display_name: if index == 17 {
                    "Unicode 資料夾".to_owned()
                } else {
                    format!("Folder {index:02}")
                },
                location: location(&format!("child-{index}")),
            })
            .collect();
        assert!(address.finish_menu(segment, generation, Ok(children)));
        assert_eq!(address.keyboard_menu_index, Some(0));
        assert!(address.move_menu_focus(MenuFocusMovement::PageNext));
        assert_eq!(address.keyboard_menu_index, Some(8));
        assert!(address.move_menu_focus(MenuFocusMovement::Last));
        assert_eq!(address.keyboard_menu_index, Some(19));
        assert!(address.typeahead_menu_focus("u"));
        assert_eq!(address.keyboard_menu_index, Some(17));
        assert!(address.typeahead_menu_focus("n"));
        assert_eq!(
            address
                .focused_menu_item()
                .map(|item| item.display_name.as_str()),
            Some("Unicode 資料夾")
        );
        assert!(address.set_menu_focus(3));
        assert_eq!(address.keyboard_menu_index, Some(3));
        assert!(!address.set_menu_focus(3));
        assert!(!address.set_menu_focus(20));
        address.close_menu();
        assert_eq!(address.keyboard_menu_index, None);
        assert!(address.menu_typeahead.is_empty());
        assert!(!address.set_menu_focus(0));
    }

    #[test]
    fn canonical_filesystem_history_exposes_complete_editable_address_text() {
        let path = r"C:\Users\fixture\Documents";
        let entry = HistoryEntry::new(LocationDescriptor::file_system(path), "Documents");
        let mut address = AddressBarState::for_entry(&entry);

        assert_eq!(address.draft, path);
        address.enter_editing();
        assert_eq!(address.mode, AddressBarMode::Editing);
        assert_eq!(address.draft, path);
    }

    #[test]
    fn history_commits_only_success_and_refresh_does_not_add_entry() {
        let initial = HistoryEntry::new(location("A"), "A");
        let mut history = NavigationHistory::with_initial(initial.clone());

        // A failed navigation never calls commit and therefore cannot mutate history.
        let _failure = ExplorerError::new(
            ExplorerErrorKind::Availability,
            "navigate",
            true,
            "Folder unavailable",
            "test failure",
        );
        assert_eq!(history.current(), Some(&initial));
        assert!(!history.can_go_back());

        history.commit_navigation(HistoryEntry::new(location("B"), "B"));
        assert!(history.can_go_back());
        assert_eq!(
            history.go_back().map(|entry| entry.display_title.as_str()),
            Some("A")
        );
        assert!(history.can_go_forward());
        assert_eq!(
            history
                .go_forward()
                .map(|entry| entry.display_title.as_str()),
            Some("B")
        );

        history.commit_navigation(HistoryEntry::new(location("B"), "B refreshed"));
        assert_eq!(
            history.go_back().map(|entry| entry.display_title.as_str()),
            Some("A")
        );
        assert!(!history.can_go_back());
    }

    #[test]
    fn multi_step_history_jumps_commit_crossed_entries_atomically() {
        let mut history = NavigationHistory::with_initial(HistoryEntry::new(location("A"), "A"));
        for name in ["B", "C", "D"] {
            history.commit_navigation(HistoryEntry::new(location(name), name));
        }
        assert_eq!(
            history
                .back_destination_at(2)
                .map(|entry| entry.display_title.as_str()),
            Some("B")
        );
        let unchanged = history.clone();
        assert!(!history.commit_back_steps(HistoryEntry::new(location("wrong"), "wrong"), 2));
        assert_eq!(history, unchanged);

        assert!(history.commit_back_steps(HistoryEntry::new(location("B"), "B resolved"), 2));
        assert_eq!(
            history.current().map(|entry| entry.display_title.as_str()),
            Some("B resolved")
        );
        assert_eq!(
            history
                .back_entries()
                .iter()
                .map(|entry| entry.display_title.as_str())
                .collect::<Vec<_>>(),
            ["A"]
        );
        assert_eq!(
            history
                .forward_entries()
                .iter()
                .map(|entry| entry.display_title.as_str())
                .collect::<Vec<_>>(),
            ["D", "C"]
        );

        assert!(history.commit_forward_steps(HistoryEntry::new(location("D"), "D resolved"), 2));
        assert_eq!(
            history.current().map(|entry| entry.display_title.as_str()),
            Some("D resolved")
        );
        assert_eq!(
            history
                .back_entries()
                .iter()
                .map(|entry| entry.display_title.as_str())
                .collect::<Vec<_>>(),
            ["A", "B resolved", "C"]
        );
        assert!(history.forward_entries().is_empty());
    }

    #[test]
    fn snapshot_diff_uses_identity_and_rename_preserves_selection() {
        let mut snapshot = DirectorySnapshot::default();
        assert_eq!(
            snapshot.upsert(entry(1, "old.txt")),
            PresentationChange::Inserted(id(1))
        );
        assert_eq!(
            snapshot.upsert(entry(2, "other.txt")),
            PresentationChange::Inserted(id(2))
        );

        let mut selection = SelectionModel::default();
        selection.select_only(id(1));
        assert_eq!(
            snapshot.upsert(entry(1, "renamed.txt")),
            PresentationChange::Updated(id(1))
        );
        selection.reconcile(&snapshot);
        assert!(selection.contains(&id(1)));
        assert_eq!(snapshot.entries()[0].display_name, "renamed.txt");

        assert_eq!(snapshot.remove(&id(1)), PresentationChange::Removed(id(1)));
        selection.reconcile(&snapshot);
        assert!(selection.is_empty());
    }

    #[test]
    fn snapshot_revision_shared_storage_and_sort_keys_track_real_mutations() {
        let mut snapshot = DirectorySnapshot::default();
        assert_eq!(snapshot.revision(), 0);
        assert_eq!(
            snapshot.upsert(entry(1, "Mixed.JpG")),
            PresentationChange::Inserted(id(1))
        );
        assert_eq!(snapshot.revision(), 1);
        assert_eq!(
            snapshot.sort_keys(0).map(FileEntrySortKeys::display_name),
            Some("mixed.jpg")
        );

        let shared = snapshot.shared_entries();
        let cloned = snapshot.clone();
        assert!(Arc::ptr_eq(&shared, &cloned.shared_entries()));
        assert_eq!(
            snapshot.upsert(entry(1, "Mixed.JpG")),
            PresentationChange::Unchanged
        );
        assert_eq!(snapshot.revision(), 1);

        assert_eq!(
            snapshot.upsert(entry(1, "Renamed.JPG")),
            PresentationChange::Updated(id(1))
        );
        assert_eq!(snapshot.revision(), 2);
        assert!(!Arc::ptr_eq(&shared, &snapshot.shared_entries()));
        assert_eq!(
            snapshot.sort_keys(0).map(FileEntrySortKeys::display_name),
            Some("renamed.jpg")
        );
        assert_eq!(cloned.entries()[0].display_name, "Mixed.JpG");

        snapshot.retain(|_| true);
        assert_eq!(snapshot.revision(), 2);
        assert_eq!(snapshot.remove(&id(1)), PresentationChange::Removed(id(1)));
        assert_eq!(snapshot.revision(), 3);
        assert!(snapshot.entries().is_empty());
    }

    #[test]
    fn accepted_batch_advances_one_revision_and_stale_batch_advances_none() {
        let mut tab = TabState::new(HistoryEntry::new(
            LocationDescriptor::file_system(r"C:\fixture"),
            "fixture",
        ));
        let current = tab.begin_navigation_request().expect("current request");
        let stale = RequestContext::new(current.tab_id, Generation::default());
        assert!(
            tab.directory
                .merge_batch(&stale, [entry(90, "stale.txt")])
                .is_err()
        );
        assert_eq!(
            tab.directory
                .snapshot()
                .expect("loading snapshot")
                .revision(),
            0
        );

        tab.directory
            .merge_batch(
                &current,
                [
                    entry(1, "one.txt"),
                    entry(2, "two.txt"),
                    entry(3, "three.txt"),
                ],
            )
            .expect("accepted batch");
        assert_eq!(
            tab.directory
                .snapshot()
                .expect("loading snapshot")
                .revision(),
            1
        );
        tab.directory.finish(&current).expect("finish");
        assert_eq!(
            tab.directory.snapshot().expect("ready snapshot").revision(),
            1
        );
    }

    #[test]
    fn selection_anchor_supports_toggle_and_inclusive_ranges() {
        let order = vec![id(1), id(2), id(3), id(4)];
        let mut selection = SelectionModel::default();
        selection.select_only(id(2));
        selection.select_range(&order, id(4), false);
        assert_eq!(selection.anchor(), Some(&id(2)));
        assert!(selection.contains(&id(2)));
        assert!(selection.contains(&id(3)));
        assert!(selection.contains(&id(4)));
        let mut shared = selection.clone();
        assert!(Arc::ptr_eq(&selection.selected, &shared.selected));
        shared.toggle(id(1));
        assert!(!Arc::ptr_eq(&selection.selected, &shared.selected));

        selection.toggle(id(3));
        assert!(!selection.contains(&id(3)));
        selection.select_range(&order, id(1), true);
        assert!(selection.contains(&id(1)));
        assert!(selection.contains(&id(2)));
        assert!(selection.contains(&id(4)));
    }

    #[test]
    fn select_all_and_invert_preserve_stable_id_semantics() {
        let order = vec![id(1), id(2), id(3)];
        let mut selection = SelectionModel::default();
        selection.select_only(id(2));
        selection.invert(&order);
        assert!(selection.contains(&id(1)));
        assert!(!selection.contains(&id(2)));
        assert!(selection.contains(&id(3)));
        selection.select_all(&order);
        assert_eq!(selection.len(), 3);
    }

    #[test]
    fn new_generation_cancels_old_and_rejects_late_batches() {
        let mut tab = TabState::new(HistoryEntry::new(location("A"), "A"));
        let first = tab.begin_navigation_request().expect("first generation");
        tab.directory
            .merge_batch(&first, [entry(1, "partial-old.txt")])
            .expect("first batch");
        let second = tab.begin_navigation_request().expect("second generation");
        assert!(first.cancellation.is_cancelled());
        assert_eq!(
            tab.directory.merge_batch(&first, [entry(1, "late.txt")]),
            Err(RequestRejection::RequestId)
        );
        assert_eq!(
            tab.directory
                .merge_batch(&second, [entry(2, "current.txt")]),
            Ok(vec![PresentationChange::Inserted(id(2))])
        );
        assert_eq!(tab.directory.finish(&second), Ok(()));
        let DirectoryState::Ready(snapshot) = &tab.directory else {
            panic!("expected ready state")
        };
        assert!(snapshot.get(&id(1)).is_none());
        assert!(snapshot.get(&id(2)).is_some());
    }

    #[test]
    fn search_generation_rejects_late_results_and_restores_directory_snapshot() {
        let mut tab = TabState::new(HistoryEntry::new(location("root"), "root"));
        let navigation = tab.begin_navigation_request().unwrap();
        tab.directory
            .merge_batch(&navigation, [entry(1, "directory.txt")])
            .unwrap();
        tab.directory.finish(&navigation).unwrap();

        let first = tab.begin_search_request("first".to_owned()).unwrap();
        let second = tab.begin_search_request("second".to_owned()).unwrap();
        assert!(first.cancellation.is_cancelled());
        assert_eq!(
            tab.merge_search_batch(
                &first,
                crate::SearchBackend::WindowsIndex,
                [entry(2, "late.txt")],
            ),
            Err(RequestRejection::RequestId)
        );
        tab.merge_search_batch(
            &second,
            crate::SearchBackend::FileSystemFallback,
            [entry(3, "result.txt")],
        )
        .unwrap();
        tab.finish_search(&second, crate::SearchTerminal::Finished)
            .unwrap();
        assert_eq!(
            tab.visible_snapshot().unwrap().entries()[0].display_name,
            "result.txt"
        );

        tab.cancel_search();
        assert_eq!(
            tab.visible_snapshot().unwrap().entries()[0].display_name,
            "directory.txt"
        );
        assert_eq!(tab.history.current().unwrap().display_title, "root");

        let partial = tab.begin_search_request("partial".to_owned()).unwrap();
        tab.merge_search_batch(
            &partial,
            crate::SearchBackend::WindowsIndex,
            [entry(4, "kept-after-source-error.txt")],
        )
        .unwrap();
        tab.finish_search(
            &partial,
            crate::SearchTerminal::Partial(ExplorerError::new(
                ExplorerErrorKind::Availability,
                "search source",
                true,
                "部分來源失敗",
                "injected backend failure",
            )),
        )
        .unwrap();
        assert!(matches!(tab.search, TabSearchState::Partial { .. }));
        assert_eq!(tab.visible_snapshot().unwrap().entries().len(), 1);

        let failed = tab.begin_search_request("failure".to_owned()).unwrap();
        tab.finish_search(
            &failed,
            crate::SearchTerminal::Failed(ExplorerError::new(
                ExplorerErrorKind::Availability,
                "search backend",
                true,
                "搜尋失敗",
                "injected complete backend failure",
            )),
        )
        .unwrap();
        assert!(matches!(tab.search, TabSearchState::Error { .. }));

        let empty = tab.begin_search_request("empty".to_owned()).unwrap();
        tab.finish_search(&empty, crate::SearchTerminal::Finished)
            .unwrap();
        assert!(matches!(tab.search, TabSearchState::Ready { .. }));
        assert!(tab.visible_snapshot().unwrap().entries().is_empty());
    }

    #[test]
    fn refresh_preserves_rows_and_failure_keeps_recoverable_snapshot() {
        let mut tab = TabState::new(HistoryEntry::new(location("A"), "A"));
        let initial = tab.begin_navigation_request().expect("initial generation");
        tab.directory
            .merge_batch(&initial, [entry(1, "kept.txt")])
            .expect("initial batch");
        tab.directory.finish(&initial).expect("initial finish");

        let refresh = tab.begin_refresh_request().expect("refresh generation");
        let DirectoryState::Loading { snapshot, .. } = &tab.directory else {
            panic!("expected loading refresh")
        };
        assert!(snapshot.get(&id(1)).is_some());
        let error = ExplorerError::new(
            ExplorerErrorKind::Availability,
            "refresh",
            true,
            "Could not refresh folder",
            "test failure",
        );
        tab.directory.fail(&refresh, error).expect("fail refresh");
        let DirectoryState::Error { previous, .. } = &tab.directory else {
            panic!("expected error state")
        };
        assert!(previous.get(&id(1)).is_some());
    }

    #[test]
    fn address_bar_state_enforces_edit_menu_error_and_resolve_transitions() {
        let current = HistoryEntry::new(location("A"), "A");
        let mut address = AddressBarState::for_entry(&current);
        assert!(matches!(address.mode, AddressBarMode::Browsing));
        assert!(!address.resolved_ancestry.is_empty());
        assert!(!address.update_draft(r"C:\blocked".to_owned()));

        address.enter_editing();
        assert!(address.update_draft(r"C:\typed".to_owned()));
        address.navigation_failed("not found".to_owned());
        assert!(matches!(address.mode, AddressBarMode::NavigationError));
        assert_eq!(address.draft, r"C:\typed");
        address.cancel_editing(&current);
        assert!(matches!(address.mode, AddressBarMode::Browsing));
        assert!(address.toggle_overflow());
        assert!(address.overflow_open);
        assert!(!address.toggle_overflow());
        assert!(!address.overflow_open);

        let segment = address.resolved_ancestry[0].id;
        let generation = address.begin_menu(segment).expect("container menu");
        assert!(matches!(
            address.mode,
            AddressBarMode::EnumeratingMenu { generation: active, .. } if active == generation
        ));
        assert!(address.menu_loading);
        assert!(!address.finish_menu(segment, generation + 1, Ok(Vec::new())));
        assert!(address.finish_menu(
            segment,
            generation,
            Ok(vec![BreadcrumbMenuItem {
                display_name: "child".to_owned(),
                location: location("child"),
            }]),
        ));
        assert!(!address.menu_loading);
        assert_eq!(address.menu_children[0].display_name, "child");
        address.close_menu();
        assert!(matches!(address.mode, AddressBarMode::Browsing));

        let resolved = HistoryEntry::new(location("B"), "B");
        address.resolve(&resolved);
        assert!(address.draft.ends_with('B'));
        assert!(address.error.is_none());
    }

    #[test]
    fn breadcrumb_ids_follow_location_identity_not_display_text_or_row_index() {
        let first = AddressBarState::for_entry(&HistoryEntry::new(location("same"), "label one"));
        let second = AddressBarState::for_entry(&HistoryEntry::new(location("same"), "label two"));
        assert_eq!(first.resolved_ancestry, second.resolved_ancestry);
        let other = AddressBarState::for_entry(&HistoryEntry::new(location("other"), "label one"));
        assert_ne!(
            first.resolved_ancestry.last().map(|segment| segment.id),
            other.resolved_ancestry.last().map(|segment| segment.id)
        );
    }

    #[test]
    fn drive_capacity_fraction_and_low_space_are_bounded() {
        let healthy = DriveMetadata {
            kind: DriveKind::Fixed,
            availability: DriveAvailability::Available,
            volume_label: Some("Data".to_owned()),
            filesystem_name: Some("NTFS".to_owned()),
            total_bytes: Some(1_000),
            available_bytes: Some(400),
        };
        assert_eq!(healthy.used_fraction(), Some(0.6));
        assert!(!healthy.is_low_space());

        let low = DriveMetadata {
            total_bytes: Some(100 * 1024 * 1024 * 1024),
            available_bytes: Some(5 * 1024 * 1024 * 1024),
            ..healthy.clone()
        };
        assert!(low.is_low_space());
        assert_eq!(low.used_fraction(), Some(0.95));

        let invalid = DriveMetadata {
            total_bytes: Some(10),
            available_bytes: Some(11),
            ..healthy
        };
        assert_eq!(invalid.used_fraction(), None);
    }

    #[test]
    fn unavailable_extension_view_falls_back_without_forgetting_or_auto_reactivating() {
        let mut settings = ViewSettings {
            mode: ViewMode::Details,
            extension_view_id: Some("extension:org.example:size-map".to_owned()),
            ..ViewSettings::default()
        };
        assert_eq!(settings.effective_extension_view_id(|_| false), None);
        assert_eq!(settings.mode, ViewMode::Details);
        assert_eq!(
            settings.extension_view_id.as_deref(),
            Some("extension:org.example:size-map")
        );
        // Merely becoming available does not mutate current model state. The
        // UI must still receive an explicit user action before switching.
        assert_eq!(
            settings.effective_extension_view_id(|id| id.ends_with("size-map")),
            Some("extension:org.example:size-map")
        );
        assert_eq!(settings.mode, ViewMode::Details);
        settings.extension_view_id = None;
        assert_eq!(settings.effective_extension_view_id(|_| true), None);
    }

    #[test]
    fn icon_cache_memory_presets_default_to_32_mib_and_cap_at_one_gib() {
        assert_eq!(ViewSettings::default().icon_cache_memory_mb, 32);
        assert_eq!(normalized_icon_cache_memory_mb(31), 32);
        assert_eq!(normalized_icon_cache_memory_mb(64), 64);
        assert_eq!(normalized_icon_cache_memory_mb(127), 128);
        assert_eq!(normalized_icon_cache_memory_mb(300), 256);
        assert_eq!(normalized_icon_cache_memory_mb(900), 1_024);
        assert_eq!(normalized_icon_cache_memory_mb(u16::MAX), 1_024);
    }

    #[test]
    fn thumbnail_cache_defaults_to_128_mib_and_is_independent_from_icons() {
        let mut settings = ViewSettings::default();
        assert_eq!(settings.icon_cache_memory_mb, 32);
        assert_eq!(settings.thumbnail_cache_memory_mb, 128);
        settings.icon_cache_memory_mb = normalized_icon_cache_memory_mb(512);
        assert_eq!(settings.thumbnail_cache_memory_mb, 128);
        settings.thumbnail_cache_memory_mb = normalized_thumbnail_cache_memory_mb(64);
        assert_eq!(settings.icon_cache_memory_mb, 512);
        assert_eq!(settings.thumbnail_cache_memory_mb, 64);
        assert_eq!(normalized_thumbnail_cache_memory_mb(u16::MAX), 1_024);
    }

    #[test]
    fn mft_folder_cache_is_numeric_clamped_and_defaults_to_512_mib() {
        assert_eq!(ViewSettings::default().mft_folder_cache_memory_mb, 512);
        assert_eq!(normalized_mft_folder_cache_memory_mb(0), 128);
        assert_eq!(normalized_mft_folder_cache_memory_mb(127), 128);
        assert_eq!(normalized_mft_folder_cache_memory_mb(333), 333);
        assert_eq!(normalized_mft_folder_cache_memory_mb(2_048), 2_048);
        assert_eq!(normalized_mft_folder_cache_memory_mb(4_096), 4_096);
        assert_eq!(normalized_mft_folder_cache_memory_mb(4_097), 4_097);
        assert_eq!(normalized_mft_folder_cache_memory_mb(16_384), 16_384);
        assert_eq!(normalized_mft_folder_cache_memory_mb(16_385), 16_384);
        assert_eq!(normalized_mft_folder_cache_memory_mb(u16::MAX), 16_384);
    }
}
