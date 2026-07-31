## Why

The current fallback recursively scans the active search root on every query, which is slow and can keep walking a drive after an accidental root search. Details view also lacks Explorer-style column header commands and formats sizes inconsistently.

## What Changes

- Detect a usable Everything IPC endpoint and automatically use the official Everything SDK for folder-scoped search without starting or configuring Everything.
- Add a persistent SQLite fallback under the application data directory that indexes only folders already enumerated and the scope of an active search.
- Stop fallback traversal and index growth promptly when a search is cleared, replaced, navigated away, its tab is closed, or the app shuts down.
- Add an Explorer-like right-click menu to Details headers for auto sizing, column visibility, and an accessible Other Columns dialog.
- Persist per-tab column visibility, order, and widths with the existing session state.
- Use one adaptive binary-unit formatter for file sizes throughout Explorer surfaces.
- Replace the static navigation pane with an interactive, lazily loaded Explorer-like tree that follows and expands the active path.
- Render This PC as a dedicated Explorer-like devices-and-drives surface with truthful capacity and free-space status.

## Capabilities

### New Capabilities

- `everything-search-provider`: Everything IPC capability detection, safe SDK loading, scoped queries, bounded results, cancellation, and fallback behavior.
- `lazy-local-search-index`: Application-owned SQLite index populated only from viewed folders and active search scopes, with cancellation-safe traversal and recovery.
- `explorer-details-columns`: Header context menu behavior, column visibility/persistence, metadata rendering, auto sizing, and consistent file-size formatting.
- `explorer-navigation-tree`: Expand/collapse, active-path synchronization, lazy child enumeration, keyboard/pointer interaction, and per-tab tree state.
- `explorer-this-pc`: Drive discovery, grouping, capacity bars, free/total space labels, low-space state, and activation behavior.

### Modified Capabilities

None.

## Impact

- Affects `explorer-search`, `explorer-shell-win`, `explorer-model`, `explorer-ui`, application composition, session persistence, build packaging, and UITEST coverage.
- Adds an embedded SQLite dependency and a pinned official Everything SDK runtime/provenance bundle.
- Extends search backend/status types and Details column metadata while retaining the existing command/event boundaries.
