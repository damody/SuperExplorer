//! Narrow UI boundary for the first runtime-provided Details column.
//!
//! The application owns the asynchronous folder walk.  This module owns only
//! the copied descriptor/value projection consumed by GPUI.

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

pub use explorer_extension_ui_api::{CellRenderContextV1, CellRenderPlanV1};

use explorer_model::{
    ColumnAlignment, ColumnApplicability, ColumnCost, ColumnDescriptor, ColumnId,
    ColumnSortSemantics, ColumnValueType, Generation, LocationDescriptor, ShellItemId, TabId,
};

const DIRECTORY_VALUE_CACHE_LIMIT: usize = 64;
const ITEM_VALUE_CACHE_LIMIT: usize = 8_192;

/// Stable cache identity so `D:`, `D:\`, and `\\?\D:\` share one entry.
pub(crate) fn directory_identity_key(location: &LocationDescriptor) -> String {
    match location {
        LocationDescriptor::FileSystem(path) => {
            let mut normalized = path
                .to_string_lossy()
                .replace('/', "\\")
                .to_ascii_lowercase();
            if let Some(rest) = normalized.strip_prefix("\\\\?\\") {
                normalized = rest.to_owned();
            }
            while normalized.ends_with('\\')
                && !(normalized.len() == 3 && normalized.as_bytes().get(1) == Some(&b':'))
            {
                normalized.pop();
            }
            if normalized.len() == 2 && normalized.as_bytes().get(1) == Some(&b':') {
                normalized.push('\\');
            }
            format!("fs:{normalized}")
        }
        LocationDescriptor::ParsingName(name) => format!("parse:{}", name.to_ascii_lowercase()),
        LocationDescriptor::KnownFolder(bytes) => {
            format!("known:{bytes:02x?}")
        }
        LocationDescriptor::ShellNamespace(bytes) => format!("shell:{bytes:02x?}"),
        LocationDescriptor::Virtual(location) => format!(
            "virtual:{}:{}:{}",
            location.provider_id.to_ascii_lowercase(),
            location.public_authority.clone().unwrap_or_default(),
            location.components.join("/")
        ),
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FolderSizeSnapshotKeyV1 {
    pub tab_id: TabId,
    pub generation: Generation,
}

impl From<&explorer_model::RequestContext> for FolderSizeSnapshotKeyV1 {
    fn from(context: &explorer_model::RequestContext) -> Self {
        Self {
            tab_id: context.tab_id,
            generation: context.generation,
        }
    }
}

/// Stable identity of the canonical Rust folder-size Details example.
pub const FOLDER_SIZE_COLUMN_PACKAGE_ID: &str = "rust-folder-size-visual-column";
pub const FOLDER_SIZE_COLUMN_ID: &str = "folder-size";

/// Controls whether the folder-size column includes its proportional bar.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FolderSizeDisplayMode {
    #[default]
    BarAndText,
    TextOnly,
}

impl FolderSizeDisplayMode {
    pub const fn shows_bar(self) -> bool {
        matches!(self, Self::BarAndText)
    }
}

/// App-owned, copied configuration for the folder-size column.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualColumnConfigV1 {
    pub descriptor: ColumnDescriptor,
    pub folder_size_display: FolderSizeDisplayMode,
}

impl Default for VisualColumnConfigV1 {
    fn default() -> Self {
        Self {
            descriptor: folder_size_column_descriptor(),
            folder_size_display: FolderSizeDisplayMode::default(),
        }
    }
}

/// One filesystem container submitted to the app-owned folder-size worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderSizeRequestV1 {
    pub context: explorer_model::RequestContext,
    pub item_id: ShellItemId,
    pub path: PathBuf,
    pub mft_cache_memory_mb: u16,
    pub require_directory_facts: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryFactsV1 {
    pub mft_generation: u64,
    pub file_count: u64,
    pub folder_count: u64,
}

