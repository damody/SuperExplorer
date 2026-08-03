//! Narrow UI boundary for the first runtime-provided Details column.
//!
//! The application owns the asynchronous folder walk.  This module owns only
//! the copied descriptor/value projection consumed by GPUI.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

pub use explorer_extension_ui_api::{CellRenderContextV1, CellRenderPlanV1};

use explorer_model::{
    ColumnAlignment, ColumnApplicability, ColumnCost, ColumnDescriptor, ColumnId,
    ColumnSortSemantics, ColumnValueType, ShellItemId,
};

/// Stable identity of the one P0 consumer-provided Details column.
pub const FOLDER_SIZE_COLUMN_PACKAGE_ID: &str = "p0-consumer";
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
    fn render_cell(&self, context: CellRenderContextV1) -> CellRenderPlanV1;
}

/// Render-time, host-owned snapshot. It has no worker handles or callbacks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderSizeColumnVisuals {
    pub config: VisualColumnConfigV1,
    pub values: HashMap<ShellItemId, FolderSizeValueV1>,
}

impl FolderSizeColumnVisuals {
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

/// Returns the one descriptor accepted by this P0 consumer slice.
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
