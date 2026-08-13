//! Cached directory presentation and deterministic viewport realization geometry.

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use explorer_model::{ColumnId, DirectorySnapshot, FileEntry, SortDescriptor, SortDirection};

pub const MAX_STANDARD_REALIZED_ITEMS: usize = 250;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailsFilterOption {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DetailsFilters {
    selected: HashMap<ColumnId, HashSet<String>>,
}

impl DetailsFilters {
    pub fn is_active(&self, column: &ColumnId) -> bool {
        self.selected
            .get(column)
            .is_some_and(|values| !values.is_empty())
    }

    pub fn is_selected(&self, column: &ColumnId, key: &str) -> bool {
        self.selected
            .get(column)
            .is_some_and(|values| values.contains(key))
    }

    pub fn toggle(&mut self, column: &ColumnId, key: &str) {
        let values = self.selected.entry(column.clone()).or_default();
        if !values.insert(key.to_owned()) {
            values.remove(key);
        }
        if values.is_empty() {
            self.selected.remove(column);
        }
    }

    pub fn clear(&mut self, column: &ColumnId) {
        self.selected.remove(column);
    }

    pub fn clear_all(&mut self) {
        self.selected.clear();
    }

    pub fn options(snapshot: &DirectorySnapshot, column: &ColumnId) -> Vec<DetailsFilterOption> {
        let mut options = snapshot
            .entries()
            .iter()
            .map(|entry| filter_value(entry, column))
            .collect::<Vec<_>>();
        options.sort_by(|left, right| left.1.cmp(&right.1));
        options.dedup_by(|left, right| left.0 == right.0);
        options
            .into_iter()
            .map(|(key, label)| DetailsFilterOption { key, label })
            .collect()
    }

    fn matches(&self, entry: &FileEntry) -> bool {
        self.selected.iter().all(|(column, selected)| {
            let (key, _) = filter_value(entry, column);
            selected.contains(&key)
        })
    }
}

fn filter_value(entry: &FileEntry, column: &ColumnId) -> (String, String) {
    match column {
        ColumnId::Name => match entry
            .display_name
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase())
        {
            Some('A'..='H') => ("name:a-h".into(), "A–H".into()),
            Some('I'..='P') => ("name:i-p".into(), "I–P".into()),
            Some('Q'..='Z') => ("name:q-z".into(), "Q–Z".into()),
            _ => ("name:other".into(), "其他".into()),
        },
        ColumnId::Size => match entry.metadata.size_bytes {
            Some(bytes) if bytes <= 16 * 1024 => ("size:tiny".into(), "極小 (0–16 KB)".into()),
            Some(bytes) if bytes <= 1024 * 1024 => ("size:small".into(), "小 (16 KB–1 MB)".into()),
            Some(bytes) if bytes <= 128 * 1024 * 1024 => {
                ("size:medium".into(), "中 (1–128 MB)".into())
            }
            Some(_) => ("size:large".into(), "大 (>128 MB)".into()),
            None => ("size:none".into(), "未指定".into()),
        },
        ColumnId::DateModified => date_filter_value(entry.metadata.modified_sort_key),
        ColumnId::DateCreated => date_filter_value(entry.metadata.created_sort_key),
        ColumnId::Type => text_filter_value(
            entry
                .metadata
                .type_display
                .as_deref()
                .or(if entry.is_container {
                    Some("檔案資料夾")
                } else {
                    None
                }),
            "type",
        ),
        ColumnId::Authors => {
            text_filter_value(entry.metadata.authors_display.as_deref(), "authors")
        }
        ColumnId::Tags => text_filter_value(entry.metadata.tags_display.as_deref(), "tags"),
        ColumnId::Title => text_filter_value(entry.metadata.title_display.as_deref(), "title"),
        ColumnId::FileCount | ColumnId::FolderCount | ColumnId::Extension { .. } => {
            ("extension:unavailable".into(), "無法使用".into())
        }
    }
}

fn text_filter_value(value: Option<&str>, prefix: &str) -> (String, String) {
    let label = value.filter(|value| !value.is_empty()).unwrap_or("未指定");
    (
        format!("{prefix}:{}", label.to_lowercase()),
        label.to_owned(),
    )
}

