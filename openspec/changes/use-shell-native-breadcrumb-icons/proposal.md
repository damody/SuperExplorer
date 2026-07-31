## Why

The browsing address bar briefly or permanently displays application-drawn folder artwork while Windows Shell icons are loading or unavailable. Breadcrumbs should match File Explorer by using Shell-provided icons for both concrete locations and the generic fallback from the first initialization request onward.

## What Changes

- Preload a generic folder icon from Windows Shell during UI initialization through the existing asynchronous icon service.
- Continue loading the exact Shell icon for every This PC, drive, folder, archive, and namespace breadcrumb location.
- Render the generic Shell folder icon until a location-specific icon arrives, then replace it in place.
- Remove application-drawn breadcrumb icon fallbacks; reserve the icon slot if neither Shell texture is available on the first frame.
- Reuse bounded memory and disk caches with DPI, theme, association-generation, and request-context correctness.
- Add unit, structural, Shell-path, and headful UTIT coverage for initialization, fallback, replacement, failure, and non-blocking behavior.
- Apply the same Shell-only contract to the navigation pane: preserve drive textures across cache epoch replacement and render ordinary folders with the Windows generic folder texture instead of application-drawn blocks.

## Capabilities

### New Capabilities

- `shell-native-breadcrumb-icons`: Defines Shell-native location icons and a Shell-provided generic fallback for every browsing address-bar breadcrumb surface.

### Modified Capabilities

None.

## Impact

- `explorer-ui` startup icon scheduling, cache snapshots, breadcrumb rendering, and navigation-pane icon resolution.
- `explorer-shell-win` generic directory icon acquisition through the existing `SHGetFileInfoW` path.
- Breadcrumb unit, structural, and headful UTIT cases and manifest evidence.
- No public API, persistence schema, dependency, or synchronous UI-thread Shell-call changes.
