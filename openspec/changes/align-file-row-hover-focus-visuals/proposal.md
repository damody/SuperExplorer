## Why

SuperExplorer file rows currently use a neutral gray hover and a white selected row with a thick blue outline, which visibly diverges from the supplied Windows File Explorer references. Matching Explorer requires isolated file-row palette and state styling so unrelated controls and accessibility themes are not recolored.

## What Changes

- Add file-row-specific semantic colors for hover fill, active selection fill, inactive selection fill, and focus outline.
- Calibrate the light-theme hover to `#E5F3FF` and focused selection to `#CCE8FF` with a one-logical-pixel black outline.
- Preserve dark-theme contrast and Windows-owned high-contrast system colors.
- Render selected rows with an explicit fill and suppress hover overlays on selected rows.
- Bound Details-row hover and selection visuals to the trailing edge of the last visible data column, including the shared leading inset used by both the header and rows.
- Add state projection, theme completeness, and render-contract regression tests.
- Do not change selection/focus mechanics, row geometry, typography, icons, or other UI surfaces.

## Capabilities

### New Capabilities

- `file-row-interaction-visuals`: Defines theme-aware hover, active-selection, inactive-selection, and focus-outline presentation for file rows.

### Modified Capabilities

None.

## Impact

- Theme contract: `crates/explorer-ui/src/theme.rs`.
- File-row state-to-style projection and renderer: `crates/explorer-ui/src/chrome.rs`.
- Visual evidence target injection: `scripts/capture_visual_fixture.ps1`.
- No public API, persisted state, extension ABI, dependency, selection-state, or input-routing impact.
