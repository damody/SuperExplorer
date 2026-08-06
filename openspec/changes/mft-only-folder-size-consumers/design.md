## Context

SuperExplorer now installs `SuperExplorerMft` as an automatic LocalSystem Windows Service and can build an aggregate NTFS index with at most eight workers. The Host also owns a persistent data-column cache keyed by canonical path identity, modification timestamp, and schema. Folder size, Size Map, and the built-in Size column do not yet consume this capability through one invariant path: fallback scanners remain reachable and built-in Size renders only Shell file length.

The approved source design is `docs/superpowers/specs/2026-08-06-mft-only-folder-size-design.md`. Windows Explorer presentation conventions remain the UI baseline, but this product intentionally extends the built-in Size column to show recursive folder bytes when the fast Host value exists.

## Goals / Non-Goals

**Goals:**

- Make `FolderSizeServiceV1` the only provider of recursive folder bytes.
- Restrict population to persistent Host cache hits or MFT Service aggregate lookup.
- Share one result and invalidation contract across built-in Size, Folder size, and Size Map.
- Render and sort built-in Size using recursive folder bytes independently of extension enablement.
- Make unavailable behavior explicit, fast, and observable.
- Preserve the current eight-worker MFT aggregation bound and privileged service boundary.

**Non-Goals:**

- Supporting non-NTFS folder-size calculation.
- Retaining Everything or recursive traversal as a compatibility fallback.
- Showing folder sizes for ZIP files or arbitrary Shell namespace containers.
- Changing ordinary file-length semantics or Explorer byte formatting.
- Moving UI work into the privileged service.

## Decisions

### One Host-owned query contract

`FolderSizeServiceV1` will expose an MFT-only aggregate query whose terminal outcomes are Host cache hit, MFT result, or unavailable. Consumers cannot select a backend. The existing full-tree Size Map request will be sourced from the same MFT aggregate/index contract rather than its recursive scanner.

Alternative: let each consumer call MFT directly. Rejected because it duplicates canonicalization, cache admission, invalidation, retry, and error policy.

### No slow fallback

Everything and recursive directory walking will be removed from reachable folder-size decision branches. Failure returns unavailable immediately and remains retryable after refresh or a newer service index.

Alternative: retain fallback behind a timeout or option. Rejected because extension/configuration combinations would again create unpredictable latency and violate the approved single-path requirement.

### Built-in Size is a first-class consumer

Folder-size requests will be scheduled whenever a visible built-in Size or Folder size consumer needs them, not only when the extension column is active. A context-scoped Host result map will provide optional recursive bytes to Details rendering and sorting. Files continue using `metadata.size_bytes`; eligible folders use the Host result; unavailable folders have no Size value.

ZIP/Shell archive rows are excluded using the enumerated Shell metadata contract: a folder-size-eligible row is a container without ordinary file bytes. The worker retains a file-system directory check as a defensive boundary.

### Cache identity and invalidation

All consumers use the existing Host persistent cache admission based on canonical path identity, folder modification timestamp, and cache schema. A timestamp or schema change is a miss and can only be repopulated from MFT. Consumers do not own persistent cache files or validity policy.

### Status and observability

The status bar will persist the latest terminal backend as `Folder size: Host cache`, `Folder size: MFT service`, or `Folder size: MFT unavailable`; an ellipsis denotes active work. MFT-unavailable must be distinguishable from an idle view and must not claim calculation is continuing.

### Sorting

The Size comparator will receive the same optional recursive bytes used for display. Known file and folder byte values participate in numeric ordering. Missing values use the existing missing-value ordering and never coerce to zero.

## Data Flow

1. Shell enumeration publishes rows and ordinary file metadata.
2. UI determines eligible visible folder rows and requests shared folder-size values when either built-in Size or Folder size needs them.
3. Host cache validates canonical identity, modification timestamp, and schema.
4. On hit, one result is published to all consumers and status becomes Host cache.
5. On miss, `FolderSizeServiceV1` reads the current MFT Service aggregate/index.
6. A complete result is admitted to Host cache and published; otherwise unavailable is published without fallback.
7. Details display and sorting consume the same optional value. Size Map consumes the same Host service/index for its hierarchy.

## Security and Platform Constraints

- Raw NTFS/MFT access remains in the installed LocalSystem service; the UI process receives only service-generated data.
- Service output validation, volume identity checks, canonical path handling, freshness checks, and bounded retries remain mandatory.
- The MFT aggregator must never exceed eight workers.
- Unsupported/non-NTFS volumes terminate as unavailable without elevation prompts from the UI.

## Risks / Trade-offs

- [MFT service missing or stopped leaves folder Size blank] → Show MFT unavailable, keep file sizes intact, and retry on refresh/service recovery.
- [Directory timestamps do not reflect every descendant edit on all workflows] → Preserve the approved timestamp contract and schema invalidation; document this cache policy rather than adding a slow verifier.
- [Built-in Size requests increase MFT demand when Folder size extension is off] → Reuse one per-volume aggregate and Host cache; deduplicate context requests.
- [Archive Shell items can report container semantics] → Require absence of ordinary file bytes and retain worker-side directory validation.
- [Removing fallback changes prior behavior on non-NTFS volumes] → Treat this as intentional product behavior and test explicit blank/unavailable presentation.

## Migration Plan

1. Add MFT-unavailable outcome/status and lock tests around the no-fallback contract.
2. Refactor the shared Host service and Size Map to eliminate reachable Everything/recursive measurement.
3. Make built-in Size request, render, and sort from the shared result map.
4. Extend UTIT and installed-build evidence with extension enabled and disabled.
5. Rebuild/install and verify the service, cache-hit relaunch, MFT path, ZIP exclusion, and status labels.

Rollback is a source rollback to the prior Host service/UI consumers; the persistent cache is schema/version guarded and can be ignored by the previous binary. Service installation does not need to change for rollback.

## Evidence-driven Corrections

- **A — task refinement:** commands, ordering, task splits, or evidence paths may change without changing requirements or gates.
- **B — design/spec correction:** an implementation discovery within approved scope requires affected work to pause, artifacts/tasks to be corrected, completed dependent evidence to be marked stale, and strict validation to rerun.
- **C — material change:** reintroducing a fallback, changing the cache validity contract, service privilege/platform, required evidence, or public behavior requires user approval.

No blocking validation threshold may be reduced silently.

## Open Questions

None. The approved design fixes the backend, cache, unavailable, presentation, and fallback policies.