/// One exact-byte folder-size result returned by the app-owned worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderSizeResultV1 {
    pub context: explorer_model::RequestContext,
    pub item_id: ShellItemId,
    /// `None` means that the provider completed without a displayable value.
    pub exact_bytes: Option<u64>,
    pub directory_facts: Option<DirectoryFactsV1>,
    pub partial: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FolderSizeBackendStatusV1 {
    #[default]
    Idle,
    HostCache,
    MftService,
    MftUnavailable,
}

impl FolderSizeBackendStatusV1 {
    pub const fn label(self, active: bool) -> Option<&'static str> {
        let label = match (self, active) {
            (Self::Idle, _) => return None,
            (Self::HostCache, false) => "Folder size: Host cache",
            (Self::HostCache, true) => "Folder size: Host cache...",
            (Self::MftService, false) => "Folder size: MFT service",
            (Self::MftService, true) => "Folder size: MFT service...",
            (Self::MftUnavailable, _) => "Folder size: MFT unavailable",
        };
        Some(label)
    }
}

/// Typed cell state. Partial totals remain displayable diagnostics but never
/// enter the exact-byte sort domain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderSizeValueV1 {
    pub exact_bytes: Option<u64>,
    pub directory_facts: Option<DirectoryFactsV1>,
    pub partial: bool,
    pub error: Option<String>,
    pub(crate) retry_after: Option<Instant>,
}

/// Application-owned async bridge. Calls occur only on the UI thread; the
/// implementation owns worker scheduling, deduplication, and cancellation.
pub trait VisualColumnRuntimePortV1: Send + Sync {
    fn config(&self) -> VisualColumnConfigV1;
    fn configure_cache_budgets(&self, _budgets: explorer_model::CacheBudgetSettingsV1) {}
    /// Keeps the privileged MFT index active only while a built-in count
    /// column is visible. Implementations should make this transition-safe.
    fn set_directory_facts_active(&self, _active: bool) {}
    fn submit_folder_size_requests(&self, requests: Vec<FolderSizeRequestV1>);
    fn cancel_folder_size_context(&self, context: &explorer_model::RequestContext);
    /// Invalidates only values whose items belong directly to this directory.
    /// In-flight work admitted before the refresh must not repopulate it.
    fn invalidate_directory_cache(&self, directory: &std::path::Path);
    fn drain_folder_size_results(&self) -> Vec<FolderSizeResultV1>;
    /// Moves completed asynchronous render plans into the host cache. Returns
    /// true only when GPUI needs another frame to consume a newly-ready plan.
    fn drain_render_results(&self) -> bool {
        false
    }
    fn backend_status(&self) -> (FolderSizeBackendStatusV1, bool) {
        (FolderSizeBackendStatusV1::Idle, false)
    }
    fn render_cell(&self, context: CellRenderContextV1) -> CellRenderPlanV1;
}

/// Render-time, host-owned snapshot. It has no worker handles or callbacks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderSizeColumnVisuals {
    pub config: VisualColumnConfigV1,
    /// Values are keyed by tab. A watcher/F5 generation bump on the same tab
    /// keeps measured aggregates so the Details cells do not flash empty.
    pub context: Option<explorer_model::RequestContext>,
    pub values: HashMap<ShellItemId, FolderSizeValueV1>,
    snapshots: HashMap<FolderSizeSnapshotKeyV1, HashMap<ShellItemId, FolderSizeValueV1>>,
    location_key: Option<String>,
    directory_cache: HashMap<String, HashMap<ShellItemId, FolderSizeValueV1>>,
    directory_lru: VecDeque<String>,
    item_cache: HashMap<ShellItemId, FolderSizeValueV1>,
    item_lru: VecDeque<ShellItemId>,
}

