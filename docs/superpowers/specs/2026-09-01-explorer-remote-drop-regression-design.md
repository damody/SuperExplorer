# Windows Explorer 拖入 ADB／SFTP 回歸修正設計

日期：2026-09-01

## 目標

修正 Windows 原生檔案總管無法把本機檔案或資料夾拖入 SuperExplorer 的 ADB、SFTP 目錄之回歸，同時維持 Local 拖放、剪貼簿及既有跨檔案系統傳輸行為。

## 成功條件

- Windows Explorer 可將單一或多個本機檔案、資料夾拖入 ADB 與 SFTP 的目前目錄空白處。
- 拖到可寫遠端資料夾列時，以該資料夾作為目的地。
- Windows 提供的 Copy／Move effect 被正確保留；Copy 保留來源，Move 僅在目的端成功後移除來源。
- Local 目的地、文字剪貼簿與圖片剪貼簿不受影響。
- 無效、空白、Link 或 None drop 維持 fail-closed，不能回報空批次成功。

## 已知脈絡

既有互通設計以標準 OLE `CF_HDROP` 接收 Windows Explorer 路徑，再由 `DropExternal`、`DataTransfer` 與 `TransferEngine` 執行 Local／ADB／SFTP 傳輸。先前同類回歸的根因是非資料夾列註冊拒絕型 child drop target；當 Details 列填滿 viewport 時，背景 target 收不到 terminal Drop，即使 DragOver 仍持續發生。

## 方案比較

### 方案一：修正 drop target 事件所有權（採用）

一般檔案列對外部檔案拖放保持透明；空白背景接收目前目錄；只有可寫資料夾列接收 child-folder drop。此方案延續既有 GPUI／OLE 與 transfer router，修改範圍最小。

### 方案二：視窗最上層攔截所有 Drop

雖可避免事件遺失，但會模糊空白區、資料夾列、導航列等目的地語意，並可能攔截書籤或欄位拖放，因此不採用。

### 方案三：新增 HWND 原生 DropTarget

控制能力完整，但會與 GPUI 現有 OLE 管線重疊，增加 COM lifetime、effect negotiation 與事件去重風險，因此不採用。

## 架構與事件流

1. GPUI 外部路徑事件在可接受區域命中 background 或 writable-folder target。
2. target 將 `CF_HDROP` 路徑、按鍵修飾狀態、allowed effect 與固定的目的地 generation 送入 `UpdateExternalDrag`／`DropExternal`。
3. `AppViewState::queue_external_drop` 驗證來源與目的地，建立唯一 `DataTransfer` command。
4. application service 把本機來源交給既有 transfer router；ADB、SFTP 使用既有 upload／staging／progress 路徑。
5. terminal success、failure 或 cancellation 清除 drag cue 與 transient session；Move 僅在目的端成功後處理來源刪除。

## 命中規則

- 檔案檢視空白處：目的地為目前 tab 的目前目錄。
- 可寫資料夾列：目的地為該資料夾，並以 stable item identity 與 tab generation 固定。
- 一般檔案列：不得註冊會吃掉外部 Drop 的拒絕型 child target，使事件能傳至背景。
- 不可寫、過期 generation、搜尋虛擬結果或不支援的 provider：明確拒絕並清除提示。

## 錯誤與診斷

- DragEnter、DragOver 與 Drop 的診斷包含 target kind、provider kind、路徑數量與 effect，但不記錄憑證。
- Drop 未建立 transfer command 時記錄具體拒絕原因，避免只有游標事件卻無 terminal 事件而不可追蹤。
- 傳輸錯誤沿既有詳細訊息列顯示來源、目的地、操作階段與 provider reason。

## 驗證策略

### 聚焦自動測試

- 一般檔案列保持外部 drop 透明；資料夾列與背景 target 正確擁有事件。
- background／folder destination、單選／多選、Copy／Move。
- 無效來源、None／Link effect、過期 generation 及 late callback。
- Local 拖放與文字／圖片剪貼簿隔離回歸。

### Windows headful 實測

- Windows Explorer → `adb://emulator-5554/sdcard/Download`：Ctrl Copy 與 Shift Move。
- Windows Explorer → `sftp://45.32.49.125/home/linuxuser`：Ctrl Copy 與 Shift Move。
- Windows Explorer → SuperExplorer Local：Ctrl Copy 回歸。
- 以目的端存在性與來源保留／移除作為 oracle；fixture 必須帶 marker，驗證後清理。

## 非目標

- 不重寫 OLE data object 或 transfer engine。
- 不改變 SuperExplorer 向 Windows Explorer 拖出的設計。
- 不增加 Link drop、雲端 placeholder 或虛擬 Shell namespace 支援。
- 不執行整個專案的完整迴歸，只執行相關 crate 與 headful matrix。
