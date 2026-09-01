## Why

SuperExplorer目前無法可靠地與 Windows 原生檔案總管交換標準檔案剪貼簿內容，導致使用者在檔案總管複製後不能貼到 local、ADB 或 SFTP，也不能把 SuperExplorer local 的選取項目複製回檔案總管。這破壞了檔案管理器最基本的跨程式工作流程，且既有文字與圖片剪貼簿隔離必須保留。

## What Changes

- 監看並解析外部 Windows OLE 檔案剪貼簿的 `CF_HDROP`、clipboard sequence 與 Preferred DropEffect。
- 讓檔案總管複製或剪下的本機檔案可貼到 SuperExplorer local、ADB 與 SFTP 目的地。
- 讓 SuperExplorer local 的複製或剪下發布標準 Shell `IDataObject`，可由檔案總管直接貼上。
- 保留檔案檢視與文字輸入焦點的快捷鍵隔離，且不將文字、圖片或 HTML 誤判為檔案。
- 對剪貼簿忙碌、陳舊資料、來源遺失與遠端傳輸失敗提供安全狀態和詳細錯誤資訊。
- 收斂既有 OLE 拖放路徑，驗證 local、ADB、SFTP、其他 SuperExplorer 視窗與原生檔案總管之間的完整互通矩陣。

## Capabilities

### New Capabilities

- `windows-file-clipboard-interoperability`: Windows 原生檔案剪貼簿與 SuperExplorer local、ADB、SFTP 之間的雙向複製、剪下及貼上契約。

### Modified Capabilities

無。

## Impact

- `explorer-shell-win` 的 OLE clipboard runtime、Shell STA 命令與 file operation 邊界。
- `explorer-app` 的 clipboard 事件轉送與 remote transfer service。
- `explorer-ui` 的 clipboard 狀態、快捷鍵分派、貼上能力與錯誤呈現。
- Windows headful smoke scripts及聚焦 Rust 測試；不新增外部依賴或破壞性公開 API。
