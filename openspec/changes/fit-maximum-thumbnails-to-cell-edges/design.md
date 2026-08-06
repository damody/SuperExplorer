## Context

The visible-item texture map currently erases whether a texture was produced by thumbnail extraction or Shell icon loading. The file-view renderer consequently treats every texture as a Shell icon and fits it into the centered square `icon_size` host. At maximum zoom, the grid cell is wider than that host, so real landscape thumbnails remain inset from the selection border.

The approved source design is `docs/superpowers/specs/2026-08-05-maximum-thumbnail-edge-fit-design.md`. The implementation must preserve aspect ratio, must not crop, and must keep the file-name region, row height, virtualization, cache budgets, and Shell-icon behavior unchanged.

## Goals / Non-Goals

**Goals:**

- Preserve enough provenance to distinguish a successful real thumbnail from Shell/fallback pixels at render time.
- Fit thumbnails into the complete realized cell width, including the row-padding span, and the existing icon-region height.
- Make landscape images reach horizontal limits and portrait images reach vertical limits whenever their aspect ratio permits.
- Retain square centered geometry for folders, type icons, and fallback icons.
- Cover geometry and provenance with deterministic tests plus maximum-icon UTIT.

**Non-Goals:**

- Cropping or distorting content.
- Changing extraction resolution, row height, label layout, cache capacity, scheduling, or public APIs.
- Stretching Shell icons across the item cell.

## Decisions

### Carry explicit visual provenance

The file-view snapshot will carry a small presentation value containing the texture and whether it is a real thumbnail or a Shell icon. Thumbnail completion inserts thumbnail provenance; Shell completion, compatible-size recovery, and base/fallback paths insert Shell provenance. Unknown provenance is treated as Shell provenance.

This is preferred over detecting thumbnails from source aspect ratio because square thumbnails and non-square Shell assets exist. Explicit provenance makes fallback behavior deterministic.

### Derive thumbnail geometry from the realized spatial cell

For stacked icon modes, a thumbnail host uses the complete realized `cell_width`, deliberately extending through the row's horizontal-padding span, and uses `icon_size` for height. The existing aspect-fit calculation chooses the largest uncropped size. The item boundary is the final clipping limit; the computed image itself is never cover-cropped.

Shell icons continue using `icon_size × icon_size`. Non-stacked modes retain their current geometry because the reported defect concerns the independent visual-and-label layout of stacked icon views.

This is preferred over increasing `icon_size`, which would alter row height, scroll geometry, cache demand, and folder-icon scale.

### Preserve an independent label region

The thumbnail host replaces only the visual child. The existing stacked gap and label child remain siblings, so image growth cannot overlap the file name.

## Risks / Trade-offs

- **Risk: full-width geometry crosses the selection border at unusual DPI.** → Clamp the host to the non-negative realized cell width, keep it centered, and unit-test representative adjusted widths without a padding subtraction.
- **Risk: a fallback texture is incorrectly marked as a thumbnail and stretched.** → Assign provenance only at successful thumbnail admission and default all uncertain paths to Shell geometry.
- **Risk: wider decoded images increase GPU memory.** → This change reuses the existing texture and changes only presentation geometry; cache admission and decoded pixel buffers remain unchanged.
- **Trade-off: aspect-fit leaves whitespace on one axis.** → This is required to preserve the complete source without cropping or distortion.

## Migration Plan

No persisted data migration is required. Ship the renderer and provenance changes together. Rollback consists of reverting the presentation value and restoring the square host; cached textures remain compatible.

## Open Questions

None. The user approved aspect-fit behavior: landscape reaches horizontal bounds, portrait reaches vertical bounds, and no content is cropped.
