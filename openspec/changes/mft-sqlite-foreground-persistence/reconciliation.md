# Durability reconciliation and inventory

## Cross-artifact disposition

| Artifact / implementation area | Reconciled contract | Current disposition |
|---|---|---|
| `mft_service.rs`, runtime and scheduler | Journal ingestion is memory-first; persistence requires an authenticated foreground lease and the ten-minute attempt/success clocks. | Implemented; deterministic tests and current installed long traces pass. |
| `mft_sqlite.rs` | Per-volume main/WAL/SHM, atomic entries-plus-cursor transactions, no automatic or close checkpoint, bounded WAL maintenance. | Implemented; schema, failure, restart and boundary suites pass. |
| `mft_focus.rs` and focus IPC | Process/image/session authenticated expiring leases aggregated across windows. | Implemented; automated security/protocol coverage passes. Remaining installed session-switch/crash coverage is task 5.2.5. |
| Service stop/shutdown lifecycle | Stop closes the lifecycle barrier and cannot initiate a final durability write; only an already-linearized commit may finish. | Fault tests pass; installer repair/uninstall exercised SCM stop. Real reboot evidence remains task 5.2.6. |
| Legacy migration and quarantine | Legacy state is read-only until both gates; temporary SQLite is verified before promotion; cleanup is root- and identity-scoped. | Full deterministic fault matrix passes. No pre-SQLite installer is available for installed legacy-upgrade coverage in 4.2.4. |
| Installer | Stop before replace/delete, bundled SQLite, preserve `%ProgramData%\SuperExplorer\MftIndex`. | Repair, uninstall, fresh reinstall, previous-version rollback and current-version restoration pass with exact hashes. |
| `event-driven-mft-index-updates` | Event-driven ingestion remains authoritative; its rapid sidecar durability wording is superseded by this SQLite contract. | Related change is complete and strict-valid. |
| Diagnostics and evidence scripts | Diagnostics are IPC/memory-backed; installed evidence uses fixed-file events and process CPU/I/O counters. | Repeatable capture script produced clean unfocused and focused traces. Working set is not used as a Defender proxy. |

## Gate summary

| Gate | Evidence disposition |
|---|---|
| `G-SQLITE-ATOMIC`, `G-WAL-BOUND`, `G-RESTART` | Passing automated transaction, failure, WAL and restart suites. |
| `G-FOCUS-AUTH`, `G-TEN-MINUTE` | Passing deterministic suites plus current 20-minute unfocused and two-deadline focused traces. |
| `G-FIXED-FILES` | Nine fixed SQLite members before and after both long traces; no generation files. |
| `G-MIGRATION` | Fault matrix and rollback preservation pass; pre-SQLite installed upgrade remains open. |
| `G-NO-SHUTDOWN-WRITE` | Injection and SCM lifecycle coverage pass; real reboot trace remains open. |
| `G-DEFENDER-IO` | CPU/I/O and file events captured with attribution limits stated; working-set-only conclusions excluded. |
| `G-REGRESSION` | Relevant application, service, extension and UI library tests pass; warnings-denied Clippy remains open due active cache-budget lints. |

Unchecked tasks are deliberate and are not weakened or represented as passing in the release disposition.
