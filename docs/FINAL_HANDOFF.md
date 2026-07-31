# SuperExplorer 最終交付

> 2026-07-29 post-parity roadmap 的最新 runtime、installer、state/cache/reset、broker/preview、測試證據與限制，請以 `docs/POST_PARITY_ROADMAP_HANDOFF.md` 為準；正式 closure review 位於 `docs/POST_PARITY_ROADMAP_REVIEW.md`。

交付日期：2026-07-27（Asia/Taipei）

OpenSpec changes：`build-rust-gpui-windows-explorer`、`match-explorer-visual-address-parity`

## 成品與固定依賴

- Debug binary：`target/debug/SuperExplorer.exe`
- Release binary：執行 `./scripts/finalize_windows_artifact.ps1 -Profile release` 後位於 `target/release/SuperExplorer.exe`
- GPUI：`vendor/gpui-ce` submodule，來源 `https://github.com/gpui-ce/gpui-ce.git`，gitlink `f9740c88e5f799cef36c14662e3bccff9e0ca363`
- Rust target：`x86_64-pc-windows-msvc`；Cargo.lock、submodule 與 Windows resource 都由 CI/architecture gate 固定驗證

## 已實作能力

- Windows 11 風格 GPUI-CE 視窗：原生 caption hit-test、Snap、light/dark/high-contrast semantic theme、IME text input、AccessKit/UIA、keyboard/mouse focus traversal、DPI-aware layout。
- 多分頁真實資料夾：per-tab history/generation、Back/Forward/Up、地址列、watcher、100k 項目分批列舉、Unicode/reparse/long path 防護。
- 真實檔案操作：建立、rename、copy、move、Recycle Bin／永久刪除、衝突決策、cancel、progress、journal/undo 與 owned destructive fixtures。
- Clipboard/OLE：Shell `IDataObject` copy/cut/paste、跨分頁 ownership、Explorer 單檔/多檔 copy/cut 互通、OLE source/target、left/right effect negotiation、folder/background/navigation target、drag cue/auto-scroll/cleanup。
- Context menu：`IContextMenu3`、background/single/multi、owner-draw routing、submenu、keyboard invoke、installed 7-Zip 真實 archive invoke。
- Search：typed query、Windows Search probe、未索引 fallback、partial/cancel/stale isolation、per-tab history、100k 項目效能與錯誤注入。

## 啟動與測試命令

```powershell
git submodule update --init --recursive
cargo run -p explorer-app --locked
./scripts/finalize_windows_artifact.ps1 -Profile release
./scripts/run_headful_validation.ps1 -SkipBuild -OutputDirectory target/headful-evidence/final
./scripts/capture_dpi_matrix.ps1 -OutputDirectory target/dpi-evidence/final
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
openspec validate build-rust-gpui-windows-explorer --strict
```

以指定真實資料夾啟動：

```powershell
$env:EXPLORER_INITIAL_PATH='D:\'
./target/debug/SuperExplorer.exe
```

環境變數只接受「已存在的絕對資料夾」；不合法值會在建立視窗前明確失敗。

## 真實資料夾與 Explorer evidence

- 最終 Explorer `D:\` light comparator：`target/explorer-reference-evidence/real-d-light-all-gates-parity-final/report.json`，15/15 regions、4/4 icons、4/4 colors、8/8 typography 通過；最大 geometry delta 8.56%。
- Breadcrumb UIA：`target/breadcrumb-uia-evidence/20260727-parity-final-v5`；真實磁碟／直接子資料夾列舉、topmost hit-test與 click navigation 通過。
- Sort/columns：`target/sort-column-evidence/20260727-parity-final/report.json`；views/panes：`target/view-pane-evidence/20260727-parity-final/report.json`；scrollbar capture：`target/scrollbar-capture-evidence/20260727-parity-final/report.json`。
- 最終七步 headful：`target/headful-evidence/20260727-parity-final-v4/report.json`；IME：`target/ime-evidence/20260727-parity-final-v2/report.json`；dark：`target/explorer-reference-evidence/dark-parity-final-v4`；high contrast：`target/high-contrast-evidence/20260727-parity-final`。
- Shell icon disk cache：`%LOCALAPPDATA%\RustGpuiExplorer\icon-cache\v1`。可在程式未執行時刪除整個 `v1` 目錄；下次啟動會由 Windows Shell 重新建立，不影響檔案本體。

- 真實 D: 參考：使用者提供的 Windows Explorer 175% DPI 畫面；app 可用 `EXPLORER_INITIAL_PATH=D:\` 走同一資料來源。
- Explorer Clipboard：`real_explorer_single_multi_copy_cut_paste_matrix_matches_disk_effects` 已通過單檔/多檔 Copy 與 Cut，逐項檢查目的檔 bytes 與 Cut 來源消失。
- App→Explorer Clipboard：`real_ole_clipboard_copy_cut_paste_crosses_tabs_and_matches_disk` 以 Explorer Shell paste 實際建立目的檔。
- Context menu：installed 7-Zip extension 真實產生非空 `.7z`。
- Headful：`target/headful-evidence/20260727-final-v3/report.json` 七步全部 exit code 0。
- DPI：`target/dpi-evidence/20260727-final/report.json`；typed 100/125/150/200% contract 通過，實際唯一螢幕為 175%，mismatch 明確保留。
- OLE drag：`docs/OLE_DRAG_DROP_EVIDENCE.md` 與 `scripts/smoke_explorer_drag_interop.ps1` 保存 strict 跨程序 matrix、DragEnter/Over 證據與本 runner 的 `DROPEFFECT_NONE` 限制。

## 已知限制

- 本 Codex desktop 的合成滑鼠 release 無法完成真正 Explorer↔app 的跨程序 OLE Drop；功能管線、真實 data object、DragEnter/Over、effect/terminal 與資源 cleanup 已分層驗證，但 physical Drop 必須在具硬體輸入／合格 input driver 的互動式 GUI runner 重跑。不得把目前 None 結果列為 parity pass。
- 唯一螢幕為 175%；100/125/150/200% 的 typed geometry 已驗證，正式 raster baseline 仍需四種實際 session。
- 完整 Shell namespace、thumbnails、preview handlers、虛擬 namespace/broker 隔離不在目前 binary 的 production hardening 範圍；目前以 filesystem-first 與既有 Shell extension boundary 為主。
- Windows Search 對 temporary/未索引 root 可能不可用，會明確切換 bounded filesystem fallback，不偽裝成 index 結果。

## 後續 hardening 邊界

- Thumbnails：加入 bounded decode/cache、取消、檔案更新失效與惡意 codec 隔離。
- Namespace：將 filesystem identity 擴充到非檔案系統 PIDL、Known Folder、network/cloud provider 與 property system columns。
- Preview：以 out-of-process broker host preview handler，加入 integrity/time/memory budget 與 crash quarantine。
- Broker：把不可信 Shell extension、thumbnail/preview codec 與長時間 search provider 移入可重啟低權限程序；維持目前 typed protocol、generation/cancellation 與 terminal-event contract。
