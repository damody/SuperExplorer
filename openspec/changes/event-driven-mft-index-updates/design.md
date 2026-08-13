## Context

`SuperExplorerMft` currently loops over drive letters, builds a complete `MftIndexV1` for each existing root, writes `<letter>.semftidx`, sleeps for 30 seconds, and repeats. A measured cycle on 2026-08-06 reached 441.3 MB working set and 519.2 MB private bytes while C: was rebuilt, dropped after publication, then repeated for D:. C: and D: base files are approximately 159 MB and 144 MB. The service must retain LocalSystem access to NTFS volume handles, while the unprivileged Host remains the only consumer-facing folder-size/cache authority.

The approved source design is `docs/superpowers/specs/2026-08-06-event-driven-mft-index-updates-design.md`. Windows USN Journal and MFT IOCTLs are the platform source of truth. Existing plugin ABI and UI presentation remain unchanged.

## Goals / Non-Goals

**Goals:**

- Perform at most one necessary full MFT snapshot per compatible volume generation, then wait for journal events.
- Apply file-system changes to proven in-memory query state without a scheduled full rescan. Durable publication timing and storage are superseded by `mft-sqlite-foreground-persistence`.
- Let the Host apply changes atomically and invalidate only affected folder-size/data-column cache ancestry.
- Detect every condition that prevents lossless continuation and recover with one serialized rebuild while retaining the previous valid generation until commit.
- Bound normal service memory independently of total MFT record count and stop cleanly while journal workers are blocked.
- Produce installed-service evidence for inactivity, mutation freshness, memory, CPU, shutdown, and upgrade behavior.

**Non-Goals:**

- Non-NTFS, remote, removable, or cloud-provider change tracking.
- A public plugin ABI for raw USN events.
- Replacing the Host-owned folder-size service or introducing a second calculation label.
- Perfectly reconstructing ambiguous hard-link or rename histories; those cases use conservative invalidation/recovery.

## Decisions

### Use USN Journal reads rather than directory watchers or event-triggered rescans

Each eligible volume receives a bounded worker that blocks in `FSCTL_READ_USN_JOURNAL` from a persisted `(volume identity, journal ID, next USN)` checkpoint. This provides volume-wide, file-reference-based changes without a timer. `ReadDirectoryChangesW` was rejected because recursive volume buffers can overflow and path-only rename correlation is weaker. Event-triggered full scans were rejected because ordinary activity would still reproduce the measured memory and I/O spikes.

### Separate privileged collection from bounded Host interpretation

The service owns volume handles, journal cursors, normalized event collection, the complete durable base/delta/checkpoint dataset, and recovery status. It does not maintain a permanent full `HashMap`. The Host may materialize complete relationships and aggregates only for an active query batch. When the batch finishes it releases those volume-wide structures, compacts retained folder snapshots to terminal aggregates, and keeps persistent data-column results only for directories at depth zero through three relative to the active folder. Size Map may materialize a full tree on demand, but that tree is not admitted to the retained Host cache. UI and plugins continue through existing Host ports.

Folder-size aggregate queries use a fixed-size local named-pipe protocol. The Service, rather than the interactive Host, materializes and caches the current per-volume generation and folder aggregates. The cache is volume-granular LRU with a user-configurable estimated resident limit: 512 MiB by default, accepting numeric values from 128 through 2048 MiB. Each request carries the normalized budget so lowering it immediately evicts least-recently-used volumes. Only aggregate fields are returned to the Host; file-level index data remains an internal Service implementation detail.

### Legacy ordered delta generations (superseded durability mechanism)

The implemented sidecar chain remains relevant only as a legacy migration input. `mft-sqlite-foreground-persistence` replaces normal delta/checkpoint publication with atomic SQLite transactions while preserving compatible identity, cursor, contiguity, and crash-consistency requirements.

This is preferred to in-place modification of the large base because a crash cannot leave partially mutated shared state. It is preferred to rewriting the base after each batch because ordinary events remain proportional to changes.

### Coalesce promptly in memory

Changes collapse by file reference in memory; old/new rename events pair when unambiguous. Queues and pending bytes have explicit bounds. The later persistence change controls durability cadence, foreground-gated recovery, and overflow disposition without weakening loss-of-correctness reporting.

### Update Host generations and invalidate both ancestry chains

