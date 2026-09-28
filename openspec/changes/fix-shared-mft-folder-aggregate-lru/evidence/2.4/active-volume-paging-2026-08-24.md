# Active-volume paging focused evidence — 2026-08-24

## Root cause

The D volume had about 437 GiB free, so the failure was not disk capacity. Installed diagnostics reported `recovery=7`, which maps to `LiveBudgetLimited`. The 1 GiB volume-index and 256 MiB file-data settings are global across mounted volumes. Startup admitted C first and left D with an incomplete working set. The complete D SQLite store contained 2,660,796 entries but its durable cursor was behind the current journal, so the service could not publish it as an exact current result.

## Implemented behavior

- The queried volume becomes the active recovery identity `(volume, journal id, observed generation, budget epoch)`.
- Non-active indexes and the target's incomplete index are released from memory; durable/observed cursors and every SQLite file remain intact.
- The target receives first claim on the configured live budgets.
- The existing per-volume watcher loads and journal-catches-up SQLite, then uses its bounded NTFS rebuild path when catch-up cannot prove exactness.
- Folder queries wait up to nine seconds for active-volume exactness, leaving response time inside the Host's ten-second terminal deadline.
- Background requests cannot preempt a recovery in progress and cause C/D oscillation.
- Partial values remain internal; genuine budget/rebuild failures report measured and configured bytes.

## Focused tests

All commands exited zero:

- `cargo test -p explorer-app --bin superexplorer-mft-service active_volume` — 4 passed.
- `cargo test -p explorer-app --bin superexplorer-mft-service queried_volume_gets_first_claim_on_the_existing_live_budget` — 1 passed.
- `cargo test -p explorer-app --bin superexplorer-mft-service budget_trimmed_target_returns_exact_after_active_recovery` — 1 passed.
- `cargo test -p explorer-app --bin superexplorer-mft-service another_volume_cannot_preempt_active_exact_recovery` — 1 passed.
- `cargo test -p explorer-app --bin superexplorer-mft-service result_lru_evicts_oldest_without_discarding_volume_indexes` — 1 passed.
- `cargo test -p explorer-app --lib folder_size_query_deadline_is_terminal_and_diagnostic` — 1 passed.
- `cargo test -p explorer-app --lib folder_size_workers_claim_independent_visible_requests` — 1 passed.
- `cargo test -p explorer-ui --lib code_lines_blocked_cells_share_limit_label_but_keep_distinct_reasons` — 1 passed.
- `cargo check -p explorer-app --bins -p explorer-ui`.
- `cargo build --release -p explorer-app --bin SuperExplorer --bin superexplorer-mft-service`.

No complete workspace test suite was run.

## Deployment boundary

The matched release service SHA-256 is `3A2929D0DC8EF230D5A2C1BDED39764749473878E1E7F12D98B860F81A1EDAAB`. `sc.exe stop SuperExplorerMft` returned `OpenService FAILED 5: Access is denied`, so the older installed LocalSystem service was not replaced and installed-path exactness is not claimed.