fn date_filter_value(filetime: Option<u64>) -> (String, String) {
    const WINDOWS_TO_UNIX_SECONDS: u64 = 11_644_473_600;
    const TICKS_PER_SECOND: u64 = 10_000_000;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let Some(seconds) = filetime
        .and_then(|value| value.checked_div(TICKS_PER_SECOND))
        .and_then(|value| value.checked_sub(WINDOWS_TO_UNIX_SECONDS))
    else {
        return ("date:none".into(), "未指定".into());
    };
    match now.saturating_sub(seconds) / 86_400 {
        0 => ("date:today".into(), "今天".into()),
        1 => ("date:yesterday".into(), "昨天".into()),
        2..=6 => ("date:this-week".into(), "這星期初".into()),
        _ => ("date:earlier".into(), "較早".into()),
    }
}

/// Immutable sorted/filtered indices into shared directory entry storage.
#[derive(Clone, Debug)]
pub struct DirectoryPresentation {
    revision: u64,
    hidden_items: bool,
    sort: SortDescriptor,
    filters: DetailsFilters,
    entries: Arc<Vec<FileEntry>>,
    ordered_indices: Arc<Vec<usize>>,
}

impl DirectoryPresentation {
    pub fn build(snapshot: &DirectorySnapshot, hidden_items: bool, sort: SortDescriptor) -> Self {
        Self::build_filtered(snapshot, hidden_items, sort, DetailsFilters::default())
    }

    pub fn build_filtered(
        snapshot: &DirectorySnapshot,
        hidden_items: bool,
        sort: SortDescriptor,
        filters: DetailsFilters,
    ) -> Self {
        let entries = snapshot.shared_entries();
        let mut ordered_indices = (0..entries.len())
            .filter(|index| {
                if !filters.matches(&entries[*index]) {
                    return false;
                }
                let metadata = &entries[*index].metadata;
                // Windows reports drive roots with HIDDEN/SYSTEM bits (for example 0x16 or
                // 0x36). Those bits describe the root object, not a user-hidden child. This PC
                // must always keep drives visible regardless of the ordinary hidden-items view
                // preference.
                if metadata.drive.is_some() {
                    return true;
                }
                let attributes = metadata.filesystem_attributes;
                let protected_system_item = attributes & 0x4 != 0;
                let ordinary_hidden_item = attributes & 0x2 != 0;
                !protected_system_item && (hidden_items || !ordinary_hidden_item)
            })
            .collect::<Vec<_>>();
        ordered_indices.sort_by(|left, right| compare_file_entries(snapshot, *left, *right, &sort));
        Self {
            revision: snapshot.revision(),
            hidden_items,
            sort,
            filters,
            entries,
            ordered_indices: Arc::new(ordered_indices),
        }
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn entries(&self) -> &Arc<Vec<FileEntry>> {
        &self.entries
    }

    pub fn ordered_indices(&self) -> &Arc<Vec<usize>> {
        &self.ordered_indices
    }

    pub fn len(&self) -> usize {
        self.ordered_indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ordered_indices.is_empty()
    }

    pub fn entry(&self, visible_index: usize) -> Option<(usize, &FileEntry)> {
        let snapshot_index = *self.ordered_indices.get(visible_index)?;
        self.entries
            .get(snapshot_index)
            .map(|entry| (snapshot_index, entry))
    }

    /// Reorders an already-filtered presentation using copied exact-byte
    /// values supplied by a runtime Details column. Missing values stay last
    /// in either direction, matching the built-in size-column contract.
    pub fn sorted_by_extension_bytes(
        &self,
        values: &HashMap<explorer_model::ShellItemId, Option<u64>>,
        direction: SortDirection,
    ) -> Self {
        let mut ordered_indices = (*self.ordered_indices).clone();
        ordered_indices.sort_by(|left_index, right_index| {
            let left = &self.entries[*left_index];
            let right = &self.entries[*right_index];
            match (left.is_container, right.is_container) {
                (true, false) => return Ordering::Less,
                (false, true) => return Ordering::Greater,
                _ => {}
            }
            compare_optional(
                values.get(&left.id).copied().flatten(),
                values.get(&right.id).copied().flatten(),
                direction,
            )
            .then_with(|| left.display_name.cmp(&right.display_name))
            .then_with(|| left.id.provider_bytes().cmp(right.id.provider_bytes()))
        });
        Self {
            ordered_indices: Arc::new(ordered_indices),
            ..self.clone()
        }
    }

    fn matches(
        &self,
        snapshot: &DirectorySnapshot,
        hidden_items: bool,
        sort: &SortDescriptor,
        filters: &DetailsFilters,
    ) -> bool {
        self.revision == snapshot.revision()
            && self.hidden_items == hidden_items
            && self.sort == *sort
            && &self.filters == filters
            && Arc::ptr_eq(&self.entries, &snapshot.shared_entries())
    }
}

/// One-entry cache because only the active file surface is rendered per window frame.
#[derive(Clone, Debug, Default)]
pub struct DirectoryPresentationCache {
    current: Option<DirectoryPresentation>,
    rebuilds: u64,
}

impl DirectoryPresentationCache {
    pub fn clear(&mut self) {
        self.current = None;
    }

