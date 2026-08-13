//! Narrow UI boundary for the first runtime-provided Details column.
//!
//! The application owns the asynchronous folder walk.  This module owns only
//! the copied descriptor/value projection consumed by GPUI.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const PARTIAL_RETRY_INTERVAL: Duration = Duration::from_secs(1);

pub use explorer_extension_ui_api::{CellRenderContextV1, CellRenderPlanV1};

use explorer_model::{
    ColumnAlignment, ColumnApplicability, ColumnCost, ColumnDescriptor, ColumnId,
    ColumnSortSemantics, ColumnValueType, Generation, ShellItemId, TabId,
};

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
    /// Values are valid only for this tab generation.  Shell item IDs may be
    /// stable across F5, so the generation must be tracked independently.
    pub context: Option<explorer_model::RequestContext>,
    pub values: HashMap<ShellItemId, FolderSizeValueV1>,
    snapshots: HashMap<FolderSizeSnapshotKeyV1, HashMap<ShellItemId, FolderSizeValueV1>>,
}

impl FolderSizeColumnVisuals {
    pub fn new(config: VisualColumnConfigV1) -> Self {
        Self {
            config,
            context: None,
            values: HashMap::new(),
            snapshots: HashMap::new(),
        }
    }

    pub fn begin_context(&mut self, context: &explorer_model::RequestContext) -> bool {
        if self.context.as_ref().is_some_and(|current| {
            current.tab_id == context.tab_id && current.generation == context.generation
        }) {
            return false;
        }
        if let Some(current) = self.context.as_ref() {
            self.snapshots.insert(
                FolderSizeSnapshotKeyV1::from(current),
                std::mem::take(&mut self.values),
            );
        }
        self.context = Some(context.clone());
        self.values = self
            .snapshots
            .remove(&FolderSizeSnapshotKeyV1::from(context))
            .unwrap_or_default();
        true
    }

    pub fn retain_snapshots(&mut self, live: &std::collections::HashSet<FolderSizeSnapshotKeyV1>) {
        self.snapshots.retain(|key, _| live.contains(key));
    }

    pub fn insert_result(&mut self, result: FolderSizeResultV1) -> bool {
        let key = FolderSizeSnapshotKeyV1::from(&result.context);
        let value = FolderSizeValueV1 {
            exact_bytes: result.exact_bytes,
            directory_facts: (!result.partial)
                .then_some(result.directory_facts)
                .flatten(),
            partial: result.partial,
            error: result.error,
            retry_after: result
                .partial
                .then(|| Instant::now() + PARTIAL_RETRY_INTERVAL),
        };
        if self
            .context
            .as_ref()
            .is_some_and(|current| FolderSizeSnapshotKeyV1::from(current) == key)
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
    fn switching_tabs_restores_values_and_generation_change_starts_empty() {
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
        assert!(visuals.values.is_empty());
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
    fn partial_value_is_displayable_but_excluded_from_exact_sort_domain() {
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
        assert_eq!(visuals.partial_value_for(&id), Some(1_250));
        assert_eq!(visuals.value_for(&id), None);
        assert_eq!(visuals.exact_sort_values().get(&id), Some(&None));
    }

    #[test]
    fn zero_partial_is_pending_and_becomes_retryable_without_entering_sort_domain() {
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

        assert!(visuals.partial_pending_for(&id));
        assert_eq!(visuals.partial_value_for(&id), None);
        assert_eq!(visuals.exact_sort_values().get(&id), Some(&None));
        assert!(visuals.take_due_retries(Instant::now()).is_empty());
        assert_eq!(
            visuals
                .take_due_retries(Instant::now() + PARTIAL_RETRY_INTERVAL)
                .as_slice(),
            &[(current.tab_id, current.generation, id)]
        );
        assert!(!visuals.has_value_for_context(&current, &item(78)));
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
