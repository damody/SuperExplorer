# Final validation

Validated on 2026-09-02 from the user-facing release artifact.

## Automated checks

- `cargo fmt --all -- --check`: passed.
- `cargo test -p explorer-app remote_cancel_targets_exact_token_cleans_registry_and_delegates_unknown_requests --lib`: passed.
- `cargo test -p explorer-ui operation_ --lib`: 11 passed.
- `cargo test -p explorer-remote cancellation --lib`: 5 passed.
- `cargo check -p explorer-app`: passed.
- `openspec validate fix-remote-cancel-and-fluent-progress --strict`: passed.
- Focused `git diff --check`: passed.

## Build and installation

- `build_test_install.bat`: passed and installed the test package.
- Release and installed `SuperExplorer.exe` SHA-256 matched:
  `C9074B089614038A5A0270E7E03685A9E61B0EBE6FF2B9693283498EF2E090D8`.

## User-perspective checks

- Local clipboard to `adb://emulator-5554/sdcard/Download`: operation UI appeared, compact Cancel button measured 151 px, Fluent rounded progress track filled the remaining region, Cancel reached terminal UI state in 102.2 ms, and no destination payload remained.
- Local clipboard to `sftp://45.32.49.125/home/linuxuser`: operation UI appeared, compact Cancel button measured 151 px, Fluent rounded progress track filled the remaining region, and Cancel reached terminal UI state in 1073.9 ms.
- Evidence screenshots and reports:
  - `build/verify-cancel-adb-passed/active-transfer.png`
  - `build/verify-cancel-adb-passed/report.json`
  - `build/verify-cancel-sftp-passed2/active-transfer.png`
  - `build/verify-cancel-sftp-passed2/report.json`

The checks used the release executable whose hash matches the installed executable, so the exercised binary is byte-identical to the installed application.
