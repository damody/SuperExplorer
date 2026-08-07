# MFT SQLite Foreground Persistence Design

## Purpose

Reduce Microsoft Defender activity caused by the MFT service creating a new
checkpoint and delta file every few seconds. The service will keep live USN
changes in memory and persist them at most once every ten minutes, only while a
Super Explorer window is focused in the foreground.

The MFT service remains the authoritative privileged reader of NTFS metadata.
Super Explorer consumers continue to query the service's current in-memory
index and do not wait for disk persistence.

## Goals

- Replace per-generation `.semftcp` and `.semftdelta` files with a fixed SQLite
  file set per NTFS volume.
- Coalesce live USN changes in service memory without disk writes during the
  normal observation loop.
- Permit persistence only when ten minutes have elapsed since the last
  successful commit and Super Explorer has a valid foreground-focus lease.
- Avoid forced persistence during service stop or Windows shutdown.
- Recover uncommitted changes from the NTFS USN journal after restart.
- Keep memory, WAL growth, recovery, and corruption behavior bounded and
  observable.

## Non-goals

- Excluding the MFT cache from Microsoft Defender.
- Changing folder-size result semantics, cache-budget UI, plugin ABI, or the
  privileged service boundary.
- Guaranteeing recovery from changes older than the retained NTFS USN journal;
  that condition requires a full MFT rebuild.
- Performing persistence or rebuilding merely because the service is stopping.

## Selected approach

Each NTFS volume has one bundled-rusqlite database under the existing fixed
service cache root, for example `C.mft.sqlite3`. SQLite uses WAL mode, producing
a stable set of at most three active files per volume:

- `C.mft.sqlite3`
- `C.mft.sqlite3-wal`
- `C.mft.sqlite3-shm`

The service applies USN events immediately to its in-memory index and a
coalesced pending-change map. Queries observe the in-memory generation. Once a
foreground lease is active and the ten-minute persistence interval is due, the
service writes the entire coalesced batch and its resulting cursor in one
transaction. The cursor and changes become durable atomically.

This approach is preferred over retaining sidecar generations because the
number of files stays fixed. It is preferred over SQLite rollback-journal mode
because that mode creates and removes a journal file for each transaction. It
is preferred over asking the LocalSystem service to inspect the foreground
window because cross-session foreground detection is unreliable.

## Components

### In-memory volume state

Each volume worker owns:

- the current MFT index;
- the last durable USN cursor and database generation;
- the latest observed USN cursor;
- a change map coalesced by file reference;
- bounded pending-change accounting;
- the last successful persistence time; and
- a typed recovery state when incremental persistence is no longer safe.

Reading and coalescing the USN journal does not write status, checkpoint, delta,
or index files. Frequently changing diagnostics remain in memory and are
returned through the existing service query/diagnostic channel.

### Foreground-focus lease

Super Explorer reports foreground focus through a versioned MFT service IPC
operation. A focused window acquires or renews a short-lived lease; loss of
focus explicitly releases it. The service also expires a lease when renewals
stop, so a crashed, suspended, disconnected, or session-switched client cannot
leave persistence enabled indefinitely.

Multiple windows are aggregated: persistence is eligible while at least one
authenticated Super Explorer client has a valid focused lease. IPC identity and
authorization follow the existing MFT query pipe boundary; arbitrary processes
cannot enable service writes.

### Persistence scheduler

Persistence eligibility requires all of the following:

1. pending changes or durable metadata work exists;
2. at least ten minutes have elapsed since the last successful commit (or since
   service startup when no commit has occurred in this process); and
3. at least one foreground-focus lease is valid.

When a due service regains foreground focus, it schedules one commit promptly.
Only one commit per volume may run at a time. Events arriving during a commit
remain in the next in-memory batch. A failed transaction does not advance the
durable cursor or ten-minute timer and is retried only while the foreground
gate remains open, with bounded backoff.

Service stop and Windows shutdown discard the in-memory pending batch without a
transaction. The next service start resumes at the durable cursor and reads the
missing interval from the USN journal.

### SQLite store

The schema is versioned and contains:

- volume identity and USN journal identity;
- the atomically committed durable cursor and generation;
- MFT entries keyed by file reference, including parent, name, type, logical
  bytes, and allocated bytes; and
- schema/rebuild metadata needed to reject incompatible or incomplete stores.

One transaction upserts/deletes the coalesced entries and updates the durable
cursor last. Foreign-key behavior is not used to cascade the MFT tree; file
references and parent references remain explicit index data.

