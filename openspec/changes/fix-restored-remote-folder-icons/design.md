## Context

Remote ADB/SFTP entries are not Windows Shell namespace items and intentionally skip per-item Shell requests. Independently, `submit_navigation_icon_loads` requests a generic Windows Shell folder texture and `navigation_icon_snapshot` includes it. `FileViewContent` currently performs only an exact item-key lookup and falls directly to a vector placeholder on a miss, leaving the authentic generic texture unused.

## Goals / Non-Goals

**Goals:**

- Show an authentic generic Windows Shell folder icon whenever a container lacks its specific visual.
- Preserve specific Shell icons and thumbnails as the highest-priority visuals.
- Make restored remote startup and later local navigation visually stable without cache resets.
- Prevent files from receiving a folder visual.

**Non-Goals:**

- Requesting Windows Shell icons for remote virtual entries.
- Changing provider routing, icon cache keys, or session restore contracts.
- Changing non-container fallback artwork.

## Decisions

At the start of file-view rendering, locate the generic folder texture in the supplied Shell texture map using the existing `is_generic_breadcrumb_folder_icon_key` predicate. Select each row visual through a pure helper: return the specific texture when present; otherwise return the generic texture only for a container; otherwise return none so the existing vector fallback renders.

This preserves the cache and request pipeline. The generic texture is shared through `Arc`, and lookup occurs once per render rather than once per row. A specific icon arriving on a later frame automatically supersedes the generic fallback.

Clearing caches on provider changes was rejected because valid textures are provider-independent and cache invalidation would add flicker. Sending remote identities to the Shell service was rejected because they are intentionally outside that namespace boundary.

## Risks / Trade-offs

- [Risk] A custom local folder briefly shows a generic folder while its specific icon loads. → Mitigation: this is preferable to the vector placeholder and the specific texture remains first priority.
- [Risk] Generic texture is not yet loaded. → Mitigation: retain the current vector fallback as the final bounded fallback.
- [Risk] Files accidentally receive the folder texture. → Mitigation: gate generic selection on `is_container` and cover the boundary with a unit test.

## Migration Plan

No migration is required. Deploy with the normal application build. Rollback restores direct item-specific lookup in the row renderer.

## Open Questions

None.
