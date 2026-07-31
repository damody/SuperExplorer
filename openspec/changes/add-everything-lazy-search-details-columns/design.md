## Context

The existing Windows search adapter probes Windows Search and then recursively walks the filesystem root for every query. It already has per-tab request generations, cancellation tokens, bounded result events, and stale-result rejection. Details view currently owns four fixed columns and persisted widths, but lacks visibility state and a header context menu. The change must preserve concurrent shared-tree changes, remain responsive on drive roots, avoid UI-thread I/O, and treat third-party SDK availability as optional.

The approved source design is `docs/superpowers/specs/2026-07-29-everything-lazy-search-details-columns-design.md`.

## Goals / Non-Goals

**Goals:**

- Prefer a usable Everything background IPC provider for folder-scoped search.
- Fall back to an application-owned SQLite index populated only by viewed folders and active searches.
- Stop traversal and index growth promptly when the correlated search is cancelled.
- Add Explorer-like Details header commands, persisted column visibility/order/width, and additional metadata columns.
- Format all displayed file sizes through one tested adaptive binary-unit helper.

**Non-Goals:**

- Installing, starting, configuring, or rebuilding Everything.
- Whole-drive background crawling, a permanent all-files watcher, or content indexing.
- Loading arbitrary property handlers on the UI thread.
- Replacing existing search UI, generation, batching, or terminal-event contracts.

## Decisions

### Dynamically load the official Everything SDK from an owned location

The adapter resolves `Everything64.dll` only from the application directory or a verified bundled runtime path, loads the Unicode query/result functions dynamically, and copies every returned value into owned Rust data before returning. A capability probe verifies that IPC and the Everything database are available; a Windows service name alone is not sufficient. Queries execute off the UI thread, include an escaped canonical folder scope, request only required fields, page results, and check cancellation before every emitted batch.

This is preferred over direct WM_COPYDATA because the official wrapper owns message-window and structure-version details, and over spawning `es.exe` because process output and cancellation are less reliable. The application never starts an Everything process. Missing DLL/functions, incompatible architecture, IPC loss, or timeout produces an unavailable status and transfers the same request to the local index.

### Use one application-owned SQLite lazy index

The database lives under `%LOCALAPPDATA%\RustGpuiExplorer\search-index\v1\index.sqlite3`, uses WAL, a schema version, busy timeout, bounded prepared statements, and no file contents. Successful directory enumeration schedules a shallow upsert of only the returned immediate children. A local search first emits matching cached rows within the exact canonical scope, then performs a breadth-first traversal only while that request remains active, upserting and evaluating small batches.

Cancellation is checked before dequeueing a directory, before and during enumeration, before each transaction, and before delivery. Cancelled work stops adding directories; a small committed batch may remain useful, while the current uncommitted transaction rolls back. Reparse points are not followed. Scope SQL uses canonical path plus separator boundaries so sibling prefixes cannot leak into results. Corrupt or incompatible databases are quarantined and rebuilt inside the owned data root.

### Preserve the typed search event boundary

`SearchBackend` gains `Everything` and `LocalIndex`; WindowsIndex stays parse-compatible for persisted diagnostics/tests but is no longer the production first choice. The Shell STA/search worker publishes existing status, batch, and exactly-one terminal events. Existing model generation checks, dedupe, tab isolation, and cancellation remain authoritative. Diagnostics record backend and counts without query text or private full paths.

### Generalize Details columns without UI-thread metadata reads

A stable `DetailsColumn` identity covers Name, DateModified, Type, Size, DateCreated, Authors, Tags, and Title. View settings own order, visibility, and widths; Name is mandatory. Session schema migration maps the old four-column state to the new defaults. Rendering consumes only metadata already owned by `FileEntry`; missing optional properties display blank.

Right-clicking a Details header opens an app-owned mutually exclusive, occluding popup with dedicated focus. It provides current/all-column auto size and one complete list of every checkable column. A column toggle applies immediately to the active tab, so there is no draft, Other Columns expansion, OK, or Cancel flow. Pointer and keyboard behavior reuse the command-menu state machine and cannot hover or activate file rows through the popup.

### Centralize file-size formatting

One pure formatter uses 1024-based KB, MB, GB, and TB thresholds, bounded precision, locale-aware numeric punctuation, and upward rounding for nonzero sub-KB files. Unknown sizes and containers remain blank. Details rows, other file views, preview metadata, and auto-size measurement consume this helper.

### Make the navigation pane a lazy interactive tree

Each tab owns expanded node identities, child-loading state, and a bounded cache of child containers. The stable roots remain immediately available. Expanding a node reuses the existing off-UI-thread child-container command/event boundary; collapsing cancels its pending request. Successful navigation synchronizes the ancestry chain, automatically expands its parents, and selects the exact active node. Clicking a chevron only expands or collapses, while clicking the row navigates. Keyboard Left/Right/Enter/Space mirror Explorer behavior, and loading/error states remain interactive and retryable.

### Give This PC a dedicated drive-status presentation

When the resolved location is This PC, the owned entry metadata is enriched off the UI thread with drive type, total bytes, available bytes, volume label, and availability. The file view groups devices and drives separately from ordinary folders and renders a bounded capacity bar plus adaptive free/total labels. Unknown, removable-without-media, disconnected, and access-failed drives remain visible with truthful unavailable state. Row selection, context menu, keyboard activation, and double-click reuse normal stable item actions.

## Risks / Trade-offs

- [Everything service exists but IPC client is absent] -> Probe the actual SDK IPC endpoint and fall back without launching anything.
- [SDK DLL search-order hijacking] -> Load only canonical owned paths and validate required exports/target architecture.
- [A search starts at `C:\`] -> Traverse only while its request token is active, do not follow reparse points, bound queues/items, and stop expansion immediately on cancellation.
- [SQLite becomes corrupt or locked] -> Use short transactions, busy timeout, integrity/version checks, quarantine, and a recoverable empty rebuild.
- [Cached rows become stale] -> Validate paths while refreshing an active scope and delete missing children when a directory is successfully re-enumerated.
- [More columns increase render cost] -> Store owned optional metadata and virtualize rows; never invoke Shell property handlers during render.
- [Session schema changes reject existing state] -> Add an explicit migration from the four-column representation and retain bounded validation.

## Migration Plan

1. Extend owned column/search model types and session migration while preserving old defaults.
2. Add SQLite storage/traversal and feed shallow observations from directory enumeration.
3. Add optional Everything runtime packaging and adapter, then switch production backend selection.
4. Add header popup/modal actions and centralized size formatting.
5. Add the lazy navigation tree and active-path synchronization.
6. Add the dedicated This PC devices-and-drives presentation.
7. Run focused tests, workspace tests, two-backend headful evidence, and ten cancellation cycles.

Rollback removes the new backend selector and returns to the previous search adapter. The SQLite directory is disposable cached state; rollback does not require reading it. Old session snapshots remain loadable through migration defaults.

## Open Questions

No blocking questions remain. Everything is used only when its current-session IPC endpoint responds; otherwise SQLite is selected automatically.
