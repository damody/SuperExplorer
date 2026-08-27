## Why

「新增資料夾」需要先讓使用者確認名稱，再於 Local、ADB 或 SFTP 建立實體資料夾；名稱確認前不得留下 provider 項目。

## What Changes

- 在目前目錄 snapshot 中產生不衝突的 `New folder`、`New folder (2)` 等預設名稱。
- 先顯示綁定目前分頁與目錄的 provisional folder row 並立即進入行內重新命名，不先提交建立操作。
- Enter 或失焦確認有效名稱後才提交 Folder 建立；Esc 只移除草稿。
- 對 Local、ADB、SFTP 使用同一狀態流程，保留各 provider 的既有建立與刷新實作。
- 在名稱無效、碰撞、取消或分頁／目錄切換時保留錯誤或安全清除草稿。
- 新增 focused state/action 測試與三種 filesystem 的 headful 驗證。

## Capabilities

### New Capabilities

- `explorer-new-folder-inline-rename`: 規範預設命名、暫存列、確認後建立與取消行為。

### Modified Capabilities

None.

## Impact

主要影響 `explorer-ui` 的 action/state、FileView 暫存列與 focused/headful 測試腳本。既有 file-operation protocol、Local Shell STA、ADB/SFTP provider API、書籤與 session schema 不需破壞性變更。
