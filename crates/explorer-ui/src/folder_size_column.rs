//! Narrow UI boundary for the first runtime-provided Details column.
//!
//! The application owns the asynchronous folder walk.  This module owns only
//! the copied descriptor/value projection consumed by GPUI.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

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
}

/// One exact-byte folder-size result returned by the app-owned worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderSizeResultV1 {
    pub context: explorer_model::RequestContext,
    pub item_id: ShellItemId,
    /// `None` means that the provider completed without a displayable value.
    pub exact_bytes: Option<u64>,
    pub partial: bool,
    pub error: Option<String>,
}

/// Typed cell state. Partial totals remain displayable diagnostics but never
/// enter the exact-byte sort domain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderSizeValueV1 {
    pub exact_bytes: Option<u64>,
    pub partial: bool,
    pub error: Option<String>,
}

/// Application-owned async bridge. Calls occur only on the UI thread; the
/// implementation owns worker scheduling, deduplication, and cancellation.
pub trait VisualColumnRuntimePortV1: Send + Sync {
    fn config(&self) -> VisualColumnConfigV1;
    fn submit_folder_size_requests(&self, requests: Vec<FolderSizeRequestV1>);
    fn drain_folder_size_results(&self) -> Vec<FolderSizeResultV1>;
    /// Moves completed asynchronous render plans into the host cache. Returns
    /// true only when GPUI needs another frame to consume a newly-ready plan.
    fn drain_render_results(&self) -> bool {
        false
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
            partial: result.partial,
            error: result.error,
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
            partial: false,
            error: None,
        });
        assert_eq!(visuals.value_for(&item(1)), Some(10));

        assert!(visuals.begin_context(&context_b));
        visuals.insert_result(FolderSizeResultV1 {
            context: context_b.clone(),
            item_id: item(2),
            exact_bytes: Some(20),
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
}
