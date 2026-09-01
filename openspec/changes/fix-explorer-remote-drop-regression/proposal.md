## Why

Windows 原生檔案總管目前無法可靠地把本機檔案或資料夾拖入 SuperExplorer 的 ADB 與 SFTP 目錄；拖曳游標可能持續更新，但 terminal Drop 沒有抵達可執行的目的地。這是已完成互通功能的回歸，會直接中斷使用者最基本的跨程式遠端檔案操作，因此需要立即恢復並以真實 Windows 拖放矩陣防止再次發生。

## What Changes

- 明確定義檔案檢視背景、可寫資料夾列與一般檔案列對外部 OLE file drop 的事件所有權。
- 讓 Windows Explorer 的標準 `CF_HDROP` 單選、多選、檔案與資料夾可拖入目前 ADB／SFTP 目錄或其可寫子資料夾。
- 保留 Windows negotiation 得到的 Copy／Move effect，Move 僅在目的端成功後移除本機來源。
- 對未建立 transfer command 的 terminal Drop 增加不含憑證的結構化拒絕原因。
- 補齊 Local 回歸、失敗邊界及 Explorer→ADB／SFTP headful 實測與 fixture 清理。

## Capabilities

### New Capabilities

- `explorer-remote-drop-target-routing`: 定義 Windows Explorer 外部檔案拖入 SuperExplorer Local、ADB、SFTP 時的 target ownership、effect、目的地固定、fail-closed 與 terminal 行為。

### Modified Capabilities

無；目前主規格目錄沒有對應的既有 capability，本變更以新 capability 固化已存在但發生回歸的產品契約。

## Impact

- 主要影響 `crates/explorer-ui` 的 GPUI external path drop target、action dispatch 與 state validation。
- 視根因可能調整 `crates/explorer-app` 的 `DropExternal`／remote transfer routing 診斷，但不改公開 API。
- 沿用 `crates/explorer-shell-win` 的標準 OLE `CF_HDROP` 與既有 `TransferEngine`，不新增外部依賴或資料遷移。
- 驗證影響 `scripts/smoke_explorer_drag_interop.ps1`、相關聚焦測試及受控 ADB／SFTP fixture。
- 不改變文字／圖片剪貼簿、SuperExplorer 拖出至 Explorer、Link drop 或虛擬 Shell namespace。
