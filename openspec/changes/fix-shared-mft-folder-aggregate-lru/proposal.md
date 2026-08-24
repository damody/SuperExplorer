## Why

Folder aggregate requests currently pass through a Host-owned snapshot cache and a single Host worker before reaching the MFT Service. This duplicates ownership across Super Explorer processes, leaves an unbounded obsolete disk cache, and lets slow work keep visible `Folder size` cells at `Calculating...` even though the MFT Service already owns the optimized volume and SQLite indexes.

## What Changes

- Route Details Folder size, File Count, and Folder Count requests directly to the installed MFT Service without a Host aggregate cache or recursive Host fallback.
- Make the service result cache a true shared LRU with promotion, accurate bounded accounting, immediate trimming, and byte plus entry-count limits.
- Preserve unaffected warm results across journal updates while invalidating changed folders and ancestors before advancing the volume cache generation.
- Coalesce concurrent same-folder, same-generation misses across Super Explorer clients into one service computation.
- Guarantee a terminal exact, partial, unavailable, timeout, or cancelled UI outcome instead of an indefinite loading state.
- Retire the obsolete Host Details snapshot namespace through bounded, path-validated cleanup while keeping Size Map projection caching isolated.
- Extend privacy-safe service diagnostics for LRU, single-flight, invalidation, and result-source verification.
- Keep existing Code Lines `Limit` admission behavior unchanged.

## Capabilities

### New Capabilities

- `shared-mft-folder-aggregates`: Defines the MFT Service as the single shared owner of Details folder aggregates, its bounded result LRU and invalidation rules, direct Host query behavior, migration, observability, and terminal failure semantics.

### Modified Capabilities

None.

## Impact

- `crates/explorer-app/src/bin/mft_service.rs`: service-global result LRU, journal invalidation, single-flight coordination, diagnostics, and focused tests.
- `crates/explorer-app/src/mft_query.rs`: versioned request/response or diagnostic contract adjustments needed for shared result behavior.
- `crates/explorer-app/src/application.rs` and `crates/explorer-app/src/folder_size_service.rs`: direct Details queries, removal of Host aggregate caching/fallback, current-view cancellation, and obsolete-cache maintenance.
- `crates/explorer-ui`: terminal projection checks only if existing partial/unavailable rendering cannot express the required outcomes.
- Installed Windows MFT Service and Super Explorer focused validation against `D:\trace`; no new dependency, network service, or source-filesystem mutation.
- The obsolete Host cache stops being a supported Details aggregate source. This is an internal cache migration, not a public extension ABI break.
