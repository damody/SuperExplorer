## Why

Native scrollbar capture reports physical pixels while GPUI scrollbar geometry uses logical pixels,
causing thumb dragging to be multiplied by display scaling (about 1.75× at 175% DPI). The existing
headful test checks capture continuity but does not detect an incorrect drag ratio.

## What Changes

- Normalize native captured pointer coordinates to GPUI logical pixels exactly once.
- Apply the corrected coordinate path to file-view vertical, navigation vertical, and Details
  horizontal scrollbars while preserving native capture and outside-window dragging.
- Add DPI-matrix unit coverage for one-to-one scrollbar movement.
- Extend the UITEST scrollbar case with quantitative drag-ratio assertions on a 240-file folder.

## Capabilities

### New Capabilities

- `dpi-correct-scrollbar-dragging`: Defines one-to-one scrollbar thumb dragging across supported DPI
  scales and quantitative multi-file headful regression coverage.

### Modified Capabilities

None. This repository has no baseline capability specs to modify.

## Impact

- Affects native pointer capture consumption and scrollbar drag dispatch in `explorer-ui`.
- Extends `scripts/smoke_scrollbar_capture.ps1` and the existing `scrollbar-headful` UITEST evidence.
- Does not change external APIs, scrollbar appearance, overlays, or scroll virtualization.
