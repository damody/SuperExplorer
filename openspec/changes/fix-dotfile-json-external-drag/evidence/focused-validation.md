# 聚焦驗證

以下檢查均通過，未執行完整迴歸：

- `cargo fmt --all`
- `cargo test -p explorer-app remote_external_drop -- --test-threads=1`：4 passed
- `cargo test -p explorer-ui drag -- --test-threads=1`：16 passed
- `cargo test -p explorer-shell-win drag_drop -- --test-threads=1`：5 passed、1個interactive-only ignored
- `cargo check -p explorer-app`
- `scripts/finalize_windows_artifact.ps1 -Profile release`：release、manifest、VERSIONINFO與PE x64驗證通過
- `build_test_install.bat --no-launch`：exit 0，產生`dist/SuperExplorer-Test-Setup-1.2026.9.1-x64.exe`（10,534,109 bytes）

最終仍執行`git diff --check`、credential掃描、44筆evidence index檢查、詳細tasks validator及`openspec validate --strict`。
