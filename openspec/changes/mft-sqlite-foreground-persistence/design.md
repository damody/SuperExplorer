## Context

`SuperExplorerMft` already consumes NTFS USN events, but the active `event-driven-mft-index-updates` implementation persists a new `.semftdelta` and `.semftcp` generation after five seconds of quiet or nine seconds of sustained activity. A live installation contained 20,025 MFT cache files (10,008 deltas and 10,011 checkpoints), while Defender repeatedly inspected newly created files. The service must retain current folder-size data without coupling query freshness to disk durability.

The approved source design is `docs/superpowers/specs/2026-08-07-mft-sqlite-foreground-persistence-design.md`. This change uses the workspace's existing bundled `rusqlite 0.32`; it adds no external service or runtime. Raw NTFS access remains in the installed LocalSystem service. Super Explorer communicates through the existing local named-pipe boundary.

The earlier unarchived change remains an implementation dependency for USN reading, coalescing, bounded queues, and service queries. Its normative five-to-ten-second *durable sidecar publication* requirement is superseded here. In-memory USN application remains immediate/event driven. The two changes must be reconciled before archive so contradictory durability requirements cannot enter the baseline.

## Goals / Non-Goals

**Goals:**

- Keep current USN-derived query state in service memory without routine cache-directory writes.
- Persist at most one coalesced transaction per volume in each ten-minute interval, and only while a valid Super Explorer foreground-focus lease exists.
- Replace unbounded per-generation sidecars with a fixed SQLite WAL file set per volume.
- Recover shutdown-discarded changes from the last durable cursor when the NTFS journal retains the gap.
- Bound pending memory and WAL growth without background rebuild or checkpoint I/O.
- Migrate and remove legacy files only after verified SQLite promotion.
- Provide security, lifecycle, correctness, and Defender-sensitive I/O evidence for an installed service.

**Non-Goals:**

- Defender exclusions or changing Defender configuration.
- Non-NTFS, remote, removable, or cloud-provider indexing.
- Folder-size semantics, cache-budget UI, public plugin ABI, or LocalSystem privilege changes.
- Forced persistence on SCM stop or Windows shutdown.
- Exact restart continuation when the durable cursor is older than the retained USN journal; that state requires a foreground-gated rebuild.

## Decisions

### Separate observed state from durable state

Each volume worker owns a current in-memory index, last durable cursor/generation, latest observed cursor, coalesced pending map, pending-byte accounting, last successful commit time, and typed recovery state. USN events update memory immediately. Query workers read a coherent in-memory generation and never wait for the ten-minute transaction.

The alternative of delaying query visibility until persistence was rejected because it would make folder sizes ten minutes stale. The alternative of retaining the existing durable sidecars with a longer timer was rejected because it still grows the file count indefinitely and causes create/scan churn.

### Use one SQLite WAL database per volume

Each eligible volume uses `<letter>.mft.sqlite3` under `%ProgramData%\SuperExplorer\MftIndex`. WAL mode keeps the active file set to the main database, `-wal`, and `-shm`. The schema stores a version, volume identity, journal identity, durable cursor and generation, rebuild/completeness metadata, and entries keyed by file reference with parent, name, kind, logical bytes, and allocated bytes.

The service is the sole writer; clients never open the database. A persistence transaction upserts/deletes the captured coalesced batch and advances durable metadata in the same commit. Rollback-journal mode was rejected because each transaction creates/removes another file. A bespoke single-file log was rejected because SQLite already supplies transactions, checksums, crash recovery, and bounded query/update primitives.

Connections use bundled SQLite, `journal_mode=WAL`, `synchronous=NORMAL`, a bounded busy timeout, `wal_autocheckpoint=0`, and `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE`. The service asserts the no-checkpoint-on-close setting on every applicable connection; persistent WAL-file flags alone are insufficient. SQLite errors remain internal typed service errors and do not cross the public plugin ABI.

### Gate persistence on interval and an authenticated focus lease

