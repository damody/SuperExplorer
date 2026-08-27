## 1. 檢查共用路由

- [x] 1.1 確認 Paste 可用性只看剪貼簿與目前位置是否可寫入。
- [x] 1.2 確認 ADB 來源可使用共用 Transfer Engine 到 Local 或任意 Virtual 目的地。
- [x] 1.3 若路由仍有限制 provider 名稱，以最小修改移除限制。

## 2. 補齊測試

- [x] 2.1 加入 ADB 複製後立即貼到 Local 的測試。
- [x] 2.2 加入 ADB 貼到 SFTP 的 download、staging、upload 測試。
- [x] 2.3 加入 ADB 貼到另一個註冊 provider 的測試。
- [x] 2.4 加入唯讀目的地與 upload 失敗的測試。
- [x] 2.5 確認背景、檔案、資料夾右鍵都貼到目前資料夾。

## 3. 最終驗證

- [x] 3.1 執行相關 UI、app 與 remote focused tests。
- [x] 3.2 執行 cargo fmt 與 workspace all-targets check。
- [x] 3.3 執行 OpenSpec strict validation。
