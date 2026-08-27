## 1. 命令契約

- [x] 1.1 在 host context command 加入 Paste 與 wire name 測試。
- [x] 1.2 在 context menu request 加入 paste_available，更新所有建立位置。

## 2. 選單與動作

- [x] 2.1 local 原生背景與 item 選單在可貼上時加入 Paste。
- [x] 2.2 將原生 Paste 委派到既有 ExplorerAction::Paste。
- [x] 2.3 remote 背景、檔案與資料夾右鍵選單都加入 Paste。

## 3. 目的地與傳輸測試

- [x] 3.1 測試右鍵命中背景、檔案或資料夾時都貼到目前資料夾。
- [x] 3.2 測試 SFTP copy 可立即經 internal clipboard 貼到 local destination。
- [x] 3.3 測試無效 clipboard 或唯讀位置不提供可用 Paste。

## 4. 驗證

- [x] 4.1 執行 model、Shell context menu、UI state、remote service focused tests。
- [x] 4.2 執行 cargo fmt、workspace all-targets check 與 OpenSpec strict validation。