Persistence requires pending durable work, ten minutes since the last successful commit (or service-start reference time before the first commit), ten minutes since the last disk-write attempt (or no prior attempt), and at least one valid focus lease. Regaining focus after both deadlines schedules one attempt promptly. The service records the attempt immediately before `BEGIN`; a failure does not advance the successful-commit clock but the independent attempt throttle prevents another write for ten minutes. Lease renewals reset neither clock.

Super Explorer reports focus through a dedicated versioned, concurrent, overlapped-I/O named-pipe lease channel so a persistent or stalled lease client cannot block ordinary one-request query IPC. Each connected authorized client owns a lease identifier. Focus acquisition/renewal sets a short expiry, explicit focus loss releases it, and disconnect/session loss/expiry removes it. The channel has bounded frame sizes, connection/resource caps, real overlapped-I/O deadlines, and stop-event cancellation. At least one valid focused lease opens the gate.

The interactive-user pipe ACL is only the first boundary and is not trusted as application identity. On connection, the service obtains and holds the client process handle from the named-pipe client PID, verifies process creation identity to defeat PID reuse, verifies its token SID and session against the active interactive session, and verifies that its canonical executable/file identity is the protected installed Super Explorer image recorded by the installed service/package. The verified client reports its own GPUI window activation state and renews a short lease; the LocalSystem service does not inspect cross-session foreground HWND state. Session switch or loss invalidates affected leases. Spoofed same-user processes, copied binaries outside the protected install root, wrong-session clients, and reused PIDs cannot authorize disk writes.

Having LocalSystem call foreground-window APIs was rejected because services run outside interactive sessions and cannot reliably infer the focused window across sessions. A permanent boolean was rejected because a crash could leave writes enabled.

### Do not persist during stop or shutdown

The service registers `SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN` and handles both `SERVICE_CONTROL_STOP` and `SERVICE_CONTROL_SHUTDOWN`. The control path first closes a lifecycle barrier so no transaction, checkpoint, migration, rebuild, or cleanup can begin. A transaction checks this barrier immediately before `BEGIN` and again immediately before invoking SQLite `COMMIT`; stop before the commit invocation rolls back. Once the `COMMIT` call is invoked, that transaction is linearized and may finish atomically, but shutdown starts no later durability work. Closing connections uses `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` and must not backfill, truncate, or unlink WAL state or mutate the cache directory as close housekeeping. Pending memory outside a linearized commit is discarded. On restart the worker opens the last valid durable cursor and replays the journal gap into memory.

This deliberately trades a longer restart catch-up for predictable shutdown I/O. If the gap is no longer retained, the service reports `rebuild-required` and waits for a focus lease before rebuilding; it never presents stale data as exact.

### Bound WAL maintenance behind the same foreground gate

Automatic and last-connection-close WAL checkpointing are disabled. Normal ten-minute commits append to the fixed WAL. A `wal_checkpoint(TRUNCATE)` is eligible only when the WAL exceeds 256 MiB, a focus lease remains valid, and no persistence transaction or query-critical refresh is active. Failure retains the WAL and retries with bounded backoff on a later foreground opportunity.

The hard WAL admission bound is the 256 MiB maintenance threshold plus at most one maximum admitted encoded pending batch (currently bounded by the existing 16 MiB pending-byte limit) plus measured SQLite page/frame overhead for that batch. Before `BEGIN`, the service conservatively estimates worst-case WAL growth. Once the current WAL plus worst-case transaction would exceed that computed bound, it prohibits further WAL-appending commits until an eligible truncate checkpoint succeeds; memory state remains queryable subject to its independent bounds. Tests measure and assert the concrete bound rather than assuming encoded bytes equal WAL bytes.

The 256 MiB threshold is a blocking public design threshold. Changing it requires a C-level material adjustment and user approval. Main DB, WAL, pending memory, cursor lag, transaction failures, checkpoint failures, and last commit time are separately observable.

### Convert overload into typed recovery without background writes

The existing pending count and byte bounds remain enforced. On overflow or ambiguous topology, the worker enters `rebuild-required`, stops retaining detailed pending entries, preserves only state needed for diagnosis, and marks unproven results unavailable/partial. It does not publish sidecars, checkpoint SQLite, or start a background full scan. Rebuild begins only while focused and is serialized across volumes.

