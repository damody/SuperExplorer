# Shared Folder Size Service Design

## Goal

Move folder-size measurement out of individual extensions into a host-owned service. Folder Size, Size Map, and future extensions consume one generation-safe snapshot instead of starting independent filesystem scans, caches, or elevation flows.

## User-visible contract

- Folder Size and Size Map remain independently switchable, but share measurement work whenever both are enabled.
- The first local-NTFS request that needs a fresh MFT index may launch one UAC prompt. The main application never runs elevated.
- Rejecting UAC, an unavailable Everything service, or a failed optimized scan never disables the feature; the service falls back to a bounded recursive scan.
- Directory reparse points are represented but never recursively followed. This matches Explorer-safe behavior and prevents cycles, cross-volume expansion, and double counting.
- Correct live-filesystem semantics take priority over a faster index whose result cannot satisfy the same boundary rules.

## Architecture

`FolderSizeService` is an application-owned worker and cache. Consumers submit a root identity, request generation, and data shape. Identical live requests are coalesced. The service selects a backend, publishes progressive snapshots, and rejects stale completion by generation.

Consumers request one of two projections:

- `FolderAggregateSnapshot`: recursive bytes, direct bytes, file count, directory count, status, and diagnostics for each requested row.
- `FolderTreeSnapshot`: stable node ID, parent ID, name, kind, direct bytes, recursive bytes, and node status for Size Map rendering.

Both projections derive from the same internal tree/index. A tree request can satisfy aggregate consumers without another scan.

## Backend policy

1. Return a valid in-memory or disk snapshot.
2. For a local NTFS volume, request an MFT index through the existing elevated helper. Elevation occurs lazily on first real demand and is coalesced per volume.
3. If elevation is declined, MFT is unavailable, or projection is incomplete, query Everything when its IPC service and adjacent SDK are available.
4. Validate Everything results against the canonical root and reparse-point policy. If validation cannot establish equivalent semantics, discard that result.
5. Fall back to a cancellable, bounded recursive scan.

The selected method is diagnostic metadata, not part of consumer behavior. Backends must produce the same snapshot schema.

## Cache and invalidation

- Memory cache retains active-tab roots and a bounded recent-root LRU.
- Disk records are schema-versioned and keyed by volume identity, root identity, and semantic policy.
- MFT records carry a USN journal checkpoint when available; watcher/USN changes invalidate or incrementally refresh affected ancestors.
- Everything records are treated as index-derived and are validated before publication.
- Recursive records are invalidated by watcher generation and source identity changes.
- A snapshot remains alive while any consumer holds the matching root/generation. Disabling one extension cannot remove data still used by another.

## Extension boundary

- Measurement is removed from the visual-column responsibility. `VisualColumnImplementationV1` renders only.
- Contributions declare host data requirements such as `folder.aggregate` or `folder.tree`.
- Folder Size receives aggregate values in its render context.
- Size Map receives the host-owned tree snapshot and only computes layout/render plans.
- Capability validation prevents extensions from requesting arbitrary filesystem scans through this service.

During ABI migration, the host may retain a compatibility adapter for older development fixtures, but built-in packages use the host service and no longer implement recursive measurement.

## Concurrency and failure behavior

- One in-flight scan exists per compatible root and semantic policy.
- Aggregate and tree subscribers share progress, cancellation, and terminal state.
- Navigation, tab closure, watcher generation changes, and shutdown release consumers and reject stale publication.
- Resource limits produce an explicit partial snapshot, never a false exact zero.
- Backend failure records are diagnostic and bounded; fallback continues automatically.

## Verification

- Unit tests cover backend selection, UAC decline, reparse points, cache invalidation, generation rejection, and consumer coalescing.
- Integration tests enable Folder Size and Size Map together and assert one physical scan with two projections.
- Extension tests prove renderers have no filesystem measurement implementation.
- UITEST enables/disables both extensions independently, verifies equal shared values, cancellation, fallback, clean shutdown, and screenshots.
- A profiling test records cold/warm timings and result equality for MFT, Everything, and recursive backends. The observed baseline for `D:\SuperExplorer` is Everything 1.01–1.37 seconds versus recursive 6.38–6.50 seconds; MFT requires UAC in the tested non-elevated process.

## Non-goals

- Running the main process elevated.
- Following directory junctions or symbolic links.
- Making one extension depend on another extension being enabled.
- Exposing raw MFT or Everything APIs directly to extensions.
