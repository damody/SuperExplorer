# Final requirement and evidence traceability

| Requirement / scenarios | Gate | Implementation | Current evidence and disposition |
|---|---|---|---|
| Memory-first query, unfocused operation, typed loss of completeness | `G-REGRESSION`, `G-RESTART` | 1.2, 3.1 | Automated suites pass. The current installed unfocused trace ran 1,206.490 seconds with 603 mutations, nine fixed files, and zero cache events. |
| Authorized, expiring, multi-window leases and spoof/stall rejection | `G-FOCUS-AUTH` | 1.2.3-1.2.4, 3.2 | Deterministic and protocol suites pass. Full installed crash/disconnect/session-switch coverage remains open in 5.2.5. |
| Ten-minute success/attempt clocks, late focus, failure and focus loss | `G-TEN-MINUTE` | 1.2.2-1.2.4, 3.1.2 | Current installed focused trace ran 1,252.204 seconds. C and D each wrote in bursts near 600 s and 1,201 s; successive per-volume bursts were at least 600,000 ms apart. |
| Atomic entry/cursor commit and restart isolation | `G-SQLITE-ATOMIC` | 2.1, 2.2 | Schema, transaction, injection and restart suites pass. |
| Fixed SQLite file set and no generation/status churn | `G-FIXED-FILES` | 2.1.3-2.1.5, 3.1.1 | Both current long traces retained exactly nine main/WAL/SHM members and created no generation files. |
| Foreground-only checkpoint and bounded WAL | `G-WAL-BOUND` | 2.3 | Boundary, busy/failure and last-close suites pass. |
| Stop/shutdown linearization and restart catch-up | `G-NO-SHUTDOWN-WRITE`, `G-RESTART` | 3.3 | Fault suites pass. Repair and uninstall exercised SCM stop; a real Windows reboot trace remains open in 5.2.6. |
| Legacy migration, promotion, quarantine and cleanup | `G-MIGRATION`, `G-FIXED-FILES` | 4.1 | Complete fault matrix passes. A pre-SQLite installed legacy package was unavailable, so installed legacy-upgrade coverage keeps 4.2.4 open. |
| Diagnostics distinguish memory, durability, focus and failures | `G-REGRESSION` | 2.3.1, 3.1.3 | Automated diagnostics pass and current installed service identity is captured. |
| Installed Defender and cache-write comparison | `G-DEFENDER-IO` | 5.2 | Unfocused service read/write deltas were zero. Defender I/O/CPU deltas are environmental telemetry only; working set is explicitly excluded. |
| Installer repair, uninstall and rollback | `G-MIGRATION`, `G-NO-SHUTDOWN-WRITE`, `G-REGRESSION` | 4.2 | Current repair, uninstall, fresh reinstall, rollback to 1.2026.8.13, and restoration to 1.2026.8.14 all exited zero. Cache names and lengths were preserved. |
| Independent review | all | 6.1.3 | The prior review predates later source edits and is stale. User-directed single-agent execution prevents representing it as current independent review. |

## Evidence lineage

- `unfocused-installed-20m-current.json` is retained as a polluted failed sample because concurrent tests acquired legitimate foreground leases.
- `unfocused-installed-20m-current-clean.json` supersedes that sample for 5.2.3.
- `focused-two-deadlines-current.json` supersedes earlier focused traces for 5.2.4.
- Binary and installer hashes, rather than the mutable working-tree revision alone, bind installed traces to the tested candidate.
- Working-set values are environment metadata and are never treated as Defender scanning evidence.
