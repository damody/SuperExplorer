//! Narrow host/UI boundary for the one Size Map example.
//!
//! The application owns filesystem measurement and adapts the public plugin
//! ABI. GPUI receives only copied node state and a data-only rectangle plan.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use explorer_model::{RequestContext, ShellItemId};

/// Stable identity of the first extension-provided view.
pub const FOLDER_SIZE_MAP_VIEW_ID: &str = "rust-folder-size-map-view:folder-size-map";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SizeMapViewConfigV1 {
    pub view_id: String,
    pub display_name: String,
}

impl Default for SizeMapViewConfigV1 {
    fn default() -> Self {
        Self {
            view_id: FOLDER_SIZE_MAP_VIEW_ID.to_owned(),
            display_name: "Size Map".to_owned(),
        }
    }
}

/// App-owned request for one direct child folder. Files retain their Shell
/// metadata size; directory totals are measured outside GPUI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SizeMapMeasureRequestV1 {
    pub context: RequestContext,
    pub item_id: ShellItemId,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SizeMapMeasureResultV1 {
    pub context: RequestContext,
    pub item_id: ShellItemId,
    pub exact_bytes: Option<u64>,
    pub partial: bool,
    pub error: Option<String>,
}

/// Public-data-shaped node snapshot passed through the app adapter to the
/// renderer. It deliberately contains no path, Shell interface or GPUI type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SizeMapNodeV1 {
    pub item_id: ShellItemId,
    pub display_name: String,
    pub type_name: String,
    pub is_container: bool,
    pub exact_bytes: Option<u64>,
    pub partial: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeMapViewportV1 {
    pub width: f32,
    pub height: f32,
    pub dpi_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SizeMapRenderContextV1 {
    pub request_context: RequestContext,
    pub nodes: Vec<SizeMapNodeV1>,
    pub selected: Vec<ShellItemId>,
    pub viewport_width_milli: u32,
    pub viewport_height_milli: u32,
    pub dark_theme: bool,
}

/// Coordinates are logical pixels within the supplied viewport. GPUI owns
/// painting and hit testing; the plugin owns only this immutable layout data.
#[derive(Clone, Debug, PartialEq)]
pub struct SizeMapRectangleV1 {
    pub item_id: ShellItemId,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub label: String,
    pub detail: String,
    pub color: crate::theme::Rgba8,
    pub status: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SizeMapRenderPlanV1 {
    pub rectangles: Vec<SizeMapRectangleV1>,
    pub status: Option<String>,
    /// False means the host must render the normal Details fallback instead
    /// of covering it with an empty extension surface.
    pub available: bool,
}

/// Application-owned bridge for this single extension view. The application
/// schedules measurement, filters stale generations, and invokes the public
/// ABI renderer; the UI only presents owned snapshots and routes actions.
pub trait ExtensionSizeMapRuntimePortV1: Send + Sync {
    fn config(&self) -> SizeMapViewConfigV1;
    fn submit_measure_requests(&self, requests: Vec<SizeMapMeasureRequestV1>);
    fn cancel_measure_context(&self, context: &RequestContext);
    fn drain_measure_results(&self) -> Vec<SizeMapMeasureResultV1>;
    /// Moves completed asynchronous render plans into the host cache. Returns
    /// true only when GPUI needs another frame to consume a newly-ready plan.
    fn drain_render_results(&self) -> bool {
        false
    }
    fn render_size_map(&self, context: SizeMapRenderContextV1) -> SizeMapRenderPlanV1;
}

pub type SizeMapRuntimeHandleV1 = Arc<dyn ExtensionSizeMapRuntimePortV1>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SizeMapVisualsV1 {
    pub values: HashMap<ShellItemId, SizeMapMeasuredValueV1>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SizeMapMeasuredValueV1 {
    pub exact_bytes: Option<u64>,
    pub partial: bool,
    pub error: Option<String>,
}

impl SizeMapVisualsV1 {
    pub fn node_for(&self, entry: &explorer_model::FileEntry) -> SizeMapNodeV1 {
        let measured = self.values.get(&entry.id);
        let exact_bytes = if entry.is_container {
            measured.and_then(|value| (!value.partial).then_some(value.exact_bytes).flatten())
        } else {
            entry.metadata.size_bytes
        };
        SizeMapNodeV1 {
            item_id: entry.id.clone(),
            display_name: entry.display_name.clone(),
            type_name: entry
                .metadata
                .type_display
                .clone()
                .unwrap_or_else(|| if entry.is_container { "Folder" } else { "File" }.to_owned()),
            is_container: entry.is_container,
            exact_bytes,
            partial: measured.is_some_and(|value| value.partial),
            error: measured.and_then(|value| value.error.clone()),
        }
    }
}

pub fn is_supported_size_map_config(config: &SizeMapViewConfigV1) -> bool {
    config.view_id == FOLDER_SIZE_MAP_VIEW_ID && !config.display_name.trim().is_empty()
}
