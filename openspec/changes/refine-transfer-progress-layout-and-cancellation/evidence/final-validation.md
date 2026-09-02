# Final validation — 2026-09-02

## Automated gates

- `cargo fmt --all -- --check`: passed after formatting corrections.
- `cargo test -p explorer-ui operation_ --lib`: 11 passed.
- `cargo test -p explorer-app transfer_progress_reporter --lib`: 1 passed.
- `cargo test -p explorer-remote --lib`: 52 passed, including real-ADB discovery/root probes when the emulator was available.
- `cargo test -p explorer-remote cancellation --lib`: 5 passed; covers ADB runner kill, SFTP cancellation guard, local mid-copy cancellation, source preservation, and pre-copy cancellation.
- `cargo check -p explorer-app`: passed without warnings after corrections.
- `openspec validate refine-transfer-progress-layout-and-cancellation --strict`: passed; no placeholders found.

## Windows Shell note

The first broad filtered run exited once with a native access violation. A single-thread rerun completed 12/13 file-operation tests; the unrelated locked-delete fixture expected a sharing violation but Windows Shell returned `Cancelled`. No changed transfer-progress code participates in that fixture. All affected crates compiled and the remaining file-operation tests passed.

## Packaging and installed-build evidence

`build_test_install.bat` completed successfully for version `1.2026.9.2`, built the release executable and NSIS package, installed and launched the test build, and verified installed SHA-256 values. Installer output:

- `dist/SuperExplorer-Test-Setup-1.2026.9.2-x64.exe`
- Installed `SuperExplorer.exe` SHA-256: `AB5EAB1AA3F5DB27AED7FD90B241D0F40C33FA85451B4461719F0E1E90CBDBDB`

## User-perspective review

- Active render structure places `operation-cancel` inside a fixed `operation-cancel-region` of 250px.
- Summary and determinate/indeterminate progress are children of the remaining-width `operation-progress-region`, so the bar no longer spans behind Cancel.
- Cancel action immediately mutates request-correlated UI state to `正在取消`, disables duplicate action, and clears on exactly one terminal event or submission failure.
- ADB runner checks cancellation every 20ms and kills/waits the owned child.
- SFTP network awaits are raced against token callbacks; chunk/item scheduling stops after cancellation.
- Local and staged transfers recheck before every chunk/stage and before move cleanup.

An installed-app headful driver was attempted against ADB but Windows foreground arbitration sent its Ctrl+L chord to the file view rather than the address editor; logs showed `SelectAllItems`/disabled `Paste`, so this controller run was rejected rather than reported as product evidence. The installed binary itself launched normally and the compiled/state/provider gates above remain authoritative.
