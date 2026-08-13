# Durability reconciliation and inventory

## Inventory

| Location | Assumption | Disposition |
|---|---|---|
| `crates/explorer-app/src/bin/mft_service.rs` | `watch_volume` publishes checkpoint/delta/status sidecars after the 5/9-second policy | Replace with memory-first state and gated SQLite transaction; retain legacy read/migration only. |
| `crates/explorer-app/src/mft_journal.rs` | `publication_due`, sidecar paths, codecs, discovery, and status writes are the live durability mechanism | Keep codecs/readers for legacy migration tests; remove live publication calls and move diagnostics to IPC. |
| `crates/explorer-app/src/folder_size_service.rs` | Host reconstructs the current volume by reading base plus every delta | Route current queries to service memory/SQLite-backed service contract; preserve legacy compatibility only during migration. |
| `scripts/smoke_mft_event_service.ps1` | Freshness is proven by a new `.semftcp` within ten seconds | Replace with foreground/background fixed-file, memory-query, ten-minute attempt, stop, and Defender-I/O observations. |
| `openspec/changes/event-driven-mft-index-updates/{proposal,design,specs,tasks}.md` | Durable sidecar publication occurs within ten seconds | Superseded explicitly; USN ingestion, coalescing, query freshness, identity/cursor validation, and bounded correctness remain. |
| `%ProgramData%\SuperExplorer\MftIndex` | Normal operation accumulates generation sidecars and rewrites status | SQLite main/WAL/SHM are the fixed live set; legacy files become verified migration inputs and then scoped cleanup targets. |

Generic uses of “generation” outside the MFT journal/service paths are unrelated and remain unchanged.

## Requirement traceability

| Approved behavior / requirement | Gate | Work packages | Evidence leaves |
|---|---|---|---|
| Memory-first live state and exactness loss | `G-REGRESSION`, `G-RESTART` | 1.2, 3.1 | 1.2.4, 3.1.3–3.1.5, 5.1.1, 5.1.3 |
| Authenticated expiring focus leases | `G-FOCUS-AUTH` | 1.2, 3.2 | 1.2.3–1.2.4, 3.2.1–3.2.6, 5.1.2, 5.2.5 |
| Ten-minute successful-commit and write-attempt clocks | `G-TEN-MINUTE` | 1.2, 3.1 | 1.2.2, 1.2.4, 3.1.2, 5.1.2, 5.2.3–5.2.4 |
| Atomic entries/cursor transaction | `G-SQLITE-ATOMIC` | 2.1, 2.2 | 2.1.2–2.1.5, 2.2.1–2.2.4, 5.1.1 |
| Fixed active file set and IPC diagnostics | `G-FIXED-FILES` | 2.1, 3.1 | 2.1.3, 2.1.5, 3.1.1, 5.1.3, 5.2.3–5.2.4 |
| 256 MiB foreground checkpoint and computed hard WAL bound | `G-WAL-BOUND` | 2.3 | 2.3.1–2.3.5, 5.1.1 |
| No new shutdown write and commit linearization | `G-NO-SHUTDOWN-WRITE` | 3.3 | 3.3.1–3.3.5, 5.1.3, 5.2.5–5.2.6 |
| Foreground-gated rebuild/overflow | `G-RESTART`, `G-REGRESSION` | 3.1, 3.3 | 3.1.4–3.1.5, 3.3.5, 5.1.3 |
| Absent-only migration, verified promotion, staged quarantine | `G-MIGRATION` | 4.1 | 4.1.1–4.1.7, 5.1.3 |
| Bundled packaging and rollback | `G-MIGRATION`, `G-REGRESSION` | 4.2 | 4.2.1–4.2.4 |
| Installed write/Defender behavior | `G-DEFENDER-IO` | 5.2 | 5.2.1–5.2.7 |
| Final cross-artifact/evidence closure | all | 6.1 | 6.1.1–6.1.5 |