    pub fn resolve(
        &mut self,
        snapshot: &DirectorySnapshot,
        hidden_items: bool,
        sort: &SortDescriptor,
    ) -> DirectoryPresentation {
        self.resolve_filtered(snapshot, hidden_items, sort, DetailsFilters::default())
    }

    pub fn resolve_filtered(
        &mut self,
        snapshot: &DirectorySnapshot,
        hidden_items: bool,
        sort: &SortDescriptor,
        filters: DetailsFilters,
    ) -> DirectoryPresentation {
        if let Some(current) = self
            .current
            .as_ref()
            .filter(|current| current.matches(snapshot, hidden_items, sort, &filters))
        {
            return current.clone();
        }
        let presentation =
            DirectoryPresentation::build_filtered(snapshot, hidden_items, sort.clone(), filters);
        self.current = Some(presentation.clone());
        self.rebuilds = self.rebuilds.saturating_add(1);
        presentation
    }

    pub const fn rebuilds(&self) -> u64 {
        self.rebuilds
    }
}

fn compare_file_entries(
    snapshot: &DirectorySnapshot,
    left_index: usize,
    right_index: usize,
    sort: &SortDescriptor,
) -> Ordering {
    let left = &snapshot.entries()[left_index];
    let right = &snapshot.entries()[right_index];
    match (left.is_container, right.is_container) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }
    let (Some(left_keys), Some(right_keys)) = (
        snapshot.sort_keys(left_index),
        snapshot.sort_keys(right_index),
    ) else {
        return left_index.cmp(&right_index);
    };
    let ordering = match &sort.column {
        ColumnId::Name => compare_text(
            Some(left_keys.display_name()),
            Some(right_keys.display_name()),
            sort.direction,
        ),
        ColumnId::DateModified => compare_optional(
            left.metadata.modified_sort_key,
            right.metadata.modified_sort_key,
            sort.direction,
        ),
        ColumnId::Type => compare_text(
            left_keys.type_display(),
            right_keys.type_display(),
            sort.direction,
        ),
        ColumnId::Size => compare_optional(
            left.metadata.size_bytes,
            right.metadata.size_bytes,
            sort.direction,
        ),
        ColumnId::DateCreated => compare_optional(
            left.metadata.created_sort_key,
            right.metadata.created_sort_key,
            sort.direction,
        ),
        ColumnId::Authors => compare_text(
            left.metadata.authors_display.as_deref(),
            right.metadata.authors_display.as_deref(),
            sort.direction,
        ),
        ColumnId::Tags => compare_text(
            left.metadata.tags_display.as_deref(),
            right.metadata.tags_display.as_deref(),
            sort.direction,
        ),
        ColumnId::Title => compare_text(
            left.metadata.title_display.as_deref(),
            right.metadata.title_display.as_deref(),
            sort.direction,
        ),
        ColumnId::FileCount | ColumnId::FolderCount | ColumnId::Extension { .. } => Ordering::Equal,
    };
    ordering
        .then_with(|| left_keys.display_name().cmp(right_keys.display_name()))
        .then_with(|| left.id.provider_bytes().cmp(right.id.provider_bytes()))
}

