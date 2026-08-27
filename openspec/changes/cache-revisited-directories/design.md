## Context

`TabState::begin_navigation_request()` currently starts `DirectoryState::Loading` with an empty snapshot. Refresh alone preserves rows because it is reloading the same location. Every navigation entry point eventually creates a normal Navigate request, so revisiting a recently completed Local, ADB or SFTP location still blanks the file view until new batches arrive.

The existing `DirectoryState` merge and finish rules already implement the revalidation half: batches upsert rows and terminal completion removes rows not present in `seen`. This change adds a target-location snapshot source and bounded ownership without moving provider I/O onto the UI thread.

## Goals / Non-Goals

**Goals:**

- Present a previously completed Local, ADB or SFTP snapshot on the first render after navigation starts.
- Always submit the authoritative background Navigate and converge through correlated batches.
- Share recent snapshots across tabs in one window with deterministic memory bounds.
- Keep failed, cancelled and stale requests from replacing valid cache entries.
- Cover all navigation entry points through one state boundary.

**Non-Goals:**

- Persisting cached rows between processes or sessions.
- Caching Shell namespace, Known Folder or synthetic-root enumeration.
- Adding TTL-based blanking, provider protocol changes or offline-mode guarantees.
- Changing sorting, view settings, selection persistence or filesystem operation semantics.

## Decisions

### Cache ownership is AppViewState

`AppViewState` owns one `DirectorySnapshotCache`, allowing all tabs to share recently completed directories while keeping the cache within the existing UI-state lifecycle. Provider services remain authoritative and unaware of presentation caching.

Alternative: one cache per provider. Rejected because it duplicates policy across Local, ADB and SFTP and cannot seed the model before provider I/O returns.

### Stable string-like cache keys

The cache derives an internal `DirectoryCacheKey` only for supported locations. Local paths are normalized for Windows case-insensitive comparison and trailing separators. Virtual paths use normalized provider ID, public authority and components while excluding `entry_id`, opaque container identity and generation. This lets the same canonical ADB/SFTP URI hit across new enumeration descriptors without conflating different authorities or paths.

### Seed Loading with target snapshot

The model gains a navigation-start method that accepts an optional target snapshot. A cache hit seeds `DirectoryState::Loading`; a miss uses an empty snapshot. `seen` starts empty in both cases. Selection clears at navigation start. Existing generation, request ID and cancellation validation remains the only gate for batches and completion.

All navigation helpers obtain the target first and call one AppViewState boundary that looks up the cache, starts the correct history or direct request with that snapshot, and returns the ordinary `ExplorerCommand::Navigate`. No entry point can opt out accidentally.

### Write only after accepted successful completion

Before applying `DirectoryFinished`, AppViewState captures the request's resolved current target. After `ExplorerWindowState::apply_event` accepts the event and the tab holds `DirectoryState::Ready`, the final snapshot is inserted. Failed, cancelled, stale or unrelated events never insert. The cache is not eagerly invalidated by operations or watchers because every cache hit immediately revalidates.

### Deterministic dual-bound LRU

The cache stores at most 64 directories and 100,000 total entries. Get and insert update a monotonically increasing access sequence. Eviction removes the smallest sequence until both limits hold. A single snapshot above the item limit is rejected without evicting existing entries. Sequence overflow is handled by compacting ranks.

## Risks / Trade-offs

- **[Stale rows are briefly visible]** → Loading state indicates background work and successful completion removes unseen rows.
- **[Remote descriptor identities vary]** → Cache keys deliberately use canonical public provider/authority/components only; tests cover authority and path isolation.
- **[Large snapshots consume memory]** → Enforce directory and aggregate item limits and reject oversized single entries.
- **[Failure could hide useful cached content]** → Existing recoverable Error state retains the seeded previous snapshot.
- **[A missed entry point reintroduces blanking]** → Centralize cache seeding in shared AppViewState navigation helpers and test direct, history and up navigation.

## Migration Plan

No durable migration is required. Introduce the model seed API, then the UI cache and routing. Rollback removes the cache and returns navigation to empty snapshots without affecting persisted sessions or providers.

## Open Questions

None. Limits and supported location classes are fixed by the approved design.
