## Context

Breadcrumb ancestry and child-menu events already schedule asynchronous Shell icon requests for concrete locations. Rendering falls back to `NavigationIcon`, so the address bar can display application-drawn artwork that differs from File Explorer. The existing Shell icon loader already supports a synthetic shared-folder request through `SHGetFileInfoW`, and the UI already owns bounded memory/disk caching, request deduplication, theme/DPI keys, and generation-aware event handling.

## Goals / Non-Goals

**Goals:**

- Preload a Shell-provided generic folder icon at application initialization.
- Prefer the exact Shell icon for each breadcrumb location and replace fallback textures asynchronously.
- Apply one fallback rule to the root, visible ancestry, overflow, and child menus.
- Preserve responsive rendering, bounded caches, DPI/theme correctness, and existing breadcrumb interaction behavior.
- Add deterministic tests and headful evidence.

**Non-Goals:**

- Synchronous Shell or GDI work on the GPUI thread.
- Replacing the shared icon service, changing thumbnails, or changing breadcrumb layout.
- Retaining Fluent, SVG, emoji, or application-drawn breadcrumb fallbacks.

## Decisions

### Reuse the shared base-folder acquisition contract

The generic breadcrumb key uses the existing synthetic folder marker, the current association generation, zero overlay generation, the current DPI, and theme. These fields make the Shell loader select `SHGFI_USEFILEATTRIBUTES | FILE_ATTRIBUTE_DIRECTORY`, yielding the Windows-configured generic folder icon without requiring a real path. A new direct Win32 loader in the UI was rejected because it would duplicate audited GDI ownership and block the UI thread.

### Submit the generic request with navigation initialization

`submit_navigation_icon_loads` includes the generic key from the first root initialization and deduplicates it through `pending_icon_keys`. Theme, DPI, or association changes derive a new key and schedule a fresh request. The response is stored in the ordinary visible Shell texture cache, so memory and disk limits remain centralized.

### Pass an optional generic texture to every breadcrumb surface

The icon renderer receives a concrete optional texture and a generic optional texture. It returns the concrete texture first, the generic texture second, or a fixed-size empty slot last. It never calls `navigation_icon`. This avoids visual flashes from invented artwork while keeping segment alignment stable on the first-ever frame.

### Keep concrete location requests unchanged

Ancestry and child-menu events continue to schedule concrete location keys. The distinct generic key cannot collide with This PC, a drive, a customized folder, archive, or namespace identity. A late concrete result naturally wins on the next render without explicit replacement state.

### Keep navigation drive identity stable across cache epochs

The navigation pane resolves a location texture by exact key first, then by the newest compatible cached key for the same location, DPI, and theme. This is required because opening This PC can load a newer drive texture for the file view and legitimately evict the navigation row's zero-epoch key. Ordinary navigation-tree folders deliberately use the generic Shell folder texture; they never fall back to `NavigationIcon::Folder`. If no Shell texture exists yet, drive and folder rows reserve the normal icon slot rather than drawing a misleading placeholder.

## Risks / Trade-offs

- [The first-ever frame can precede the generic Shell result] → Reserve the icon slot and submit the request during root initialization; later runs can use the validated disk cache.
- [A concrete handler can fail or hang] → Existing STA deadlines and failure events leave the generic texture visible without blocking GPUI.
- [Association or DPI changes can show stale artwork] → Include association generation, DPI, and theme in the generic key and reuse invalidation paths.
- [Broad renderer signature changes can miss a breadcrumb surface] → Centralize fallback in one helper and add structural assertions for root, ancestry, overflow, and child menus.

## Migration Plan

No persisted schema migration is required. Deploy the generic initialization request, then switch rendering to the Shell-only fallback helper. Rollback restores `NavigationIcon` fallback calls and removes the generic request; any cached generic bitmap remains harmless and bounded.

## Open Questions

None. The user approved asynchronous Shell-native rendering with a Shell-provided generic fallback and no application-drawn breadcrumb icon.
