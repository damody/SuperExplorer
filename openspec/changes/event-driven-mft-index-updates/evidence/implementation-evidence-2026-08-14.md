# Event-driven MFT index implementation evidence — 2026-08-14

Source revision: `fd66c9e8a759ca903b55b1e9ea156c42d37518e1`

## Outcome

The event-driven journal protocol, memory-first service state, atomic SQLite durability path, Host delta application, scoped cache invalidation, migration, installed-service lifecycle, and NTFS mutation fixture are implemented. This pass also fixed an all-or-nothing defect: an `Invalidate` change previously detached a child before returning the recovery error. `MftIndexV1::apply_change` now rejects ambiguous topology before any mutation.

The later `mft-sqlite-foreground-persistence` change supersedes normal sidecar durability. Legacy checkpoint/delta records remain supported only as bounded migration input, and their chain validation is now shared by the Host loader.

## Gate records

| Gate | Evidence |
| --- | --- |
| G-PROTOCOL | `mft_journal` round-trip, corruption, publication ordering, cursor compatibility, and exact-next-chain tests passed. |
| G-JOURNAL | Bounded journal parsing, reason normalization, coalescing, cursor rejection, and cancellation tests passed in both library and service-binary suites. |
| G-INIT | Runtime startup catch-up, initial-only expedited recovery, failed replacement restoration, incomplete snapshot promotion, and idempotent restart replay tests passed. |
| G-COALESCE | Quiet/max-deadline, redundant-reference accounting, ambiguity, pending-memory bounds, and failed-batch merge tests passed. |
| G-LIFECYCLE | Named-pipe/journal cancellation, stop-inhibited persistence, lifecycle barrier, and shutdown linearization tests passed. |
| G-HOST-INDEX | Exact contiguous delta validation now rejects gaps, duplicates, journal mismatch, volume mismatch, and USN regression before cursor publication. SQLite transaction/failure tests prove entries and cursor remain atomic. |
| G-CACHE | The mutation matrix covers grow, overwrite/truncate, create, delete, same-parent rename, cross-parent move, hard-link accounting, ambiguity, old/new ancestry, and unrelated topology retention. |
| G-MIGRATION | Legacy cleanup/quarantine, compatible read-only restart, schema/corruption rejection, interrupted promotion, backup recovery, rollback-journal migration, and path/identity defenses passed. |
| G-NTFS | `test_mft_event_ntfs_mutations.ps1` executed all real NTFS mutations successfully in an isolated temporary fixture. It also contains bounded blocked-reader stop/restart and an opt-in disposable-volume discontinuity procedure; production journals are never modified implicitly. |
| G-INSTALLED | Formatting and scoped OpenSpec validation passed; both focused Rust suites passed; the UTIT manifest parser/validator unit suite passed. The repository-wide UTIT coverage gate still reports 201 requirements belonging to other active changes, so it is recorded as an unrelated workspace condition rather than attributed to this change. |

## Commands and results

- `cargo fmt --all -- --check` — PASS.
- `cargo test -p explorer-app --lib mft_ -- --test-threads=1` — PASS: 86 passed, 0 failed, 1 ignored opt-in real-volume test.
- `cargo test -p explorer-app --bin superexplorer-mft-service -- --test-threads=1` — PASS: 98 passed, 0 failed, 2 ignored opt-in installed/elevated tests.
- `cargo test -p explorer-uitest --lib -- --nocapture` — PASS: 15 passed, 0 failed.
- `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/test_mft_event_ntfs_mutations.ps1 -OutputDirectory target/openspec-evidence/event-driven-mft-index-updates/4.2.ntfs-mutations-local -SkipServiceStop` — PASS; report status `passed`, NTFS mutation matrix complete.
- `openspec validate event-driven-mft-index-updates --strict` — PASS.
- `cargo run -p explorer-uitest --bin explorer-uitest -- --validate-only` — manifest parsed, then FAIL at the global coverage gate due to 201 uncovered requirements across other active changes. The new case itself is structurally valid and the `explorer-uitest` validation tests pass.

## Source hashes

- `crates/explorer-app/src/mft_journal.rs`: `4e5e511c1135b925049b10ad4f480c241495420b08276bbad4bf60501de70004`
- `crates/explorer-app/src/mft_size_map.rs`: `dd08b8a20750ba22591c4268f3f38a6f093c037a7e6edea57db7cb321c67982d`
- `crates/explorer-app/src/folder_size_service.rs`: `2a5221def0f267bb4ebfc0e3dc42cdc8aadef68c8c27a7653490d1c0229bcc9d`
- `scripts/test_mft_event_ntfs_mutations.ps1`: `9503f3bd34e799df5663743f63e05e2c3213805eccc394f2c335207633d1d2e5`
- `uitest/manifest.json`: `98b945262b11aecd029892636ab5a40c833d198f9737262a29f925842cb950e2`

## Capability disposition

The local fixture volume is NTFS and all non-destructive mutation operations ran. Service stop/restart was deliberately not exercised by the local mutation-only command because that mode was selected explicitly; the installed-service stop behavior remains covered by the service cancellation tests and the already-registered installed lifecycle/resource cases. Journal recreation remains opt-in and requires an explicitly supplied disposable NTFS volume; absence of such a volume is reported as a capability skip, never as a pass.
