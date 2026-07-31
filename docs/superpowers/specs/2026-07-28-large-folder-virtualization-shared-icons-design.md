# Large-Folder Virtualization and Shared Icon Design

Date: 2026-07-28
Status: Approved

## Problem

Opening `\\damody\gdrive\H漫畫` exposes a scalability failure in the file view. The directory contains 15,667 direct children, and a direct PowerShell enumeration takes about 5.9 seconds on the current network path. Slow provider enumeration explains delayed completion, but it does not explain why wheel scrolling and scrollbar dragging remain sluggish after entries are available.

The UI currently performs work proportional to the complete directory on redraw:

- A non-zoom wheel event refreshes all windows.
- Rendering deep-clones `AppViewState` into several child components and clones the visible `DirectoryState` again.
- `sorted_file_entries` clones every `FileEntry`, filters the complete collection, and sorts it for every file-view render.
- Case-insensitive comparisons allocate lowercase strings during sorting.
- The file view creates a GPUI element tree, accessibility metadata, strings, and pointer handlers for every entry instead of only the visible range.
- Directory enumeration publishes batches of at most 64 entries. Each applied group can trigger another increasingly expensive full render while enumeration is in progress.

The current 100,000-row UI test verifies only that initial icon requests are capped. It does not render or scroll 100,000 rows and therefore does not protect the actual bottleneck.

## Goals

1. Make file-view rendering and scrolling proportional to the visible range rather than the directory size.
2. Keep the UI responsive while a slow local, network, cloud, or Shell provider is still enumerating.
3. Share normal folder icons and file-association icons between items of the same normalized extension.
4. Preserve per-item overlays such as OneDrive and TortoiseGit badges.
5. Preserve custom folder icons, shortcuts, executable icons, thumbnails, selection, rename, drag/drop, keyboard navigation, and accessibility behavior.
6. Add performance tests that exercise rendering and scrolling rather than only request limits.

## Non-Goals

- Replacing GPUI with the native Windows ListView control.
- Recursively enumerating directory descendants.
- Eagerly extracting thumbnails or overlays for offscreen entries.
- Removing bounded Shell worker, broker, cancellation, or generation-safety contracts.
- Guaranteeing that a slow network provider completes enumeration within a fixed wall-clock time.

## Considered Approaches

### Minimal redraw and caching patch

Memoizing sorting and removing selected refresh calls would reduce some work, but the UI would still construct an element tree for every item. This does not meet the 100,000-entry target and is rejected as the final architecture.

### GPUI virtualization with shared presentation state

The selected approach keeps the existing GPUI composition and interaction reducer, introduces shared immutable snapshots and cached presentation indices, and virtualizes every file-view mode. It fixes the scaling model without abandoning existing styling and behavior.

### Native virtual ListView

A Windows owner-data ListView offers mature virtualization, but integrating it would split rendering, focus, accessibility, drag/drop, theme, and view-mode behavior across two UI systems. The compatibility and maintenance cost is disproportionate, so this approach is rejected.

## Architecture

### Shared directory storage

The model remains the owner of mutable directory state. The UI consumes a revisioned shared snapshot instead of deep-cloning the complete directory during every render.

The presentation contract consists of:

- a monotonically increasing directory revision;
- shared immutable entry storage addressable by stable `ShellItemId` and entry index;
- a cached presentation index containing entry indices rather than cloned `FileEntry` values;
- lightweight view state such as selection, focused identity, view settings, and current scroll geometry.

GPUI components receive only the state they render. `WindowChrome`, `NavigationBar`, `CommandBar`, `NavigationPane`, `OperationCenter`, and `StatusBar` must not each own a deep copy of the active directory snapshot. `FileViewHost` receives shared storage and a presentation projection.

### Presentation projection

`DirectoryPresentation` owns the ordered, filtered view of a snapshot. Its cache identity is:

```text
(directory_revision, sort_column, sort_direction, hidden_items)
```

The ordered representation is `Vec<entry_index>`. It never clones the underlying entries merely to sort them.

Each entry receives reusable normalized sort data when it enters or changes in the snapshot. At minimum this includes a case-folded display-name key. Type text and other string columns receive equivalent normalized keys when needed. Sort comparison must not allocate lowercase strings.

Scrolling, focus changes, hover, and selection do not invalidate the presentation projection. It is invalidated only by:

- insertion, removal, or rename;
- metadata changes that affect the active filter or sort column;
- sort descriptor changes; or
- hidden-item visibility changes.

Multiple directory batches received within one UI frame are merged before rebuilding the projection. The projection is rebuilt at most once per frame.

### One-dimensional virtualization

Details, List, and Content modes use a one-dimensional virtual list. The list calculates the visible row range from scroll offset, viewport height, and row height. It renders the visible range plus two viewports of overscan above and below, clamped to the collection bounds.

The scroll extent represents the complete presentation length even though offscreen rows have no element tree. The fixed Details header remains outside the vertically translated row surface and keeps its independent horizontal-scroll behavior.

