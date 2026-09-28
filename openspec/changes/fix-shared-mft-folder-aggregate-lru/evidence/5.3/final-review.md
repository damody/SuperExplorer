# Focused implementation review — 2026-08-24

## Implemented

- Details Folder size, File Count, and Folder Count now use one direct `mft_query::query_folder` response. The Host aggregate cache and `aggregate_or_scan` fallback are not used by this Details worker.
- The MFT service no longer holds its global result-cache mutex while SQLite or MFT subtree computation runs.
- Same `(volume serial, folder reference, observed generation)` misses share one service-global flight. Waiters block only on the per-flight condition variable; different keys remain independent across the four pipe workers.
- Successful cache reads and replacements promote the access sequence. Result accounting uses a conservative 192-byte minimum, trims the oldest entry for both byte and count ceilings, and never evicts volume indexes.
- When an observed generation changes, stale results for that volume are retired before lookup; other volumes remain warm. A computation is rejected if the observed generation changes before publication.
- Obsolete `%LOCALAPPDATA%\SuperExplorer\folder-snapshot-cache\v2` records are retired in batches of at most 256. Cleanup accepts only immediate, regular, non-reparse `.json` files with the expected record schema and never traverses subdirectories.
- The pre-existing `RemoteExplorerService` composition in `application.rs` was preserved.
- Four bounded visible-query workers prevent one slow folder from blocking all later rows. Each request has a hard ten-second Host deadline.
- Partial service responses are rejected before cache publication and render as `Unavailable`; Host and service console messages include the path, request/generation context, numeric partial facts, elapsed time, and durability diagnostics available at that layer.
- `RemoteExplorerService` now forwards local cache telemetry, allowing Folder Options to show measured current usage. Confirmed telemetry failures show `Unavailable / <limit>`, while a pending refresh retains the last successful sample.
- Global live-memory limits now operate as an active-volume working set. A query for an incomplete D index evicts only non-active/incomplete memory, preserves SQLite and cursors, waits for SQLite catch-up or bounded NTFS rebuild, and prevents background C/D requests from preempting the recovery.

## Focused verification

All commands exited 0:

- `cargo test -p explorer-app --bin superexplorer-mft-service result_lru_evicts_oldest_without_discarding_volume_indexes -- --nocapture`
- `cargo test -p explorer-app --bin superexplorer-mft-service generation_advance_retires_only_that_volumes_stale_results -- --nocapture`
- `cargo test -p explorer-app --lib obsolete_details_snapshot_cleanup_is_bounded_and_non_recursive -- --nocapture`
- `cargo test -p explorer-app --lib active_folder_size_request_is_not_queued_again_by_repeated_ui_submissions -- --nocapture`
- `cargo test -p explorer-app --lib partial_response_uses_typed_status_without_changing_fixed_frame -- --nocapture`
- `cargo test -p explorer-app --lib folder_size_query_deadline_is_terminal_and_diagnostic`
- `cargo test -p explorer-app --lib folder_size_workers_claim_independent_visible_requests`
- `cargo test -p explorer-app --lib remote_decorator_forwards_local_cache_telemetry`
- `cargo test -p explorer-app --bin superexplorer-mft-service shared_service_rejects_partial_values_with_cursor_diagnostics`
- `cargo test -p explorer-ui --lib partial_value_becomes_unavailable_and_is_excluded_from_sort_domain`
- `cargo test -p explorer-ui --lib zero_partial_is_terminal_unavailable_without_automatic_retry`
- `cargo test -p explorer-ui --lib cache_budget_usage_text_reserves_unavailable_for_confirmed_failure`
- `cargo test -p explorer-ui pending_sample_retains_last_success_and_unavailable_does_not_claim_pending`
- `cargo test -p explorer-app --bin superexplorer-mft-service active_volume`
- `cargo test -p explorer-app --bin superexplorer-mft-service budget_trimmed_target_returns_exact_after_active_recovery`
- `cargo test -p explorer-app --bin superexplorer-mft-service another_volume_cannot_preempt_active_exact_recovery`
- `cargo test -p explorer-ui --lib code_lines_blocked_cells_share_limit_label_but_keep_distinct_reasons`
- `cargo build --release -p explorer-app --bin superexplorer-mft-service --bin SuperExplorer`
- `openspec validate fix-shared-mft-folder-aggregate-lru --strict`
- `git diff --check`

Release artifacts:

- `target/release/SuperExplorer.exe`: SHA-256 `F4DDF1C3D011DEFF235E44D3C12F473AEAF00946048170FCD5D70D10F9EFBEDD`
- `target/release/superexplorer-mft-service.exe`: SHA-256 `3A2929D0DC8EF230D5A2C1BDED39764749473878E1E7F12D98B860F81A1EDAAB`

## Installed validation boundary

The running installed service remained `Running`; `sc.exe stop SuperExplorerMft` returned `OpenService FAILED 5: Access is denied`, so no installed binary was stopped, replaced, or restarted. The newly built service cannot admit the protected live MFT volume without LocalSystem privileges. Therefore the installed required-location cold/warm gate is not claimed as passed.

The existing installed service was queried read-only at `D:\`, `D:\SuperExplorer`, and `D:\UE_5.7`. After ten seconds at each location, all twelve sampled children still returned typed partial values; the new client rejected them in 13–24 ms. Installed diagnostics reported `D` as `exact=false` with durable generation `0`. This establishes exact-only rejection and the deployment blocker, not acceptance of the older installed service binary. Full details are in `evidence/5.2/required-locations-exact-only-2026-08-24.md`.

No complete workspace test suite was run, per the requested scope.
