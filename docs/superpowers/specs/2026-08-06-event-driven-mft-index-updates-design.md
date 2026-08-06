# Event-Driven MFT Index Updates Design

## Goal

Replace the `SuperExplorerMft` service's fixed 30-second full-volume rebuild loop with one initial MFT snapshot followed by NTFS USN Journal updates. Folder-size data should normally become current within 5–10 seconds without recurring 400–500 MB service memory spikes.

## Scope

This change covers fixed NTFS volumes supported by the existing service, the persisted service index consumed by the Host, Host-side index refresh and folder-size cache invalidation, recovery from an unusable journal cursor, service lifecycle, diagnostics, and automated verification. It does not add support for non-NTFS change journals or remote volumes.

## Architecture

### Initial snapshot

For each eligible fixed NTFS volume, the service reads the volume identity and USN Journal metadata, builds one complete MFT snapshot, writes it atomically, and records a checkpoint containing the volume identity, journal ID, and next USN. The in-memory snapshot is dropped before the service begins watching the next volume.

An existing valid snapshot and compatible checkpoint may be reused after service restart. A full scan is required when no snapshot exists, its format or identity is invalid, or the recorded journal position can no longer be read.

### Blocking journal readers

After initialization, one bounded worker per eligible volume blocks in `FSCTL_READ_USN_JOURNAL`. This is an event-driven wait, not a timer that initiates full scans. Workers feed normalized changes into a bounded channel. Service shutdown cancels or wakes the blocking reads and joins all workers.

The reader requests the reasons required for folder-size correctness: create, delete, rename, data overwrite, data extension, data truncation, hard-link changes, and relevant metadata/security changes that can affect reachability or allocation. Unknown reasons are retained conservatively as invalidations.

### Five-second coalescing

The coordinator groups changes by volume and file reference. The first event starts a five-second debounce window. Additional events extend the batch only up to a bounded maximum delay of ten seconds, preventing a permanently busy volume from starving publication.

Within a batch, redundant changes are collapsed. Rename old/new records are paired by file reference when possible. An incomplete pair or ambiguous hard-link transition is published as a conservative subtree or volume invalidation rather than guessed.

### Persisted delta protocol

The existing full snapshot remains the base generation. Incremental changes are published as an append-safe or atomic delta generation containing:

- schema version and volume identity;
- journal ID and inclusive/exclusive USN bounds;
- changed file reference and parent reference;
- old parent/name when available;
- new parent/name when available;
- current logical and allocated sizes for live files;
- create, update, delete, rename, or conservative invalidation kind;
- checksum and committed-record marker.

The checkpoint advances only after the corresponding delta is durable. Temporary or partially written files are ignored. Delta compaction may create a new atomic base snapshot when a bounded size/count threshold is crossed, but compaction must not reintroduce periodic full-volume scanning.

### Host application and cache invalidation

The Host loads a base generation and applies subsequent delta generations in order. It rejects gaps, identity mismatches, checksum failures, or journal discontinuities and asks the service for recovery instead of serving knowingly inconsistent data.

For each accepted change, the Host updates the affected entry and parent/child relationships. Folder-size aggregates and persistent data-column cache entries are invalidated for the changed item, its old ancestor chain, and its new ancestor chain. Unrelated folder cache entries remain valid. The Host publishes a new immutable index generation atomically so active queries never observe a partially applied batch.

The built-in Size column and Folder size extension continue to consume the same Host-owned result. The UI does not implement separate USN handling.

## Recovery and Explorer-Compatible Semantics

A complete rebuild is permitted only when required for correctness:

- first initialization;
- journal ID changed, journal was deleted, or requested USN is older than the journal's retained range;
- volume identity changed;
- persisted base/delta/checkpoint is corrupt or has a generation gap;
- change volume exceeds bounded queues and lossless recovery is impossible.

Rebuilds are serialized per volume and use atomic publication. The previous valid generation remains readable until replacement succeeds. Access-denied entries, disappearing files, rename races, reparse points, sparse files, compression, and hard links follow the existing File Explorer-oriented logical/allocated-size policy; uncertain events invalidate rather than fabricate totals.

## Resource Bounds

- No fixed interval may trigger a complete MFT scan.
- Journal workers use fixed-size read buffers and bounded channels.
- Pending changes are bounded by count and bytes; overflow enters explicit recovery.
- A completed full snapshot is dropped promptly.
- Normal idle service working set should remain near its post-scan baseline.
- Normal incremental processing must not allocate memory proportional to the total number of records on the volume.

## Status and Diagnostics

Per-volume status records the mode (`initializing`, `journal`, `recovering`, or `error`), base generation, last committed USN, pending change count, last publication time, rebuild reason, and memory-relevant queue sizes. Existing UI backend status should identify MFT service data without exposing transient implementation details as a different calculation method.

## Verification

Unit tests cover event coalescing, rename pairing, ancestor invalidation, journal cursor validation, bounded queues, atomic checkpoint ordering, and corrupt/gapped delta rejection.

Windows integration tests create, grow, truncate, rename, move, hard-link, and delete files on an NTFS fixture and verify affected folder totals update within ten seconds while unrelated cached totals remain valid. Tests also simulate journal discontinuity and confirm a single recovery rebuild.

An installed-service test records working set, private bytes, CPU, index generations, and output timestamps across idle and mutation periods. Acceptance requires:

- no full-index file rewrite during at least two minutes of inactivity;
- mutation visible to Host folder-size queries within ten seconds;
- no normal incremental memory spike proportional to the 300 MB base index;
- no repeated 30-second scan signature;
- clean service stop during a blocking journal read;
- installer upgrade preserves or safely rebuilds compatible cache state.

