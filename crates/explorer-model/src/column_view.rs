//! Finder-style local column branch, eligibility, and auxiliary listing coordination.
//!
//! Ancestor listings are not tab navigation. The active tab location remains the deepest selected
//! directory; this module only decides which ancestor columns to retain, load, or discard.

use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

use explorer_common::{ExplorerError, ExplorerErrorKind, RequestId};

use crate::{
    DirectorySnapshot, DriveKind, FileEntry, Generation, ItemDescriptor, LocationDescriptor,
    NavigationHistory, RequestContext, ShellItemId, TabId, ViewMode, is_wsl_unc_path,
    network_unc_parts,
};

pub const COLUMN_WIDTH_DEFAULT: u16 = 240;
pub const COLUMN_WIDTH_MIN: u16 = 180;
pub const COLUMN_WIDTH_MAX: u16 = 480;
pub const COLUMN_PREVIEW_WIDTH_DEFAULT: u16 = 280;
pub const COLUMN_PREVIEW_WIDTH_MIN: u16 = 180;
pub const COLUMN_PREVIEW_WIDTH_MAX: u16 = 640;
pub const COLUMN_LOAD_CONCURRENCY: usize = 2;
/// Two 16px filename lines plus 4px of breathing room. Scroll, virtualization,
/// keyboard reveal, and hit testing all use this fixed height.
pub const COLUMN_ROW_HEIGHT: f32 = 36.0;
const MAX_PERSISTED_COLUMN_WIDTHS: usize = 32;

pub const fn normalized_column_width(value: u16) -> u16 {
    if value < COLUMN_WIDTH_MIN {
        COLUMN_WIDTH_MIN
    } else if value > COLUMN_WIDTH_MAX {
        COLUMN_WIDTH_MAX
    } else {
        value
    }
}

pub const fn normalized_column_preview_width(value: u16) -> u16 {
    if value < COLUMN_PREVIEW_WIDTH_MIN {
        COLUMN_PREVIEW_WIDTH_MIN
    } else if value > COLUMN_PREVIEW_WIDTH_MAX {
        COLUMN_PREVIEW_WIDTH_MAX
    } else {
        value
    }
}

pub fn normalized_column_widths(values: &[u16]) -> Vec<u16> {
    values
        .iter()
        .take(MAX_PERSISTED_COLUMN_WIDTHS)
        .copied()
        .map(normalized_column_width)
        .collect()
}

/// Effective built-in mode. Columns is kept as the selected preference even when this returns
/// Details.
pub fn effective_view_mode(
    selected: ViewMode,
    location: Option<&LocationDescriptor>,
    media: DriveKind,
) -> ViewMode {
    if selected != ViewMode::Columns {
        return selected;
    }
    match location {
        Some(location) if columns_location_eligible(location, media) => ViewMode::Columns,
        _ => ViewMode::Details,
    }
}

pub fn columns_location_eligible(location: &LocationDescriptor, media: DriveKind) -> bool {
    let LocationDescriptor::FileSystem(path) = location else {
        return false;
    };
    if is_wsl_unc_path(path) || network_unc_parts(path).is_some() {
        return false;
    }
    if !is_drive_letter_directory(path) {
        return false;
    }
    matches!(media, DriveKind::Fixed | DriveKind::Removable)
}

fn is_drive_letter_directory(path: &Path) -> bool {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return false;
    };
    matches!(prefix.kind(), std::path::Prefix::Disk(_))
        && matches!(components.next(), Some(Component::RootDir))
}

/// Ancestor chain including the directory itself. `C:\a\b` yields `C:\`, `C:\a`, `C:\a\b`.
pub fn filesystem_column_chain(path: &Path) -> Vec<PathBuf> {
    let mut current = PathBuf::new();
    let mut chain = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => current.push(component.as_os_str()),
            Component::RootDir => {
                current.push(component.as_os_str());
                chain.push(current.clone());
            }
            Component::Normal(name) => {
                current.push(name);
                chain.push(current.clone());
            }
            Component::CurDir | Component::ParentDir => {}
        }
    }
    chain
}

