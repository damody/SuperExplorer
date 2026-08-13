## Why

The event-driven MFT service currently creates a durable delta and checkpoint roughly every nine seconds on active volumes, leaving more than 20,000 sidecar files and repeatedly triggering Defender inspection. Persistence must be decoupled from live query freshness so current USN changes remain available in memory while disk writes occur no more than once per ten minutes and only when Super Explorer is focused.

## What Changes

- Replace per-generation `.semftdelta` and `.semftcp` persistence with one bundled-SQLite WAL store per NTFS volume and a fixed active file set.
- Keep live USN changes and current query results in MFT service memory; persist one coalesced transaction only after ten minutes and while an authenticated Super Explorer foreground-focus lease is valid.
- Add a dedicated concurrent/cancelable focus-lease IPC channel with renewal, explicit release, disconnect expiry, multi-window aggregation, and verified installed-SuperExplorer process/session/foreground identity.
- Disable automatic and last-close SQLite WAL checkpointing, allow a foreground-gated `TRUNCATE` checkpoint only after the WAL exceeds 256 MiB, and stop WAL-appending commits at the computed hard bound until maintenance succeeds.
- Drop uncommitted memory changes on explicit SCM stop or Windows shutdown and recover them from the last durable USN cursor on restart; define the in-flight transaction linearization boundary and require a foreground-gated rebuild when the journal can no longer cover the gap.
- Bound pending memory, expose durable/observed cursors and SQLite/WAL/failure telemetry, and never claim exactness after an overflow or recovery condition.
- Migrate valid legacy base/sidecar state only through a foreground-gated, reopen-verified SQLite promotion, then remove only recognized legacy cache files.
- Supersede the active `event-driven-mft-index-updates` change's five-to-ten-second durable sidecar publication contract; live in-memory freshness remains event driven, but durability follows this change's ten-minute foreground gate.
- Preserve folder-size consumer behavior, cache-budget controls, plugin ABI, LocalSystem service boundary, and NTFS-only scope.

## Capabilities

### New Capabilities

- `mft-sqlite-foreground-persistence`: Defines in-memory USN freshness, foreground-focus leases, ten-minute SQLite transactions, bounded WAL maintenance, shutdown/restart recovery, legacy migration, overload behavior, telemetry, and fixed-file Defender-sensitive I/O gates.

### Modified Capabilities

None in the archived baseline. The unarchived `event-driven-mft-index-updates` change is an implementation dependency whose durable-publication requirements are explicitly superseded by this new capability and must be reconciled before either change is archived.

## Impact

- MFT service coordinator, journal ingestion, index storage, query diagnostics, and named-pipe framing in `crates/explorer-app`.
- Super Explorer window focus/lifecycle integration and brokered MFT client state.
- `%ProgramData%\SuperExplorer\MftIndex` storage schema and destructive-but-verified legacy cache cleanup.
- Existing workspace `rusqlite`/bundled SQLite dependency becomes part of the installed MFT service binary; no new external runtime or network dependency is introduced.
- Installer upgrade/service lifecycle, Windows NTFS fixtures, foreground/background smoke tests, Defender CPU/I/O comparison, migration, recovery, and rollback evidence.
- Compatibility: the IPC addition is versioned; legacy cache files remain until SQLite verification succeeds; older binaries can require rebuilding after rollback and must not consume the SQLite store as legacy sidecars.
- Non-goals: Defender exclusions, non-NTFS indexing, folder-size semantic changes, cache-budget UI changes, plugin ABI changes, or shutdown-time persistence.