Connections use bundled SQLite, WAL mode, busy timeout, synchronous `NORMAL`,
and disabled automatic WAL checkpointing. The service is the sole writer.
Readers use the service IPC and do not open the database directly.

### WAL control

Ordinary ten-minute commits append to the fixed WAL and do not trigger an
immediate SQLite checkpoint. A WAL checkpoint is eligible only when:

- the WAL exceeds a configured bounded threshold, initially 256 MiB;
- a valid foreground-focus lease exists; and
- no persistence transaction or query-critical index refresh is active.

The service uses `wal_checkpoint(TRUNCATE)` so successful maintenance returns
the WAL to a small fixed file. Failure is non-fatal and retried with backoff on
a later foreground opportunity. Cache telemetry reports main database bytes,
WAL bytes, pending-memory bytes, last durable cursor, last observed cursor,
commit failures, and checkpoint failures separately.

## Bounds and overload behavior

The coalesced pending map retains the existing entry and byte limits. Crossing
a limit does not cause background disk I/O or an immediate full rebuild.
Instead the volume enters `rebuild-required`, releases detailed pending entries,
continues tracking the latest journal state needed for diagnostics, and returns
only results whose completeness is still proven. A full rebuild begins only
when a foreground lease is valid.

If the durable cursor falls behind the retained USN journal, the journal ID
changes, or an ambiguous event invalidates incremental correctness, the same
foreground-gated rebuild path is used. Rebuild work is serialized across
volumes to preserve the existing working-set bound.

## Startup and migration

Startup prefers a valid SQLite store. It verifies schema, volume identity,
journal identity, committed cursor, and database integrity before admitting the
store. It then catches up from the durable cursor into memory without writing.

When SQLite is absent and a valid legacy base/sidecar chain exists, the service
loads that chain and marks a one-time SQLite migration as pending. Migration is
performed only after the ten-minute and foreground conditions are satisfied.
The new database is built at a temporary fixed-cache path, committed and
validated, then atomically promoted.

Legacy `.semftcp`, `.semftdelta`, `.semftidx`, and status files are removed only
after the promoted SQLite database has been reopened and its durable state has
been verified. Cleanup is scoped to the recognized volume-specific legacy file
patterns inside `ProgramData\SuperExplorer\MftIndex`. Failed migration leaves
the legacy store intact and retryable.

## Error handling

- A transaction failure preserves the prior durable cursor and pending memory.
- SQLite corruption quarantines the affected database within the fixed cache
  root and marks the volume rebuild-required; it is never treated as an exact
  index.
- Loss of focus during an already-started transaction does not interrupt the
  atomic commit, but prevents subsequent commits and WAL maintenance.
- IPC disconnect expires the associated focus lease.
- Service stop cancels reads and rebuild work where safe, closes SQLite, and
  performs no final commit or WAL checkpoint.
- If the USN journal cannot cover the gap after restart, the service reports a
  typed rebuilding/unavailable state until a foreground-gated rebuild succeeds.

## Testing

Unit and integration tests will prove:

- the ten-minute boundary alone cannot persist without foreground focus;
- foreground focus alone cannot persist before ten minutes;
- focus acquired after the deadline schedules exactly one transaction;
- repeated focus renewals do not create additional transactions;
- shutdown drops pending memory without changing the durable cursor;
- restart catches up an uncommitted interval from the USN journal;
- one transaction atomically applies changes and advances the cursor;
- transaction failure never advances the cursor;
- automatic WAL checkpointing is disabled and threshold maintenance is
  foreground-gated;
- focus leases expire after crash/disconnect and aggregate across windows;
- pending-memory overflow enters rebuild-required without background writes;
- legacy migration preserves legacy files until SQLite reopen verification;
- corruption, journal rollover, and volume identity mismatch require rebuild;
  and
- normal idle operation creates no per-generation files.

The Windows smoke test will run the installed service while Super Explorer is
backgrounded and focused, record cache-directory file counts and timestamps,
and verify that the active file count remains bounded. It will also capture
Defender CPU/I/O comparatively; Defender working-set size alone is not used as
proof of scanning activity.

## Success criteria

- Background or unfocused operation performs no periodic MFT cache writes.
- Focused operation persists no more than once per ten-minute interval per
  volume, excluding explicitly foreground-gated migration/recovery maintenance.
- Normal steady state uses no per-generation checkpoint/delta files and keeps a
  fixed SQLite file set per volume.
- Queries continue to reflect current in-memory USN changes before persistence.
- A normal stop or Windows shutdown does not force persistence.
- Restart is exact when the durable cursor remains available in the USN journal;
  otherwise the service reports rebuild-required rather than stale exact data.
