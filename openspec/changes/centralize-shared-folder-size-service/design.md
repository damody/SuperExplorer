## Context

Folder Size currently dispatches a visual-column measurement callback whose fixture owns recursion and a disk cache. Size Map separately asks the application to construct a recursive tree and already has an MFT fast path. Everything is packaged for search but is not a folder-size backend. These paths duplicate I/O and disagree about lifecycle, cache identity, reparse points, and elevation.

The approved source design is `docs/superpowers/specs/2026-08-05-shared-folder-size-service-design.md`. The implementation targets Windows, keeps GPUI and the main process non-elevated, preserves independent feature toggles, and must coexist with temporarily installed legacy development fixtures.

## Goals / Non-Goals

**Goals:**

- Produce aggregate and tree projections from one host-owned, generation-safe snapshot.
- Select cached, MFT/UAC, Everything, or recursive backends without changing consumer behavior.
- Guarantee canonical-root containment and no directory-reparse recursion for every backend.
- Coalesce concurrent Folder Size and Size Map demand and retain data until the final consumer releases it.
- Make extension renderers data-only and capability-bound.
- Preserve partial progress, cancellation, stale rejection, accessibility, packaging, and clean shutdown.

**Non-Goals:**

- Elevating the application process.
- Exposing raw MFT, Everything, paths, or native handles to extensions.
- Following directory junctions/symlinks or making plugins depend on each other.
- Silently accepting a faster backend whose result differs from recursive reference semantics.

## Decisions

### One normalized internal tree

`FolderSizeService` owns normalized nodes keyed by stable filesystem identity plus a root-relative identity. Each node stores parent, name, kind, direct logical bytes, recursive logical bytes, status, and generation. Aggregate rows and Size Map nodes are projections of this tree. A tree snapshot satisfies aggregate subscribers; an aggregate-only request may use a cheaper backend projection when it remains upgradeable or is invalidated before a later tree request.

Alternative rejected: sharing only a utility crate. It would retain duplicate jobs, prompts, caches, and inconsistent terminal states.

### Lazy per-volume MFT elevation

The existing helper remains the only elevated component. The first uncached demand on a local NTFS volume starts one coalesced UAC request. Helper output stays in the validated user Temp location, uses a versioned bounded binary format, and is canonicalized, size-bounded, parsed, and removed by the non-elevated host. User decline, timeout, malformed output, journal mismatch, or projection failure falls through without faulting consumers.

Alternative rejected: elevation at startup or elevating the app. Both violate least privilege and Explorer-like launch behavior.

### Backend equivalence gate

Backends implement one adapter contract and emit method diagnostics. MFT and Everything results are compared against deterministic recursive fixtures covering reparse points, hard links, inaccessible subtrees, mutation, and root boundaries. A backend is eligible only when its adapter passes equality gates. Everything query results require canonical root-prefix validation and filtering; stale/missing results trigger recursive fallback.

Hard links retain the existing logical per-directory-entry default. Directory reparse points are represented with zero descendant contribution and are not followed.

### Cache and invalidation

Memory cache is a bounded LRU keyed by volume/root identity, semantic policy, and schema. Disk records add backend data version and journal/watcher checkpoint. Active consumer leases pin snapshots. Watcher changes invalidate affected ancestors; F5/manual refresh advances generation. A USN-backed incremental refresh may update MFT snapshots, but inability to prove continuity invalidates the snapshot and rebuilds it.

### ABI migration

Add declarative contribution data requirements (`folder.aggregate`, `folder.tree`) and host-provided snapshot values. The official Folder Size package becomes render-only; Size Map keeps its data-only layout callback. The old visual-column measure callback is first gated behind a compatibility adapter for legacy local fixtures, excluded from official package registration, then removed with an ABI/schema/fingerprint bump after all in-tree consumers migrate.

### QoS and observability

The service uses bounded queues, visible-root priority, cancellation tokens, progressive batches, and coalesced UI invalidation. Counters expose backend attempts, fallback reason, physical scans, subscribers, nodes, elapsed time, cache hit, partial state, and stale rejection without recording user paths.

