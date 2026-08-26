## Why

ADB 與 SFTP 已可瀏覽，但目前遠端目錄尚未完整接上新增資料夾、永久刪除、檔案 clipboard、跨 provider 傳輸與原生檔案總管拖放，因此使用者無法把遠端位置當成可操作的檔案系統。現在補齊這條既有但未整合完成的操作鏈，讓 Local、ADB、SFTP 使用一致且可取消的檔案操作語意。

## What Changes

- 讓可寫入的 ADB／SFTP 目錄背景右鍵支援新增資料夾與貼上，項目右鍵支援刪除、複製與剪下。
- 讓 `Ctrl+C`、`Ctrl+X`、`Ctrl+V` 與右鍵命令共用 typed file clipboard，且不攔截文字、圖片或其他非檔案 clipboard 格式。
- 讓 Local ↔ ADB、Local ↔ SFTP、ADB ↔ SFTP 支援檔案與遞迴資料夾的 Copy／Move；遠端對遠端使用 scoped 本機暫存目錄。
- 將跨 provider Move 定義為完整複製後刪除來源；刪除失敗回報 Partial 並保留來源。
- 讓 Remote 刪除經不可復原確認後永久執行，Local 繼續使用 Windows 資源回收筒。
- 讓應用程式內拖放使用相同 Transfer Engine，並補齊 Windows Explorer 本機拖入遠端與遠端 staged 拖出。
- 加入 capability、路徑邊界、取消、衝突、暫存清理、過期結果及非秘密診斷的聚焦測試。

## Capabilities

### New Capabilities

- `cross-provider-file-operations`: Local、ADB、SFTP 之間的新增資料夾、永久刪除、typed clipboard、Copy／Move、scoped staging 與原生／應用程式內拖放契約。

### Modified Capabilities

無。現有正式 specs 尚未包含未封存的 ADB／SFTP provider 契約；本變更以新增且可獨立驗證的跨 provider 操作能力收斂目前行為，不改寫其他已封存能力。

## Impact

影響 `explorer-model` 的操作／clipboard／能力契約、`explorer-remote` provider 與 Transfer Engine、`explorer-app` remote service 路由、`explorer-ui` 右鍵／快捷鍵／確認／狀態整合，以及 `explorer-shell-win` clipboard 與 OLE drag/drop 邊界。沿用現有 `tempfile`、ADB、SFTP 與 Windows Shell 依賴，不新增外部服務、憑證格式或使用者可設定的危險清除路徑。