Journal ID replacement, volume mismatch, corrupt/incompatible SQLite, cursor truncation, or non-contiguous state use the same typed path. A failed rebuild leaves the prior durable database intact but not falsely exact for the current journal state.

### Migrate through verified promotion, then perform scoped cleanup

Only when the canonical SQLite main/WAL/SHM set is entirely absent and a valid legacy base/sidecar chain is available does startup load legacy state and mark migration pending without writing. After both gates are satisfied, the service builds a temporary self-contained SQLite main database inside the fixed cache root using a migration-only rollback-journal connection, closes/fsyncs it, and validates schema, integrity, volume/journal identity, entry count/bounds, and cursor before promotion. It atomically installs the canonical main database as one file, confirms no stale canonical WAL/SHM companion exists, reopens it in the approved WAL/no-checkpoint-on-close configuration, and repeats admission verification before cleanup. A live temporary WAL/SHM set is never promoted piecemeal.

Any existing invalid canonical main/WAL/SHM set takes precedence over legacy migration: it is rejected, inventoried as one set, and marked quarantine/rebuild-required during unfocused startup without filesystem mutation. A foreground-gated maintenance action first fsyncs a versioned quarantine-intent manifest under a unique scoped quarantine directory, identifying every existing member by canonical path and file identity. It then moves main, WAL, and SHM members individually and records each completed step durably. The presence of an incomplete intent manifest makes every remaining canonical member inadmissible as exact state; restart validates identities and resumes the idempotent sequence. Completion is marked only after all inventoried members are disposed and the quarantine inventory is fsynced. A rebuild, not legacy migration, then produces replacement state. Every step is evidenced and never targets an unrecognized or out-of-root file.

Only after reopen verification may the primary agent/installed service remove files matching recognized per-volume legacy patterns inside the resolved fixed cache root. Temporary, unrelated, unrecognized, or out-of-root files are never deleted. Failure preserves legacy state and leaves migration retryable. Rollback documentation states that an older build ignores SQLite and can rebuild its legacy cache; SQLite is retained unless explicitly removed by its owning installer path.

### Keep diagnostics memory-first

Frequently changing status is returned through IPC rather than rewritten as `.semftstatus`. Diagnostics include mode, schema, durable and observed cursors, pending count/bytes, last commit, focus-lease count/expiry state, database/WAL bytes, transaction/checkpoint failures, migration state, and machine-readable recovery reason without exposing raw paths.

### Adjustment governance and evidence lineage

- **A — task refinement:** task split/order/owner/command changes that do not change scope, requirements, gates, thresholds, permission model, or public contracts.
- **B — design/spec correction:** an implementation discovery within approved scope pauses affected work; design/spec/tasks are corrected together and dependent evidence is marked stale.
- **C — material change:** any change to the ten-minute gate, foreground requirement, 256 MiB WAL threshold, shutdown no-write rule, platform/framework, IPC authorization, destructive migration boundary, blocking gates, or required evidence requires user approval.

No adjustment may silently weaken a gate. Evidence is append-only and records replacement lineage. Failed, blocked, stale, or unexecuted checks are not complete.

## Data and control flow

1. The volume worker opens and validates SQLite, or loads valid legacy state, then obtains the current journal metadata.
2. It replays from the durable cursor into the in-memory index without persistence.
3. New USN events update a coherent memory generation and coalesced pending map.
4. Queries read current memory state through existing service IPC.
5. The interactive process renews/releases focus leases through versioned IPC.
6. The scheduler snapshots the pending batch only when both gates are open.
7. One SQLite transaction applies that snapshot and advances its cursor atomically; later events remain pending.
8. WAL truncation or rebuild/migration runs only under its additional eligibility conditions.
9. Stop discards pending state; restart repeats catch-up from the durable cursor.

## Risks / Trade-offs

