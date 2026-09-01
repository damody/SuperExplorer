# Focused validation evidence

Date: 2026-09-01 (Asia/Taipei)

- `cargo check -p explorer-remote -p explorer-app`: passed.
- `cargo test -p explorer-remote adb::tests`: 25 passed.
- `cargo test -p explorer-remote transfer::tests`: 9 passed.
- `cargo test -p explorer-model operation::tests`: 5 passed.
- `cargo test -p explorer-app remote_service::tests::transfer_progress_reporter_is_monotonic_degrades_unknown_and_rejects_late_callbacks`: passed.
- `cargo test -p explorer-ui chrome::tests::operation_message_distinguishes_create_rename_delete_and_terminal_results`: passed.
- `remote_owned_fixture_probe adb-progress emulator-5554`: `adb_native_progress_verified=true`; both push and pull produced strictly increasing intermediate observations before the terminal total and verified the downloaded length.
- `remote_owned_fixture_probe cross <controlled-profile>`: `cross_provider_matrix_verified=true`; ADB→SFTP and SFTP→ADB completed with monotonic two-stage progress and cleaned marker-owned remote fixtures.
- Source scan found no former progress-only tree-size/stat polling loop in `adb.rs`.
- Progress callbacks transport only byte deltas; they add no path, username, password, or credential field.
- `cargo fmt --all` and `git diff --check`: passed (Git emitted line-ending notices only).

Credentialed fixture input was supplied interactively and is intentionally absent from commands and evidence.