Rows use stable item identities for selection, rename, drag/drop, and callback routing. A visible ordinal may be used for geometry and accessibility position, but it must not become persistent item identity.

### Two-dimensional virtualization

Small, Medium, Large, and Extra Large Icons, Tiles, and any wrapped mode compute:

- column count from viewport width and cell width;
- visible grid-row range from vertical scroll offset and cell height;
- entry range from grid rows and column count; and
- the same two-viewport overscan policy.

Resizing the viewport or changing zoom recomputes geometry without cloning or sorting entries. Scroll extent is derived from the total grid-row count.

### Scroll behavior

Normal wheel events update the tracked scroll state. They must not unconditionally call `refresh_windows()` for the entire application. A file-view notification is scheduled only when the virtual visible range changes or a fixed overlay needs repainting, and notifications are coalesced to one per frame.

Scrollbar thumb dragging follows the same path. Pointer capture behavior remains unchanged, while content work stays bounded by the virtual range.

## Shared Icon Design

### Two-layer presentation

File rows render two conceptual icon layers:

1. A shared base icon, available immediately from a class-based cache.
2. An optional per-item visible override containing an overlay, custom icon, or other identity-specific Shell result.

The override replaces or composites over the base when it arrives. Only visible and near-visible items request per-item work. Leaving the scheduled range removes the consumer and permits cancellation.

### Base icon classification

The base cache key includes physical size bucket, DPI, theme, and the appropriate invalidation epoch. Its classification is:

| Item class | Shared base identity | Per-item behavior |
| --- | --- | --- |
| Normal folder | Generic folder class | Visible item may load custom folder icon and overlay |
| Normal associated file | Normalized lowercase extension | Visible item may load overlay |
| Extensionless file | Generic file class | Visible item may load an identity-specific override |
| Executable, icon, shortcut, or another identity-icon type | Stable item identity | Always identity-specific when visible |
| Drive, Shell namespace, known folder, or special container | Stable Shell identity | Identity-specific |
| Image, video, or other thumbnail candidate | Association base until thumbnail arrives | Thumbnail scheduler remains separate |

Extension normalization treats `.JPG` and `.jpg` as the same association class. Paths and navigation generations are not part of a normal extension base key.

The initial special-type set includes `.exe`, `.dll`, `.ico`, `.lnk`, `.url`, `.cpl`, and Shell namespace items. The classifier is a single tested model function so additional Windows identity-icon types can be added without changing UI code.

### Overlay and custom-icon preservation

The current Shell loader returns an item-specific rendered result rather than a reusable standalone overlay bitmap. The optimized design therefore renders the shared base immediately, then asks the existing Shell boundary for a full per-item visible result where overlays or custom icons may apply. The visible result is stored in a bounded item cache and replaces the shared base for that row.

This preserves OneDrive and TortoiseGit badges without requesting a unique icon for every offscreen item. It also preserves `desktop.ini` custom folder icons for visible folders. A cached negative result records that an item currently has no distinct visible override, preventing repeated Shell calls while its overlay epoch remains valid.

### Cache separation and invalidation

Three caches remain logically separate:

- `BaseIconCache`: shared folder and extension association pixels;
- `VisibleItemIconCache`: per-item overlays, custom icons, and identity icons;
- `ThumbnailCache`: content thumbnails with its existing generation-safe scheduler.

All caches use entry-count and decoded-byte budgets with LRU eviction. Textures are held by `Arc<RenderImage>` so every matching row shares the same allocation.

Navigation generation must no longer invalidate association icons. Invalidation uses:

- `association_epoch`, advanced only for Windows association or theme/DPI changes relevant to the key;
- per-item or scoped `overlay_epoch`, advanced by watcher or Shell notifications that can affect badges or custom icons; and
- thumbnail source generation, retained independently for content correctness.

Theme and DPI remain explicit key dimensions. An overlay change does not discard unrelated extension base icons.

## Enumeration and Event Flow

The Shell boundary keeps its 64-item and byte-size batch caps. These are safety and cancellation boundaries, not render boundaries.

The UI service pump:

1. drains a bounded set of available events;
2. applies all correlated directory batches to the model;
3. updates the directory revision once for the accepted transaction;
4. rebuilds the presentation at most once for the frame;
5. schedules base icons by unique class for the virtual range;
6. schedules item overrides and thumbnails only for the virtual range; and
7. notifies the file view once.

The first accepted batch can therefore produce visible rows immediately. Enumeration may continue for several seconds without making scrolling, cancellation, tab switching, or window interaction wait for completion.

Stale generations and cancelled requests remain rejected by the model. Icon or thumbnail completion for an item outside the current consumer range may populate a bounded cache but must not restore stale selection or trigger an unbounded repaint.

## Accessibility and Interaction

Virtualization must preserve the logical collection size and each visible row's position. UI Automation exposes the full item count and accurate one-based positions for realized rows. Offscreen items are realized through the existing focus and keyboard-navigation path before focus is moved to them.

The following behaviors continue to use stable item identity:

- single, additive, and range selection;
- keyboard focus and activation;
- inline rename;
- drag source and drop target resolution;
- context menus;
- watcher updates; and
- sorting or filtering transitions.