impl FolderSizeColumnVisuals {
    pub fn new(config: VisualColumnConfigV1) -> Self {
        Self {
            config,
            context: None,
            values: HashMap::new(),
            snapshots: HashMap::new(),
            location_key: None,
            directory_cache: HashMap::new(),
            directory_lru: VecDeque::new(),
            item_cache: HashMap::new(),
            item_lru: VecDeque::new(),
        }
    }

    pub fn begin_context(&mut self, context: &explorer_model::RequestContext) -> bool {
        if self.context.as_ref().is_some_and(|current| {
            current.tab_id == context.tab_id && current.generation == context.generation
        }) {
            return false;
        }
        let previous = self.context.clone();
        if let Some(current) = previous.as_ref() {
            self.snapshots.insert(
                FolderSizeSnapshotKeyV1::from(current),
                std::mem::take(&mut self.values),
            );
        }
        self.context = Some(context.clone());
        let key = FolderSizeSnapshotKeyV1::from(context);
        self.values = if previous
            .as_ref()
            .is_some_and(|current| current.tab_id == context.tab_id)
        {
            previous
                .as_ref()
                .and_then(|current| self.snapshots.get(&FolderSizeSnapshotKeyV1::from(current)))
                .cloned()
                .or_else(|| self.snapshots.remove(&key))
                .unwrap_or_default()
        } else {
            self.snapshots.remove(&key).unwrap_or_default()
        };
        true
    }

    pub fn retain_snapshots(&mut self, live: &std::collections::HashSet<FolderSizeSnapshotKeyV1>) {
        self.snapshots.retain(|key, _| live.contains(key));
    }

    pub fn clear_values(&mut self) {
        if let Some(key) = self.location_key.as_ref() {
            self.directory_cache.remove(key);
            self.directory_lru.retain(|cached| cached != key);
        }
        for item_id in self.values.keys() {
            self.item_cache.remove(item_id);
            self.item_lru.retain(|cached| cached != item_id);
        }
        self.values.clear();
        self.snapshots.clear();
        self.context = None;
    }

    pub fn store_current_directory(&mut self) {
        let Some(key) = self.location_key.clone() else {
            return;
        };
        if self.values.is_empty() {
            return;
        }
        self.remember_directory(key, self.values.clone());
    }

    pub fn activate_location(&mut self, location: Option<&LocationDescriptor>) -> bool {
        let new_key = location.map(directory_identity_key);
        if self.location_key.as_ref() == new_key.as_ref() {
            return false;
        }
        let had_location = self.location_key.is_some();
        self.store_current_directory();
        self.location_key = new_key.clone();
        if let Some(key) = new_key {
            if let Some(cached) = self.directory_cache.get(&key).cloned() {
                self.values = cached;
            } else if had_location {
                self.values.clear();
            }
        } else if had_location {
            self.values.clear();
        }
        true
    }

    pub fn hydrate_items(&mut self, item_ids: impl IntoIterator<Item = ShellItemId>) -> bool {
        let mut changed = false;
        for item_id in item_ids {
            if self.values.contains_key(&item_id) {
                continue;
            }
            if let Some(cached) = self.item_cache.get(&item_id).cloned() {
                self.values.insert(item_id, cached);
                changed = true;
            }
        }
        changed
    }

    fn remember_directory(&mut self, key: String, values: HashMap<ShellItemId, FolderSizeValueV1>) {
        if self.directory_cache.contains_key(&key) {
            self.directory_lru.retain(|cached| cached != &key);
        } else {
            while self.directory_lru.len() >= DIRECTORY_VALUE_CACHE_LIMIT {
                if let Some(oldest) = self.directory_lru.pop_front() {
                    self.directory_cache.remove(&oldest);
                } else {
                    break;
                }
            }
        }
        self.directory_lru.push_back(key.clone());
        self.directory_cache.insert(key, values);
    }

