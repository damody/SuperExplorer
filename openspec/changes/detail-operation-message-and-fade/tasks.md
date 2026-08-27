## 1. 操作摘要與時間狀態

- [x] 1.1 加入 Local、ADB、SFTP location 與多來源路徑摘要純函式及聚焦測試。
- [x] 1.2 加入各 FileOperationKind 的操作、進度與終止結果摘要及聚焦測試。
- [x] 1.3 在 AppViewState 記錄最新 accepted terminal request 的單調時間，並於新操作取代時清除舊生命週期。
- [x] 1.4 加入 0–7 秒完整顯示、7–8 秒線性 opacity、滿 8 秒隱藏的純函式測試與實作。

## 2. OperationCenter 顯示生命週期

- [x] 2.1 將 generic completion text 改為詳細 typed operation summary，保留取消按鈕與最多五筆 partial rows。
- [x] 2.2 加入七秒與八秒邊界的延遲 invalidation，以及最後一秒 animation-frame 淡出。
- [x] 2.3 讓過期 terminal notice 完全不渲染並釋放訊息欄高度，且 hover 不影響期限。
- [x] 2.4 加入 stale completion、新操作取代及敏感 SFTP 資訊不外洩的測試。

## 3. 限定範圍驗證

- [x] 3.1 執行 operation message 聚焦測試與 explorer-ui compile check。
- [x] 3.2 執行真實視窗檔案操作，擷取詳細訊息與八秒後消失的證據。
- [x] 3.3 執行格式、diff check 與嚴格 OpenSpec validation，確認沒有執行完整迴歸。
