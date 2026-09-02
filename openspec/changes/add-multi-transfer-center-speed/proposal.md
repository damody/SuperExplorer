## Why

目前底部 OperationCenter 只能可靠呈現最後一筆記錄，無法在多個傳輸並行時回退到仍在執行的工作，也沒有每秒速度；ADB 進度在使用者視角下更新過慢。需要一個不壓縮檔案清單、可追蹤本次執行期間所有工作的傳輸中心，同時修正 Shift+Delete 過早顯示準備狀態的行為。

## What Changes

- 為 Local、ADB、SFTP Copy／Move 顯示平滑後的即時傳輸速度。
- 將遠端活動進度的可見更新節奏定為 200 ms，保留階段、取消與終止的立即事件。
- 底部只顯示最後啟動且仍在執行的傳輸；較新工作先結束時回退到先前未完成工作。
- 在右上工具列加入 Fluent 傳輸按鈕、活動數徽章與可展開的本次執行期間工作清單。
- 讓每筆活動工作可獨立取消，終止記錄可導向本機或遠端位置。
- Shift+Delete 等待確認與執行期間不顯示底部狀態，操作終止後才顯示完整結果並依八秒規則淡出。
- 增加模型、UI、ADB、SFTP、打包安裝及使用者視角驗證。

## Capabilities

### New Capabilities

- `multi-transfer-center-speed`: 定義即時速度、200 ms 更新、多工作前景回退、右上傳輸面板與永久刪除延後通知。

### Modified Capabilities

無。

## Impact

- `explorer-model`：操作記錄的穩定順序、速度樣本與語意查詢。
- `explorer-app`／`explorer-remote`：ADB 與通用遠端進度發布、保活與取消。
- `explorer-ui`：底部前景選擇、工具列按鈕、Fluent 面板、互動狀態與 Shift+Delete 可見性。
- 不改變遠端 provider 公開協定、不持久化歷史、不新增網路服務或第三方相依套件。