- **[USN gap grows while unfocused]** → Preserve the durable cursor, expose cursor lag, and enter foreground-gated rebuild-required if journal retention is lost.
- **[Pending memory exceeds bounds]** → Drop detailed pending state only after marking completeness lost; never trigger a background write or claim exactness.
- **[SQLite/WAL files are still scanned]** → Limit writes to one transaction per ten minutes while focused, keep file identities fixed, and compare Defender I/O rather than promising zero scanning.
- **[Focus IPC can be spoofed]** → Treat the pipe ACL as insufficient, validate held process creation identity/token/session/protected installed image, bind leases to that connection, expire client-reported focus promptly, and test same-user spoofing.
- **[Crash during transaction]** → Let SQLite rollback/recover atomically; admit only the last committed cursor.
- **[Loss of focus during a transaction]** → Finish the already-started atomic commit, then close the gate for subsequent work.
- **[Migration cleanup removes user data]** → Resolve the exact fixed root, whitelist volume-specific legacy patterns, verify SQLite after promotion, and inventory every removed file.
- **[WAL reaches threshold while unfocused]** → Report over-threshold state but defer checkpointing; pending/volume resource bounds remain independent.
- **[Older build rollback cannot read SQLite]** → Preserve documented rebuild behavior and do not reinterpret SQLite as legacy sidecars.
- **[Concurrent active OpenSpec changes conflict]** → Reconcile the earlier durability text and mark its superseded evidence stale before archive.

## Migration Plan

1. Add versioned schema/store and focus-lease protocol behind internal compatibility handling while retaining legacy readers.
2. Implement in-memory observed/durable separation and scheduler with deterministic clock/lease tests.
3. Route new installations to SQLite and prove restart, corruption, overflow, shutdown, and WAL behavior.
4. On upgrade, load legacy state read-only; wait for the approved gates; build, promote, reopen, and verify SQLite.
5. Inventory and remove only verified legacy files, recording hashes/paths and recovery disposition in evidence.
6. Update installer packaging and run installed-service upgrade/rollback tests.
7. Reconcile the active `event-driven-mft-index-updates` artifacts so its sidecar durability scenarios are superseded, not archived concurrently.
8. Rollback uses the prior binary's normal MFT rebuild path; it must not require deleting SQLite during service stop.

## Blocking gates

- `G-SQLITE-ATOMIC`: transaction failure/crash never advances the durable cursor; committed batches and cursor advance atomically.
- `G-FOCUS-AUTH`: unauthorized, expired, disconnected, and unfocused clients cannot open the write gate; multiple valid windows aggregate correctly.
- `G-TEN-MINUTE`: no volume makes more than one disk-write attempt in ten minutes, successful commits remain at least ten minutes apart, and no attempt occurs without focus.
- `G-NO-SHUTDOWN-WRITE`: SCM/Windows stop creates no new transaction, WAL checkpoint, migration, rebuild, or legacy cleanup; a transaction whose SQLite `COMMIT` invocation was already linearized may finish, and close performs no implicit checkpoint/backfill/truncate/unlink.
- `G-RESTART`: retained journal gaps replay exactly; unavailable gaps become foreground-gated rebuild-required.
- `G-WAL-BOUND`: automatic and close-time checkpointing are off; truncate checkpoint requires WAL above 256 MiB and focus; WAL-appending commits stop at the computed threshold-plus-one-bounded-batch hard limit.
- `G-MIGRATION`: legacy files remain until atomic promotion, reopen verification, and scoped inventory succeed.
- `G-FIXED-FILES`: normal steady state creates no per-generation files and uses only the fixed SQLite file set per volume.
- `G-DEFENDER-IO`: installed background/unfocused observation records no periodic MFT cache writes and the focused run records at most the allowed transaction cadence; Defender evidence uses CPU/I/O and file events, not working set alone.
- `G-REGRESSION`: folder-size queries remain current from memory, existing budgets/IPC compatibility work, service lifecycle passes, and workspace quality gates pass.

Evidence records live under `target/openspec-evidence/mft-sqlite-foreground-persistence/<task-id>/result.json` and contain task ID/subcheck, command or manual procedure, expected/actual result, exit status or reviewer, artifact hashes, gate IDs, source revision, timestamp, and adjustment lineage.

## Open Questions

None. Lease duration, renewal cadence, SQLite busy timeout, transaction batch mechanics, and test-fixture sizes are A-level implementation refinements only when they preserve all normative gates and bounds.