Blocking performance gates compare equal-result methods on deterministic fixtures and record an informational local profile. No fixed machine-time promise is public; the release gate requires no duplicate physical scan for compatible concurrent consumers and no regression beyond the existing recursive reference on the same fixture/run environment.

### Aggregate-first MFT projection and visible backend status

Folder Size uses a dedicated aggregate projection. Loading a service MFT index precomputes recursive totals once per volume and retains them with the index, so subsequent folder rows are constant-time lookups and do not construct paths, call per-descendant filesystem metadata APIs, or materialize Size Map nodes. The precomputation partitions independent volume-root subtrees across at most eight worker threads. Size Map remains the only consumer that requests the bounded normalized tree projection.

The status bar shows the Host-observed Folder Size source at its right edge: `Host cache`, `MFT service`, or `Recursive scan`, with an ellipsis while work is active. If concurrent work uses mixed sources, an active recursive fallback takes precedence because it explains the visible wait. The status is diagnostic UI state only and does not affect backend selection.

## Risks / Trade-offs

- **UAC fatigue** → Prompt lazily once per volume demand, coalesce prompts, cache valid snapshots, and fall back after decline.
- **Everything index differs from live traversal** → Enforce containment/reparse/existence validation and equality fixtures; otherwise discard and recurse.
- **MFT format or USN discontinuity** → Version and bound helper output; rebuild on any unproven continuity.
- **Large snapshot memory** → Bounded node count, compact IDs, progressive partial states, LRU eviction, and consumer leases.
- **ABI break strands local plugins** → Compatibility adapter plus explicit diagnostics and fixture migration before fingerprint bump.
- **Dirty worktree integration conflicts** → Restrict edits to owned service/contract/consumer seams, preserve unrelated changes, and use focused diff review.

## Migration Plan

1. Introduce internal snapshot types, backend trait, reference recursive adapter, coalescing, leases, and tests without changing consumers.
2. Adapt the existing MFT helper and add a validated Everything adapter behind equivalence gates.
3. Route Size Map through the service while preserving its public render snapshot.
4. Route Folder Size through aggregate snapshots; migrate the official package to render-only.
5. Add declarative requirements, compatibility diagnostics, ABI/schema/fingerprint and packaging updates.
6. Run lifecycle, fallback, performance, installer, and headful gates. Rollback selects the recursive adapter and compatibility path without reverting persisted user settings.

## Evidence-driven adjustment policy

- **A — task refinement:** commands, splits, order, or ownership may change without altering contracts or gates.
- **B — design/spec correction:** an in-scope backend or lifecycle assumption disproved by evidence pauses affected work; design, specs, tasks, and stale evidence are updated and revalidated.
- **C — material change:** scope, public ABI commitment, semantic policy, elevation boundary, blocking gate, or required evidence change requires user approval.

## Open Questions

No unresolved product decisions remain. Backend eligibility is deliberately evidence-gated; an unavailable or non-equivalent MFT/Everything adapter reaches a tested fallback disposition rather than blocking correctness.

## Evidence-backed correction (2026-08-06)

Installed-build inspection found SCM error 1060 for `SuperExplorerMft`, while the installer had continued after unchecked `sc.exe` failures. The same run showed an Everything-derived shallow tree being marked complete, so directory-only rows became exact `0 B`. This is an in-scope B-level correction: the service installation, accelerated-backend completeness, consumer integration, packaging, and final evidence tasks are reopened.

The installer SHALL check service create/configure/start exit codes and poll SCM until `RUNNING`; failure aborts installation with a useful diagnostic. The main executable remains non-elevated and the service runs as LocalSystem. MFT projection SHALL fail closed when bounded output truncates or cannot prove a complete subtree. Everything remains ineligible for exact snapshots until recursive-equivalence and subtree-completeness evidence exists. An exact zero may be cached or rendered only from a complete recursive traversal or an accelerated snapshot carrying equivalent completeness proof.
