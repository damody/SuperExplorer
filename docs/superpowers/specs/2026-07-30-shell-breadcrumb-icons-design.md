# Shell-native breadcrumb icons

## Context

The browsing address bar already requests a Windows Shell icon for each resolved breadcrumb location. Until that asynchronous request succeeds, however, the renderer falls back to an application-drawn `NavigationIcon`. This produces folder artwork that differs from File Explorer and makes the first frame visually inconsistent. The application also does not preload a Shell-provided generic folder icon for unresolved or failed locations.

## Goals

- Render the same location-specific icons Windows Shell exposes to File Explorer for This PC, drives, folders, archives, and namespace items.
- Obtain the generic folder fallback from Windows Shell during application initialization instead of drawing it in the UI crate.
- Keep breadcrumb rendering asynchronous and non-blocking.
- Reuse the existing bounded memory and disk caches, with correct DPI, theme, and association invalidation.
- Cover initialization, key separation, rendering fallback, replacement, failure, and DPI/theme behavior with unit, structural, and headful UTIT evidence.

## Non-goals

- Replacing the shared Shell icon service or changing file-view thumbnail policy.
- Loading breadcrumb icons synchronously on the UI thread.
- Recreating Windows icons from Fluent assets, SVG, emoji, or application-drawn primitives.
- Changing breadcrumb spacing, typography, chevrons, hover behavior, or navigation semantics.

## Architecture

### Canonical generic folder request

The UI will define one helper that derives a generic breadcrumb folder `ShellIconKey` from the current Shell icon DPI, theme, and association epoch. Its location uses the existing synthetic folder marker, and its nonzero association generation plus zero overlay generation selects the audited `SHGetFileInfoW` path with `SHGFI_USEFILEATTRIBUTES | FILE_ATTRIBUTE_DIRECTORY`. The resulting bitmap therefore comes from the same Windows Shell association data used by File Explorer without requiring a real folder to exist.

The generic request is submitted as part of the existing initialization-time navigation icon batch. It uses the normal command/event pipeline, never invokes Shell COM or GDI from the GPUI thread, and is stored in the existing bounded `VisibleItemIconCache` plus the Shell disk cache.

### Per-location icons

An ancestry event continues to submit one deduplicated request for each concrete breadcrumb location. The location key remains distinct from the generic key, so This PC, drive volume artwork, customized folder icons, archives, and namespace identities replace the generic bitmap as soon as their Shell result arrives.

Theme, DPI, and association changes derive new keys and submit fresh requests. Stale responses remain subject to the existing request-context and generation checks.

### Rendering and fallback

Breadcrumb renderers receive both the location-specific icon snapshot and the optional generic Shell folder texture. The rendering order is:

1. Use the exact location-specific Shell texture when available.
2. Otherwise use the generic Shell folder texture.
3. If neither asynchronous request has completed on the first-ever frame, reserve the normal icon slot without drawing an invented icon.

This rule applies to the root, visible segments, overflow items, and child menus. The breadcrumb path must not call `navigation_icon` or any other application-drawn icon fallback.

## Data flow

1. Root initialization calculates the generic folder key and submits `LoadShellIcon` with the active tab context.
2. The Shell STA resolves it through `SHGetFileInfoW` using folder attributes and returns owned RGBA pixels.
3. The UI inserts the texture into the existing Shell icon cache and rerenders.
4. Ancestry and child-menu events submit concrete location keys.
5. Concrete results replace the generic fallback by ordinary key lookup; no special mutation or synchronous wait is required.

## Error handling

- A failed concrete location request leaves the generic Shell texture visible.
- A failed first-run generic request leaves a stable empty icon slot and may retry after association/theme/DPI invalidation through the normal request path.
- Corrupt disk entries are rejected by the existing disk-cache validation and fall through to a live Shell load.
- Queue overload and service disconnect remain observable through existing diagnostics and never block rendering.

## Testing

- Pure tests verify the generic key uses the synthetic folder location, nonzero association generation, zero overlay generation, and differs from concrete location keys.
- Initialization tests verify the generic request is included once and remains deduplicated across repeated navigation-icon submissions.
- Structural tests verify every breadcrumb surface uses location texture, then generic texture, then an empty reserved slot, with no `navigation_icon` fallback.
- Shell tests verify the generic request takes the `SHGFI_USEFILEATTRIBUTES` directory path.
- Headful UTIT navigates a multi-level fixture, captures the address bar, and records that icon slots are present for the root and every segment while the UI remains interactive.
- UTIT or deterministic render evidence verifies a late location-specific texture replaces the generic texture and that failure retains the generic texture.
- Existing breadcrumb interaction, hover, keyboard, and accessibility tests remain green.

## Rollback

Rollback restores the application-drawn breadcrumb fallback and removes the initialization request. No persisted user data or schema migration is involved; unused disk-cache entries remain harmless and bounded.
