## Why

目前工作區只有已核准的 Windows 11 檔案總管設計規格，尚無可編譯、可使用、可驗收的 Rust／GPUI 檔案管理器。只交付靜態 M0/M1 shell 無法驗證產品核心價值；本 change 必須繼續完成多分頁真實資料夾瀏覽、原生檔案操作、Windows Shell 資料交換／選單與搜尋，並以真實資料夾及 Explorer 互通測試證明行為。

## What Changes

- 建立 Windows-only Cargo workspace、分層 crate 骨架與可執行的 GPUI 應用程式。
- 將 GPUI 改用 `gpui-ce/gpui-ce` Git submodule，固定 revision `6c799b8e994266233014cea66d7769675ec1967c` 並提交可重現的 `Cargo.lock`；因目前 `gpui-component` main 與該 revision 有 11 個已驗證的 API 編譯衝突，本 change 使用 GPUI-CE 原生 elements 與專案內 semantic control helpers，不混用第二套 GPUI 或未發布的私有 fork。
- 建立 logging、panic hook、DPI／Windows 啟動先決條件、受控啟動與關閉流程，以及最小而真實的 Shell STA 生命週期邊界。
- 實作可調整大小的 Windows 11 25H2 Explorer shell：title／tab chrome、多分頁、command bar、history/up controls、可輸入 breadcrumb/address、search、navigation pane、真實 file view 與 status bar。
- 建立集中式 light／dark／high-contrast-ready semantic theme 與 layout tokens，並提供基本 actions、快捷鍵與 focus routing。
- 建立 per-tab location、history、generation/cancellation、stable item identity、增量列舉與 watcher merge，使多分頁可同時瀏覽真實本機資料夾而不互相污染狀態。
- 以小型、100,000 項目、Unicode、長檿名、reparse point、permission denied、快速變動、rename storm 與 watcher overflow 等真實暫存資料夾執行自動／整合測試。
- 透過 `IFileOperation` 實作建立、重新命名、複製、移動、回收刪除、永久刪除、進度、取消、衝突與 partial failure，並只為安全可反向的已完成操作提供 undo/redo。
- 實作與 Windows Explorer 雙向相容的 Clipboard copy/cut/paste、OLE drag source/target、right-drag、drop effect、auto-scroll，以及 background/single/multi-select `IContextMenu3`。
- 實作可取消的搜尋 session、搜尋語法 AST、Windows Search backend 與能力清楚標示的 fallback，舊 query 結果不得污染目前分頁。
- 建立各階段自動測試、真實資料夾與 Explorer 互通測試、固定環境視覺基準、手動驗收、狀態文件與正式 parity matrix。
- Preview Handler、完整 namespace/Home/Gallery/ZIP/Libraries、thumbnail/icon views、session restore 與第三方 extension 完整 process isolation 仍可由後續 change 完成；本 change 不建立無行為的預留 API。

## Capabilities

### New Capabilities

- `windows-app-foundation`: Windows-only Rust／GPUI workspace、固定依賴、程序生命週期、診斷與品質閘門。
- `explorer-shell-ui`: Windows 11 Explorer 視窗結構、多分頁 chrome、file view/status surfaces、resize、theme、layout、actions、快捷鍵與 focus 行為。
- `tabbed-folder-navigation`: 多分頁狀態、真實本機資料夾增量列舉、history、取消、stable identity、watcher 與真實資料夾測試。
- `native-file-operations`: 以 Windows Shell API 執行建立、rename、copy、move、delete、進度、取消、衝突、partial failure 與安全 undo/redo。
- `shell-data-transfer-and-menus`: Windows Explorer 雙向 Clipboard、OLE drag-and-drop、drop effects、auto-scroll 與 `IContextMenu3` 選單相容性。
- `file-search`: 搜尋語法、per-tab 可取消搜尋 session、Windows Search、fallback、增量結果與 stale-result 隔離。
- `parity-verification`: 全部納入範圍能力的 parity matrix、真實資料夾／Explorer 互通、自動／手動／視覺驗收證據與完成條件。

### Modified Capabilities

無；工作區目前沒有既有 OpenSpec capability。

## Impact

- 新增有實際責任的 `crates/explorer-app`、`crates/explorer-ui`、`crates/explorer-common`、`crates/explorer-model`、`crates/explorer-shell-win`、`crates/explorer-jobs`、`crates/explorer-search` 與 `crates/explorer-test-support`；每個 crate 都由本 change 的 production flow 或測試直接使用，不產生空 package 或空 API。
- 新增根層 Cargo 設定、Windows resources／manifest、logging 與測試設定。
- 新增 `docs/IMPLEMENTATION_PLAN.md`、`docs/STATUS.md`、`docs/MANUAL_TESTS.md`、`docs/PARITY_MATRIX.md` 與視覺驗收產物規範。
- 依賴 Rust toolchain、Git revision 固定的 GPUI-CE、具最小 Win32/COM/OLE/Property/Search features 的 `windows` crate，以及 Windows 11 25H2 x64 實機驗收環境。
- 新增 app/model 與 service 間的 typed command/event contract、Shell STA/OLE apartment 工作、operation progress sink、watcher、search session 和可控制的 test fakes。
- 不更動外部 API 或既有程式行為；這是新專案基線。
