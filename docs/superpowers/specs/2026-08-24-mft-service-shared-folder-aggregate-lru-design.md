# MFT Service Shared Folder Aggregate LRU Design

## Goal

Make the installed MFT Service the single optimized folder-aggregate database for every Super Explorer process. Details Folder size, File Count, and Folder Count requests must go directly to the service so all windows and processes share its index, aggregate work, single-flight requests, and result LRU. The Host must not maintain a competing long-lived folder-aggregate cache or fall back to recursive filesystem scanning.

## Scope

This change covers the aggregate facts used by Details columns:

- recursive folder size;
- recursive file count;
- recursive descendant-folder count.

Size Map tree projections may retain their own short-lived bounded projection cache, but that cache must not intercept or answer Details aggregate requests. Code Lines admission behavior is unchanged: red `Limit` cells continue to mean the existing File Count admission rule rejected or could not admit the folder.

## Architecture

The MFT Service is the only owner of long-lived folder aggregate results. Each Super Explorer process keeps only current UI request, cancellation, and projection state. It does not read or write `%LOCALAPPDATA%\SuperExplorer\folder-snapshot-cache` for Details aggregates and does not use a Host memory cache before querying the service.

Each aggregate request identifies the volume, folder file reference, and requested facts. The service resolves the request against its current observed journal generation. A retained result is keyed by volume identity and folder reference and contains folder size, file count, folder count, completeness, and the latest generation through which the service proved it unchanged. The service invalidates affected keys before advancing the volume cache generation, allowing unaffected results to remain warm safely across journal updates.

Cross-restart optimization comes from the service-owned SQLite MFT index. The in-memory result LRU intentionally starts cold after service restart; its first miss uses the optimized SQLite or in-memory aggregate index rather than walking user files. Successful results then warm the shared LRU for every connected Super Explorer process.

## Shared Service Data Flow

1. Super Explorer submits a bounded aggregate query directly to the MFT Service.
2. The service validates the volume and resolves its current journal generation.
3. The service checks the global result LRU for the volume and folder reference and verifies that journal invalidation has carried that entry safely through the current volume cache generation.
4. A hit promotes the entry using a service-global monotonic access sequence and returns it immediately.
5. Concurrent misses for the same key join one single-flight computation.
6. The computation prefers an already-built memory aggregate index, then a read-only SQLite aggregate query, and only builds missing aggregate state when required.
7. A complete or explicitly typed partial result is published to all joined waiters. Cacheable results enter the shared LRU and are subject to its hard limits.
8. Every response includes its service generation. The Host rejects responses for a cancelled request, an obsolete tab generation, or a superseded view.

No Details aggregate request uses recursive Host scanning as a fallback. This keeps expensive filesystem work centralized and lets all Super Explorer clients benefit from the same optimized data.

## LRU Semantics and Limits

The service result cache uses true least-recently-used semantics:

- every successful lookup advances a monotonic access sequence and promotes the entry;
- replacement of an existing key updates its value, generation proof, cost, and recency;
- insertion evicts the lowest access sequence first;
- lowering a configured limit trims immediately;
- accounting includes the map key, aggregate value, recency metadata, and a bounded per-entry overhead estimate;
- the cache enforces both its configured byte budget and a defensive entry-count ceiling, so a large byte budget cannot admit millions of tiny results;
- one entry that cannot fit is returned to its waiters but is not retained.

The entry-count ceiling is `min(effective_lru_bytes / 192, 262_144)`, with a minimum of one admissible entry. The 192-byte divisor is also the minimum accounted cost of a retained result even if its direct Rust payload is smaller. This rule is deterministic, testable, and automatically scales with the existing MFT Service LRU setting without adding another Folder Options field.

Evicting result entries never removes SQLite MFT index rows, the live volume index, file data, or aggregate index structures. Those stores remain governed by their existing independent budgets.

## Invalidation and Consistency

Journal application invalidates cached results for every changed folder and its known ancestors before advancing that volume's cache generation. Unaffected entries are then valid through the new generation and remain warm. An entry not proven valid through the current volume cache generation must never satisfy a lookup. When the service cannot prove the exact affected ancestor set, it clears the result entries for that volume rather than risking a stale exact value.

Single-flight state is generation-bound. A generation change prevents an old computation from entering the current LRU. Existing waiters receive a typed stale/partial/unavailable outcome or retry against the new generation; the old result is never presented as current.

Raising an LRU limit does not recreate evicted results. Future optimized queries repopulate them. Lowering the limit affects only result retention and must not mark the underlying MFT index incomplete.

## Host Behavior and Error Handling

The Host retains only bounded current-view request state. It deduplicates repeated UI submissions for the same item and generation, cancels obsolete view generations, and projects accepted service responses into the visible columns.

Every visible request must reach a terminal UI state:

- a complete response displays the exact aggregate;
- a typed partial response displays the existing partial presentation;
- service unavailable, malformed response, or response timeout displays an explicit unavailable result;
- cancellation ends the obsolete request without publishing into the new view.

Cache maintenance and telemetry failures degrade to a service cache miss. They must not prevent an aggregate query from using the live or SQLite index. Host recursive scanning is not used to hide service failures, and no row may remain indefinitely at `Calculating...` after its request has terminated.

## Migration and Existing Host Cache

The obsolete Host Details aggregate cache is no longer read or written. On startup, a versioned maintenance pass validates the exact `%LOCALAPPDATA%\SuperExplorer\folder-snapshot-cache` directory and removes at most 256 regular cache files per process launch, oldest first, until the obsolete namespace is empty across subsequent launches. Symlinks, reparse points, subdirectories, unexpected files, and targets outside that exact cache directory are never followed or removed. Failure to remove an old record is non-fatal and does not block service queries. No source directory, including `D:\trace`, is modified.

Tree projections still needed by Size Map must use a separate namespace and ownership boundary so migration cannot remove active Size Map data accidentally.

## Observability

MFT diagnostics expose enough privacy-safe counters to prove shared behavior:

- result LRU entries, accounted bytes, and effective limits;
- hit, miss, insertion, replacement, and eviction counts;
- single-flight leader and joined-waiter counts;
- stale-generation rejection count;
- memory-index, SQLite, and aggregate-build source counts.

Paths and file names are not emitted. Multiple Super Explorer processes observe the same service-global counters and cache state.

## Focused Verification

Only tests and checks directly related to this change are required; the complete workspace test suite is intentionally out of scope.

Focused unit and integration coverage must verify:

- lookup promotion and eviction of the true least-recently-used entry;
- simultaneous byte and entry-count enforcement, immediate trimming after a lower limit, and oversized-entry rejection;
- no mutation of SQLite or other MFT index stores during result eviction;
- invalidation of affected folders and ancestors, rejection of old-generation results, and safe volume fallback when the affected set is unknown;
- one single-flight computation for concurrent same-key requests from distinct clients;
- direct Host-to-service Details queries with no Host folder snapshot hit and no recursive fallback;
- terminal complete, partial, unavailable, timeout, cancellation, and stale-response behavior;
- service restart followed by an optimized SQLite result and subsequent shared LRU hit.

Installed focused validation opens `D:\trace` in Details view and confirms that every visible folder reaches an exact, partial, or explicit unavailable terminal state. A second Super Explorer instance queries the same folders and demonstrates shared warm-cache hits. Evidence records cold and warm latency, result LRU counters, entry/byte limits, eviction behavior, and MFT Service working set. Code Lines `Limit` presentation remains unchanged.
