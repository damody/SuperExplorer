# Restored Remote Folder Icon Fallback Design

## Problem

When the application restores a tab whose current location is an ADB/SFTP path, remote rows deliberately skip Windows Shell icon requests. The file view nevertheless receives the already requested generic Windows Shell folder texture for navigation surfaces. Its row renderer ignores that texture when an item-specific texture is absent and immediately draws the simplified vector placeholder. After returning to a local directory, that placeholder remains visible until every item-specific Shell request completes or recovers from transient admission pressure.

## Decision

Use one explicit file-row icon selection order:

1. the item-specific Shell icon or thumbnail;
2. the generic Windows Shell folder texture when the entry is a container;
3. the existing vector folder/document placeholder.

The generic texture is already requested by the navigation icon pipeline and included in the render snapshot. The file view will identify it once per render and clone the shared texture only for container rows missing a specific visual. A later item-specific texture always wins without cache invalidation.

## Alternatives Considered

- Replace the vector placeholder artwork. Rejected because it would hide the missing Shell visual rather than reuse the authentic texture already available.
- Clear all icon caches when switching between remote and local providers. Rejected because it introduces avoidable reloads, flicker, and loss of valid navigation/file textures.
- Submit Windows Shell requests for remote rows. Rejected because remote virtual identities are not Windows Shell namespace items and are intentionally unsupported by that service boundary.

## Boundaries

- File rows only; breadcrumb and navigation fallback behavior remains unchanged.
- Containers may use the generic Shell folder texture; non-container files must never use it.
- No session schema, provider routing, cache key, or drag/navigation behavior changes.

## Verification

- Unit-test the selection helper for specific-first, container-generic, and non-container rejection behavior.
- Verify the generic folder texture remains present in the render snapshot after a restored remote tab initialization.
- Run focused explorer-ui tests, formatting, and `cargo check -p explorer-ui`.

## Rollback

Restore the file-row renderer's direct item-specific lookup. No data migration is required.
