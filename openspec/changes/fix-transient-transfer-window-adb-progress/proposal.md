## Why

目前傳輸中心是主視窗內的 deferred overlay，會被檔案欄位等內容遮擋，失焦後也可能殘留並阻塞 UI；同時取消工作被錯誤摘要成「部分完成」，而 ADB push／pull 沒有完整利用 CLI 原生即時進度。這些問題直接影響遠端大檔案傳輸的可見性、可取消性與操作可靠度，需在現有多工作傳輸中心上修正。

## What Changes

- 將傳輸中心改為 owner 綁定、無工作列與 Alt+Tab 項目的 transient tool window，並定義與 modal 的正確層級。
- 加入靠右錨定、monitor work-area 修正、toggle、Escape 與 owned-window 群組失焦隱藏行為。
- 統一視窗 handle、owner、visible 狀態與 idempotent hide lifecycle，避免 overlay/focus callback 競爭造成 UI 卡住。
- 將使用者取消後的 terminal 正規化為 `Cancelled`，隔離遲到 progress／terminal，並顯示「已取消」而非「部分完成 0/Y」。
- 強化 ADB PTY frame parser 與 monotonic adapter，直接消費 push／pull 的 carriage-return、ANSI、percent 與 byte-pair 原生輸出。
- 保留 200 ms publisher，但改為定期發布 parser 最新快照；phase、取消與 terminal 邊界立即發布。
- 增加工具視窗、取消狀態、ADB 原生 cadence、ADB/SFTP 實機與打包安裝證據。

## Capabilities

### New Capabilities

- `transient-transfer-window-adb-progress`: 規範 transient 傳輸工具視窗的層級、focus 與生命週期，以及取消 terminal 和 ADB 原生進度資料流。

### Modified Capabilities

無。

## Impact

- `crates/explorer-ui`：傳輸中心呈現、typed actions、主／工具視窗狀態、focus 與定位。
- `crates/explorer-app`：工具視窗 composition、工作事件同步與 200 ms publisher。
- `crates/explorer-model`：取消 terminal、遲到事件與工作摘要語意。
- `crates/explorer-remote`：ADB PTY reader、frame parser、progress adapter 與 fallback diagnostics。
- Windows/GPUI 視窗建立選項與 owner/tool-window 原生樣式；不新增公開擴充 ABI，也不改變 SFTP 協定、衝突策略或跨程式執行歷史保存。
- 驗證會使用現有 emulator、已保存的 SFTP profile 與 `build_test_install.bat`；外部測試檔只建立在明確測試目的地並於驗證後清理。
