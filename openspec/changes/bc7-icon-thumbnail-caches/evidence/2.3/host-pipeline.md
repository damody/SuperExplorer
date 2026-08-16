# G-HOST-PIPELINE evidence

Status: passed on 2026-08-14.

The Host now owns a bounded background runtime in `crates/explorer-shell-win/src/bc7_pipeline.rs`. Its conversion identity includes content kind, Shell source identity, presentation size, association/overlay generations, and format schema. A 32-entry synchronous queue, two named workers, a 64 MiB aggregate staging reservation, codec dimension/output validation, and a ten-second default deadline bound admitted work. Exact duplicates single-flight; newer generations supersede older jobs; cancellation, elapsed deadlines, disabled per-kind gates, and stale generations are checked again immediately before atomic publication.

Cold icon and thumbnail provider results return owned RGBA immediately and enqueue BC7 persistence. Warm validated disk hits return compressed blocks directly. Icon and thumbnail gates, roots, and quota ownership remain independent. Temporary encode buffers and aggregate reservations are released on success, rejection, cancellation, stale completion, and persistence failure.

`Bc7JobStatsV1` exposes current/peak queue, active jobs, staging reservation and limits plus submitted, completed, duplicate, overload, oversized, cancelled, stale, persistence-error, and fallback counters. Existing independent memory, disk, encoder, and GPU telemetry remains available through its owning caches.

Verification:

- `cargo test -p explorer-shell-win --lib --locked --offline bc7_pipeline -- --nocapture --test-threads=1`: 7 passed, 0 failed.
- `cargo test -p explorer-shell-win --lib --locked --offline icon_disk_cache::tests -- --nocapture --test-threads=1`: 15 passed, 0 failed.
- `cargo test -p explorer-shell-win --lib --locked --offline thumbnail::tests -- --nocapture --test-threads=1`: 5 passed, 0 failed.
- The background-persistence icon contract passes. The remainder of the broad `icon::tests` filter is environment-blocked because `tempfile` resolves a fixture under missing `D:\test\target`; subsequent tests observe the shared test mutex poisoned.
- `cargo check -p explorer-shell-win --locked --offline`: passed.
- `cargo clippy -p explorer-shell-win --lib --locked --offline -- -D warnings`: the new BC7 files are clean; the command remains blocked by pre-existing warnings in unrelated `context_menu`, `drag_drop`, `everything`, `file_operation`, `navigation`, `search`, `sta`, `watcher`, and public-doc code.

Source SHA-256 after verification:

- `bc7_pipeline.rs`: `D94E352A6F8A651B2BF3E9F65285A9E9C89F50F1A3DE54967DD8D3494A1BF2F5`
- `icon.rs`: `A17874146D9FADE6FA0F36FD163F5F4FC31388DC725E54108DEF913882A2EF6A`
- `thumbnail.rs`: `5C6955EF5D5877138A20CD7A46778235E70A14532F71A5127179ACBFAD63164E`
- `icon_disk_cache.rs`: `8A27724E3376429550E3B878756006A31E8B3693FBFE87F38BBB339396934EDC`

These hashes describe the implementation at the time of the initial gate run; subsequent formatting-only or follow-on telemetry edits must be represented by later evidence rather than silently rewriting this record.
