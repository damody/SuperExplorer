## Why

SFTP 檔案目前無法與 Windows 原生檔案總管雙向拖放，破壞了檔案管理器的核心互動。既有程式雖具備外部 drop、remote staging 與 OLE `DoDragDrop` 元件，但端到端路由、effect 與 staging lifetime 尚未形成可靠契約。

## What Changes

- 接通 Windows 檔案總管 `CF_HDROP` 拖入 SFTP 的上傳路由。
- 接通 SFTP 項目 staging 後以標準 Shell `IDataObject` 拖到檔案總管的路由。
- 固定跨檔案系統預設 Copy、Shift 才提出 Move，且只在目的端成功後刪除來源。
- 補齊資料夾遞迴、部分失敗、取消、staging ownership／清理與詳細診斷。
- 以指定 SFTP 路徑執行雙向 headful 驗證，並檢查 ADB 與 local 共用拖放流程沒有回歸。

## Capabilities

### New Capabilities

- `sftp-windows-explorer-drag-drop`: SFTP 與 Windows 原生檔案總管之間的雙向 OLE 檔案拖放、effect、安全刪除與暫存生命週期契約。

### Modified Capabilities

無。

## Impact

- `explorer-ui` 的 external drop target、effect 協商與拖曳命令分派。
- `explorer-app` remote service 的外部上傳、遠端 staging、terminal 與清理。
- `explorer-shell-win` 的 `IDataObject`、Preferred DropEffect 與 `DoDragDrop` 結果。
- 聚焦 Rust 測試與 Windows headful smoke；不新增外部依賴或公開破壞性 API。