pub fn column_resolved_key(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('/', r"\");
    if text.len() > 3 && text.ends_with('\\') {
        text.pop();
    }
    text.to_ascii_lowercase()
}

/// Volume and file identity produced by `filesystem_identity` on the shell thread.
///
/// The leading `F` distinguishes a resolved file id from a path fallback. Comparing these
/// values detects a junction that points at an ancestor even when the literal paths differ.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnFilesystemIdentity(Vec<u8>);

impl ColumnFilesystemIdentity {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.first() == Some(&b'F') && bytes.len() >= 25 {
            Some(Self(bytes.to_vec()))
        } else {
            None
        }
    }

    pub fn from_shell_item(id: &ShellItemId) -> Option<Self> {
        Self::from_bytes(id.provider_bytes())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Result of resolving one clicked folder against its column ancestors off the UI thread.
#[derive(Clone, Debug)]
pub enum ColumnCycleOutcome {
    /// The clicked folder is the same file as `matched`.
    Cycle { matched: LocationDescriptor },
    /// Identities for the compared locations, including the clicked folder when it resolved.
    Distinct {
        identities: Vec<(LocationDescriptor, ColumnFilesystemIdentity)>,
    },
    /// The location is not a local column target. Do not open it.
    Unsupported(ExplorerError),
    /// Identity could not be read. Availability still opens the literal folder.
    /// Cancellation must not.
    Failed(ExplorerError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ColumnPhase {
    Pending,
    Loading,
    Ready(DirectorySnapshot),
    Empty,
    Error(ColumnFault),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnFault {
    Inaccessible,
    Cycle,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnLevel {
    pub location: LocationDescriptor,
    pub resolved_key: String,
    pub branch_child: Option<ShellItemId>,
    /// Resolved file identity of this directory, when the shell has already produced one.
    pub filesystem_id: Option<ColumnFilesystemIdentity>,
    pub phase: ColumnPhase,
    pub vertical_offset: f32,
    pub request_id: Option<RequestId>,
    pub width: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnBranch {
    revision: u64,
    levels: Vec<ColumnLevel>,
    active: usize,
    horizontal_offset: f32,
    pending_file: Option<ShellItemId>,
    /// Set while a column click has moved the branch and history has not committed yet.
    pending_navigation: Option<LocationDescriptor>,
    selected: Vec<ShellItemId>,
    anchor: Option<ShellItemId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnSelectEffect {
    pub navigate_to: Option<LocationDescriptor>,
    pub record_history: bool,
    pub pending_file: Option<ShellItemId>,
    pub clear_preview: bool,
    pub open_file: bool,
}

impl ColumnBranch {
    pub fn from_location(location: LocationDescriptor, default_width: u16) -> Self {
        let mut branch = Self {
            revision: 1,
            levels: Vec::new(),
            active: 0,
            horizontal_offset: 0.0,
            pending_file: None,
            pending_navigation: None,
            selected: Vec::new(),
            anchor: None,
        };
        branch.align_to_location(location, default_width, None);
        branch
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn levels(&self) -> &[ColumnLevel] {
        &self.levels
    }

    pub const fn active_index(&self) -> usize {
        self.active
    }

    pub const fn horizontal_offset(&self) -> f32 {
        self.horizontal_offset
    }

    pub fn set_horizontal_offset(&mut self, offset: f32) {
        self.horizontal_offset = offset.max(0.0);
    }

    pub fn pending_file(&self) -> Option<&ShellItemId> {
        self.pending_file.as_ref()
    }

    pub fn take_pending_file(&mut self) -> Option<ShellItemId> {
        self.pending_file.take()
    }

    pub fn hold_pending_navigation(&mut self, location: LocationDescriptor) {
        self.pending_navigation = Some(location);
    }

    pub fn clear_pending_navigation(&mut self) {
        self.pending_navigation = None;
    }

    pub fn pending_navigation(&self) -> Option<&LocationDescriptor> {
        self.pending_navigation.as_ref()
    }

    pub fn selected_ids(&self) -> &[ShellItemId] {
        &self.selected
    }

    pub fn active_location(&self) -> Option<&LocationDescriptor> {
        self.levels.get(self.active).map(|level| &level.location)
    }

    /// Directory the tab has actually navigated to. Focus can move left without changing it.
    pub fn navigated_location(&self) -> Option<&LocationDescriptor> {
        self.levels.last().map(|level| &level.location)
    }

    pub fn align_to_location(
        &mut self,
        location: LocationDescriptor,
        default_width: u16,
        saved_widths: Option<&[u16]>,
    ) {
        let Some(path) = location.path().map(Path::to_path_buf) else {
            self.replace_levels(vec![self.level(
                location,
                String::new(),
                default_width,
                ColumnPhase::Error(ColumnFault::Unsupported),
            )]);
            return;
        };
        let chain = filesystem_column_chain(&path);
        if chain.is_empty() {
            self.replace_levels(vec![self.level(
                location,
                column_resolved_key(&path),
                default_width,
                ColumnPhase::Pending,
            )]);
            return;
        }
        let mut next: Vec<ColumnLevel> = Vec::new();
        let mut seen = Vec::new();
        for (index, directory) in chain.iter().enumerate() {
            let key = column_resolved_key(directory);
            if seen.iter().any(|existing| existing == &key) {
                if let Some(last) = next.last_mut() {
                    last.phase = ColumnPhase::Error(ColumnFault::Cycle);
                    last.branch_child = None;
                }
                break;
            }
            seen.push(key.clone());
            let width = saved_widths
                .and_then(|widths| widths.get(index).copied())
                .unwrap_or(default_width);
            let preserved = self
                .levels
                .iter()
                .find(|level| level.resolved_key == key)
                .cloned();
            let mut level = preserved.unwrap_or_else(|| {
                self.level(
                    LocationDescriptor::file_system(directory.clone()),
                    key,
                    normalized_column_width(width),
                    ColumnPhase::Pending,
                )
            });
            level.width = normalized_column_width(width);
            if index + 1 < chain.len() {
                let child_name = chain[index + 1]
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned());
                level.branch_child = child_name.and_then(|name| {
                    snapshot_child_id(&level, &name).or(level.branch_child.clone())
                });
            }
            next.push(level);
        }
        if let Some(last) = next.last_mut() {
            last.location = location;
            last.branch_child = self.pending_file.clone().or(last.branch_child.clone());
        }
        self.replace_levels(next);
    }

    fn replace_levels(&mut self, levels: Vec<ColumnLevel>) {
        let changed = self.levels.len() != levels.len()
            || self
                .levels
                .iter()
                .zip(levels.iter())
                .any(|(left, right)| left.location != right.location);
        self.levels = levels;
        if self.levels.is_empty() {
            self.active = 0;
            return;
        }
        if changed {
            self.bump();
            self.selected.clear();
            self.anchor = None;
        }
        self.active = self.levels.len() - 1;
    }

    pub fn select_child(
        &mut self,
        column: usize,
        entry: &FileEntry,
        resolved_key: Option<&str>,
        default_width: u16,
    ) -> ColumnSelectEffect {
        if column >= self.levels.len() {
            return self.inert_effect();
        }
        let current = self.levels.last().map(|level| level.location.clone());
        if entry.is_container {
            let already_current = current.as_ref() == Some(&entry.location);
            if already_current {
                self.levels[column].branch_child = Some(entry.id.clone());
                self.set_single_selection(column, entry.id.clone());
                return ColumnSelectEffect {
                    navigate_to: None,
                    record_history: false,
                    pending_file: None,
                    clear_preview: false,
                    open_file: false,
                };
            }
            if self.filesystem_identity_cycles(column, entry) {
                return self.mark_cycle(column, &entry.id);
            }
            let key = resolved_key
                .map(str::to_owned)
                .or_else(|| entry.location.path().map(column_resolved_key))
                .unwrap_or_default();
            if !key.is_empty()
                && self
                    .levels
                    .iter()
                    .take(column + 1)
                    .any(|level| level.resolved_key == key)
            {
                return self.mark_cycle(column, &entry.id);
            }
            self.truncate_after(column);
            self.levels[column].branch_child = Some(entry.id.clone());
            let width = self
                .levels
                .get(column)
                .map(|level| level.width)
                .unwrap_or(default_width);
            let mut child = self.level(entry.location.clone(), key, width, ColumnPhase::Pending);
            child.filesystem_id = ColumnFilesystemIdentity::from_shell_item(&entry.id);
            self.levels.push(child);
            self.active = self.levels.len() - 1;
            self.selected.clear();
            self.anchor = None;
            self.pending_file = None;
            self.bump();
            ColumnSelectEffect {
                navigate_to: (!already_current).then(|| entry.location.clone()),
                record_history: !already_current,
                pending_file: None,
                clear_preview: true,
                open_file: false,
            }
        } else if column + 1 == self.levels.len()
            && current.as_ref() == Some(&self.levels[column].location)
        {
            self.set_single_selection(column, entry.id.clone());
            self.levels[column].branch_child = Some(entry.id.clone());
            self.pending_file = None;
            ColumnSelectEffect {
                navigate_to: None,
                record_history: false,
                pending_file: None,
                clear_preview: false,
                open_file: false,
            }
        } else {
            self.truncate_after(column);
            self.active = column;
            self.pending_file = Some(entry.id.clone());
            self.set_single_selection(column, entry.id.clone());
            self.levels[column].branch_child = Some(entry.id.clone());
            let already_current = current.as_ref() == Some(&self.levels[column].location);
            self.bump();
            ColumnSelectEffect {
                navigate_to: (!already_current).then(|| self.levels[column].location.clone()),
                record_history: !already_current,
                pending_file: Some(entry.id.clone()),
                clear_preview: false,
                open_file: false,
            }
        }
    }

    pub fn toggle_additional(&mut self, column: usize, item_id: ShellItemId) {
        if column != self.active || column >= self.levels.len() {
            // A modifier click in another column replaces that column's selection.
            // It must not keep identities that belong to the column being left.
            self.set_single_selection(column, item_id);
            return;
        }
        if let Some(index) = self.selected.iter().position(|id| id == &item_id) {
            self.selected.remove(index);
        } else {
            self.selected.push(item_id.clone());
        }
        self.anchor = Some(item_id);
        self.active = column;
    }

    /// Selects the inclusive anchor-to-target span in the column's displayed order.
    ///
    /// `order` is that full order. A one-item slice cannot describe the rows between
    /// the anchor and the target, so a missing anchor falls back to selecting the target.
    /// The anchor itself stays put. The target remains the focused identity.
    pub fn select_range(&mut self, column: usize, item_id: ShellItemId, order: &[ShellItemId]) {
        if column != self.active || column >= self.levels.len() {
            self.set_single_selection(column, item_id);
            return;
        }
        let Some(anchor) = self
            .anchor
            .clone()
            .or_else(|| self.selected.first().cloned())
        else {
            self.set_single_selection(column, item_id);
            return;
        };
        let anchor_index = order.iter().position(|id| id == &anchor);
        let target_index = order.iter().position(|id| id == &item_id);
        let (Some(start), Some(end)) = (anchor_index, target_index) else {
            self.set_single_selection(column, item_id);
            return;
        };
        let (from, to) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let mut selected = order[from..=to].to_vec();
        if selected.last() != Some(&item_id) {
            selected.retain(|id| id != &item_id);
            selected.push(item_id);
        }
        self.selected = selected;
        self.active = column;
    }

    pub fn select_only(&mut self, column: usize, item_id: ShellItemId) {
        self.set_single_selection(column, item_id);
    }

    /// True when a container click must wait for a shell identity event.
    ///
    /// Literal path cycles and already-known file identities are decided here. A shell file id
    /// whose ancestors are not all known must not extend the branch on the UI thread.
    pub fn needs_filesystem_cycle_resolution(&self, column: usize, entry: &FileEntry) -> bool {
        if !entry.is_container || entry.metadata.archive_member || column >= self.levels.len() {
            return false;
        }
        let key = entry
            .location
            .path()
            .map(column_resolved_key)
            .unwrap_or_default();
        if !key.is_empty()
            && self
                .levels
                .iter()
                .take(column + 1)
                .any(|level| level.resolved_key == key)
        {
            return false;
        }
        if ColumnFilesystemIdentity::from_shell_item(&entry.id).is_none() {
            return false;
        }
        if self.filesystem_identity_cycles(column, entry) {
            return false;
        }
        self.levels
            .iter()
            .take(column + 1)
            .any(|level| level.filesystem_id.is_none())
    }

    /// Applies one shell identity result. A cycle never extends the branch.
    pub fn consume_cycle_resolution(
        &mut self,
        column: usize,
        entry: &FileEntry,
        outcome: &ColumnCycleOutcome,
        default_width: u16,
    ) -> ColumnSelectEffect {
        match outcome {
            ColumnCycleOutcome::Cycle { .. } => self.mark_cycle(column, &entry.id),
            ColumnCycleOutcome::Unsupported(_) => self.inert_effect(),
            ColumnCycleOutcome::Distinct { identities } => {
                self.note_filesystem_identities(identities);
                self.select_child(column, entry, None, default_width)
            }
            ColumnCycleOutcome::Failed(error) if error.kind == ExplorerErrorKind::Cancellation => {
                self.inert_effect()
            }
            ColumnCycleOutcome::Failed(_) => self.select_child(column, entry, None, default_width),
        }
    }

    /// Moves keyboard focus to a column without truncating descendants or clearing selection.
    pub fn focus_column(&mut self, column: usize) -> bool {
        if column >= self.levels.len() {
            return false;
        }
        self.active = column;
        true
    }

    /// Keeps a multi-selection when the pointer hits one of its members.
    ///
    /// The hit becomes the focused identity (last). A hit in another column, or outside the
    /// set, returns false so the caller can replace the selection instead.
    pub fn focus_selected_member(&mut self, column: usize, item_id: &ShellItemId) -> bool {
        if column >= self.levels.len() || self.active != column {
            return false;
        }
        if !self.selected.iter().any(|id| id == item_id) {
            return false;
        }
        let item_id = item_id.clone();
        self.selected.retain(|id| id != &item_id);
        self.selected.push(item_id);
        true
    }

    /// Replaces the active column's selection. An empty list clears it without moving focus
    /// to a different column when `column` is already active.
    pub fn replace_selection(&mut self, column: usize, ids: Vec<ShellItemId>) {
        if column >= self.levels.len() {
            return;
        }
        self.active = column;
        self.anchor = ids.last().cloned();
        self.selected = ids;
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    pub fn move_vertical(&mut self, direction: i8, entries: &[FileEntry]) -> bool {
        self.move_vertical_with(
            direction,
            entries.len(),
            |index, id| entries.get(index).is_some_and(|entry| &entry.id == id),
            |index| entries.get(index).map(|entry| entry.id.clone()),
        )
    }

    /// Moves within `len` visible identities. `matches_index` must not allocate.
    pub fn move_vertical_with(
        &mut self,
        direction: i8,
        len: usize,
        mut matches_index: impl FnMut(usize, &ShellItemId) -> bool,
        mut id_at: impl FnMut(usize) -> Option<ShellItemId>,
    ) -> bool {
        if len == 0 || self.levels.is_empty() {
            return false;
        }
        let current = self
            .selected
            .last()
            .and_then(|id| (0..len).find(|index| matches_index(*index, id)))
            .unwrap_or(0);
        let next = if direction < 0 {
            current.saturating_sub(1)
        } else {
            current.saturating_add(1).min(len - 1)
        };
        let Some(id) = id_at(next) else {
            return false;
        };
        self.set_single_selection(self.active.min(self.levels.len() - 1), id);
        true
    }

    pub fn focus_parent(&mut self) -> bool {
        if self.active == 0 || self.levels.is_empty() {
            return false;
        }
        let parent = self.active - 1;
        let child = self.levels[parent].branch_child.clone();
        self.active = parent;
        self.selected = child.into_iter().collect();
        self.anchor = self.selected.first().cloned();
        true
    }

    pub fn reveal_focused_child(
        &mut self,
        entries: &[FileEntry],
        default_width: u16,
    ) -> ColumnSelectEffect {
        let Some(id) = self.selected.last().cloned() else {
            return self.inert_effect();
        };
        let Some(entry) = entries.iter().find(|entry| entry.id == id).cloned() else {
            return self.inert_effect();
        };
        if entry.is_container {
            self.select_child(self.active, &entry, None, default_width)
        } else {
            ColumnSelectEffect {
                navigate_to: None,
                record_history: false,
                pending_file: Some(entry.id),
                clear_preview: false,
                open_file: true,
            }
        }
    }

    pub fn command_items(&self, entries: &[FileEntry]) -> Vec<ItemDescriptor> {
        self.selected
            .iter()
            .filter_map(|id| entries.iter().find(|entry| &entry.id == id))
            .map(|entry| ItemDescriptor {
                id: entry.id.clone(),
                location: entry.location.clone(),
            })
            .collect()
    }

    /// Clamp one column's vertical offset to the rows actually shown.
    ///
    /// An empty column cannot scroll. A non-positive viewport is not measured yet,
    /// so only the lower bound applies unless that column has no rows.
    pub fn clamp_vertical_offset(
        offset: f32,
        row_count: usize,
        row_height: f32,
        viewport_height: f32,
    ) -> f32 {
        if !offset.is_finite() {
            return 0.0;
        }
        let offset = offset.max(0.0);
        if row_count == 0 || !row_height.is_finite() || row_height <= 0.0 {
            return 0.0;
        }
        if !viewport_height.is_finite() || viewport_height <= 0.0 {
            return offset;
        }
        let content = row_count as f32 * row_height;
        let maximum = (content - viewport_height).max(0.0);
        offset.min(maximum)
    }

    /// Scroll offset that keeps `row_index` inside the column list viewport.
    pub fn reveal_row_offset(
        row_index: usize,
        row_count: usize,
        row_height: f32,
        viewport_height: f32,
        current: f32,
    ) -> f32 {
        if row_count == 0 || !row_height.is_finite() || row_height <= 0.0 {
            return 0.0;
        }
        let mut offset = if current.is_finite() {
            current.max(0.0)
        } else {
            0.0
        };
        if viewport_height.is_finite() && viewport_height > 0.0 {
            let row_index = row_index.min(row_count - 1);
            let row_top = row_index as f32 * row_height;
            let row_bottom = row_top + row_height;
            if row_top < offset {
                offset = row_top;
            } else if row_bottom > offset + viewport_height {
                offset = (row_bottom - viewport_height).max(0.0);
            }
        }
        Self::clamp_vertical_offset(offset, row_count, row_height, viewport_height)
    }

    pub fn set_vertical_offset(&mut self, column: usize, offset: f32) {
        if let Some(level) = self.levels.get_mut(column) {
            level.vertical_offset = offset.max(0.0);
        }
    }

    pub fn set_width(&mut self, column: usize, width: u16) {
        if let Some(level) = self.levels.get_mut(column) {
            level.width = normalized_column_width(width);
        }
    }

    pub fn widths(&self) -> Vec<u16> {
        self.levels.iter().map(|level| level.width).collect()
    }

    pub fn apply_cached_snapshot(&mut self, column: usize, snapshot: DirectorySnapshot) -> bool {
        let Some(level) = self.levels.get_mut(column) else {
            return false;
        };
        if matches!(level.phase, ColumnPhase::Ready(_)) {
            return false;
        }
        level.phase = if snapshot.entries().is_empty() {
            ColumnPhase::Empty
        } else {
            ColumnPhase::Ready(snapshot)
        };
        level.request_id = None;
        true
    }

    pub fn begin_load(&mut self, column: usize, request_id: RequestId) {
        if let Some(level) = self.levels.get_mut(column) {
            level.phase = ColumnPhase::Loading;
            level.request_id = Some(request_id);
        }
    }

    /// Detach an auxiliary listing so a later batch or terminal cannot change this branch.
    /// Partial rows from that request are dropped; a preserved ready snapshot has no request id.
    pub fn abandon_load(&mut self, request_id: RequestId) {
        for level in &mut self.levels {
            if level.request_id == Some(request_id) {
                level.request_id = None;
                level.phase = ColumnPhase::Pending;
            }
        }
    }

    pub fn apply_batch(
        &mut self,
        revision: u64,
        request_id: RequestId,
        location: &LocationDescriptor,
        entries: Vec<FileEntry>,
    ) -> bool {
        if revision != self.revision {
            return false;
        }
        let Some(level) = self
            .levels
            .iter_mut()
            .find(|level| level.request_id == Some(request_id) && &level.location == location)
        else {
            return false;
        };
        let mut snapshot = match &level.phase {
            ColumnPhase::Ready(snapshot) => snapshot.clone(),
            _ => DirectorySnapshot::default(),
        };
        for entry in entries {
            let _ = snapshot.upsert(entry);
        }
        level.phase = ColumnPhase::Ready(snapshot);
        true
    }

    pub fn apply_terminal(
        &mut self,
        revision: u64,
        request_id: RequestId,
        location: &LocationDescriptor,
        outcome: &crate::ColumnListingTerminal,
    ) -> bool {
        if revision != self.revision {
            return false;
        }
        let Some(index) = self
            .levels
            .iter()
            .position(|level| level.request_id == Some(request_id) && &level.location == location)
        else {
            return false;
        };
        let level = &mut self.levels[index];
        let next_phase = match outcome {
            crate::ColumnListingTerminal::Finished => match &level.phase {
                ColumnPhase::Ready(snapshot) if snapshot.entries().is_empty() => ColumnPhase::Empty,
                ColumnPhase::Ready(snapshot) => ColumnPhase::Ready(snapshot.clone()),
                _ => ColumnPhase::Empty,
            },
            crate::ColumnListingTerminal::Empty => ColumnPhase::Empty,
            crate::ColumnListingTerminal::Cancelled => ColumnPhase::Pending,
            crate::ColumnListingTerminal::Failed(_) => {
                ColumnPhase::Error(ColumnFault::Inaccessible)
            }
        };
        level.request_id = None;
        level.phase = next_phase;
        if matches!(level.phase, ColumnPhase::Error(_)) {
            self.truncate_after(index);
        }
        true
    }

    pub fn invalidate_location(&mut self, location: &LocationDescriptor) {
        let Some(index) = self
            .levels
            .iter()
            .position(|level| &level.location == location)
        else {
            return;
        };
        self.levels[index].phase = ColumnPhase::Pending;
        self.levels[index].request_id = None;
        self.bump();
    }

    pub fn remember_branch_child(&mut self, column: usize, child: ShellItemId) {
        if let Some(level) = self.levels.get_mut(column) {
            level.branch_child = Some(child);
        }
    }

    pub fn drop_missing_branch_child(
        &mut self,
        column: usize,
        present: impl Fn(&ShellItemId) -> bool,
    ) {
        let Some(child) = self
            .levels
            .get(column)
            .and_then(|level| level.branch_child.clone())
        else {
            return;
        };
        if present(&child) {
            return;
        }
        if let Some(level) = self.levels.get_mut(column) {
            level.branch_child = None;
        }
        self.truncate_after(column);
        self.pending_file = None;
        self.selected.clear();
        self.anchor = None;
        self.active = column;
        self.bump();
    }

    #[cfg(test)]
    fn focus_entry_for_test(&mut self, item_id: ShellItemId) {
        self.set_single_selection(self.levels.len().saturating_sub(1), item_id);
    }

    pub fn reveal_offset(&self, viewport_width: f32, preview_width: f32) -> f32 {
        reveal_column_offset(
            self.active,
            &self
                .widths()
                .iter()
                .map(|width| f32::from(*width))
                .collect::<Vec<_>>(),
            viewport_width,
            preview_width,
            self.horizontal_offset,
        )
    }

    fn filesystem_identity_cycles(&self, column: usize, entry: &FileEntry) -> bool {
        let Some(identity) = ColumnFilesystemIdentity::from_shell_item(&entry.id) else {
            return false;
        };
        self.levels
            .iter()
            .take(column + 1)
            .any(|level| level.filesystem_id.as_ref() == Some(&identity))
    }

    fn note_filesystem_identities(
        &mut self,
        identities: &[(LocationDescriptor, ColumnFilesystemIdentity)],
    ) {
        for (location, identity) in identities {
            if let Some(level) = self
                .levels
                .iter_mut()
                .find(|level| &level.location == location)
            {
                level.filesystem_id = Some(identity.clone());
            }
        }
    }

    fn mark_cycle(&mut self, column: usize, item_id: &ShellItemId) -> ColumnSelectEffect {
        if column >= self.levels.len() {
            return self.inert_effect();
        }
        self.levels[column].phase = ColumnPhase::Error(ColumnFault::Cycle);
        self.truncate_after(column);
        self.set_single_selection(column, item_id.clone());
        self.pending_file = None;
        self.pending_navigation = None;
        self.bump();
        ColumnSelectEffect {
            navigate_to: None,
            record_history: false,
            pending_file: None,
            clear_preview: true,
            open_file: false,
        }
    }

    fn truncate_after(&mut self, column: usize) {
        if column + 1 < self.levels.len() {
            self.levels.truncate(column + 1);
        }
    }

    fn set_single_selection(&mut self, column: usize, item_id: ShellItemId) {
        self.active = column.min(self.levels.len().saturating_sub(1));
        self.selected = vec![item_id.clone()];
        self.anchor = Some(item_id);
    }

    fn bump(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }

    fn inert_effect(&self) -> ColumnSelectEffect {
        ColumnSelectEffect {
            navigate_to: None,
            record_history: false,
            pending_file: None,
            clear_preview: false,
            open_file: false,
        }
    }

    fn level(
        &self,
        location: LocationDescriptor,
        resolved_key: String,
        width: u16,
        phase: ColumnPhase,
    ) -> ColumnLevel {
        ColumnLevel {
            location,
            resolved_key,
            branch_child: None,
            filesystem_id: None,
            phase,
            vertical_offset: 0.0,
            request_id: None,
            width: normalized_column_width(width),
        }
    }
}

fn snapshot_child_id(level: &ColumnLevel, name: &str) -> Option<ShellItemId> {
    let ColumnPhase::Ready(snapshot) = &level.phase else {
        return None;
    };
    snapshot
        .entries()
        .iter()
        .find(|entry| entry.display_name.eq_ignore_ascii_case(name))
        .map(|entry| entry.id.clone())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnLoadRequest {
    pub column: usize,
    pub context: RequestContext,
    pub location: LocationDescriptor,
    pub branch_revision: u64,
}

#[derive(Clone, Debug)]
struct ColumnInFlight {
    tab_id: TabId,
    revision: u64,
    generation: Generation,
    request_id: RequestId,
    /// The branch no longer wants this result. The slot stays until its one terminal.
    superseded: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ColumnLoadCoordinator {
    in_flight: Vec<ColumnInFlight>,
    max_in_flight: usize,
}

impl ColumnLoadCoordinator {
    pub fn new(max_in_flight: usize) -> Self {
        Self {
            in_flight: Vec::new(),
            max_in_flight: max_in_flight.max(1),
        }
    }

    pub fn plan(
        &mut self,
        tab_id: TabId,
        generation: Generation,
        branch: &mut ColumnBranch,
        visible: std::ops::Range<usize>,
        cached: impl Fn(&LocationDescriptor) -> Option<DirectorySnapshot>,
    ) -> (Vec<RequestId>, Vec<ColumnLoadRequest>) {
        let revision = branch.revision();
        let cancel = self.supersede_stale(tab_id, revision, generation);
        for request_id in &cancel {
            branch.abandon_load(*request_id);
        }
        let last = branch.levels().len().saturating_sub(1);
        let mut missing = Vec::new();
        let mut cached_hits = Vec::new();
        for (index, level) in branch.levels().iter().enumerate() {
            if index == last {
                continue;
            }
            if matches!(
                level.phase,
                ColumnPhase::Ready(_)
                    | ColumnPhase::Empty
                    | ColumnPhase::Error(_)
                    | ColumnPhase::Loading
            ) {
                continue;
            }
            if let Some(snapshot) = cached(&level.location) {
                cached_hits.push((index, snapshot));
                continue;
            }
            missing.push(index);
        }
        for (index, snapshot) in cached_hits {
            let _ = branch.apply_cached_snapshot(index, snapshot);
        }
        // Visible columns, then the adjacent level, then farther ancestors. Distance to the
        // active column breaks ties so a narrow strip loads what the user can see first.
        missing.sort_by_key(|index| column_auxiliary_load_rank(*index, visible.clone(), last));
        let mut requests = Vec::new();
        for column in missing {
            // Superseded listings stay active until their terminal, so a fast switch cannot
            // start another wave and exceed the cap while the shell is still enumerating.
            if self.in_flight.len() >= self.max_in_flight {
                break;
            }
            let Some(location) = branch
                .levels()
                .get(column)
                .map(|level| level.location.clone())
            else {
                continue;
            };
            let context = RequestContext::new(tab_id, generation);
            branch.begin_load(column, context.request_id);
            self.in_flight.push(ColumnInFlight {
                tab_id,
                revision,
                generation,
                request_id: context.request_id,
                superseded: false,
            });
            requests.push(ColumnLoadRequest {
                column,
                context,
                location,
                branch_revision: revision,
            });
        }
        (cancel, requests)
    }

    /// Mark this tab's outdated revision or generation listings without freeing their slots.
    ///
    /// Another tab's running ancestor listing stays active. A batch never releases a slot, and
    /// exactly one terminal does, including a terminal for a listing this method superseded.
    pub fn supersede_stale(
        &mut self,
        tab_id: TabId,
        revision: u64,
        generation: Generation,
    ) -> Vec<RequestId> {
        let mut cancel = Vec::new();
        for flight in &mut self.in_flight {
            let stale = flight.tab_id == tab_id
                && (flight.revision != revision || flight.generation != generation);
            if stale && !flight.superseded {
                flight.superseded = true;
                cancel.push(flight.request_id);
            }
        }
        cancel
    }

    /// Cancel one tab's auxiliary listings and leave every other tab alone.
    pub fn supersede_tab(&mut self, tab_id: TabId) -> Vec<RequestId> {
        let mut cancel = Vec::new();
        for flight in &mut self.in_flight {
            if flight.tab_id == tab_id && !flight.superseded {
                flight.superseded = true;
                cancel.push(flight.request_id);
            }
        }
        cancel
    }

    /// Drop every auxiliary listing. Slots stay occupied until each request's terminal.
    pub fn supersede_all(&mut self) -> Vec<RequestId> {
        let mut cancel = Vec::new();
        for flight in &mut self.in_flight {
            if !flight.superseded {
                flight.superseded = true;
                cancel.push(flight.request_id);
            }
        }
        cancel
    }

    /// Release the slot for one terminal. A repeated terminal for the same request is a no-op.
    pub fn complete(&mut self, request_id: RequestId) -> bool {
        let before = self.in_flight.len();
        self.in_flight
            .retain(|flight| flight.request_id != request_id);
        self.in_flight.len() != before
    }

    pub fn in_flight_count(&self, tab_id: TabId) -> usize {
        self.in_flight
            .iter()
            .filter(|flight| flight.tab_id == tab_id)
            .count()
    }

    pub fn active_count(&self) -> usize {
        self.in_flight.len()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ColumnBranchStore {
    branches: HashMap<TabId, ColumnBranch>,
}

impl ColumnBranchStore {
    pub fn get(&self, tab_id: TabId) -> Option<&ColumnBranch> {
        self.branches.get(&tab_id)
    }

    pub fn get_mut(&mut self, tab_id: TabId) -> Option<&mut ColumnBranch> {
        self.branches.get_mut(&tab_id)
    }

    pub fn insert(&mut self, tab_id: TabId, branch: ColumnBranch) {
        self.branches.insert(tab_id, branch);
    }

    pub fn remove(&mut self, tab_id: TabId) {
        self.branches.remove(&tab_id);
    }

    pub fn abandon_loads(&mut self, request_ids: &[RequestId]) {
        if request_ids.is_empty() {
            return;
        }
        for branch in self.branches.values_mut() {
            for request_id in request_ids {
                branch.abandon_load(*request_id);
            }
        }
    }
}

/// Minimum column-strip span kept when a preview is visible and the file surface is wider.
pub const COLUMN_STRIP_MIN_SPAN: f32 = 120.0;

/// Split the file surface into the scrollable strip and the drawn preview.
///
/// The saved preview width is not painted in full when that would leave the strip unreachable.
/// A narrow surface still keeps both regions, and the saved preference is left unchanged.
pub fn column_layout_spans(
    file_surface_width: f32,
    preferred_preview_width: f32,
    preview_visible: bool,
) -> (f32, f32) {
    let surface = if file_surface_width.is_finite() {
        file_surface_width.max(0.0)
    } else {
        0.0
    };
    if !preview_visible || surface <= 0.0 {
        return (surface, 0.0);
    }
    let preferred = if preferred_preview_width.is_finite() {
        preferred_preview_width.max(0.0)
    } else {
        0.0
    };
    if surface <= COLUMN_STRIP_MIN_SPAN {
        let preview = preferred.min(surface * 0.45);
        return ((surface - preview).max(0.0), preview);
    }
    let preview = preferred.min(surface - COLUMN_STRIP_MIN_SPAN);
    ((surface - preview).max(0.0), preview)
}

/// Keep a manual scroll where both the first column and the end of the last column can be reached.
/// A non-positive strip span means the viewport is not known yet, so only the lower bound applies.
pub fn clamp_column_horizontal_offset(offset: f32, content_width: f32, strip_span: f32) -> f32 {
    if !offset.is_finite() {
        return 0.0;
    }
    let offset = offset.max(0.0);
    if !content_width.is_finite() || content_width <= 0.0 {
        return 0.0;
    }
    if !strip_span.is_finite() || strip_span <= 0.0 {
        return offset;
    }
    offset.min((content_width - strip_span).max(0.0))
}

pub fn reveal_column_offset(
    active: usize,
    widths: &[f32],
    viewport_width: f32,
    preview_width: f32,
    current: f32,
) -> f32 {
    if widths.is_empty() || !viewport_width.is_finite() || viewport_width <= 0.0 {
        return current.max(0.0);
    }
    let content_width = if preview_width.is_finite() {
        (viewport_width - preview_width.max(0.0)).max(0.0)
    } else {
        viewport_width.max(0.0)
    };
    let total: f32 = widths.iter().copied().sum();
    if content_width <= 1.0 {
        return clamp_column_horizontal_offset(current, total, content_width);
    }
    let start: f32 = widths.iter().take(active).sum();
    let column_width = widths
        .get(active)
        .copied()
        .unwrap_or(f32::from(COLUMN_WIDTH_DEFAULT));
    let end = start + column_width;
    let mut offset = current.max(0.0);
    if end > offset + content_width {
        offset = end - content_width;
    }
    if start < offset {
        offset = start;
    }
    clamp_column_horizontal_offset(offset, total, content_width)
}

/// Columns whose horizontal span overlaps the measured strip. An unknown strip selects nothing,
/// so planning can prefer ancestors next to the active column instead of treating every level
/// as visible.
pub fn visible_column_range(
    widths: &[f32],
    horizontal_offset: f32,
    strip_span: f32,
) -> std::ops::Range<usize> {
    if widths.is_empty() {
        return 0..0;
    }
    let offset = if horizontal_offset.is_finite() {
        horizontal_offset.max(0.0)
    } else {
        0.0
    };
    let span = if strip_span.is_finite() {
        strip_span.max(0.0)
    } else {
        0.0
    };
    if span <= 0.0 {
        return 0..0;
    }
    let view_end = offset + span;
    let mut start = None;
    let mut end = 0_usize;
    let mut cursor = 0.0_f32;
    for (index, width) in widths.iter().copied().enumerate() {
        let width = if width.is_finite() && width > 0.0 {
            width
        } else {
            f32::from(COLUMN_WIDTH_DEFAULT)
        };
        let next = cursor + width;
        if next > offset && cursor < view_end {
            if start.is_none() {
                start = Some(index);
            }
            end = index + 1;
        }
        cursor = next;
    }
    start.unwrap_or(0)..end
}

fn column_auxiliary_load_rank(
    index: usize,
    visible: std::ops::Range<usize>,
    anchor: usize,
) -> (u8, usize, usize) {
    let toward_anchor = index.abs_diff(anchor);
    if !visible.is_empty() && visible.contains(&index) {
        return (0, toward_anchor, index);
    }
    let outside = if visible.is_empty() {
        toward_anchor
    } else if index < visible.start {
        visible.start - index
    } else {
        index.saturating_add(1).saturating_sub(visible.end)
    };
    let tier = if outside <= 1 { 1 } else { 2 };
    (tier, outside, toward_anchor)
}

pub fn column_visible_row_bounds(
    entry_count: usize,
    row_height: f32,
    viewport_height: f32,
    scroll_offset: f32,
    overscan_viewports: usize,
) -> std::ops::Range<usize> {
    if entry_count == 0 || row_height <= 0.0 || viewport_height <= 0.0 {
        return 0..0;
    }
    let visible_rows = (viewport_height / row_height).ceil().max(1.0) as usize;
    let first_visible = (scroll_offset.max(0.0) / row_height).floor() as usize;
    let overscan = visible_rows.saturating_mul(overscan_viewports);
    let start = first_visible.saturating_sub(overscan).min(entry_count);
    let end = first_visible
        .saturating_add(visible_rows)
        .saturating_add(overscan)
        .min(entry_count);
    start..end
}

pub fn column_listing_rejected(
    location: &LocationDescriptor,
    media: DriveKind,
) -> Option<ExplorerError> {
    if columns_location_eligible(location, media) {
        None
    } else {
        Some(ExplorerError::new(
            ExplorerErrorKind::Input,
            "enumerate column",
            false,
            "分欄檢視只支援本機磁碟資料夾。",
            "column enumeration rejected unsupported location",
        ))
    }
}

/// Formal navigation helper used by reducer tests. Production UI still submits `Navigate`.
#[derive(Clone, Debug)]
pub struct ColumnNavigationSession {
    pub history: NavigationHistory,
    pub branch: ColumnBranch,
    pub default_width: u16,
}

impl ColumnNavigationSession {
    pub fn new(location: LocationDescriptor, title: impl Into<String>, width: u16) -> Self {
        let history =
            NavigationHistory::with_initial(crate::HistoryEntry::new(location.clone(), title));
        Self {
            branch: ColumnBranch::from_location(location, width),
            history,
            default_width: width,
        }
    }

    pub fn select_folder(&mut self, column: usize, entry: &FileEntry) -> ColumnSelectEffect {
        let effect = self
            .branch
            .select_child(column, entry, None, self.default_width);
        if let Some(location) = &effect.navigate_to {
            self.history.commit_navigation(crate::HistoryEntry::new(
                location.clone(),
                entry.display_name.clone(),
            ));
            self.branch.align_to_location(
                location.clone(),
                self.default_width,
                Some(&self.branch.widths()),
            );
        }
        effect
    }

    pub fn select_file(&mut self, column: usize, entry: &FileEntry) -> ColumnSelectEffect {
        let effect = self
            .branch
            .select_child(column, entry, None, self.default_width);
        if let Some(location) = effect.navigate_to.clone() {
            let title = self
                .branch
                .active_location()
                .and_then(|location| location.path())
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| entry.display_name.clone());
            self.history
                .commit_navigation(crate::HistoryEntry::new(location.clone(), title));
            self.branch.align_to_location(
                location,
                self.default_width,
                Some(&self.branch.widths()),
            );
            if let Some(file) = effect.pending_file.clone() {
                self.branch.pending_file = Some(file);
            }
        }
        effect
    }

    pub fn go(&mut self, location: LocationDescriptor, title: impl Into<String>) {
        self.history
            .commit_navigation(crate::HistoryEntry::new(location.clone(), title));
        self.branch
            .align_to_location(location, self.default_width, Some(&self.branch.widths()));
    }

    pub fn back(&mut self) -> bool {
        let Some(entry) = self.history.go_back().cloned() else {
            return false;
        };
        self.branch.align_to_location(
            entry.location,
            self.default_width,
            Some(&self.branch.widths()),
        );
        true
    }

    pub fn forward(&mut self) -> bool {
        let Some(entry) = self.history.go_forward().cloned() else {
            return false;
        };
        self.branch.align_to_location(
            entry.location,
            self.default_width,
            Some(&self.branch.widths()),
        );
        true
    }

    pub fn up(&mut self) -> bool {
        let Some(parent) = self
            .history
            .current()
            .and_then(|entry| entry.location.path())
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
        else {
            return false;
        };
        if parent.as_os_str().is_empty() {
            return false;
        }
        self.go(
            LocationDescriptor::file_system(parent.clone()),
            parent.to_string_lossy().into_owned(),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ColumnListingTerminal, ExplorerCommand, ExplorerEvent, HistoryEntry, TerminalLedger,
    };

    fn path_location(path: &str) -> LocationDescriptor {
        LocationDescriptor::file_system(path)
    }

    fn entry(id: u8, path: &str, container: bool) -> FileEntry {
        FileEntry {
            id: ShellItemId::from_provider_bytes([id]).expect("id"),
            display_name: Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_owned()),
            location: path_location(path),
            is_container: container,
            metadata: crate::FileEntryMetadata::default(),
        }
    }

    #[test]
    fn eligibility_accepts_local_fixed_and_removable_drive_directories_only() {
        let local = path_location(r"C:\Users\fixture");
        assert!(columns_location_eligible(&local, DriveKind::Fixed));
        assert!(columns_location_eligible(&local, DriveKind::Removable));
        assert!(!columns_location_eligible(&local, DriveKind::Network));
        assert!(!columns_location_eligible(&local, DriveKind::Unknown));
        assert!(!columns_location_eligible(
            &path_location(r"\\server\share\folder"),
            DriveKind::Fixed
        ));
        assert!(!columns_location_eligible(
            &path_location(r"\\wsl$\Ubuntu\home"),
            DriveKind::Fixed
        ));
        assert!(columns_location_eligible(
            &path_location(r"C:\archives\bundle.zip"),
            DriveKind::Fixed
        ));
        assert!(!columns_location_eligible(
            &LocationDescriptor::ParsingName("shell:Downloads".to_owned()),
            DriveKind::Fixed
        ));
        let remote =
            LocationDescriptor::try_virtual("adb", [1; 16], 1, None, vec!["sdcard".into()])
                .expect("virtual");
        assert!(!columns_location_eligible(&remote, DriveKind::Fixed));
        assert_eq!(
            effective_view_mode(ViewMode::Columns, Some(&local), DriveKind::Network),
            ViewMode::Details
        );
        assert_eq!(
            effective_view_mode(ViewMode::Columns, Some(&local), DriveKind::Fixed),
            ViewMode::Columns
        );
        assert_eq!(
            effective_view_mode(ViewMode::List, Some(&remote), DriveKind::Fixed),
            ViewMode::List
        );
    }

    #[test]
    fn widths_normalize_and_deep_reveal_keeps_ancestors_reachable() {
        assert_eq!(normalized_column_width(10), COLUMN_WIDTH_MIN);
        assert_eq!(normalized_column_width(9_000), COLUMN_WIDTH_MAX);
        assert_eq!(normalized_column_preview_width(0), COLUMN_PREVIEW_WIDTH_MIN);
        let widths = vec![240.0; 12];
        let offset = reveal_column_offset(11, &widths, 800.0, 280.0, 0.0);
        let start = 11.0 * 240.0;
        assert!(offset <= start);
        assert!(start < offset + (800.0 - 280.0) + 1.0);
        assert!(offset < start);
        let rows = column_visible_row_bounds(100_000, COLUMN_ROW_HEIGHT, 720.0, 0.0, 2);
        assert!(rows.len() <= 150);
        assert!(rows.end <= 100_000);
    }

    #[test]
    fn nested_folder_selection_keeps_ancestors_and_records_one_history_step() {
        let mut session = ColumnNavigationSession::new(path_location(r"C:\a"), "a", 240);
        let folder_b = entry(2, r"C:\a\b", true);
        let column = session.branch.levels().len() - 1;
        let effect = session.select_folder(column, &folder_b);
        assert!(effect.record_history);
        assert_eq!(
            session
                .history
                .current()
                .map(|entry| entry.location.clone()),
            Some(path_location(r"C:\a\b"))
        );
        assert!(session.branch.levels().len() >= 2);
        let folder_c = entry(3, r"C:\a\b\c", true);
        let column = session.branch.levels().len() - 1;
        session.select_folder(column, &folder_c);
        assert_eq!(
            session
                .history
                .current()
                .map(|entry| entry.display_title.clone()),
            Some("c".to_owned())
        );
        assert!(session.back());
        assert_eq!(
            session
                .history
                .current()
                .map(|entry| entry.location.clone()),
            Some(path_location(r"C:\a\b"))
        );
        assert!(
            session
                .branch
                .levels()
                .iter()
                .any(|level| level.location == path_location(r"C:\a"))
        );
    }

    #[test]
    fn sibling_selection_drops_descendants_before_the_new_branch_loads() {
        let mut session = ColumnNavigationSession::new(path_location(r"C:\a\b"), "b", 240);
        assert!(session.branch.levels().len() >= 3);
        let sibling = entry(9, r"C:\a\z", true);
        let effect = session.select_folder(1, &sibling);
        assert!(effect.clear_preview);
        assert_eq!(
            session
                .history
                .current()
                .map(|entry| entry.location.clone()),
            Some(path_location(r"C:\a\z"))
        );
        assert!(
            session
                .branch
                .levels()
                .iter()
                .all(|level| level.location != path_location(r"C:\a\b"))
        );
    }

    #[test]
    fn ancestor_file_selection_makes_the_containing_folder_current() {
        let mut session = ColumnNavigationSession::new(path_location(r"C:\a\b"), "b", 240);
        let file = entry(4, r"C:\a\notes.txt", false);
        let effect = session.select_file(1, &file);
        assert_eq!(effect.pending_file.as_ref(), Some(&file.id));
        assert_eq!(
            session
                .history
                .current()
                .map(|entry| entry.location.clone()),
            Some(path_location(r"C:\a"))
        );
        assert!(
            session
                .branch
                .levels()
                .last()
                .is_some_and(|level| level.location == path_location(r"C:\a"))
        );
    }

    #[test]
    fn repeated_current_folder_click_does_not_add_history() {
        let mut session = ColumnNavigationSession::new(path_location(r"C:\a"), "a", 240);
        let again = entry(1, r"C:\a", true);
        let before = session.history.back_entries().len();
        let column = session.branch.levels().len() - 1;
        let effect = session.select_folder(column, &again);
        assert!(!effect.record_history);
        assert_eq!(session.history.back_entries().len(), before);
    }

    #[test]
    fn junction_cycle_stops_extension_and_keeps_the_ancestor() {
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a"), 240);
        let link = entry(7, r"C:\a\link", true);
        let effect =
            branch.select_child(0, &link, Some(&column_resolved_key(Path::new(r"C:\"))), 240);
        assert!(effect.navigate_to.is_none());
        assert!(matches!(
            branch.levels().last().map(|level| &level.phase),
            Some(ColumnPhase::Error(ColumnFault::Cycle))
        ));
        assert!(branch.levels().len() <= 2);
    }

    #[test]
    fn column_view_cancellation_failure_does_not_open_the_literal_folder() {
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a"), 240);
        let before = branch.levels().len();
        let folder = entry(3, r"C:\a\child", true);
        let column = branch.levels().len() - 1;
        let cancelled = branch.consume_cycle_resolution(
            column,
            &folder,
            &ColumnCycleOutcome::Failed(ExplorerError::new(
                ExplorerErrorKind::Cancellation,
                "resolve column cycle",
                true,
                "已取消資料夾載入。",
                "cancelled",
            )),
            240,
        );
        assert!(cancelled.navigate_to.is_none());
        assert_eq!(branch.levels().len(), before);
        let unavailable = branch.consume_cycle_resolution(
            column,
            &folder,
            &ColumnCycleOutcome::Failed(ExplorerError::new(
                ExplorerErrorKind::Availability,
                "read item identity",
                true,
                "無法確認這個資料夾是否指回上層。",
                "unavailable",
            )),
            240,
        );
        assert_eq!(
            unavailable.navigate_to.as_ref(),
            Some(&path_location(r"C:\a\child"))
        );
    }

    #[test]
    fn refresh_drops_invalid_descendants_and_keeps_a_navigable_ancestor() {
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a\b\c"), 240);
        let ancestor = 1;
        branch.remember_branch_child(ancestor, ShellItemId::from_provider_bytes([9]).expect("id"));
        branch.focus_entry_for_test(ShellItemId::from_provider_bytes([4]).expect("id"));
        branch.drop_missing_branch_child(ancestor, |_| false);
        assert!(branch.selected_ids().is_empty());
        assert!(branch.active_index() <= ancestor);
        assert!(
            branch
                .active_location()
                .is_some_and(|location| location.path().is_some())
        );
        assert!(branch.levels().len() <= ancestor + 1);
    }

    #[test]
    fn coordinator_prioritizes_visible_columns_and_rejects_stale_revisions() {
        let tab = TabId::new();
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a\b\c\d"), 240);
        let mut coordinator = ColumnLoadCoordinator::new(COLUMN_LOAD_CONCURRENCY);
        let generation = Generation::new(3);
        let (_cancel, requests) = coordinator.plan(tab, generation, &mut branch, 1..3, |_| None);
        assert!(requests.len() <= COLUMN_LOAD_CONCURRENCY);
        assert!(coordinator.in_flight_count(tab) <= COLUMN_LOAD_CONCURRENCY);
        let stale_revision = branch.revision();
        let request = requests[0].clone();
        branch.align_to_location(path_location(r"C:\a\z"), 240, None);
        assert!(!branch.apply_batch(
            stale_revision,
            request.context.request_id,
            &request.location,
            vec![entry(8, r"C:\a\old.txt", false)]
        ));
        let (cancel, _) = coordinator.plan(tab, generation, &mut branch, 0..1, |_| None);
        assert!(cancel.contains(&request.context.request_id));
        let held = coordinator.active_count();
        assert_eq!(held, requests.len());
        assert!(held <= COLUMN_LOAD_CONCURRENCY);
        assert!(coordinator.complete(request.context.request_id));
        assert!(!coordinator.complete(request.context.request_id));
        assert_eq!(coordinator.active_count(), held - 1);
    }

    #[test]
    fn auxiliary_load_holds_its_slot_until_one_terminal_and_prefers_visible_columns() {
        let tab = TabId::new();
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a\b\c\d\e\f"), 240);
        let widths = branch
            .widths()
            .iter()
            .map(|width| f32::from(*width))
            .collect::<Vec<_>>();
        assert_eq!(visible_column_range(&widths, 720.0, 480.0), 3..5);
        assert!(visible_column_range(&widths, 0.0, 0.0).is_empty());
        branch.set_horizontal_offset(720.0);
        let mut coordinator = ColumnLoadCoordinator::new(COLUMN_LOAD_CONCURRENCY);
        let generation = Generation::new(1);
        let visible = visible_column_range(&widths, branch.horizontal_offset(), 480.0);
        let (_cancel, first) =
            coordinator.plan(tab, generation, &mut branch, visible.clone(), |_| None);
        assert_eq!(first.len(), COLUMN_LOAD_CONCURRENCY);
        assert_eq!(coordinator.active_count(), COLUMN_LOAD_CONCURRENCY);
        assert_eq!(first[0].column, 4);
        assert_eq!(first[1].column, 3);
        let kept = &first[0];
        let mut names = Vec::new();
        for (index, name) in ["one.txt", "two.txt", "three.txt"].into_iter().enumerate() {
            let row = entry(10 + index as u8, &format!(r"C:\a\b\c\d\{name}"), false);
            names.push(row.display_name.clone());
            assert!(branch.apply_batch(
                kept.branch_revision,
                kept.context.request_id,
                &kept.location,
                vec![row]
            ));
            assert_eq!(coordinator.active_count(), COLUMN_LOAD_CONCURRENCY);
        }
        let (_cancel, blocked) =
            coordinator.plan(tab, generation, &mut branch, visible.clone(), |_| None);
        assert!(blocked.is_empty());
        assert!(coordinator.active_count() <= COLUMN_LOAD_CONCURRENCY);
        assert!(branch.apply_terminal(
            kept.branch_revision,
            kept.context.request_id,
            &kept.location,
            &ColumnListingTerminal::Finished
        ));
        let listed = match &branch.levels()[kept.column].phase {
            ColumnPhase::Ready(snapshot) => snapshot
                .entries()
                .iter()
                .map(|entry| entry.display_name.clone())
                .collect::<Vec<_>>(),
            other => panic!("finished column should keep every batch, got {other:?}"),
        };
        assert_eq!(listed, names);
        assert!(coordinator.complete(kept.context.request_id));
        assert!(!coordinator.complete(kept.context.request_id));
        assert_eq!(coordinator.active_count(), COLUMN_LOAD_CONCURRENCY - 1);
        let (_cancel, next) = coordinator.plan(tab, generation, &mut branch, visible, |_| None);
        assert_eq!(next.len(), 1);
        assert_ne!(next[0].column, 0);
        assert!(coordinator.active_count() <= COLUMN_LOAD_CONCURRENCY);
        let stale = next[0].clone();
        assert!(!branch.apply_batch(
            stale.branch_revision.wrapping_add(1),
            stale.context.request_id,
            &stale.location,
            vec![entry(40, r"C:\a\stale.txt", false)]
        ));
        assert!(coordinator.complete(stale.context.request_id));
        let other = TabId::new();
        let mut other_branch = ColumnBranch::from_location(path_location(r"C:\a\b\c\d\e\f"), 240);
        let (cancel, other_requests) =
            coordinator.plan(other, generation, &mut other_branch, 0..1, |_| None);
        assert!(
            !cancel.contains(&first[1].context.request_id),
            "planning another tab must not cancel this tab's ancestor listing"
        );
        assert_eq!(other_requests.len(), 1);
        assert!(coordinator.active_count() <= COLUMN_LOAD_CONCURRENCY);
        assert!(branch.apply_batch(
            first[1].branch_revision,
            first[1].context.request_id,
            &first[1].location,
            vec![entry(41, r"C:\a\b\c\kept.txt", false)]
        ));
        assert!(coordinator.complete(first[1].context.request_id));
        assert!(!coordinator.complete(first[1].context.request_id));
        assert_eq!(coordinator.active_count(), other_requests.len());
    }

    #[test]
    fn column_view_load_isolation_keeps_other_tabs_until_one_terminal() {
        let tab_a = TabId::new();
        let tab_b = TabId::new();
        let mut branch_a = ColumnBranch::from_location(path_location(r"C:\a\b\c\d"), 240);
        let mut branch_b = ColumnBranch::from_location(path_location(r"C:\a\b\c\d"), 240);
        let mut coordinator = ColumnLoadCoordinator::new(COLUMN_LOAD_CONCURRENCY);
        let generation_a = Generation::new(4);
        let generation_b = Generation::new(7);
        let (cancel, first) = coordinator.plan(tab_a, generation_a, &mut branch_a, 0..2, |_| None);
        assert!(cancel.is_empty());
        assert_eq!(first.len(), COLUMN_LOAD_CONCURRENCY);
        let kept = first[0].clone();
        assert!(branch_a.apply_batch(
            kept.branch_revision,
            kept.context.request_id,
            &kept.location,
            vec![entry(1, r"C:\a\b\one.txt", false)]
        ));
        assert_eq!(coordinator.active_count(), COLUMN_LOAD_CONCURRENCY);

        let (switch_cancel, blocked) =
            coordinator.plan(tab_b, generation_b, &mut branch_b, 0..2, |_| None);
        assert!(switch_cancel.is_empty());
        assert!(
            blocked.is_empty(),
            "shared cap stays full while A is running"
        );
        assert!(branch_a.apply_batch(
            kept.branch_revision,
            kept.context.request_id,
            &kept.location,
            vec![entry(2, r"C:\a\b\two.txt", false)]
        ));
        assert!(branch_a.apply_terminal(
            kept.branch_revision,
            kept.context.request_id,
            &kept.location,
            &ColumnListingTerminal::Finished
        ));
        assert!(coordinator.complete(kept.context.request_id));
        assert!(!coordinator.complete(kept.context.request_id));

        let (again_cancel, started_b) =
            coordinator.plan(tab_b, generation_b, &mut branch_b, 0..2, |_| None);
        assert!(again_cancel.is_empty());
        assert_eq!(started_b.len(), 1);
        assert!(coordinator.active_count() <= COLUMN_LOAD_CONCURRENCY);
        assert!(branch_b.apply_batch(
            started_b[0].branch_revision,
            started_b[0].context.request_id,
            &started_b[0].location,
            vec![entry(3, r"C:\a\b\from-b.txt", false)]
        ));
        assert_eq!(coordinator.active_count(), COLUMN_LOAD_CONCURRENCY);

        let (return_cancel, _) =
            coordinator.plan(tab_a, generation_a, &mut branch_a, 0..2, |_| None);
        assert!(
            !return_cancel.contains(&started_b[0].context.request_id),
            "returning to A must not cancel B"
        );
        assert!(branch_b.apply_terminal(
            started_b[0].branch_revision,
            started_b[0].context.request_id,
            &started_b[0].location,
            &ColumnListingTerminal::Finished
        ));
        assert!(coordinator.complete(started_b[0].context.request_id));

        branch_a.align_to_location(path_location(r"C:\a\z"), 240, None);
        let (replaced, replacement_requests) =
            coordinator.plan(tab_a, generation_a, &mut branch_a, 0..1, |_| None);
        assert!(replaced.contains(&first[1].context.request_id));
        assert!(!replacement_requests.is_empty());
        assert!(!branch_a.apply_batch(
            first[1].branch_revision,
            first[1].context.request_id,
            &first[1].location,
            vec![entry(4, r"C:\a\stale.txt", false)]
        ));
        let (generation_cancel, _) =
            coordinator.plan(tab_a, Generation::new(9), &mut branch_a, 0..1, |_| None);
        for request in &replacement_requests {
            assert!(
                generation_cancel.contains(&request.context.request_id),
                "a new generation supersedes only this tab's previous listings"
            );
        }
        assert!(!generation_cancel.contains(&first[1].context.request_id));
        assert!(coordinator.complete(first[1].context.request_id));
        assert!(!coordinator.complete(first[1].context.request_id));
        let (_b_cancel, b_requests) =
            coordinator.plan(tab_b, generation_b, &mut branch_b, 0..1, |_| None);
        assert_eq!(
            b_requests.len(),
            1,
            "a freed slot lets the other tab continue"
        );
        let closed = coordinator.supersede_tab(tab_b);
        assert_eq!(
            closed,
            b_requests
                .iter()
                .map(|request| request.context.request_id)
                .collect::<Vec<_>>()
        );
        assert!(
            closed
                .iter()
                .all(|request_id| *request_id != first[1].context.request_id)
        );
        for request_id in closed {
            assert!(coordinator.complete(request_id));
            assert!(!coordinator.complete(request_id));
        }
        assert!(coordinator.active_count() <= COLUMN_LOAD_CONCURRENCY);
    }

    #[test]
    fn out_of_order_completion_applies_only_the_matching_request() {
        let tab = TabId::new();
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a\b\c"), 240);
        let mut coordinator = ColumnLoadCoordinator::new(2);
        let (_, requests) = coordinator.plan(tab, Generation::new(1), &mut branch, 0..2, |_| None);
        assert!(requests.len() >= 1);
        let first = &requests[0];
        assert!(branch.apply_batch(
            first.branch_revision,
            first.context.request_id,
            &first.location,
            vec![entry(5, r"C:\a\kept.txt", false)]
        ));
        assert!(branch.apply_terminal(
            first.branch_revision,
            first.context.request_id,
            &first.location,
            &ColumnListingTerminal::Finished
        ));
        assert!(!branch.apply_terminal(
            first.branch_revision,
            RequestId::new(),
            &first.location,
            &ColumnListingTerminal::Finished
        ));
    }

    #[test]
    fn column_enumeration_has_exactly_one_terminal_and_rejects_unsupported_locations() {
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let location = path_location(r"\\server\share");
        let command = ExplorerCommand::EnumerateColumn {
            context: context.clone(),
            location: location.clone(),
            branch_revision: 4,
        };
        let mut ledger = TerminalLedger::default();
        assert!(ledger.register(&command).is_ok());
        let error = column_listing_rejected(&location, DriveKind::Fixed).expect("rejected");
        let event = ExplorerEvent::ColumnDirectoryFinished {
            context: context.clone(),
            branch_revision: 4,
            location,
            outcome: ColumnListingTerminal::Failed(error),
        };
        assert!(event.is_terminal());
        assert!(ledger.record_terminal(&event).is_ok());
        assert!(ledger.verify_drained().is_ok());
        assert!(ledger.record_terminal(&event).is_err());

        let cancel = ExplorerEvent::ColumnDirectoryFinished {
            context: RequestContext::new(TabId::new(), Generation::new(2)),
            branch_revision: 1,
            location: path_location(r"C:\fixture"),
            outcome: ColumnListingTerminal::Cancelled,
        };
        assert!(cancel.is_terminal());
    }

    #[test]
    fn tab_branches_are_isolated() {
        let mut store = ColumnBranchStore::default();
        let first = TabId::new();
        let second = TabId::new();
        store.insert(
            first,
            ColumnBranch::from_location(path_location(r"C:\one"), 240),
        );
        store.insert(
            second,
            ColumnBranch::from_location(path_location(r"C:\two"), 200),
        );
        store.get_mut(first).expect("first").align_to_location(
            path_location(r"C:\one\child"),
            240,
            None,
        );
        assert_eq!(
            store
                .get(second)
                .and_then(|branch| branch.active_location())
                .and_then(|location| location.path())
                .map(|path| path.to_string_lossy().into_owned()),
            Some(r"C:\two".to_owned())
        );
    }

    #[test]
    fn keyboard_right_then_left_keeps_the_path() {
        let mut session = ColumnNavigationSession::new(path_location(r"C:\a"), "a", 240);
        let folder = entry(2, r"C:\a\b", true);
        session.branch.focus_entry_for_test(folder.id.clone());
        let effect = session
            .branch
            .reveal_focused_child(std::slice::from_ref(&folder), 240);
        assert!(effect.navigate_to.is_some());
        assert!(session.branch.focus_parent());
        assert!(
            session
                .branch
                .levels()
                .iter()
                .any(|level| level.location == path_location(r"C:\a"))
        );
    }

    #[test]
    fn saved_width_round_trip_does_not_keep_preview_pixels() {
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a\b"), 240);
        branch.set_width(0, 10);
        branch.set_width(1, 900);
        let widths = normalized_column_widths(&branch.widths());
        assert!(
            widths
                .iter()
                .all(|width| { (COLUMN_WIDTH_MIN..=COLUMN_WIDTH_MAX).contains(width) })
        );
        let restored = ColumnBranch::from_location(path_location(r"C:\a\b"), 240);
        assert!(restored.pending_file().is_none());
        let _ = HistoryEntry::new(path_location(r"C:\a\b"), "b");
    }

    #[test]
    fn resize_narrow_window_and_dpi_keep_logical_width_selection_and_ancestors() {
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a\b\c\d\e\f"), 240);
        branch.set_width(0, 12);
        branch.set_width(1, 9_999);
        assert_eq!(branch.widths()[0], COLUMN_WIDTH_MIN);
        assert_eq!(branch.widths()[1], COLUMN_WIDTH_MAX);
        let selected = ShellItemId::from_provider_bytes([4]).expect("id");
        branch.focus_entry_for_test(selected.clone());
        let widths = branch
            .widths()
            .iter()
            .map(|width| f32::from(*width))
            .collect::<Vec<_>>();
        let narrow = reveal_column_offset(widths.len() - 1, &widths, 360.0, 180.0, 0.0);
        assert!(narrow > 0.0);
        let start: f32 = widths[..widths.len() - 1].iter().sum();
        let content = f32::max(360.0 - 180.0, 120.0);
        assert!(start < narrow + content + 1.0);
        assert!(narrow <= start);
        for scale in [1.0_f32, 1.25, 1.5, 2.0] {
            let logical = widths[0];
            let physical = logical * scale;
            assert!((physical / scale - logical).abs() < 0.01);
            assert_eq!(branch.selected_ids(), std::slice::from_ref(&selected));
        }
    }

    #[test]
    fn keyboard_vertical_and_modifier_selection_stay_inside_one_column() {
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a"), 240);
        let first = entry(1, r"C:\a\one.txt", false);
        let second = entry(2, r"C:\a\two.txt", false);
        let third = entry(3, r"C:\a\three.txt", false);
        let rows = [first.clone(), second.clone(), third.clone()];
        branch.focus_entry_for_test(first.id.clone());
        assert!(branch.move_vertical(1, &rows));
        assert_eq!(branch.selected_ids(), std::slice::from_ref(&second.id));
        branch.toggle_additional(branch.active_index(), third.id.clone());
        assert_eq!(branch.selected_ids().len(), 2);
        assert_eq!(branch.active_index(), branch.levels().len() - 1);
        let order = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        branch.select_range(branch.active_index(), first.id.clone(), &order);
        assert_eq!(branch.selected_ids().len(), 3);
        let parent = branch.active_index().saturating_sub(1);
        branch.toggle_additional(parent, first.id.clone());
        assert_eq!(branch.active_index(), parent);
        assert_eq!(branch.selected_ids(), std::slice::from_ref(&first.id));
        let file = entry(9, r"C:\a\open.txt", false);
        branch.focus_entry_for_test(file.id.clone());
        let effect = branch.reveal_focused_child(std::slice::from_ref(&file), 240);
        assert!(effect.open_file);
        assert!(effect.navigate_to.is_none());
    }

    #[test]
    fn navigated_location_stays_on_the_deepest_directory_when_focus_moves_left() {
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a\b\c"), 240);
        let deepest = branch.navigated_location().cloned();
        assert!(branch.focus_parent());
        assert_eq!(branch.navigated_location().cloned(), deepest);
        assert_ne!(branch.active_location().cloned(), deepest);
    }

    #[test]
    fn narrow_strip_can_reach_the_first_ancestor_and_the_final_column() {
        let (strip, preview) = column_layout_spans(200.0, 280.0, true);
        assert!(strip >= COLUMN_STRIP_MIN_SPAN - f32::EPSILON);
        assert!(preview > 0.0);
        assert!((strip + preview - 200.0).abs() < 0.01);
        let (tiny_strip, tiny_preview) = column_layout_spans(90.0, 280.0, true);
        assert!(tiny_strip > 0.0 && tiny_preview > 0.0);
        assert!((tiny_strip + tiny_preview - 90.0).abs() < 0.01);

        let widths = [240.0, 240.0, 240.0, 80.0];
        let revealed = reveal_column_offset(3, &widths, 200.0, 160.0, 0.0);
        assert_eq!(revealed, 720.0);
        assert_eq!(clamp_column_horizontal_offset(-20.0, 800.0, 40.0), 0.0);
        assert_eq!(clamp_column_horizontal_offset(10_000.0, 800.0, 40.0), 760.0);
        assert_eq!(clamp_column_horizontal_offset(12.0, 800.0, 0.0), 12.0);
    }

    #[test]
    fn vertical_offset_clamps_empty_short_and_extreme_viewports_and_reveals_ends() {
        let row = COLUMN_ROW_HEIGHT;
        assert_eq!(
            ColumnBranch::clamp_vertical_offset(4_000.0, 0, row, 80.0),
            0.0
        );
        assert_eq!(
            ColumnBranch::clamp_vertical_offset(4_000.0, 0, row, 0.0),
            0.0
        );
        assert_eq!(
            ColumnBranch::clamp_vertical_offset(4_000.0, 1, row, 80.0),
            0.0
        );
        assert_eq!(
            ColumnBranch::clamp_vertical_offset(4_000.0, 2, row, 48.0),
            (2.0 * row - 48.0).max(0.0)
        );
        assert_eq!(
            ColumnBranch::clamp_vertical_offset(10_000.0, 3, row, 8.0),
            (3.0 * row - 8.0).max(0.0)
        );
        assert_eq!(
            ColumnBranch::clamp_vertical_offset(-8.0, 10, row, 48.0),
            0.0
        );
        assert_eq!(
            ColumnBranch::clamp_vertical_offset(f32::NAN, 10, row, 48.0),
            0.0
        );
        assert_eq!(
            ColumnBranch::clamp_vertical_offset(32.0, 10, row, 0.0),
            32.0
        );
        assert_eq!(
            ColumnBranch::clamp_vertical_offset(32.0, 10, row, f32::NAN),
            32.0
        );

        assert_eq!(
            ColumnBranch::reveal_row_offset(0, 20, row, 48.0, 200.0),
            0.0
        );
        assert_eq!(
            ColumnBranch::reveal_row_offset(19, 20, row, 48.0, 0.0),
            (20.0 * row - 48.0).max(0.0)
        );
        assert_eq!(
            ColumnBranch::reveal_row_offset(1, 20, row, 48.0, 0.0),
            (2.0 * row - 48.0).max(0.0)
        );
        assert_eq!(
            ColumnBranch::reveal_row_offset(2, 20, row, 48.0, 0.0),
            (3.0 * row - 48.0).max(0.0)
        );
        assert_eq!(
            ColumnBranch::reveal_row_offset(2, 3, row, 8.0, 0.0),
            (3.0 * row - 8.0).max(0.0)
        );
        assert_eq!(ColumnBranch::reveal_row_offset(0, 3, row, 8.0, 64.0), 0.0);
        assert_eq!(ColumnBranch::reveal_row_offset(0, 0, row, 48.0, 80.0), 0.0);

        let mut branch = ColumnBranch::from_location(path_location(r"C:\a\b"), 240);
        branch.set_vertical_offset(0, 12.0);
        branch.set_vertical_offset(1, 48.0);
        assert_eq!(branch.levels()[0].vertical_offset, 12.0);
        assert_eq!(branch.levels()[1].vertical_offset, 48.0);
        let revealed =
            ColumnBranch::reveal_row_offset(0, 30, row, 48.0, branch.levels()[1].vertical_offset);
        branch.set_vertical_offset(1, revealed);
        assert_eq!(branch.levels()[0].vertical_offset, 12.0);
        assert_eq!(branch.levels()[1].vertical_offset, 0.0);
    }

    #[test]
    fn shift_range_uses_the_full_displayed_order_and_keeps_the_anchor() {
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a"), 240);
        let rows = [
            entry(1, r"C:\a\a.txt", false),
            entry(2, r"C:\a\b.txt", false),
            entry(3, r"C:\a\c.txt", false),
            entry(4, r"C:\a\d.txt", false),
            entry(5, r"C:\a\e.txt", false),
        ];
        let order = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let column = branch.active_index();
        branch.focus_entry_for_test(rows[1].id.clone());
        branch.select_range(column, rows[3].id.clone(), &order);
        assert_eq!(
            branch.selected_ids(),
            &[rows[1].id.clone(), rows[2].id.clone(), rows[3].id.clone()]
        );
        assert_eq!(branch.selected_ids().last(), Some(&rows[3].id));
        branch.select_range(column, rows[0].id.clone(), &order);
        assert_eq!(branch.selected_ids().last(), Some(&rows[0].id));
        assert_eq!(branch.selected_ids().len(), 2);
        assert!(branch.selected_ids().contains(&rows[1].id));
        assert!(!branch.selected_ids().contains(&rows[3].id));

        branch.focus_entry_for_test(rows[1].id.clone());
        branch.select_range(
            column,
            rows[4].id.clone(),
            std::slice::from_ref(&rows[4].id),
        );
        assert_eq!(
            branch.selected_ids(),
            std::slice::from_ref(&rows[4].id),
            "a click that only supplies itself cannot invent the in-between rows"
        );
    }

    #[test]
    fn ctrl_toggle_replaces_a_different_column_instead_of_leaking_it() {
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a\b"), 240);
        let current = branch.active_index();
        let parent = current - 1;
        let kept = entry(1, r"C:\a\b\one.txt", false);
        let extra = entry(2, r"C:\a\b\two.txt", false);
        let ancestor = entry(3, r"C:\a\notes.txt", false);
        branch.focus_entry_for_test(kept.id.clone());
        branch.toggle_additional(current, extra.id.clone());
        assert_eq!(branch.selected_ids().len(), 2);
        branch.toggle_additional(current, kept.id.clone());
        assert_eq!(branch.selected_ids(), std::slice::from_ref(&extra.id));
        branch.toggle_additional(parent, ancestor.id.clone());
        assert_eq!(branch.active_index(), parent);
        assert_eq!(branch.selected_ids(), std::slice::from_ref(&ancestor.id));
        branch.toggle_additional(parent, ancestor.id.clone());
        assert!(branch.selected_ids().is_empty());
        assert_ne!(branch.active_index(), current);
    }

    #[test]
    fn truncation_drops_the_anchor_so_the_next_shift_cannot_revive_it() {
        let mut branch = ColumnBranch::from_location(path_location(r"C:\a\b\c"), 240);
        let stale = ShellItemId::from_provider_bytes([4]).expect("id");
        let target = ShellItemId::from_provider_bytes([5]).expect("id");
        branch.remember_branch_child(1, ShellItemId::from_provider_bytes([9]).expect("child"));
        branch.focus_entry_for_test(stale.clone());
        branch.drop_missing_branch_child(1, |_| false);
        assert!(branch.selected_ids().is_empty());
        let column = branch.active_index();
        branch.select_range(column, target.clone(), &[stale.clone(), target.clone()]);
        assert_eq!(branch.selected_ids(), std::slice::from_ref(&target));
    }
}
