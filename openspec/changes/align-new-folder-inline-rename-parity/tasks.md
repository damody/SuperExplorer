## 1. 暫存建立狀態

- [x] 1.1 加入不區分大小寫的預設名稱碰撞編號測試與實作。
- [x] 1.2 加入綁定分頁、generation、父目錄與暫存 identity 的資料夾草稿。
- [x] 1.3 讓 FileView 顯示暫存列並立即使用既有行內重新命名元件。

## 2. 確認與取消

- [x] 2.1 Enter／失焦時驗證最終名稱，確認後才產生 Folder 建立 request。
- [x] 2.2 Esc 清除暫存列且不產生任何 provider 操作。
- [x] 2.3 保留名稱無效／碰撞時的 editor 錯誤，並隔離既有 F2 與批次建立流程。
- [x] 2.4 導航、分頁與 generation 不相符時清除 stale 草稿。

## 3. 限定範圍驗證

- [x] 3.1 執行聚焦測試，證明確認前不建立、確認後才送出 request。
- [x] 3.2 執行格式、explorer-ui 編譯、diff check 與嚴格 OpenSpec 驗證。
- [x] 3.3 執行 Local、ADB、SFTP 真實視窗測試並安全清理測試資料夾。
