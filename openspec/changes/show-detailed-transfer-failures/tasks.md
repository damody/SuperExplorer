## 1. 傳輸診斷資料流

- [x] 1.1 擴充 TransferResult，使失敗與部分完成保留具型別的傳輸階段及實際 anyhow 錯誤鏈。
- [x] 1.2 在 Local、遠端下載、遠端上傳、衝突檢查及移動來源刪除邊界加入精確 context，並維持取消語意。
- [x] 1.3 實作診斷安全化純函式，遮蔽 URI userinfo 與 password、token、secret 值，空原因使用明確降級文字。
- [x] 1.4 加入 transfer engine 聚焦測試，涵蓋不同階段、部分移動失敗、實際 provider 原因及取消。

## 2. 服務與訊息列整合

- [x] 2.1 修改 remote_service 轉換，將安全化的階段與 diagnostic 寫入每筆 OperationItemResult，不再使用固定傳輸失敗英文。
- [x] 2.2 讓 OperationCenter 以 outcome 的來源、計算後目標、階段、native code 與安全原因格式化失敗／部分完成列。
- [x] 2.3 加入多筆不同原因、Local／ADB／SFTP 路徑、目的檔名推導、缺少原因及五筆上限的聚焦測試。

## 3. 限定範圍驗證

- [x] 3.1 執行 explorer-remote、explorer-app 與 explorer-ui 的相關聚焦測試。
- [x] 3.2 執行三個受影響 crate 的 compile check，修正本變更造成的錯誤。
- [x] 3.3 執行格式、diff check 與嚴格 OpenSpec validation，確認未執行完整迴歸測試。
