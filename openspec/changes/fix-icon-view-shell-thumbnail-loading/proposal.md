## Why

Extra-large, large, medium, small-icon, and tile views can remain on generic fallback blocks instead of showing the Windows Shell icon or an image/video thumbnail. Mode or DPI changes can also leave obsolete requests occupying the visible-item budget, so returning to a folder produces incorrect visuals and unnecessary reloads.

## What Changes

- Define Explorer-compatible icon and thumbnail behavior for all icon-oriented view modes.
- Give Shell icon fallback and content-thumbnail work independent, bounded scheduling capacity.
- Key and admit results by the current tab, folder generation, view size, DPI/theme, association generation, and overlay generation so obsolete work cannot overwrite the active view.
- Reuse compatible completed cache entries when switching folders or returning to a previous view size.
- Prevent maximum-size cache churn with DPI/size-aware prefetch and a user-configurable 128 MiB default cache limit up to 1 GiB.
- When an exact maximum-size folder icon is unavailable, enlarge the largest compatible real or shared Windows Shell folder icon instead of falling back to the fixed generic yellow glyph.
- Add Rust regression tests and headful UTIT coverage for mode switching, scrolling newly realized rows, and image/video thumbnail replacement.

## Capabilities

### New Capabilities

- `icon-view-visual-loading`: Specifies Shell icon fallback, content-thumbnail preference, scheduling, cache reuse, stale-result rejection, and visible-view verification for icon-oriented views.

### Modified Capabilities

None.

## Impact

- `crates/explorer-model`: view-mode thumbnail policy, backward-compatible persisted cache setting, and unit tests.
- `crates/explorer-ui`: visible-item icon/thumbnail scheduling, compatible-size folder fallback selection, request identity, cache admission, Folder Options cache presets, and regression tests.
- `uitest`: a Windows headful scenario and manifest entry covering the five affected view modes.
- No public extension ABI change; the session payload adds a serde-defaulted field that remains backward compatible.