    fn remember_item(&mut self, item_id: ShellItemId, value: FolderSizeValueV1) {
        if self.item_cache.contains_key(&item_id) {
            self.item_lru.retain(|cached| cached != &item_id);
        } else {
            while self.item_lru.len() >= ITEM_VALUE_CACHE_LIMIT {
                if let Some(oldest) = self.item_lru.pop_front() {
                    self.item_cache.remove(&oldest);
                } else {
                    break;
                }
            }
        }
        self.item_lru.push_back(item_id.clone());
        self.item_cache.insert(item_id, value);
    }

    pub fn insert_result(&mut self, result: FolderSizeResultV1) -> bool {
        let key = FolderSizeSnapshotKeyV1::from(&result.context);
        let rejected_partial = result.partial;
        let value = FolderSizeValueV1 {
            exact_bytes: (!rejected_partial).then_some(result.exact_bytes).flatten(),
            directory_facts: (!rejected_partial)
                .then_some(result.directory_facts)
                .flatten(),
            partial: false,
            error: result.error.or_else(|| {
                rejected_partial.then(|| {
                    "MFT Service returned an incomplete folder aggregate; exact size is unavailable"
                        .to_owned()
                })
            }),
            retry_after: None,
        };
        self.remember_item(result.item_id.clone(), value.clone());
        if self
            .context
            .as_ref()
            .is_some_and(|current| current.tab_id == result.context.tab_id)
        {
            return self.values.insert(result.item_id, value.clone()) != Some(value);
        }
        let snapshot = self.snapshots.entry(key).or_default();
        snapshot.insert(result.item_id, value.clone()) != Some(value)
    }

    pub fn value_for(&self, item_id: &ShellItemId) -> Option<u64> {
        self.values
            .get(item_id)
            .filter(|value| !value.partial)
            .and_then(|value| value.exact_bytes)
    }

    pub fn directory_facts_for(&self, item_id: &ShellItemId) -> Option<DirectoryFactsV1> {
        self.values
            .get(item_id)
            .filter(|value| !value.partial)
            .and_then(|value| value.directory_facts)
    }

    pub fn file_count_for(&self, item_id: &ShellItemId) -> Option<u64> {
        self.directory_facts_for(item_id)
            .map(|facts| facts.file_count)
    }

    pub fn folder_count_for(&self, item_id: &ShellItemId) -> Option<u64> {
        self.directory_facts_for(item_id)
            .map(|facts| facts.folder_count)
    }

    pub fn exact_file_count_sort_values(&self) -> HashMap<ShellItemId, Option<u64>> {
        self.values
            .iter()
            .map(|(id, value)| {
                (
                    id.clone(),
                    (!value.partial)
                        .then_some(value.directory_facts.map(|facts| facts.file_count))
                        .flatten(),
                )
            })
            .collect()
    }

    pub fn exact_folder_count_sort_values(&self) -> HashMap<ShellItemId, Option<u64>> {
        self.values
            .iter()
            .map(|(id, value)| {
                (
                    id.clone(),
                    (!value.partial)
                        .then_some(value.directory_facts.map(|facts| facts.folder_count))
                        .flatten(),
                )
            })
            .collect()
    }

    pub fn partial_value_for(&self, item_id: &ShellItemId) -> Option<u64> {
        self.values
            .get(item_id)
            .filter(|value| value.partial)
            .and_then(|value| value.exact_bytes)
            .filter(|bytes| *bytes > 0)
    }

    pub fn partial_pending_for(&self, item_id: &ShellItemId) -> bool {
        self.values.get(item_id).is_some_and(|value| {
            value.partial
                && value.error.is_none()
                && value.exact_bytes.is_none_or(|bytes| bytes == 0)
        })
    }

    pub fn take_due_retries(&mut self, now: Instant) -> Vec<(TabId, Generation, ShellItemId)> {
        let Some(context) = self.context.as_ref() else {
            return Vec::new();
        };
        let tab_id = context.tab_id;
        let generation = context.generation;
        self.values
            .iter_mut()
            .filter_map(|(item_id, value)| {
                value
                    .retry_after
                    .is_some_and(|retry_after| retry_after <= now)
                    .then(|| {
                        value.retry_after = None;
                        (tab_id, generation, item_id.clone())
                    })
            })
            .collect()
    }