fn compare_text(left: Option<&str>, right: Option<&str>, direction: SortDirection) -> Ordering {
    compare_optional(left, right, direction)
}

fn compare_optional<T: Ord>(
    left: Option<T>,
    right: Option<T>,
    direction: SortDirection,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => match direction {
            SortDirection::Ascending => left.cmp(&right),
            SortDirection::Descending => right.cmp(&left),
        },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Realized range and spacer geometry for a fixed-height list.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VirtualRange {
    pub items: Range<usize>,
    pub leading_logical_pixels: u32,
    pub trailing_logical_pixels: u32,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "validated finite positive viewport geometry is clamped to collection and u32 bounds"
)]
pub fn fixed_virtual_range(
    item_count: usize,
    row_height: f32,
    viewport_height: f32,
    scroll_offset: f32,
    overscan_viewports: usize,
) -> VirtualRange {
    if item_count == 0 || row_height <= 0.0 || viewport_height <= 0.0 {
        return VirtualRange::default();
    }
    let visible_rows = (viewport_height / row_height).ceil().max(1.0) as usize;
    let first_visible = (scroll_offset.max(0.0) / row_height).floor() as usize;
    let overscan = visible_rows.saturating_mul(overscan_viewports);
    let start = first_visible.saturating_sub(overscan).min(item_count);
    let end = first_visible
        .saturating_add(visible_rows)
        .saturating_add(overscan)
        .min(item_count);
    let logical_height = |count: usize| {
        ((count as f64) * f64::from(row_height))
            .round()
            .clamp(0.0, f64::from(u32::MAX)) as u32
    };
    VirtualRange {
        items: start..end,
        leading_logical_pixels: logical_height(start),
        trailing_logical_pixels: logical_height(item_count.saturating_sub(end)),
    }
}

/// Column and realized-range geometry for fixed-size wrapped views.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VirtualGrid {
    pub columns: usize,
    pub total_rows: usize,
    pub items: Range<usize>,
    pub leading_logical_pixels: u32,
    pub trailing_logical_pixels: u32,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "validated finite positive viewport geometry is clamped to collection and u32 bounds"
)]
pub fn fixed_grid_virtual_range(
    item_count: usize,
    cell_width: f32,
    cell_height: f32,
    viewport_width: f32,
    viewport_height: f32,
    scroll_offset: f32,
    overscan_viewports: usize,
) -> VirtualGrid {
    if item_count == 0
        || cell_width <= 0.0
        || cell_height <= 0.0
        || viewport_width <= 0.0
        || viewport_height <= 0.0
    {
        return VirtualGrid::default();
    }
    let columns = (viewport_width / cell_width).floor().max(1.0) as usize;
    let total_rows = item_count.div_ceil(columns);
    let mut rows = fixed_virtual_range(
        total_rows,
        cell_height,
        viewport_height,
        scroll_offset,
        overscan_viewports,
    );
    let visible_rows = (viewport_height / cell_height).ceil().max(1.0) as usize;
    let first_visible = (scroll_offset.max(0.0) / cell_height).floor() as usize;
    let maximum_rows = MAX_STANDARD_REALIZED_ITEMS
        .checked_div(columns)
        .unwrap_or_default()
        .max(visible_rows);
    if rows.items.len() > maximum_rows {
        let extra_rows = maximum_rows.saturating_sub(visible_rows);
        let start = first_visible.saturating_sub(extra_rows / 2).min(total_rows);
        let end = start.saturating_add(maximum_rows).min(total_rows);
        let start = end.saturating_sub(maximum_rows);
        let logical_height = |count: usize| {
            ((count as f64) * f64::from(cell_height))
                .round()
                .clamp(0.0, f64::from(u32::MAX)) as u32
        };
        rows = VirtualRange {
            items: start..end,
            leading_logical_pixels: logical_height(start),
            trailing_logical_pixels: logical_height(total_rows.saturating_sub(end)),
        };
    }
    VirtualGrid {
        columns,
        total_rows,
        items: rows.items.start.saturating_mul(columns)
            ..rows.items.end.saturating_mul(columns).min(item_count),
        leading_logical_pixels: rows.leading_logical_pixels,
        trailing_logical_pixels: rows.trailing_logical_pixels,
    }
}

