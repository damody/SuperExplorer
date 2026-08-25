## Why

Folder aggregate requests currently pass through a Host-owned snapshot cache and a single Host worker before reaching the MFT Service. This duplicates ownership across Super Explorer processes, leaves an unbounded obsolete disk cache, and lets slow work keep visible `Folder size` cells at `Calculating...` even though the MFT Service already owns the optimized volume and SQLite indexes.

## What Changes

- Route Details Folder size, File Count, and Folder Count requests directly to the installed MFT Service without a Host aggregate cache or recursive Host fallback.
- Make the service result cache a true shared LRU with promotion, accurate bounded accounting, immediate trimming, and byte plus entry-count limits.
- Preserve unaffected warm results across journal updates while invalidating changed folders and ancestors before advancing the volume cache generation.
- Coalesce concurrent same-folder, same-generation misses across Super Explorer clients into one service computation.
- Treat the configured live-index budgets as an active-volume working set: page non-active volume indexes out of memory and recover the queried volume exactly instead of immediately returning partial because C, D, and E do not all fit simultaneously.
- Guarantee a terminal exact, unavailable, timeout, or cancelled UI outcome within ten seconds; partial aggregate values are never displayed as folder sizes.
- Schedule visible folder queries with bounded parallelism so one slow folder cannot block all later rows.
- Submit visible folder requests through a bounded batch IPC stream; the service shares one volume recovery, computes independent folders concurrently, and returns each item as soon as it finishes.
- Forward and refresh cache telemetry through service decorators so Folder Options shows measured current usage or confirmed `Unavailable`.
- Visually group and label the five MFT Service-owned resource budgets so their shared service ownership and restart persistence are unambiguous.
- Retire the obsolete Host Details snapshot namespace through bounded, path-validated cleanup while keeping Size Map projection caching isolated.
- Extend privacy-safe service diagnostics for LRU, single-flight, invalidation, and result-source verification.
- Keep existing Code Lines `Limit` admission behavior unchanged.

## Capabilities

### New Capabilities

- `shared-mft-folder-aggregates`: Defines the MFT Service as the single shared owner of Details folder aggregates, its bounded result LRU and invalidation rules, direct Host query behavior, migration, observability, and terminal failure semantics.

### Modified Capabilities

None.

## Impact

- `crates/explorer-app/src/bin/mft_service.rs`: service-global result LRU, journal invalidation, single-flight coordination, active-volume paging/recovery, diagnostics, and focused tests.
- `crates/explorer-app/src/mft_query.rs`: backward-compatible single-folder frames plus bounded batch request and completion-order response-stream framing.
- `crates/explorer-app/src/application.rs` and `crates/explorer-app/src/folder_size_service.rs`: direct Details queries, removal of Host aggregate caching/fallback, current-view cancellation, and obsolete-cache maintenance.
- `crates/explorer-ui`: exact-only terminal projection, `Unavailable` rendering, bounded retry removal, and Folder Options telemetry refresh/presentation.
- Installed Windows MFT Service and Super Explorer focused validation against `D:\trace`; no new dependency, network service, or source-filesystem mutation.
- The obsolete `%LOCALAPPDATA%\SuperExplorer\folder-snapshot-cache\v2` cache stops being a supported Details aggregate source. This is an internal cache migration, not a public extension ABI break.
