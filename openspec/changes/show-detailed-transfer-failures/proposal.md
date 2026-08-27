## Why

Local、ADB、SFTP 之間的跨檔案系統傳輸目前會把底層失敗壓縮成 `A file could not be transferred.`，導致使用者看不到是哪個項目、哪個階段或哪個 provider 原因造成失敗。操作訊息列已能呈現詳細路徑與結果，現在需要讓傳輸診斷以同樣精度抵達該介面。

## What Changes

- 保留每個傳輸項目的來源、目的地、失敗階段與底層 provider 診斷。
- 將安全化後的實際診斷放入 `OperationItemResult`，不再以固定英文覆蓋。
- 在 OperationCenter 的 partial outcome 列顯示每筆具體來源、目標、階段、錯誤碼與原因。
- 過濾 SFTP userinfo、密碼、token 等敏感資訊，並為缺少底層原因的情況提供明確降級文字。
- 保留既有成功行為、八秒訊息生命週期、取消、衝突策略與五筆明細上限。

## Capabilities

### New Capabilities

- `transfer-failure-diagnostics`: Local、ADB、SFTP 跨檔案系統傳輸的逐項、安全、可操作失敗診斷。

### Modified Capabilities

無。

## Impact

- `explorer-remote` 的傳輸結果與錯誤階段擷取。
- `explorer-app` 的遠端傳輸結果至 `ExplorerError` 轉換。
- `explorer-ui` 的 OperationCenter partial outcome 格式化。
- 聚焦單元測試與服務測試；不新增外部依賴、不變更公開 URI 格式。
