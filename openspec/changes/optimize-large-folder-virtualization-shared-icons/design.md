## Context

The GPUI file view currently scales redraw work with total directory size. `ExplorerRoot` clones the complete `AppViewState`, `ExplorerWindow` clones it into multiple children, `visible_directory_state` clones the active directory again, and `FileViewHost` clones and sorts all entries before realizing every row. A wheel event also refreshes all windows. With 15,667 direct children on a network path, provider enumeration takes about 5.9 seconds and the growing all-row render makes both loading and steady-state scrolling unresponsive.

The Shell boundary already emits cancellation-aware batches capped at 64 items and 256 KiB. Thumbnail scheduling already has bounded queues and decoded-byte accounting. The change must preserve those ownership boundaries, all current view modes and interactions, OneDrive/TortoiseGit overlays, custom icons, thumbnails, and generation safety.

## Goals / Non-Goals

**Goals:**

- Make steady-state scroll and render cost proportional to the realized viewport, not total entries.
- Share immutable directory data and cached presentation indices without deep-cloning complete snapshots.
- Virtualize one-dimensional and wrapped two-dimensional views with stable item identity.
- Share normal folder and extension association icons while preserving visible item overlays and identity-specific presentation.
- Coalesce incremental directory work and add release-build performance gates that render and scroll 100,000 entries.

**Non-Goals:**

- Replace GPUI with a native owner-data ListView.
- Introduce variable-height rows or recursive directory enumeration.
- Eagerly load offscreen thumbnails, overlays, or custom icons.
- Change external command-line or file-operation APIs.
- Promise a fixed completion time for slow network or Shell providers.

## Decisions

### Use revisioned shared presentation storage

The model remains the mutable owner and publishes shared immutable entry storage plus a monotonically increasing directory revision. UI children receive lightweight view models and shared references rather than cloned `AppViewState` or `DirectoryState` values. The presentation layer stores ordered entry indices, while stable `ShellItemId` remains the identity for selection and actions.

This is preferred over retaining deep-owned `RenderOnce` inputs because reference-counted immutable snapshots make ownership explicit and keep scroll work bounded. A native ListView was rejected because it would split theme, focus, accessibility, drag/drop, and view-mode behavior across UI systems.

### Cache filtered and sorted index projections

`DirectoryPresentation` caches `Vec<entry_index>` by `(directory_revision, sort_column, sort_direction, hidden_items)`. Entries precompute reusable case-folded string keys when inserted or when relevant metadata changes. Comparators do not allocate strings, and scrolling, hover, focus, or selection do not invalidate the projection.

During enumeration, all accepted batches in one UI frame are merged before a projection rebuild. At most one rebuild occurs per frame, even though the Shell safety boundary continues to publish smaller batches.

### Virtualize every file-view mode

Details, List, and Content use one-dimensional fixed-height virtualization. Wrapped icon and tile modes derive column count from viewport width, then virtualize grid rows. Both paths realize the viewport plus two viewports of overscan on each scroll axis where applicable, clamped to collection bounds. Standard test viewports must never realize more than 250 items.

The virtual surface reports full scroll extent. Details keeps the fixed header separate from vertically translated rows and preserves horizontal scrolling. Keyboard navigation resolves the target through the presentation index, scrolls it into range, realizes it, and then moves focus.

### Coalesce scroll-driven notifications

Normal wheel and scrollbar events update tracked offset and virtual range without calling an unconditional application-wide refresh. File-view notification occurs only when the realized range or a fixed overlay changes and is coalesced to one notification per frame.

### Split shared base icons from visible item overrides

A `BaseIconKey` classifies normal folders by a generic folder class and normal associated files by normalized lowercase extension. Size bucket, DPI, theme, and association epoch remain key dimensions. Extensionless generic files use a generic file class.

`.exe`, `.dll`, `.ico`, `.lnk`, `.url`, `.cpl`, drives, known folders, and Shell namespace items retain stable identity keys because their icon can differ per item. The classifier lives in one tested model function.

Rows render a shared base immediately. Only realized and near-realized items request a full Shell item result that can contain an overlay, custom folder icon, or another identity-specific override. The existing Shell loader may return a composed bitmap, so the override replaces the base rather than requiring extraction of a standalone badge. Negative override results are cached for the current overlay epoch.

### Separate cache and invalidation domains

`BaseIconCache`, `VisibleItemIconCache`, and `ThumbnailCache` remain independent. Each uses entry-count and decoded-byte budgets with LRU eviction, and matching rows share `Arc<RenderImage>` allocations.

Navigation generation does not invalidate normal association icons. `association_epoch` changes for relevant association, theme, or DPI changes. Per-item or scoped `overlay_epoch` changes for watcher or Shell notifications that can affect badges or custom icons. Thumbnail source generation remains independent.

### Preserve bounded background ownership

The Shell batch caps, STA ownership, broker boundaries, request correlation, and cancellation tokens remain unchanged. The UI publishes the virtual range to icon and thumbnail schedulers. Consumers leaving the range are cancelled, and stale completions may populate a bounded cache but cannot restore stale UI state or force an unbounded repaint.

### Make performance behavior observable

Release diagnostics record projection rebuild count and duration, realized count, render and scroll percentiles, complete-snapshot clone count, cache hits/misses/bytes/evictions, queue depth, and cancellations. Diagnostics do not log full paths or filenames.

## Risks / Trade-offs

- **Item-specific Shell APIs may return a composed icon rather than an overlay bitmap** → Use the composed result only as a visible override and cache negative results.
- **A custom folder initially shows the generic folder base** → Treat this as progressive presentation and replace it asynchronously when the visible override arrives.
- **Virtualization can break selection after sorting** → Persist stable identity and translate ordinals only through the current presentation index.
- **Virtualization can under-report accessibility collection state** → Publish total set size and accurate realized positions; realize offscreen keyboard/UIA targets before moving focus.
- **Rapid batches can thrash presentation caches** → Merge accepted batches and rebuild at most once per frame.
- **Two render paths can diverge** → Migrate every mode and remove the legacy all-row production path after regression coverage passes.

## Migration Plan

1. Add baseline diagnostics and capture release evidence for the real 15,667-item network directory and a generated 100,000-entry fixture.
2. Introduce shared revisioned snapshots and cached projections while preserving current output.
3. Migrate Details, List, and Content to one-dimensional virtualization.
4. Migrate icon and tile modes to two-dimensional virtualization and remove unconditional whole-window wheel refresh.
5. Introduce shared base icon classification, independent epochs, and bounded visible-item overrides.
6. Coalesce directory/file-view notifications and enforce performance, interaction, accessibility, provider, and overlay gates.
7. Remove the legacy all-row production path after all modes pass.

Each phase lands with tests for its invariant. If a phase regresses behavior, revert that phase while retaining the previously validated shared contracts; no persistent user-data migration is required.

## Open Questions

None. Overscan, icon classification, invalidation, cache separation, and acceptance thresholds are fixed by the approved source design.