    pub fn has_value_for_context(
        &self,
        context: &explorer_model::RequestContext,
        item_id: &ShellItemId,
    ) -> bool {
        let key = FolderSizeSnapshotKeyV1::from(context);
        let is_terminal_or_waiting = |value: &FolderSizeValueV1| {
            !value.partial
                || value
                    .retry_after
                    .is_some_and(|retry_after| retry_after > Instant::now())
        };
        if self
            .context
            .as_ref()
            .is_some_and(|current| FolderSizeSnapshotKeyV1::from(current) == key)
        {
            self.values.get(item_id).is_some_and(is_terminal_or_waiting)
        } else {
            self.snapshots
                .get(&key)
                .and_then(|values| values.get(item_id))
                .is_some_and(is_terminal_or_waiting)
        }
    }

    pub fn error_for(&self, item_id: &ShellItemId) -> Option<&str> {
        self.values
            .get(item_id)
            .and_then(|value| value.error.as_deref())
    }

    pub fn exact_sort_values(&self) -> HashMap<ShellItemId, Option<u64>> {
        self.values
            .iter()
            .map(|(id, value)| {
                (
                    id.clone(),
                    (!value.partial).then_some(value.exact_bytes).flatten(),
                )
            })
            .collect()
    }

    pub fn maximum_value(&self) -> u64 {
        self.values
            .values()
            .filter(|value| !value.partial)
            .filter_map(|value| value.exact_bytes)
            .max()
            .unwrap_or_default()
    }
}

pub type VisualColumnRuntimeHandleV1 = Arc<dyn VisualColumnRuntimePortV1>;

/// Returns the one descriptor accepted by this folder-size example slice.
pub fn folder_size_column_descriptor() -> ColumnDescriptor {
    ColumnDescriptor {
        id: ColumnId::Extension {
            package_id: FOLDER_SIZE_COLUMN_PACKAGE_ID.to_owned(),
            column_id: FOLDER_SIZE_COLUMN_ID.to_owned(),
        },
        display_name: "Folder size".to_owned(),
        value_type: ColumnValueType::Bytes,
        default_width: 168,
        minimum_width: 112,
        maximum_width: 360,
        alignment: ColumnAlignment::End,
        applicability: ColumnApplicability::Containers,
        file_systems: explorer_model::ColumnFileSystems::LOCAL,
        sort_semantics: ColumnSortSemantics::Bytes,
        cost: ColumnCost::BackgroundAggregate,
    }
}

pub(crate) fn applies_to_shell_entry(is_container: bool, size_bytes: Option<u64>) -> bool {
    is_container && size_bytes.is_none()
}

pub(crate) fn builtin_size_bytes(
    is_container: bool,
    ordinary_bytes: Option<u64>,
    recursive_bytes: Option<u64>,
) -> Option<u64> {
    if applies_to_shell_entry(is_container, ordinary_bytes) {
        recursive_bytes
    } else {
        ordinary_bytes
    }
}

