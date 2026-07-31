# 驗證與需求追蹤

| Requirement | Automated verification | Evidence |
|---|---|---|
| 主程式使用一致的 SuperExplorer 產品識別 | `explorer-app/tests/product_identity.rs`; `finalize_windows_artifact.ps1 -Profile release` | `target/release/SuperExplorer.exe`; `target/manifest-evidence/SuperExplorer-release.manifest`; `target/uitest-runs/superexplorer-product-identity/report.json` |
| 視窗與工作列標題反映作用中瀏覽位置 | `explorer-ui` title projection tests; `explorer-ui/tests/window_title.rs`; `superexplorer-window-title` headful UITEST | `target/uitest-runs/superexplorer-window-title/report.json`; `target/uitest-runs/superexplorer-window-title/evidence/superexplorer-window-title/window-title-cross-drive.png` |
| 虛擬位置具有可讀且安全的標題回退 | `virtual_window_title_never_exposes_empty_or_internal_identity`; `resolved_history_entries_project_native_window_titles` | targeted `cargo test -p explorer-ui window_title --locked` pass |
| 封裝與驗證工具只依賴新的主程式產物 | product identity static consumer test; NSIS `/WX`; roadmap installer smoke | `dist/SuperExplorer-Setup-1.2026.7.29-x64.exe`; `target/roadmap-installer-evidence/superexplorer-rename/report.json` |
| 改名不得遺失既有使用者資料或降低自我程序保護 | persisted-root contract; Restart Manager new/legacy image-name test | `cargo test -p explorer-app --test product_identity`; serial `cargo test -p explorer-shell-win --lib` pass |

## Gates

- `cargo fmt --check`: PASS
- OpenSpec strict validation: PASS
- UITEST manifest／active OpenSpec coverage: 270/270 requirements
- `superexplorer-product-identity`: PASS
- `superexplorer-window-title`: PASS across owned C:/D: fixtures; process title and UIA title both matched the complete active path
- release PE/manifest/VERSIONINFO finalization: PASS
- NSIS install, in-place upgrade, installed-path launch/broker handshake and uninstall: PASS
- combined parallel package test initially encountered transient Windows Clipboard contention (`OpenClipboard 0x800401D0`) and poisoned dependent locks; isolated single-thread rerun passed 110 tests with 7 environment-gated ignores and zero failures
