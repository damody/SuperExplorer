## Why

操作訊息欄目前只顯示籠統的 `File operation completed`，使用者無法辨識剛完成的是哪個操作、來源及目的地，而且完成訊息會持續占用畫面。需要讓 Local、ADB、SFTP 操作結果可追溯，同時在合理時間後自動收起。

## What Changes

- 依 typed file-operation request 顯示操作類型、完整路徑、來源／目的地及項目數。
- 進行中操作持續顯示進度；成功、取消、部分成功與失敗顯示相應結果及必要明細。
- 終止訊息前七秒完整顯示，第八秒淡出，滿八秒移除並釋放訊息欄高度。
- 新操作取代舊訊息並重設生命週期；敏感 SFTP 驗證資訊不得顯示。
- 加入格式化、時間邊界、渲染及真實視窗聚焦驗證。

## Capabilities

### New Capabilities

- `operation-message-lifecycle`: 規範檔案操作摘要內容、路徑安全顯示、終止後八秒生命週期與淡出行為。

### Modified Capabilities

None.

## Impact

主要影響 `explorer-ui` 的 operation state、OperationCenter render 與 GPUI frame scheduling，以及聚焦測試與 headful 驗證腳本。不修改 file-operation protocol、provider API、持久化格式或 Local／ADB／SFTP 實際 I/O。
