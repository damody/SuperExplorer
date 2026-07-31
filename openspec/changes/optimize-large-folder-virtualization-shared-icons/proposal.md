## Why

Large directories currently make scrolling and scrollbar dragging stall because each redraw clones, sorts, and realizes the complete directory. A real 15,667-item network folder exposes the issue today, while the existing 100,000-item test limits only icon requests and does not exercise rendering.

## What Changes

- Introduce revisioned shared directory presentation state so render paths do not deep-clone complete snapshots.
- Cache filtered and sorted entry-index projections, including allocation-free normalized sort keys.
- Virtualize every file-view mode so only the viewport plus bounded overscan is realized.
- Coalesce directory batches and file-view notifications so incremental enumeration rebuilds presentation at most once per frame.
- Share normal folder icons and file-association icons by normalized extension, size, DPI, theme, and association epoch.
- Preserve OneDrive, TortoiseGit, custom-folder, shortcut, executable, Shell namespace, and thumbnail presentation through bounded visible-item overrides.
- Add release-build diagnostics and real render/scroll regression gates for large local and network directories.

## Capabilities

### New Capabilities

- `virtualized-file-view`: Revisioned presentation projections and bounded one- and two-dimensional realization for every file-view mode while preserving interaction and accessibility semantics.
- `shared-shell-icons`: Class-based shared base icons with independent association/overlay invalidation and per-item visible overrides for overlays and identity-specific icons.
- `large-directory-responsiveness`: Frame-coalesced directory updates, bounded background scheduling, privacy-safe performance diagnostics, and measurable large-directory responsiveness gates.

### Modified Capabilities

None. This repository has no baseline capability specs to modify.

## Impact

- Affects `explorer-model` directory snapshots, presentation identity, icon key contracts, and invalidation epochs.
- Affects `explorer-ui` state ownership, sorting/filtering, scrolling, row realization, accessibility, icon scheduling, and rendering diagnostics.
- Affects `explorer-shell-win` icon classification/loading and notification-driven invalidation while preserving existing Shell apartment and ownership boundaries.
- Affects `explorer-jobs` bounded visible-item and thumbnail scheduling where consumer cancellation or cache accounting must be shared.
- Adds unit, release performance, headful interaction, provider, overlay, and 100,000-entry regression coverage without changing external command-line or file-operation APIs.
