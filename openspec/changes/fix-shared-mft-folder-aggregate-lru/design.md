## Context

The installed LocalSystem MFT Service already owns the live NTFS topology, SQLite persistence, aggregate indexes, and a process-global result map. Details aggregate requests nevertheless enter `ApplicationVisualColumnRuntimeV1`, consult `HostExtensionColumnCacheV1`, serialize through one Host worker and `FolderSizeServiceV1`, and may fall back through non-service paths. The Host also persists snapshots in `%LOCALAPPDATA%\SuperExplorer\folder-snapshot-cache` without a bounded retirement policy.

The current service result map updates a `last_use` clock on hits, but its accounting is the direct `size_of` of a tuple, it has no entry-count ceiling, and its generation behavior does not preserve provably unaffected warm results explicitly. Multiple application processes reach the same named-pipe service, so placing the complete cache contract there gives all processes the same optimization and eliminates duplicated Host caches.

The approved source design is `docs/superpowers/specs/2026-08-24-mft-service-shared-folder-aggregate-lru-design.md`. This change is Windows-only, adds no dependency or network endpoint, must not mutate source filesystem content, and uses focused verification rather than the complete workspace test suite.

## Goals / Non-Goals

**Goals:**

- Make the MFT Service the single long-lived owner of Details Folder size, File Count, and Folder Count results.
- Share optimized memory-index, SQLite, aggregate-build, single-flight, and LRU behavior across every Super Explorer process.
- Implement deterministic true-LRU promotion and hard byte plus entry limits without coupling result eviction to other MFT stores.
- Preserve unaffected results safely across journal generations and reject every stale publication.
- Ensure every non-obsolete visible request reaches an exact, partial, or explicit unavailable terminal state.
- Retire the obsolete Host snapshot namespace incrementally and safely.
- Prove the feature with focused automated checks and installed `D:\trace` validation.

**Non-Goals:**

- Changing Code Lines File Count admission thresholds or red `Limit` presentation.
- Replacing the Size Map tree projection cache with the aggregate result LRU.
- Adding a new Folder Options setting; the existing MFT Service LRU byte setting remains authoritative.
- Persisting the result LRU across service restarts; SQLite remains the durable optimized source.
- Running the entire workspace test suite.
- Changing public extension ABI contracts or supporting non-NTFS aggregate fallback.

## Decisions

### 1. Details queries bypass Host aggregate caches

`ApplicationVisualColumnRuntimeV1` will use a Details-specific direct service query path. The Host retains only bounded pending identities, cancellation state, and result projection. It will not consult or populate `HostExtensionColumnCacheV1<FolderSizeCachedValueV1>` and will not call recursive or Everything fallback for Details facts.

`FolderSizeServiceV1` remains available to Size Map tree consumers, but aggregate-only APIs and persistent writes are removed from the Details path. This separates UI tree projection ownership from service database ownership.

Alternative rejected: retaining a small Host L1 cache. Even a bounded L1 duplicates invalidation and prevents different Super Explorer processes from observing one shared recency order.

### 2. Result cache keys survive safe journal updates

The retained result key is volume identity plus folder file reference. Each volume owns a cache generation, and each entry records the latest generation through which it was proved valid. Journal application computes changed references and their ancestors from the pre/post-apply topology, invalidates those result keys, and only then advances the volume cache generation. Remaining entries are carried forward as unchanged.

If ancestor closure cannot be proven, all result entries for that volume are cleared before generation advance. Request single-flight identities include the observed generation so a computation started against an old snapshot cannot publish into the new cache.

Alternative rejected: including generation directly in every retained-result key. That makes every journal event cold-start the volume and contradicts the shared warm-database goal.

### 3. True LRU uses monotonic access sequence and conservative cost

Each entry stores the aggregate response, valid-through generation, accounted cost, and last-access sequence. A successful hit or replacement advances the service-global wrapping sequence with zero skipped. Eviction selects the smallest sequence; tests force promotion and replacement order.

The minimum accounted entry cost is 192 bytes and actual accounting is at least that amount while including key, value, and bounded container overhead. The entry ceiling is `max(1, min(effective_lru_bytes / 192, 262_144))`. Insert and limit changes trim until both byte and entry constraints pass. A non-retainable entry is returned to the requester but not cached.

Alternative rejected: direct `size_of` accounting only. It excludes allocator/map overhead and lets a large byte limit admit an unreasonable number of tiny records.

### 4. Same-key misses are service-global single-flight work