/// Rejects a runtime configuration that tries to repurpose this UI seam for a
/// different extension identity or non-byte sort domain.
pub fn is_supported_folder_size_descriptor(descriptor: &ColumnDescriptor) -> bool {
    descriptor.id
        == ColumnId::Extension {
            package_id: FOLDER_SIZE_COLUMN_PACKAGE_ID.to_owned(),
            column_id: FOLDER_SIZE_COLUMN_ID.to_owned(),
        }
        && descriptor.value_type == ColumnValueType::Bytes
        && descriptor.sort_semantics == ColumnSortSemantics::Bytes
        && descriptor.validate().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(tab_id: TabId, generation: u64) -> explorer_model::RequestContext {
        explorer_model::RequestContext::new(tab_id, Generation::new(generation))
    }

    fn item(id: u64) -> ShellItemId {
        ShellItemId::from_provider_bytes(id.to_le_bytes()).expect("item identity")
    }

    #[test]
    fn switching_tabs_restores_values_and_same_tab_refresh_keeps_them() {
        let tab_a = TabId::new();
        let tab_b = TabId::new();
        let context_a = context(tab_a, 1);
        let context_b = context(tab_b, 1);
        let mut visuals = FolderSizeColumnVisuals::new(VisualColumnConfigV1::default());

        assert!(visuals.begin_context(&context_a));
        visuals.insert_result(FolderSizeResultV1 {
            context: context_a.clone(),
            item_id: item(1),
            exact_bytes: Some(10),
            directory_facts: None,
            partial: false,
            error: None,
        });
        assert_eq!(visuals.value_for(&item(1)), Some(10));

        assert!(visuals.begin_context(&context_b));
        visuals.insert_result(FolderSizeResultV1 {
            context: context_b.clone(),
            item_id: item(2),
            exact_bytes: Some(20),
            directory_facts: None,
            partial: false,
            error: None,
        });
        assert_eq!(visuals.value_for(&item(2)), Some(20));

        assert!(visuals.begin_context(&context_a));
        assert_eq!(visuals.value_for(&item(1)), Some(10));
        assert!(visuals.begin_context(&context_b));
        assert_eq!(visuals.value_for(&item(2)), Some(20));

        assert!(visuals.begin_context(&context(tab_b, 2)));
        assert_eq!(
            visuals.value_for(&item(2)),
            Some(20),
            "a watcher/F5 generation bump on the same tab must not flash empty aggregates"
        );
    }

    #[test]
    fn returning_to_a_directory_restores_cached_aggregates() {
        let tab = TabId::new();
        let home = LocationDescriptor::file_system(r"C:\Users\Damody");
        let downloads = LocationDescriptor::file_system(r"C:\Users\Damody\Downloads");
        let mut visuals = FolderSizeColumnVisuals::new(VisualColumnConfigV1::default());
        visuals.begin_context(&context(tab, 1));
        visuals.activate_location(Some(&home));
        visuals.insert_result(FolderSizeResultV1 {
            context: context(tab, 1),
            item_id: item(1),
            exact_bytes: Some(24_200_000_000),
            directory_facts: Some(DirectoryFactsV1 {
                mft_generation: 1,
                file_count: 21_888,
                folder_count: 2_143,
            }),
            partial: false,
            error: None,
        });

        visuals.begin_context(&context(tab, 2));
        visuals.activate_location(Some(&downloads));
        assert_eq!(visuals.value_for(&item(1)), None);
        visuals.insert_result(FolderSizeResultV1 {
            context: context(tab, 2),
            item_id: item(9),
            exact_bytes: Some(12),
            directory_facts: None,
            partial: false,
            error: None,
        });

        visuals.begin_context(&context(tab, 3));
        visuals.activate_location(Some(&home));
        assert_eq!(visuals.value_for(&item(1)), Some(24_200_000_000));
        assert_eq!(visuals.file_count_for(&item(1)), Some(21_888));
        assert_eq!(visuals.value_for(&item(9)), None);
    }

    #[test]
    fn drive_root_path_aliases_share_one_cache_entry() {
        assert_eq!(
            directory_identity_key(&LocationDescriptor::file_system(r"D:")),
            directory_identity_key(&LocationDescriptor::file_system(r"D:\"))
        );
        assert_eq!(
            directory_identity_key(&LocationDescriptor::file_system(r"D:\")),
            directory_identity_key(&LocationDescriptor::file_system(r"\\?\D:\"))
        );
        let tab = TabId::new();
        let mut visuals = FolderSizeColumnVisuals::new(VisualColumnConfigV1::default());
        visuals.begin_context(&context(tab, 1));
        visuals.activate_location(Some(&LocationDescriptor::file_system(r"D:\")));
        visuals.insert_result(FolderSizeResultV1 {
            context: context(tab, 1),
            item_id: item(1),
            exact_bytes: Some(42),
            directory_facts: Some(DirectoryFactsV1 {
                mft_generation: 1,
                file_count: 7,
                folder_count: 2,
            }),
            partial: false,
            error: None,
        });
        visuals.begin_context(&context(tab, 2));
        visuals.activate_location(Some(&LocationDescriptor::file_system(r"C:\")));
        assert_eq!(visuals.value_for(&item(1)), None);
        visuals.begin_context(&context(tab, 3));
        visuals.activate_location(Some(&LocationDescriptor::file_system(r"D:")));
        assert_eq!(visuals.value_for(&item(1)), Some(42));
        assert_eq!(visuals.file_count_for(&item(1)), Some(7));
    }

    #[test]
    fn item_cache_restores_values_even_if_the_parent_listing_changed() {
        let tab = TabId::new();
        let mut visuals = FolderSizeColumnVisuals::new(VisualColumnConfigV1::default());
        visuals.begin_context(&context(tab, 1));
        visuals.activate_location(Some(&LocationDescriptor::file_system(r"D:\")));
        visuals.begin_context(&context(tab, 2));
        visuals.activate_location(Some(&LocationDescriptor::file_system(r"C:\")));
        visuals.insert_result(FolderSizeResultV1 {
            context: context(tab, 1),
            item_id: item(3),
            exact_bytes: Some(99),
            directory_facts: Some(DirectoryFactsV1 {
                mft_generation: 1,
                file_count: 4,
                folder_count: 1,
            }),
            partial: false,
            error: None,
        });
        visuals.begin_context(&context(tab, 3));
        visuals.activate_location(Some(&LocationDescriptor::file_system(r"D:\")));
        assert!(visuals.hydrate_items([item(3)]));
        assert_eq!(visuals.value_for(&item(3)), Some(99));
        assert_eq!(visuals.file_count_for(&item(3)), Some(4));
    }

    #[test]
    fn older_generation_result_updates_current_values_on_the_same_tab() {
        let tab = TabId::new();
        let first = context(tab, 1);
        let refreshed = context(tab, 2);
        let id = item(2);
        let mut visuals = FolderSizeColumnVisuals::new(VisualColumnConfigV1::default());
        visuals.begin_context(&first);
        visuals.insert_result(FolderSizeResultV1 {
            context: first,
            item_id: id.clone(),
            exact_bytes: Some(20),
            directory_facts: Some(DirectoryFactsV1 {
                mft_generation: 1,
                file_count: 4,
                folder_count: 1,
            }),
            partial: false,
            error: None,
        });
        assert!(visuals.begin_context(&refreshed));
        assert!(visuals.insert_result(FolderSizeResultV1 {
            context: context(tab, 1),
            item_id: id.clone(),
            exact_bytes: Some(30),
            directory_facts: Some(DirectoryFactsV1 {
                mft_generation: 1,
                file_count: 5,
                folder_count: 1,
            }),
            partial: false,
            error: None,
        }));
        assert_eq!(visuals.value_for(&id), Some(30));
        assert_eq!(visuals.file_count_for(&id), Some(5));
    }

    #[test]
    fn complete_mft_facts_feed_both_count_domains_but_partial_facts_do_not() {
        let current = context(TabId::new(), 1);
        let exact = item(90);
        let partial = item(91);
        let mut visuals = FolderSizeColumnVisuals::new(VisualColumnConfigV1::default());
        visuals.begin_context(&current);
        visuals.insert_result(FolderSizeResultV1 {
            context: current.clone(),
            item_id: exact.clone(),
            exact_bytes: Some(10),
            directory_facts: Some(DirectoryFactsV1 {
                mft_generation: 4,
                file_count: 999,
                folder_count: 2,
            }),
            partial: false,
            error: None,
        });
        visuals.insert_result(FolderSizeResultV1 {
            context: current,
            item_id: partial.clone(),
            exact_bytes: Some(5),
            directory_facts: Some(DirectoryFactsV1 {
                mft_generation: 4,
                file_count: 3,
                folder_count: 1,
            }),
            partial: true,
            error: None,
        });
        assert_eq!(visuals.file_count_for(&exact), Some(999));
        assert_eq!(visuals.folder_count_for(&exact), Some(2));
        assert_eq!(visuals.file_count_for(&partial), None);
        assert_eq!(visuals.exact_file_count_sort_values()[&partial], None);
    }

    #[test]
    fn partial_value_becomes_unavailable_and_is_excluded_from_sort_domain() {
        let current = context(TabId::new(), 1);
        let id = item(77);
        let mut visuals = FolderSizeColumnVisuals::new(VisualColumnConfigV1::default());
        visuals.begin_context(&current);
        visuals.insert_result(FolderSizeResultV1 {
            context: current,
            item_id: id.clone(),
            exact_bytes: Some(1_250),
            directory_facts: None,
            partial: true,
            error: None,
        });
        assert_eq!(visuals.partial_value_for(&id), None);
        assert_eq!(visuals.value_for(&id), None);
        assert_eq!(visuals.exact_sort_values().get(&id), Some(&None));
        assert!(visuals.error_for(&id).unwrap().contains("incomplete"));
    }

    #[test]
    fn zero_partial_is_terminal_unavailable_without_automatic_retry() {
        let current = context(TabId::new(), 1);
        let id = item(78);
        let mut visuals = FolderSizeColumnVisuals::new(VisualColumnConfigV1::default());
        visuals.begin_context(&current);
        visuals.insert_result(FolderSizeResultV1 {
            context: current.clone(),
            item_id: id.clone(),
            exact_bytes: Some(0),
            directory_facts: None,
            partial: true,
            error: None,
        });

        assert!(!visuals.partial_pending_for(&id));
        assert_eq!(visuals.partial_value_for(&id), None);
        assert_eq!(visuals.exact_sort_values().get(&id), Some(&None));
        assert!(visuals.take_due_retries(Instant::now()).is_empty());
        assert!(
            visuals
                .take_due_retries(Instant::now() + std::time::Duration::from_secs(1))
                .is_empty()
        );
        assert!(visuals.has_value_for_context(&current, &item(78)));
    }

    #[test]
    fn descriptor_applies_only_to_containers() {
        assert_eq!(
            folder_size_column_descriptor().applicability,
            ColumnApplicability::Containers
        );
    }

    #[test]
    fn shell_archives_are_not_treated_as_file_system_folders() {
        assert!(applies_to_shell_entry(true, None));
        assert!(!applies_to_shell_entry(true, Some(5_100_000_000)));
        assert!(!applies_to_shell_entry(false, Some(42)));
    }

    #[test]
    fn builtin_size_uses_recursive_folders_and_ordinary_files() {
        assert_eq!(builtin_size_bytes(true, None, Some(1_250)), Some(1_250));
        assert_eq!(builtin_size_bytes(true, None, None), None);
        assert_eq!(builtin_size_bytes(false, Some(42), Some(99)), Some(42));
        assert_eq!(builtin_size_bytes(true, Some(5_100), Some(99)), Some(5_100));
    }

    #[test]
    fn backend_status_labels_include_active_work() {
        assert_eq!(
            FolderSizeBackendStatusV1::MftService.label(true),
            Some("Folder size: MFT service...")
        );
        assert_eq!(
            FolderSizeBackendStatusV1::MftUnavailable.label(true),
            Some("Folder size: MFT unavailable")
        );
        assert_eq!(FolderSizeBackendStatusV1::Idle.label(true), None);
    }
}