#[cfg(test)]
mod tests {
    use explorer_model::{
        ColumnId, DriveAvailability, DriveKind, DriveMetadata, FileEntryMetadata,
        LocationDescriptor, ShellItemId, SortDescriptor, SortDirection,
    };

    use super::*;

    fn entry(id: u64, name: &str) -> FileEntry {
        FileEntry {
            id: ShellItemId::from_provider_bytes(id.to_le_bytes()).expect("id"),
            display_name: name.to_owned(),
            location: LocationDescriptor::file_system(format!(r"C:\fixture\{name}")),
            is_container: false,
            metadata: FileEntryMetadata::default(),
        }
    }

    #[test]
    fn this_pc_keeps_system_hidden_drive_roots_visible() {
        let mut drive = entry(7, "Local Disk (C:)");
        drive.location = LocationDescriptor::file_system(r"C:\");
        drive.is_container = true;
        drive.metadata.filesystem_attributes = 0x16;
        drive.metadata.drive = Some(DriveMetadata {
            kind: DriveKind::Fixed,
            availability: DriveAvailability::Available,
            volume_label: Some("Local Disk".to_owned()),
            filesystem_name: Some("NTFS".to_owned()),
            total_bytes: Some(2_000),
            available_bytes: Some(800),
        });
        let mut hidden = entry(8, "desktop.ini");
        hidden.metadata.filesystem_attributes = 0x6;
        let mut snapshot = DirectorySnapshot::default();
        snapshot.upsert(drive);
        snapshot.upsert(hidden);

        let presentation = DirectoryPresentation::build(
            &snapshot,
            false,
            SortDescriptor {
                column: ColumnId::Name,
                direction: SortDirection::Ascending,
            },
        );
        assert_eq!(presentation.len(), 1);
        assert_eq!(
            presentation.entry(0).unwrap().1.display_name,
            "Local Disk (C:)"
        );
    }

    #[test]
    fn presentation_reuses_revision_and_normalized_ordering() {
        let mut snapshot = DirectorySnapshot::default();
        snapshot.upsert(entry(1, "zeta.TXT"));
        snapshot.upsert(entry(2, "Alpha.txt"));
        let mut cache = DirectoryPresentationCache::default();
        let sort = SortDescriptor {
            column: ColumnId::Name,
            direction: SortDirection::Ascending,
        };
        let first = cache.resolve(&snapshot, false, &sort);
        let second = cache.resolve(&snapshot, false, &sort);
        assert_eq!(cache.rebuilds(), 1);
        assert!(Arc::ptr_eq(
            first.ordered_indices(),
            second.ordered_indices()
        ));
        assert_eq!(first.entry(0).unwrap().1.display_name, "Alpha.txt");

        snapshot.upsert(entry(1, "aardvark.txt"));
        let changed = cache.resolve(&snapshot, false, &sort);
        assert_eq!(cache.rebuilds(), 2);
        assert_eq!(changed.entry(0).unwrap().1.display_name, "aardvark.txt");

        // Hover, focus, selection, viewport geometry and resizing are deliberately absent from
        // the projection key, so resolving again cannot sort or allocate another index vector.
        let after_interactions = cache.resolve(&snapshot, false, &sort);
        assert_eq!(cache.rebuilds(), 2);
        assert!(Arc::ptr_eq(
            changed.ordered_indices(),
            after_interactions.ordered_indices()
        ));

        let descending_sort = SortDescriptor {
            direction: SortDirection::Descending,
            ..sort
        };
        let descending = cache.resolve(&snapshot, false, &descending_sort);
        assert_eq!(cache.rebuilds(), 3);
        assert_eq!(descending.entry(0).unwrap().1.display_name, "Alpha.txt");
    }

    #[test]
    fn details_filters_group_values_and_apply_before_sorting() {
        let mut alpha = entry(1, "Alpha.txt");
        alpha.metadata.size_bytes = Some(8 * 1024);
        alpha.metadata.type_display = Some("Text Document".into());
        let mut zebra = entry(2, "Zebra.log");
        zebra.metadata.size_bytes = Some(2 * 1024 * 1024);
        zebra.metadata.type_display = Some("Log File".into());
        let mut snapshot = DirectorySnapshot::default();
        snapshot.upsert(zebra);
        snapshot.upsert(alpha);
        let sort = SortDescriptor {
            column: ColumnId::Name,
            direction: SortDirection::Ascending,
        };

        let mut filters = DetailsFilters::default();
        filters.toggle(&ColumnId::Name, "name:a-h");
        filters.toggle(&ColumnId::Size, "size:tiny");
        let presentation = DirectoryPresentation::build_filtered(&snapshot, false, sort, filters);
        assert_eq!(presentation.len(), 1);
        assert_eq!(presentation.entry(0).unwrap().1.display_name, "Alpha.txt");

        let name_options = DetailsFilters::options(&snapshot, &ColumnId::Name);
        assert!(name_options.iter().any(|option| option.label == "A–H"));
        assert!(name_options.iter().any(|option| option.label == "Q–Z"));
    }

    #[test]
    fn changing_details_filters_invalidates_presentation_cache() {
        let mut snapshot = DirectorySnapshot::default();
        snapshot.upsert(entry(1, "Alpha.txt"));
        snapshot.upsert(entry(2, "Zebra.txt"));
        let sort = SortDescriptor {
            column: ColumnId::Name,
            direction: SortDirection::Ascending,
        };
        let mut cache = DirectoryPresentationCache::default();
        let all = cache.resolve_filtered(&snapshot, false, &sort, DetailsFilters::default());
        assert_eq!(all.len(), 2);
        let mut filters = DetailsFilters::default();
        filters.toggle(&ColumnId::Name, "name:q-z");
        let filtered = cache.resolve_filtered(&snapshot, false, &sort, filters.clone());
        let reused = cache.resolve_filtered(&snapshot, false, &sort, filters);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered.entry(0).unwrap().1.display_name, "Zebra.txt");
        assert_eq!(cache.rebuilds(), 2);
        assert!(Arc::ptr_eq(
            filtered.ordered_indices(),
            reused.ordered_indices()
        ));
    }

