# 最終驗證

- OpenSpec：30/30 tasks complete，strict validation passed。
- 自動檢查：format、model terminal/newest-first/speed、ADB parser/provider、app reporter、tool-window UI 與 explorer-app check 全部 exit 0。
- 真實 ADB：原生 PTY 百分比 frame、非零速度、取消與「已取消」通過；installed smoke 取消延遲 69.7 ms。
- 真實 SFTP：非零速度、取消與「已取消」通過；取消延遲 73.9 ms。
- 工具視窗：owner HWND 正確、`WS_EX_TOOLWINDOW` 有效、`WS_EX_APPWINDOW` 關閉、外部失焦隱藏通過。
- 打包：`build_test_install.bat` 成功；release 與 installed `SuperExplorer.exe` SHA-256 同為 `0CCC729E43E102AE11CD99F32F77A5706B9351E086B6184256A05CA8A26FDD0E`。
- 安裝程式：`dist/SuperExplorer-Test-Setup-1.2026.9.2-x64.exe`，SHA-256 `B4933EFFC2FE677ACB1529200AF1A416B3C9F2A04B84BFC2F8437A3FE7F3F4B3`。
- ADB 明確測試檔已刪除；本機測試 fixture 保留於 `target/`，可由建置清理流程回收。

主要使用者視角證據：

- `evidence/user-perspective/adb-debug15/report.json`
- `evidence/user-perspective/sftp-debug1/report.json`
- `evidence/user-perspective/adb-installed-final/report.json`
- 對應目錄內的 `transfer-tool-window.png`、`speed-active.png` 與 `cancelling.png`