The service maintains an in-flight registry keyed by volume identity, folder reference, and observed generation. The first requester is leader; later request handlers wait for the same terminal result without holding the cache or live-volume mutex. Leader completion publishes once to waiters, conditionally inserts only if the generation is still current, and always removes the in-flight record. Panic/error/disconnect paths terminalize waiters and cannot leave a permanent registry entry.

Alternative rejected: relying on the existing cache mutex to serialize queries. It blocks unrelated folders and does not express joined work or stale-publication rules.

### 5. Optimized source order remains inside the service

On a result miss the service prefers an already-built live aggregate index, then a read-only SQLite aggregate query when the durable cursor matches the current journal, and builds bounded missing aggregate state only when required. It never opens user file contents. Source counters distinguish memory, SQLite, and build paths.

After service restart the result cache is cold by design. SQLite produces the first optimized answer, which warms the LRU for later clients.

### 6. Host terminal semantics are explicit

The existing bounded request context and stale-generation checks remain. A complete response displays exact facts; typed partial uses existing partial rendering; malformed response, service unavailable, or the existing aggregate response timeout publishes unavailable; cancellation silently ends only the obsolete context. UI loading state is removed once any non-cancelled request terminates.

No recursive Host fallback masks service failure. Recovery occurs through refresh or a later visible retry after service reconnection.

### 7. Obsolete Host cache cleanup is bounded and path-safe

Startup maintenance resolves the exact `%LOCALAPPDATA%\SuperExplorer\folder-snapshot-cache` root without following symlinks or reparse points. It examines only immediate regular files matching the obsolete snapshot record shape and removes at most 256 oldest files per launch. Directories, links, unexpected records, and anything outside the validated root are skipped. Cleanup failure is logged without blocking startup or aggregate queries.

Size Map projections use a distinct namespace; the cleanup cannot target it. Rollback remains safe because old binaries can rebuild Host snapshots, although their prior cache may have been incrementally retired.

### 8. Diagnostics extend the existing versioned frame

The MFT diagnostic contract adds or reuses versioned fields for result entries/bytes/limits, hit/miss/insert/replace/eviction, single-flight leaders/joined waiters, stale rejections, and source selection. The endpoint remains backward-safe: unsupported versions fail validation rather than shifting existing fields. Diagnostics contain no paths or names.

### 9. Evidence-driven correction rules

- **A — task refinement:** task split, ordering, focused command, or evidence-file changes that preserve scope, requirements, thresholds, and public contracts may update `tasks.md` and the evidence index.
- **B — design/spec correction:** an implementation finding within approved scope pauses affected work, updates proposal/design/spec/tasks together, marks dependent evidence stale, and re-runs strict validation.
- **C — material change:** changing cache limits/formula, fallback policy, supported platform, permissions, destructive targets, required evidence, or public contract requires user approval before work continues.

Blocking gates and focused evidence requirements cannot be weakened through A- or B-level adjustments.

## Risks / Trade-offs

- [Direct service dependency exposes outages instead of hiding them with recursive scans] → Publish explicit unavailable terminal state and retry only through bounded refresh/reconnect behavior.
- [Fine-grained ancestor invalidation could miss a topology edge case] → Clear the volume result cache whenever complete closure is not proven; never retain a questionable exact result.
- [Single-flight waiters could deadlock behind a leader] → Never wait while holding shared cache/index locks, use terminal cleanup on every leader exit, and test failure/disconnect paths.
- [Linear minimum-sequence eviction is O(n)] → The 262,144 entry ceiling bounds the scan; implementation may use a heap/list only if it preserves the exact observable LRU contract.
- [192-byte accounting is conservative and may underuse the configured byte budget] → Prefer a stable upper safety bound over allocator-dependent precision; keep formula explicit and tested.
- [Incremental old-cache cleanup leaves files for several launches] → The namespace is no longer read, removal is capped at 256 files per launch, and leftover files cannot affect correctness.
- [Existing dirty worktree overlaps application code] → Apply must inspect and preserve unrelated user changes, limiting edits to the approved paths and reviewing the final diff.

## Migration Plan

1. Add focused service LRU/invalidation/single-flight behavior and tests behind the existing versioned IPC boundary.
2. Extend diagnostics and verify old/invalid frames fail safely.
3. Switch Details aggregate requests to the direct service path and remove Host cache/fallback participation.
4. Add bounded obsolete-cache maintenance with path-safety tests.
5. Run focused crate tests and installed `D:\trace` cold/warm multi-process validation.
6. Roll forward by installing the updated app and service together. Rollback installs the prior matched app/service pair; SQLite is unchanged and old Host caches can be rebuilt.

## Open Questions

None. Thresholds, ownership, fallback policy, migration limit, and verification scope are fixed by the approved design.

