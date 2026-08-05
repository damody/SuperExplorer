#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! Public GPUI-facing extension API boundary.
//!
//! This crate re-exports the public, data-only visual-column author contract.
//! It must not depend on the private `explorer-ui` implementation or on the
//! extension host.

pub use explorer_extension_api::{
    CellAggregateV1, CellColorV1, CellRenderContextV1, CellRenderPlanV1, CellThemeV1,
    EXTENSION_ID_NAMESPACE_V1, FolderSizeMeasureRequestV1, FolderSizeMeasureResultV1,
    NavigationRequestV1, PluginValueV1, SizeMapNodeKindV1, SizeMapNodeStatusV1, SizeMapNodeV1,
    SizeMapRectangleV1, SizeMapRenderContextV1, SizeMapRenderPlanV1, SizeMapViewImplementationV1,
    SizeMapViewObjectV1, SizeMapViewportV1, StableIdV1, ViewNavigationOperationV1,
    ViewSelectionOperationV1, ViewSelectionRequestV1, ViewSnapshotIdentityV1,
    VisualColumnImplementationV1, VisualColumnObjectV1,
};
