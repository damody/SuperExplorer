## 1. Baseline and Owned Contracts

- [x] 1.1 Record focused search/model/UI/session baselines and preserve unrelated concurrent shared-tree changes
- [x] 1.2 Extend typed search backend/status contracts with Everything and LocalIndex while retaining compatible WindowsIndex parsing
- [x] 1.3 Generalize DetailsColumn identity, owned optional metadata, visibility/order/width settings, and bounded validation
- [x] 1.4 Add session schema migration and round-trip tests from the existing four-column representation
- [x] 1.5 Centralize adaptive file-size formatting and add exhaustive unit-boundary tests

## 2. Everything SDK Provider

- [x] 2.1 Pin the official Everything SDK runtime/header/license with provenance and offline build packaging
- [x] 2.2 Implement canonical owned-path DLL loading, required Unicode export resolution, architecture validation, and cleanup
- [x] 2.3 Implement real IPC/database capability probing without starting or configuring Everything
- [x] 2.4 Translate parsed expressions and canonical folder scope into escaped Everything queries
- [x] 2.5 Implement bounded paged result extraction into owned FileEntry batches with stable filesystem identity
- [x] 2.6 Add cancellation, timeout, safe diagnostics, IPC-loss status, and same-request LocalIndex failover
- [x] 2.7 Add fake API/provider tests for availability, injection escaping, paging, cancellation, failure, and privacy

## 3. SQLite Lazy Index

- [x] 3.1 Add SQLite dependency and resolve the contained `%LOCALAPPDATA%\RustGpuiExplorer\search-index\v1` data path
- [x] 3.2 Implement versioned WAL schema, bounded connection settings, migrations, integrity checks, and corruption quarantine
- [x] 3.3 Implement canonical metadata upsert/delete and exact descendant-scope parameterized queries
- [x] 3.4 Feed successful directory enumeration into shallow immediate-child observation without recursive indexing
- [x] 3.5 Implement cached-first breadth-first active-scope traversal with reparse exclusion and bounded transactions
- [x] 3.6 Stop queue growth, enumeration, delivery, and uncommitted writes on replacement/clear/navigation/tab-close/shutdown cancellation
- [x] 3.7 Enforce queue/result/visited/path/database bounds and publish partial terminal status when reached
- [x] 3.8 Add temporary-directory tests for shallow observation, refresh deletion, cache-first results, path boundaries, corruption, reparse points, and storage bounds
- [x] 3.9 Add a cancellation oracle proving database row count stops increasing after a root-scope request is cancelled

## 4. Search Pipeline Integration

- [x] 4.1 Compose Everything capability and LocalIndex at application/Shell startup without making optional provider failures fatal
- [x] 4.2 Replace production WindowsIndex-plus-full-scan flow with Everything-first and LocalIndex fallback selection
- [x] 4.3 Preserve existing generation rejection, dedupe, per-tab isolation, source status, and exactly-one terminal semantics
- [x] 4.4 Add integration tests for Everything available, unavailable, mid-query failover, and two-tab cancellation isolation
- [x] 4.5 Repair the top-right search field focus/input/submit/clear flow and verify visible results with both backends
- [x] 4.6 Restore the original directory snapshot and cancel the active generation when the search editor is emptied by text deletion

## 5. Details Header Menu and Columns

- [x] 5.1 Add header right-click actions and an exclusive column popup/modal state with dedicated focus restoration
- [x] 5.2 Render an occluding Explorer-like menu with current/all auto-size, checkable common columns, and Other Columns
- [x] 5.3 Implement hover and full pointer/keyboard activation without passing interaction through to headers or file rows
- [x] 5.4 Wire column visibility/order/width changes per tab while keeping Name mandatory
- [x] 5.5 Render Date Created, Authors, Tags, and Title from owned metadata with blank unsupported cells
- [x] 5.6 Implement accessible Other Columns modal with bounded list, check state, confirm/cancel, and no background activation
- [x] 5.7 Use the centralized file-size formatter in Details, Tiles, Content, Preview metadata, and auto-size measurement
- [x] 5.8 Add reducer/render tests for hit testing, focus, keyboard, exactly-once actions, per-tab settings, metadata blanks, and every size surface
- [x] 5.9 Close every app-owned context menu on Escape or pointer activation outside the popup without triggering underlying rows

## 6. Verification and Evidence

- [x] 6.1 Run formatting, source/provenance audits, focused crate tests, workspace all-target tests, and release build
- [x] 6.2 Add UITEST manifest cases for Everything and LocalIndex backend selection with truthful prerequisites/skips
- [x] 6.3 Add headful temporary-fixture coverage for search results, immediate cancellation, header menu options, row-hover isolation, and size units
- [x] 6.4 Run ten consecutive cancellation cycles across two tabs and two drive paths and retain database-growth evidence
- [x] 6.5 Validate OpenSpec strictly and mark only tasks supported by code and retained evidence complete
- [x] 6.6 Add UTIT cases for search entry/results/clear and context-menu outside-click/Escape dismissal
- [x] 6.7 Add headful UTIT regression coverage for empty-text search restoration and selected-item Shell menu Escape/outside-click cancellation

## 7. Explorer Navigation Tree

- [x] 7.1 Add per-tab expanded-node, loading/error, request, and bounded child-cache state with stable filesystem identities
- [x] 7.2 Reuse the asynchronous child-container protocol for expand/collapse and reject or cancel stale node requests
- [x] 7.3 Render nested drive/folder rows with distinct chevron and row hit targets, hover, selection, loading, and retry states
- [x] 7.4 Automatically expand the active ancestry and keep selection/focus isolated across tabs
- [x] 7.5 Implement Explorer-like pointer and Left/Right/Enter/Space keyboard interaction
- [x] 7.6 Add reducer/render coverage and UTIT cases for expand, collapse, navigate, auto-reveal, cancellation, and tab isolation
- [x] 7.7 Canonicalize This PC volume-root presentation so Shell ancestry and stable drive rows cannot render duplicates, with unit and UTIT uniqueness oracles
- [x] 7.8 Invalidate expanded navigation child caches after watcher and successful mutation events, with Rust regression coverage and external/app-delete UTIT evidence

## 8. This PC Devices and Drives

- [x] 8.1 Extend owned entry metadata with optional drive type, total bytes, available bytes, and availability state
- [x] 8.2 Populate drive status off the UI thread during This PC enumeration and handle unavailable/removable/network drives truthfully
- [x] 8.3 Render Explorer-like Devices and drives grouping, adaptive capacity labels, bounded usage bars, and low-space warning state
- [x] 8.4 Preserve selection, keyboard, double-click navigation, context menu, refresh, and drag/drop behavior for drive tiles
- [x] 8.5 Add unit/render tests and UTIT cases for multiple drives, capacity math/units, unavailable media, low space, and activation

## 9. Immediate Complete Details Column Menu

- [x] 9.1 Remove the Other Columns draft/confirm state and render every supported column in the header context menu by default
- [x] 9.2 Apply optional-column visibility changes immediately while keeping Name mandatory and preserving per-tab isolation
- [x] 9.3 Add Rust and headful UTIT regression coverage for the complete menu, absent Other/OK/Cancel commands, and immediate show/hide behavior
- [x] 9.4 Run focused tests, the registered UTIT case, formatting, Clippy, build, and strict OpenSpec validation
