## 1. DPI Coordinate Normalization

- [x] 1.1 Normalize native captured scrollbar X/Y coordinates from physical to logical pixels at the `ExplorerRoot` boundary.
- [x] 1.2 Preserve the GPUI logical-coordinate fallback and native capture terminal behavior.
- [x] 1.3 Add 100%, 125%, 150%, 175%, and 200% unit coverage for vertical and horizontal one-to-one drag geometry.

## 2. Quantitative Headful Coverage

- [x] 2.1 Extend the 240-file scrollbar fixture to record DPI, physical pointer delta, scrollbar geometry, expected range movement, and observed movement.
- [x] 2.2 Enforce drag-ratio tolerance for file-view vertical, navigation vertical, and Details horizontal scrollbars.
- [x] 2.3 Preserve and run outside-window capture, release, horizontal-overflow, and hidden-scrollbar assertions.

## 3. Verification

- [x] 3.1 Run format, clippy, focused unit tests, and UITEST manifest validation.
- [x] 3.2 Run the interactive `scrollbar-headful` UITEST and retain its report and screenshot evidence.
- [x] 3.3 Validate the OpenSpec change strictly and confirm every task is complete.