    #[test]
    fn fixed_ranges_bound_one_hundred_thousand_entries() {
        let list = fixed_virtual_range(100_000, 24.0, 720.0, 800_000.0, 2);
        assert!(list.items.len() <= 150);
        assert!(list.items.end <= 100_000);
        assert!(list.leading_logical_pixels > 0);
        assert!(list.trailing_logical_pixels > 0);

        let grid = fixed_grid_virtual_range(100_000, 96.0, 112.0, 1_120.0, 720.0, 400_000.0, 2);
        assert_eq!(grid.columns, 11);
        assert!(grid.items.len() <= MAX_STANDARD_REALIZED_ITEMS);
        assert!(grid.items.end <= 100_000);
        assert!(grid.leading_logical_pixels > 0);
        assert!(grid.trailing_logical_pixels > 0);
    }

    #[test]
    fn every_view_family_has_bounded_standard_viewport_realization() {
        for (mode, row_height) in [
            (explorer_model::ViewMode::Details, 24.0),
            (explorer_model::ViewMode::List, 24.0),
            (explorer_model::ViewMode::Content, 72.0),
        ] {
            let range = fixed_virtual_range(100_000, row_height, 720.0, 900_000.0, 2);
            assert!(range.items.len() <= MAX_STANDARD_REALIZED_ITEMS, "{mode:?}");
        }
        for (mode, width, height) in [
            (explorer_model::ViewMode::SmallIcons, 240.0, 24.0),
            (explorer_model::ViewMode::MediumIcons, 104.0, 88.0),
            (explorer_model::ViewMode::LargeIcons, 120.0, 104.0),
            (explorer_model::ViewMode::ExtraLargeIcons, 144.0, 136.0),
            (explorer_model::ViewMode::Tiles, 280.0, 64.0),
        ] {
            let grid =
                fixed_grid_virtual_range(100_000, width, height, 1_120.0, 720.0, 900_000.0, 2);
            assert!(grid.items.len() <= MAX_STANDARD_REALIZED_ITEMS, "{mode:?}");
        }
    }
}
