//! UI boundary for the public-SDK Rust tokei Code lines example.
//!
//! The application owns bounded file I/O and plugin dispatch. This module owns
//! only copied requests/results and the host-side Details-column projection.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

pub use explorer_extension_ui_api::{CellRenderContextV1, CellRenderPlanV1};
use explorer_model::{
    ColumnAlignment, ColumnApplicability, ColumnCost, ColumnDescriptor, ColumnId,
    ColumnSortSemantics, ColumnValueType, RequestContext, ShellItemId,
};

pub const CODE_LINES_COLUMN_PACKAGE_ID: &str = "rust-tokei";
pub const CODE_LINES_COLUMN_ID: &str = "code-lines";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CodeLinesDisplayMode {
    #[default]
    CodeOnly,
    WithCommentAndBlank,
}

impl CodeLinesDisplayMode {
    pub const fn shows_detail(self) -> bool {
        matches!(self, Self::WithCommentAndBlank)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLinesColumnConfigV1 {
    pub descriptor: ColumnDescriptor,
    pub display: CodeLinesDisplayMode,
}

impl Default for CodeLinesColumnConfigV1 {
    fn default() -> Self {
        Self {
            descriptor: code_lines_column_descriptor(),
            display: CodeLinesDisplayMode::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLinesRequestV1 {
    pub context: RequestContext,
    pub item_id: ShellItemId,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLinesValueV1 {
    pub language: String,
    pub code: u64,
    pub comments: u64,
    pub blanks: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLinesResultV1 {
    pub context: RequestContext,
    pub item_id: ShellItemId,
    pub value: Option<CodeLinesValueV1>,
    pub error: Option<String>,
}

pub trait CodeLinesRuntimePortV1: Send + Sync {
    fn config(&self) -> CodeLinesColumnConfigV1;
    fn submit_code_lines_requests(&self, requests: Vec<CodeLinesRequestV1>);
    fn drain_code_lines_results(&self) -> Vec<CodeLinesResultV1>;
    /// Moves completed asynchronous render plans into the host cache. Returns
    /// true only when GPUI needs another frame to consume a newly-ready plan.
    fn drain_render_results(&self) -> bool {
        false
    }
    fn render_cell(&self, context: CellRenderContextV1) -> CellRenderPlanV1;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLinesColumnVisuals {
    pub config: CodeLinesColumnConfigV1,
    pub values: HashMap<ShellItemId, CodeLinesValueV1>,
    pub errors: HashMap<ShellItemId, String>,
}

impl CodeLinesColumnVisuals {
    pub fn exact_sort_values(&self) -> HashMap<ShellItemId, Option<u64>> {
        self.values
            .iter()
            .map(|(id, value)| (id.clone(), Some(value.code)))
            .collect()
    }

    pub fn maximum_value(&self) -> u64 {
        self.values
            .values()
            .map(|value| value.code)
            .max()
            .unwrap_or(0)
    }
}

pub type CodeLinesRuntimeHandleV1 = Arc<dyn CodeLinesRuntimePortV1>;

pub fn code_lines_column_descriptor() -> ColumnDescriptor {
    ColumnDescriptor {
        id: ColumnId::Extension {
            package_id: CODE_LINES_COLUMN_PACKAGE_ID.to_owned(),
            column_id: CODE_LINES_COLUMN_ID.to_owned(),
        },
        display_name: "Code lines".to_owned(),
        value_type: ColumnValueType::Integer,
        default_width: 168,
        minimum_width: 104,
        maximum_width: 360,
        alignment: ColumnAlignment::End,
        applicability: ColumnApplicability::Files,
        sort_semantics: ColumnSortSemantics::Integer,
        cost: ColumnCost::BackgroundBatch,
    }
}

pub fn is_supported_code_lines_descriptor(descriptor: &ColumnDescriptor) -> bool {
    descriptor.id == code_lines_column_descriptor().id
        && descriptor.value_type == ColumnValueType::Integer
        && descriptor.sort_semantics == ColumnSortSemantics::Integer
        && descriptor.validate().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_uses_exact_integer_background_batch_semantics() {
        let descriptor = code_lines_column_descriptor();
        assert!(is_supported_code_lines_descriptor(&descriptor));
        assert_eq!(descriptor.cost, ColumnCost::BackgroundBatch);
    }
}
