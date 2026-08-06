## Why

Maximum-zoom thumbnails are currently constrained to the centered square Shell-icon host, leaving unnecessary whitespace between a valid thumbnail and the selected item border. The visual loading work now needs to distinguish thumbnail presentation geometry from Shell-icon geometry so real content can use the available cell region without cropping.

## What Changes

- Preserve thumbnail-versus-Shell provenance in the file-view visual snapshot.
- Fit real thumbnails into the complete realized cell width above the independent file-name area without retaining a padding-sized horizontal gutter: landscape sources reach the horizontal edges and portrait sources approach the vertical edges while preserving aspect ratio.
- Keep folders, file-type Shell icons, failed-thumbnail fallbacks, and generic icons centered in the existing square icon host.
- Add deterministic geometry/provenance tests and extend maximum-icon UTIT coverage to reject inset real thumbnails without stretching Shell icons.

## Capabilities

### New Capabilities

- `thumbnail-edge-fit-presentation`: Aspect-preserving cell-edge geometry for real thumbnails while retaining bounded Shell-icon presentation.

### Modified Capabilities

None.

## Impact

- Affected UI code: `crates/explorer-ui/src/lib.rs` visual cache/snapshot data and `crates/explorer-ui/src/chrome.rs` file-view geometry/rendering.
- Affected verification: focused `explorer-ui` tests, `scripts/smoke_icon_view_visual_loading.ps1`, and `uitest/manifest.json` only if a distinct registered case is required.
- No public API, storage format, cache-size policy, extraction policy, row height, file-name layout, or dependency changes.