For create/update/delete/rename, the Host derives the old ancestor chain before mutation and the new chain after mutation. It invalidates aggregate and persistent data-column cache keys for the item and union of both chains, then atomically publishes the updated index generation. Unrelated cache entries stay valid. Ambiguous link/topology changes invalidate conservatively; a generation gap rejects the entire delta chain.

### Rebuild only for correctness loss

A rebuild is allowed for first initialization, incompatible/corrupt persisted data, changed volume or journal identity, a cursor older than the journal's retained range, generation/USN gaps, or bounded-queue overflow. Recovery is serialized per volume. There is no scheduled rebuild interval and no idle base rewrite.

### Make service lifecycle cancellation explicit

Workers use cancellable volume handles or bounded journal timeouts plus a stop event so SCM stop does not wait indefinitely. The coordinator joins workers before reporting `SERVICE_STOPPED`. Handles, temporary files, and pending batches use RAII cleanup.

### Diagnostics are persisted per volume

Status includes `initializing`, `journal`, `recovering`, or `error`; base/delta generation; journal ID and committed USN; pending count/bytes; publication time; rebuild reason; and queue high-water marks. Diagnostics must not expose raw paths beyond existing local cache policy.

### Adjustment governance

- **A — task refinement:** task ordering, file ownership, command, fixture, or evidence mechanics may change without changing scope, thresholds, requirements, or public contracts.
- **B — design/spec correction:** an implementation discovery within approved scope pauses affected work; design, spec, tasks, and stale evidence are updated and revalidated.
- **C — material change:** changing scope, Windows platform source, public ABI, permission model, in-memory freshness, resource gates, or required installed-service evidence requires user approval. Durability cadence is governed by `mft-sqlite-foreground-persistence`.

No adjustment may silently weaken a blocking gate. Superseded evidence remains traceable.

## Risks / Trade-offs

- **USN journal deletion, wrap, or ID replacement** → Validate journal metadata before every continuation and trigger one explicit recovery rebuild.
- **Crash between delta and checkpoint publication** → Commit delta first; checkpoint advances last; replay is idempotent by generation and USN bounds.
- **Rename pairs split across batches** → Retain bounded pairing state across one publication boundary, then conservatively invalidate if incomplete.
- **Hard links have multiple parents while the current model has one parent** → Detect hard-link reasons and invalidate affected topology conservatively; do not invent an exact relationship.
- **Busy volumes exceed pending bounds** → Record overflow diagnostics and rebuild once; never drop changes while claiming freshness.
- **Host and service versions cross during upgrade** → Version all files, ignore incompatible deltas, and preserve the last valid readable base until replacement succeeds.
- **Blocking read delays service stop** → Use explicit cancellation/finite wait and test SCM stop latency.
- **Delta accumulation increases startup cost** → Compact only when bounded count/bytes thresholds are exceeded, driven by actual delta size rather than time; compaction is atomic and not a full MFT scan.

## Migration Plan

1. Ship readers that understand the existing base plus the new checkpoint/delta/status files.
2. On service start, treat an existing base without a compatible checkpoint as reusable only if a journal cursor can be safely established; otherwise perform one rebuilding migration.
3. Start journal workers only after base and checkpoint publication is durable.
4. The Host keeps using its last valid generation while applying compatible deltas or while a replacement base is constructed.
5. Installer upgrade preserves `%ProgramData%\SuperExplorer\MftIndex`; incompatible files are quarantined/version-replaced rather than partially consumed.
6. Rollback to a build that only understands the old base remains possible because the base file is retained; new sidecar files are ignored. If the old build resumes periodic scans, uninstall/rollback documentation identifies that behavior.

## Testing and Blocking Gates

- Unit gates: cursor validation, reason normalization, coalescing, rename pairing, bounded overflow, atomic commit ordering, delta validation, ancestor invalidation, idempotent replay, and immutable generation publication.
- NTFS integration gates: create, grow, overwrite, truncate, rename, move, hard-link, delete, journal discontinuity, and service stop.
- Installed-service gate: two idle minutes without base rewrite or 30-second scan signature; affected mutation visible in at most ten seconds; unrelated cached result retained; normal delta processing without memory proportional to the base; clean stop; compatible upgrade/recovery.
- Evidence records use `target/openspec-evidence/event-driven-mft-index-updates/<task-id>/result.json` with task ID, command/procedure, expected/actual, exit status, hashes, gate IDs, source revision, timestamp, and any adjustment lineage.

## Open Questions

None. Exact queue and compaction byte/count constants are implementation refinements only if measured tests demonstrate they preserve all normative resource and freshness gates.
