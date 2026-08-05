//! Narrow host/UI boundary for the one Size Map example.
//!
//! The application owns filesystem measurement and adapts the public plugin
//! ABI. GPUI receives only copied node state and a data-only rectangle plan.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use explorer_extension_ui_api::{
    NavigationRequestV1, StableIdV1, ViewNavigationOperationV1, ViewSelectionOperationV1,
    ViewSelectionRequestV1, ViewSnapshotIdentityV1,
};
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
    /// Bounded owned recursive nodes discovered below this direct child.
    pub tree_nodes: Vec<SizeMapTreeNodeV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SizeMapTreeNodeV1 {
    pub item_id: ShellItemId,
    pub root_item_id: ShellItemId,
    pub parent_item_id: ShellItemId,
    pub location: explorer_model::LocationDescriptor,
    pub display_name: String,
    pub type_name: String,
    pub is_container: bool,
    pub exact_bytes: Option<u64>,
    pub partial: bool,
    pub error: Option<String>,
}

/// Public-data-shaped node snapshot passed through the app adapter to the
/// renderer. It deliberately contains no path, Shell interface or GPUI type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SizeMapNodeV1 {
    pub item_id: ShellItemId,
    pub selection_item_id: ShellItemId,
    pub parent_item_id: Option<ShellItemId>,
    pub location: explorer_model::LocationDescriptor,
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
    /// Opaque public node identity used only by the host interaction bridge.
    pub node_id: Option<StableIdV1>,
    /// `None` is a host-owned aggregate such as `Other`; it is accessible but
    /// deliberately has no selection/open authority.
    pub item_id: Option<ShellItemId>,
    pub interaction_target: Option<SizeMapInteractionTargetV1>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub label: String,
    pub detail: String,
    pub color: crate::theme::Rgba8,
    pub status: String,
    /// Real items represented by a synthetic aggregate rectangle. They remain
    /// searchable and keyboard/UIA reachable without receiving a visible
    /// rectangle or granting the aggregate itself navigation authority.
    pub aggregate_items: Vec<SizeMapAggregateItemV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SizeMapInteractionTargetV1 {
    pub item_id: ShellItemId,
    pub selection_item_id: ShellItemId,
    pub location: explorer_model::LocationDescriptor,
    pub is_container: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SizeMapAggregateItemV1 {
    pub item_id: ShellItemId,
    pub label: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SizeMapRenderPlanV1 {
    pub snapshot: Option<ViewSnapshotIdentityV1>,
    pub rectangles: Vec<SizeMapRectangleV1>,
    pub status: Option<String>,
    /// False means the host must render the normal Details fallback instead
    /// of covering it with an empty extension surface.
    pub available: bool,
}

/// Host-owned bridge from public opaque node requests to current row indexes.
/// It contains no callback and is never sent across the plugin ABI.
#[derive(Clone, Debug)]
pub struct ViewSelectionBridgeV1 {
    snapshot: ViewSnapshotIdentityV1,
    rows: HashMap<StableIdV1, usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorizedViewNavigationV1 {
    pub row_index: usize,
    pub new_tab: bool,
    pub reveal_only: bool,
}

impl ViewSelectionBridgeV1 {
    #[must_use]
    pub fn new(snapshot: ViewSnapshotIdentityV1, rows: HashMap<StableIdV1, usize>) -> Self {
        Self { snapshot, rows }
    }

    #[must_use]
    pub const fn snapshot(&self) -> ViewSnapshotIdentityV1 {
        self.snapshot
    }

    #[must_use]
    pub fn authorize_selection(&self, request: &ViewSelectionRequestV1) -> Option<usize> {
        let known = self.rows.keys().copied().collect();
        if request.operation != ViewSelectionOperationV1::REPLACE
            || request.node_ids.len() != 1
            || !request.validate_for_snapshot(self.snapshot, &known)
        {
            return None;
        }
        self.rows.get(&request.node_ids[0]).copied()
    }

    #[must_use]
    pub fn authorize_navigation(
        &self,
        request: &NavigationRequestV1,
    ) -> Option<AuthorizedViewNavigationV1> {
        let known = self.rows.keys().copied().collect();
        if !request.is_authorized_for(self.snapshot, &known) {
            return None;
        }
        Some(AuthorizedViewNavigationV1 {
            row_index: *self.rows.get(&request.node_id)?,
            new_tab: request.operation == ViewNavigationOperationV1::OPEN_NEW_TAB,
            reveal_only: request.operation == ViewNavigationOperationV1::REVEAL,
        })
    }
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
    pub tree_nodes: HashMap<ShellItemId, SizeMapTreeNodeV1>,
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
            selection_item_id: entry.id.clone(),
            parent_item_id: None,
            location: entry.location.clone(),
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

    pub fn recursive_nodes_for(&self, entries: &[explorer_model::FileEntry]) -> Vec<SizeMapNodeV1> {
        let mut nodes = entries
            .iter()
            .map(|entry| self.node_for(entry))
            .collect::<Vec<_>>();
        nodes.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        let mut emitted = nodes
            .iter()
            .map(|node| node.item_id.clone())
            .collect::<std::collections::HashSet<_>>();
        let mut remaining = self.tree_nodes.values().cloned().collect::<Vec<_>>();
        remaining.sort_by(|left, right| {
            left.display_name.cmp(&right.display_name).then_with(|| {
                left.item_id
                    .provider_bytes()
                    .cmp(right.item_id.provider_bytes())
            })
        });
        while !remaining.is_empty() {
            let before = remaining.len();
            let mut deferred = Vec::new();
            for node in remaining {
                if !emitted.contains(&node.parent_item_id) {
                    deferred.push(node);
                    continue;
                }
                emitted.insert(node.item_id.clone());
                nodes.push(SizeMapNodeV1 {
                    item_id: node.item_id,
                    selection_item_id: node.root_item_id,
                    parent_item_id: Some(node.parent_item_id),
                    location: node.location,
                    display_name: node.display_name,
                    type_name: node.type_name,
                    is_container: node.is_container,
                    exact_bytes: node.exact_bytes,
                    partial: node.partial,
                    error: node.error,
                });
            }
            if deferred.len() == before {
                break;
            }
            remaining = deferred;
        }
        nodes
    }
}

pub fn is_supported_size_map_config(config: &SizeMapViewConfigV1) -> bool {
    config.view_id == FOLDER_SIZE_MAP_VIEW_ID && !config.display_name.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_extension_ui_api::{EXTENSION_ID_NAMESPACE_V1, ViewNavigationOperationV1};

    #[test]
    fn interaction_bridge_authorizes_current_opaque_nodes_only() {
        let snapshot = ViewSnapshotIdentityV1 {
            location_generation: 2,
            refresh_generation: 3,
            render_revision: 5,
        };
        let node_id = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 8);
        let bridge = ViewSelectionBridgeV1::new(snapshot, HashMap::from([(node_id, 7)]));
        let selection = ViewSelectionRequestV1 {
            snapshot,
            operation: ViewSelectionOperationV1::REPLACE,
            node_ids: vec![node_id].into(),
        };
        assert_eq!(bridge.authorize_selection(&selection), Some(7));
        let navigation = NavigationRequestV1 {
            snapshot,
            operation: ViewNavigationOperationV1::OPEN_NEW_TAB,
            node_id,
        };
        assert_eq!(
            bridge.authorize_navigation(&navigation),
            Some(AuthorizedViewNavigationV1 {
                row_index: 7,
                new_tab: true,
                reveal_only: false,
            })
        );
        let stale = ViewSnapshotIdentityV1 {
            refresh_generation: 4,
            ..snapshot
        };
        assert_eq!(
            bridge.authorize_navigation(&NavigationRequestV1 {
                snapshot: stale,
                ..navigation
            }),
            None
        );
    }
}
