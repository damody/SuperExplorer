# Final requirement and evidence traceability

| Requirement / scenarios | Blocking gate | Implementation tasks | Evidence task | Current disposition |
|---|---|---|---|---|
| Memory-first query; unfocused operation; typed loss of completeness | `G-REGRESSION`, `G-RESTART` | 1.2, 3.1 | 5.1.2, 5.1.3, 5.2.3 | Automated pass; the 1,201.232-second trace is stale after the final cursor/catch-up fix and must be repeated |
| Authorized, expiring and multi-window focus leases; spoof/stall rejection | `G-FOCUS-AUTH` | 1.2.3–1.2.4, 3.2.1–3.2.6 | 5.1.2, 5.2.5 | Automated pass; installed lifecycle matrix pending |
| Ten-minute success/attempt clocks; late focus; failure; focus loss | `G-TEN-MINUTE` | 1.2.2–1.2.4, 3.1.2 | 5.1.2, 5.2.4 | Deterministic pass; exact-candidate two-deadline trace pending |
| Atomic entry and cursor commit; failure/restart/later-event isolation | `G-SQLITE-ATOMIC` | 2.1, 2.2 | 5.1.1 | Passed |
| Fixed SQLite file set; no generation/status churn | `G-FIXED-FILES` | 2.1.3–2.1.5, 3.1.1 | 5.1.3, 5.2.3, 5.2.4 | Automated pass; current installed set is nine files and zero legacy files |
| Foreground-only checkpoint above 256 MiB; hard WAL bound; close behavior | `G-WAL-BOUND` | 2.3 | 5.1.1 | Passed |
| Stop/shutdown linearization; restart catch-up | `G-NO-SHUTDOWN-WRITE`, `G-RESTART` | 3.3 | 5.1.3, 5.2.5, 5.2.6 | Fault tests pass; installed reboot trace pending |
| Bounded overflow and focused serialized rebuild | `G-RESTART`, `G-REGRESSION` | 3.1.4–3.1.5 | 5.1.3 | Passed |
| Legacy migration, promotion, quarantine, scoped cleanup | `G-MIGRATION`, `G-FIXED-FILES` | 4.1 | 5.1.3, 4.2.4 | Fault matrix pass; installed lifecycle procedure pending |
| Diagnostics distinguish memory, durability, focus, failures | `G-REGRESSION` | 2.3.1, 3.1.3 | 5.1.2, 5.1.3, 5.2.2 | Automated pass; exact installed diagnostics captured |
| Existing folder-size consumers, IPC and budgets remain compatible | `G-REGRESSION` | 3.1.3, 3.2 | 5.1.2, 5.1.3 | Passed; D-drive exact query and extension headful smokes pass |
| Installed Defender and cache-write comparison | `G-DEFENDER-IO` | 5.2 | 5.2.1–5.2.7 | Post-fix short trace passes; final-fingerprint long traces remain pending |
| Installer identity, repair, uninstall and rollback | `G-MIGRATION`, `G-NO-SHUTDOWN-WRITE`, `G-REGRESSION` | 4.2 | 4.2.4 | Static lifecycle contract passes; destructive installed matrix pending |
| Final independent source review | all | 6.1 | 6.1.3 | Prior review is stale after later fixes; must be repeated |

## Evidence lineage

- Evidence with a source fingerprint other than the current validator fingerprint is stale and is not counted.
- The earlier focused file-event trace that observed a 599.995-second burst separation is superseded only after a new exact-candidate trace proves the final timing behavior; source reasoning alone is not counted.
- The earlier twenty-minute unfocused trace is superseded by the final exact-candidate trace recorded under the current service/application identities and source fingerprint.
- Working-set deltas are retained as environment metadata and never used as a Defender scanning proxy.
