# Fix MFT Folder Size Zero Results

## Problem

The installed/runtime state can report every directory as `0 B` even though the
directory contains files. Two independent defects combine into this result:

1. The installer does not make successful creation and startup of the
   `SuperExplorerMft` Windows service a blocking condition. The observed machine
   returns SCM error 1060 because the service does not exist.
2. An accelerated provider can return a shallow or incomplete set of descendants,
   yet the normalized snapshot is published as `Complete`. Directory records have
   zero direct bytes, so an incomplete tree becomes a plausible but false `0 B`.

Size Map showing top-level files while omitting directory contents is the same
completeness defect, not a separate rendering bug.

## Required behavior

- The installer runs elevated, installs `SuperExplorerMft` as an automatic
  `LocalSystem` service, starts it, and aborts installation with a useful error if
  SCM create/config/start or the running-state verification fails.
- Upgrade stops the previous service before replacing its executable, then
  configures, starts, and verifies the new incarnation.
- Uninstall stops and deletes the service before deleting its executable.
- The ordinary SuperExplorer process remains non-elevated.
- Folder Size and Size Map consume only the shared normalized snapshot.
- An accelerated result is `Complete` only when subtree completeness is proven.
- Missing service, stale cache, malformed index, missing parents, escaped paths,
  reparse points, hard-link ambiguity, mutation, cancellation, or an incomplete
  Everything result causes typed fallback. It must never become an exact zero.
- A legitimate empty directory may display `0 B` only after a complete scan proves
  that it has no countable descendants.

## Architecture

### Installer service gate

Move SCM operations into checked NSIS helpers. Every command captures its exit
code. Installation accepts either a successful create or the documented
already-exists result followed by successful config. It then starts the service,
polls SCM until `RUNNING` with a bounded timeout, and aborts/rolls back when the
service cannot run. The installer log records the exact failing SCM operation.

### Service health and cache contract

The service publishes versioned volume indexes through its fixed ProgramData
location. Each record carries schema version, volume identity, creation time,
entry count, and a completed marker written last. The app accepts only a bounded,
fresh, completed record whose volume identity matches the requested root.

The app records which backend supplied a snapshot (`MftService`, `MftHelper`,
`Everything`, or `Recursive`) and why a fallback occurred. Plugins never receive
the raw volume index.

### Completeness gate

MFT projection proves completeness by reaching the requested root record and
walking every indexed descendant without exceeding bounds. Every projected node
must have a valid parent chain and pass live metadata checks.

Everything is eligible only when its subtree query and result count prove that all
descendants—not merely direct children—were returned. A shallow query is rejected.
If the SDK cannot provide a trustworthy completeness proof, Everything remains a
search acceleration backend and is not used for exact Folder Size snapshots.

The recursive backend is the semantic reference and final fallback. Partial,
cancelled, unavailable, and resource-limited outcomes render as non-exact states,
never `0 B`.

## Data flow

1. Folder Size or Size Map subscribes to a canonical root and generation.
2. The shared service checks a valid normalized snapshot.
3. It tries a healthy MFT service record and validates completeness.
4. If unavailable, it may use the one-shot MFT helper compatibility path.
5. It tries Everything only when subtree completeness can be proven.
6. Otherwise it performs the bounded recursive reference scan.
7. A complete snapshot supplies both aggregate bytes and tree nodes. Both
   consumers therefore display values derived from the same physical scan.

## Error handling

- Installer service failures stop installation instead of silently succeeding.
- Runtime service failures are non-fatal and produce a privacy-safe diagnostic.
- Accelerated-provider validation failures discard the entire candidate snapshot.
- Existing cached `0 B` values are invalidated by a cache/schema version bump.
- Exact zero is accepted only when `status == Complete`, file count is zero, and
  the backend supplied a completeness proof.

## Verification

Unit and integration tests cover:

- SCM create, already-exists/config, start timeout, stopped, and missing-service
  installer outcomes.
- Complete MFT aggregate/tree equality against recursive fixtures.
- Shallow Everything results, missing descendants, stale entries, mutation,
  escaped paths, reparse points, and hard links all reject acceleration.
- Non-empty directories can never publish exact zero.
- Empty directories can publish exact zero after a complete scan.
- Folder Size and Size Map enabled together share one snapshot and identical totals.
- Installed production binaries report the service as `RUNNING`, then headful
  Folder Size and Size Map screenshots are reviewed on `D:\SuperExplorer`.

## Scope

This correction does not add new UI, expose MFT to plugins, or change code-lines
columns. It corrects service installation, accelerated snapshot completeness,
fallback behavior, cache invalidation, and verification evidence.