Keyboard movement may calculate a target ordinal through the presentation index, scroll that ordinal into view, and then realize and focus the corresponding row.

## Performance Instrumentation

Release-build diagnostics record:

- directory item count and presentation revision;
- presentation rebuild count and duration;
- realized row count and overscan range;
- render and scroll callback duration percentiles;
- complete-snapshot clone count, which must be zero on the scroll path;
- base-icon class hits and misses;
- visible-item override hits, misses, negative hits, cancellations, and queue depth;
- thumbnail queue depth and cancellations; and
- decoded texture bytes and evictions by cache.

No diagnostic log includes private full paths or filenames.

## Implementation Phases

### Phase 1: Baseline and ownership

- Add render, sort, clone, realized-row, and scroll timing instrumentation.
- Capture a Release baseline for the 15,667-entry network directory and a generated 100,000-entry fixture.
- Introduce shared directory snapshot ownership and lightweight component view models.
- Remove complete `AppViewState` and `DirectoryState` cloning from the render path.

### Phase 2: Cached presentation

- Add directory revisions and `DirectoryPresentation`.
- Add precomputed normalized sort keys.
- Rebuild presentation once per relevant revision or settings change and at most once per frame during batch loading.
- Convert selection and action routing from cloned rows to stable identity plus presentation index.

### Phase 3: Virtual file views

- Implement Details, List, and Content one-dimensional virtualization.
- Implement wrapped icon and tile two-dimensional virtualization.
- Preserve fixed headers, horizontal scrolling, keyboard reveal, marquee selection, drag/drop, rename, and accessibility semantics.
- Remove unconditional whole-window wheel refreshes.

### Phase 4: Shared icons and visible overrides

- Introduce the base icon classifier and class-based keys.
- Separate association and overlay invalidation epochs from navigation generation.
- Schedule one shared request per visible base class.
- Add the bounded visible-item override cache, negative results, cancellation, and overlay/custom-icon replacement.
- Keep thumbnail scheduling separate and based on the same virtual range.

### Phase 5: Coalescing and regression gates

- Coalesce directory presentation work and file-view notifications per frame.
- Add headful scroll, scrollbar drag, resize, zoom, sort, and rapid-navigation tests.
- Run local, network, cloud-placeholder, overlay-provider, and 100,000-entry stress matrices.
- Publish measured before/after evidence and make the agreed thresholds regression gates.

## Acceptance Criteria

All performance criteria use an optimized Release build.

1. The 15,667-entry `\\damody\gdrive\H漫畫` directory remains interactive during enumeration and scrolls smoothly after the first batch arrives.
2. A 100,000-entry fixture realizes no more than 250 file rows or cells at once under standard test viewport sizes.
3. Steady-state scrolling does not rebuild the sort projection or clone the complete directory snapshot.
4. Scroll-frame time is at most 16.7 ms at p95 on the reference local fixture. The network/provider matrix permits 33 ms p95 but no individual UI-thread stall above 100 ms attributable to file-view rendering.
5. After the first batch is visible, pointer and keyboard input latency is at most 50 ms p95 while enumeration continues.
6. Normal folders of the same size, DPI, and theme issue one shared base-icon request per cache epoch.
7. Normal files with the same case-insensitive extension issue one shared base-icon request per size, DPI, theme, and association epoch.
8. OneDrive, TortoiseGit, and equivalent per-item overlays remain visible for realized items.
9. Custom folder icons, shortcuts, executables, Shell namespace items, and thumbnails retain their identity-specific presentation.
10. Fast scrolling keeps icon and thumbnail queues within configured bounds and cancels consumers that leave the virtual range.
11. Sorting, selection, range selection, rename, context menus, drag/drop, keyboard navigation, details header behavior, and UI Automation semantics have no functional regression.
12. The new 100,000-entry test performs actual file-view realization and scroll transitions; an icon-request-only test is not sufficient evidence.

## Risks and Mitigations

- **Overlay APIs may not expose a reusable badge bitmap.** Use the existing item-specific Shell result only for visible overrides and cache negative results.
- **Variable-height content could complicate scroll geometry.** Keep each current view mode's existing fixed cell or row height; variable-height rows are outside this change.
- **Rapid batches could repeatedly invalidate sorting.** Merge accepted batches and rebuild at most once per frame.
- **Virtualization could break selection after sorting.** Persist identity, never row ordinal, and translate through the presentation index.
- **Accessibility could report only realized items.** Publish total set size and realize the keyboard/UIA target before moving focus.
- **Custom folder icons might briefly show the generic base.** Treat this as progressive presentation: a correct generic folder appears immediately and the visible override replaces it asynchronously.

## Rollout

Land each implementation phase behind tests that protect its new invariant. The virtualized file view becomes the only production path after all existing view modes pass functional and visual tests; maintaining parallel full-render and virtual-render paths is not a long-term option. Performance instrumentation remains available after rollout so future work cannot reintroduce O(total entries) behavior on the scroll path.
