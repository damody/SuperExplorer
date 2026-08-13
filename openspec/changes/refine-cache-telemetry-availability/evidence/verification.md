# Verification

- `cargo test -p explorer-model cache_telemetry --no-fail-fast`: passed (3 tests).
- `cargo test -p explorer-ui cache_budget_usage_text_reserves_unavailable_for_confirmed_failure --no-fail-fast`: passed.
- `cargo test -p explorer-ui pending_sample_retains_last_success_and_unavailable_does_not_claim_pending --no-fail-fast`: passed.
- `cargo test -p explorer-app mft_missing_telemetry_is_pending_until_a_terminal_protocol_failure --lib --no-fail-fast`: passed.
- `cargo fmt --all -- --check`: passed.
- `openspec validate refine-cache-telemetry-availability --strict`: passed.
- Debug headful Folder Options smoke: passed; `telemetry/folder-options-cache-mft.png` shows pending MFT rows as `— / configured limit` rather than `Unavailable`.
- `build_test_install.bat --no-launch`: produced `dist/SuperExplorer-Setup-1.2026.8.7-x64.exe` with SHA-256 `A2D3A5ED07D6D79B663BCC81F3AEF808D22C6EC68030942080FCDF51AC3B7E73`.
- Installed binary verification: `C:\Program Files\SuperExplorer\SuperExplorer.exe` and `target\release\SuperExplorer.exe` both have SHA-256 `54C8DCD6C7C7327DE3539A13A358B1328A5433F607042CF59417124349604133`.
- Installed headful cache-budget smoke: passed; screenshots and report are under `installed/`.
- Windows service after installation: `SuperExplorerMft`, Running, Automatic.
