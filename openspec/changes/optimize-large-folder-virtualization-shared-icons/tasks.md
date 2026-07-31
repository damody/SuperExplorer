## 1. Baseline and Performance Instrumentation

- [x] 1.1 Add privacy-safe counters and timers for directory revision, presentation rebuilds, realized items, complete-snapshot clones, render callbacks, and scroll callbacks.
- [x] 1.2 Extend icon and thumbnail diagnostics with per-cache entry/byte budgets, hits, misses, negative hits, queue depth, cancellations, and evictions.
- [x] 1.3 Add a deterministic 100,000-entry model fixture that does not perform real Shell or network I/O.
- [x] 1.4 Add Release benchmark plumbing that records p50/p95/max frame and input latency for wheel and scrollbar interactions.
- [x] 1.5 Capture and document the pre-change Release baseline for the generated fixture and the 15,667-entry network directory without logging private paths or filenames.

## 2. Revisioned Shared Directory Presentation

- [x] 2.1 Add a monotonically increasing revision to accepted directory snapshot mutations and tests for insert, update, remove, retain, refresh, and stale-event rejection.
- [x] 2.2 Introduce shared immutable entry storage addressable by stable `ShellItemId` and entry index without changing model ownership or request correlation.
- [x] 2.3 Add reusable normalized display-name and optional text-column sort keys that update only when their source metadata changes.
- [x] 2.4 Implement `DirectoryPresentation` as an ordered `Vec<entry_index>` cached by revision, sort column, direction, and hidden-item setting.
- [x] 2.5 Replace allocating lowercase comparisons with normalized-key comparisons and add case-insensitive deterministic ordering tests.
- [x] 2.6 Add invalidation tests proving that scrolling, hover, focus, and selection reuse the projection while relevant data or view-setting changes rebuild it.
- [x] 2.7 Coalesce accepted directory mutations so presentation revision and projection rebuild occur at most once per UI frame.

## 3. Lightweight GPUI State Ownership

- [x] 3.1 Define lightweight view models for window chrome, navigation/address, command bar, navigation pane, operation center, status bar, and file view.
- [x] 3.2 Replace render-time `AppViewState` deep clones with shared or component-specific state while preserving current reducer ownership.
- [x] 3.3 Replace `visible_directory_state` cloning on the production render path with shared presentation access for directory and search results.
- [x] 3.4 Change `FileViewHost` to consume shared entries, ordered indices, stable selection state, and viewport geometry rather than an owned complete `DirectoryState`.
- [x] 3.5 Add a render-path test asserting zero complete-snapshot clones during steady-state scroll.

## 4. One-Dimensional File-View Virtualization

- [x] 4.1 Implement tested fixed-row virtual-range geometry with viewport bounds, scroll offset, collection size, and two-viewport overscan.
- [x] 4.2 Implement virtual spacers or translated content so the scroll extent represents the complete collection without realizing offscreen rows.
- [x] 4.3 Migrate Details mode to bounded realization while keeping the header vertically fixed and horizontally synchronized.
- [x] 4.4 Migrate List mode to bounded realization and preserve its existing row sizing and selection styling.
- [x] 4.5 Migrate Content mode to bounded realization and preserve its existing metadata layout.
- [x] 4.6 Remove unconditional application-wide refresh from normal file-view wheel input and coalesce range-change notifications to one per frame.
- [x] 4.7 Add 100,000-entry tests asserting no more than 250 realized rows in each one-dimensional mode.

## 5. Two-Dimensional Virtualization and Interaction Preservation

- [x] 5.1 Implement tested grid geometry for column count, total grid rows, visible grid-row range, overscan, and entry-index translation.
- [x] 5.2 Migrate Small, Medium, Large, and Extra Large Icons to bounded grid realization.
- [x] 5.3 Migrate Tiles and any remaining wrapped file-view mode to bounded grid realization.
- [x] 5.4 Preserve stable-identity single, additive, and range selection across virtual ranges, sorting, and filtering.
- [x] 5.5 Implement offscreen keyboard reveal by resolving the target ordinal, scrolling it into range, realizing it, and then transferring focus.
- [x] 5.6 Preserve inline rename, activation, context menus, marquee selection, drag source, drop target, and watcher-update routing through stable identity.
- [x] 5.7 Publish complete accessibility set size and accurate realized positions, and realize offscreen UIA targets before focus or invocation.
- [x] 5.8 Add 100,000-entry tests asserting no more than 250 realized cells in each wrapped mode and no re-sort on resize or zoom.

## 6. Shared Base Icon Contracts

- [x] 6.1 Add a model-level `BaseIconKey` and classifier for generic folders, normalized extensions, generic extensionless files, and identity-specific classes.
- [x] 6.2 Add classifier tests for case-insensitive extensions and identity-specific `.exe`, `.dll`, `.ico`, `.lnk`, `.url`, `.cpl`, drive, known-folder, and Shell namespace items.
- [x] 6.3 Introduce association and overlay invalidation epochs independent from navigation generation and add scoped invalidation tests.
- [x] 6.4 Update Shell base-icon loading to request one reusable base per class, size bucket, DPI, theme, and association epoch.
- [x] 6.5 Add byte- and entry-bounded `BaseIconCache` storage whose textures are shared through `Arc<RenderImage>`.
- [x] 6.6 Update file-row rendering to display the shared base immediately and reuse it across matching normal folders or extensions.
- [x] 6.7 Add tests proving navigation does not invalidate reusable bases and association changes reload affected classes on demand.

## 7. Visible Item Overrides, Overlays, and Thumbnails

- [x] 7.1 Add a byte- and entry-bounded `VisibleItemIconCache` for composed overlays, custom folder icons, and identity-specific Shell results.
- [x] 7.2 Publish the virtual visible and near-visible identity range to icon scheduling and submit item-specific Shell work only for that range.
- [x] 7.3 Render a completed item-specific result over or instead of the shared base without mutating other rows that share the base.
- [x] 7.4 Cache negative visible-item results by overlay epoch and test that re-entering the range does not repeat Shell work.
- [x] 7.5 Cancel item-result consumers that leave the virtual range and reject stale completions from obsolete navigation or overlay generations.
- [x] 7.6 Preserve OneDrive and TortoiseGit overlays plus `desktop.ini` custom folder icons in real-provider validation.
- [x] 7.7 Route thumbnail scheduling from the same virtual range, retain independent source generations and cache budgets, and prevent thumbnails from entering the shared base cache.
- [x] 7.8 Add fast-scroll tests proving icon and thumbnail queues, concurrency, and decoded in-flight bytes remain bounded while current visible work is prioritized.

## 8. Directory Pump Coalescing and Generation Safety

- [x] 8.1 Keep the existing 64-item and 256-KiB Shell enumeration batch boundaries and document that they remain cancellation boundaries rather than render boundaries.
- [x] 8.2 Drain and apply correlated batches in a bounded UI transaction and publish no more than one file-view notification per frame.
- [x] 8.3 Ensure the first accepted batch becomes immediately realizable before terminal enumeration.
- [x] 8.4 Add navigation-during-load tests proving stale directory, icon, and thumbnail events cannot alter the new presentation, selection, or focus.
- [x] 8.5 Add slow-provider tests proving scrolling, tab switching, and cancellation remain available throughout incremental enumeration.

## 9. Regression Gates and Rollout

- [x] 9.1 Add actual Release render-and-scroll coverage for 100,000 entries across one-dimensional and wrapped view families; retain but do not treat the existing icon-request-only test as sufficient.
- [x] 9.2 Enforce local p95 scroll-frame time at or below 16.7 ms and network/provider p95 at or below 33 ms with no file-view render stall above 100 ms.
- [x] 9.3 Enforce p95 pointer and keyboard input latency at or below 50 ms after first-batch realization while enumeration continues.
- [x] 9.4 Run headful sorting, selection, range selection, rename, context-menu, drag/drop, keyboard, Details header, and UI Automation regressions in every view family.
- [x] 9.5 Run real Shell validation for normal folder and extension sharing, association changes, overlays, custom folders, shortcuts, executables, namespace items, cloud placeholders, and thumbnails.
- [x] 9.6 Run workspace format, clippy, unit, integration, UITEST, and relevant Windows smoke gates and fix all change-related failures.
- [x] 9.7 Remove the legacy all-row production render path after all virtualized modes and gates pass.
- [x] 9.8 Publish before/after performance and resource evidence, including realized counts, clone/sort invariants, cache budgets, queue bounds, and known provider variability.
