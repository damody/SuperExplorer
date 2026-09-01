# 集中驗證證據

## 2026-08-31 拖放矩陣收尾

- `cargo test -p explorer-model drag_drop`：5 passed。
- `cargo test -p explorer-shell-win drag_drop`：5 passed、1 個 interactive-only ignored；cancel soak 的 handle、GDI、USER 與 active native drag 全數回到基線。
- `cargo test -p explorer-ui drag`：16 passed。
- `cargo test -p explorer-app remote_service`：通過，包含 remote drop 無效來源與 effect 的 fail-closed 測試。
- `cargo check --workspace --all-targets`：通過。
- `scripts/smoke_explorer_drag_interop.ps1 -Direction app-internal`：同磁碟預設 Move、Shift Move、Ctrl Copy、Cancel 全數通過磁碟存在性 oracle；報告位於 `build/drag-final-app-internal-uia3/report.json`。
- Headful runner 已改由 UI Automation 取得真實來源與目的地 rectangle，不再依賴過期的 Details row 固定座標。
- `openspec validate windows-file-clipboard-interoperability --strict`：通過。

## 2026-08-31 使用者角度跨程式複查

- SuperExplorer → Windows Explorer：Move、Ctrl Copy、左鍵 Cancel、右鍵 Cancel 共 4/4 通過；報告位於 `build/drag-final-app-to-explorer-uia/report.json`。
- Windows Explorer → SuperExplorer Copy：通過；報告位於 `build/drag-final-explorer-to-app-copy-only/report.json`。
- Windows Explorer → SuperExplorer Move：通過；報告位於 `build/drag-final-explorer-to-app-move-only/report.json`。
- Windows Explorer → SuperExplorer 左/右 Cancel：通過；報告位於 `build/drag-final-explorer-to-app-cancel-only/report.json`。
- 同一 SuperExplorer 程序連續 Default Move、Shift Move、Ctrl Copy、Cancel：4/4 通過；報告位於 `build/drag-final-app-internal-uia3/report.json`。
- Runner 的跨程式來源列改由 UI Automation 實際 rectangle 驅動；目的點改為檔案區右下空白背景，避免檔案列正確拒絕 drop 被誤判為產品失敗。

## 2026-08-31 Windows Explorer → ADB 實機修正

- 在修正前，以真實 Windows Explorer 拖曳至 `adb://emulator-5554/sdcard/Download` 可穩定重現：只有 `UpdateExternalDrag`，沒有 `DropExternal`，裝置檔案 oracle 失敗。
- 根因是非資料夾列也註冊了拒絕型 child drop target；當 Details 列表填滿 viewport，背景 drop target 無法擁有放置事件。
- 修正後非資料夾列對外部 drop 保持透明，資料夾列仍獨占 child-folder drop，避免改變既有資料夾目的地語意。
- Windows Explorer → ADB Ctrl Copy：通過，裝置檔案存在且本機來源保留；報告位於 `build/drag-final-explorer-to-adb-copy/report.json`。
- Windows Explorer → ADB Shift Move：通過，裝置檔案存在且本機來源移除；報告位於 `build/drag-final-explorer-to-adb-move/report.json`。
- Windows Explorer → SuperExplorer local Ctrl Copy 回歸：通過；報告位於 `build/drag-regression-explorer-to-local-copy-retry/report.json`。
- `cargo test -p explorer-app remote_service --locked`：14 passed；`cargo check --workspace --all-targets --locked`：通過。

驗證日期：2026-08-30。

## 聚焦測試

- `cargo test -p explorer-shell-win clipboard::tests -- --test-threads=1`：8 passed，0 failed。
- `cargo test -p explorer-app remote_service::tests -- --test-threads=1`：11 passed，0 failed。
- `cargo test -p explorer-ui clipboard -- --test-threads=1`：相關 unit 與 contract tests 通過。

## Headful Windows 檔案剪貼簿

測試腳本：`scripts/smoke_windows_file_clipboard_interop.ps1`。

- 標準外部 `CF_HDROP` → SuperExplorer local：通過，報告位於 `build/clipboard-interop-local3/report.json`。
- SuperExplorer local `Ctrl+C` → 標準 Windows FileDropList：通過，同一報告確認選取路徑完全相符。
- 標準外部 `CF_HDROP` → `adb://emulator-5554/sdcard/Download`：通過，報告位於 `build/clipboard-interop-adb2/report.json`。
- 標準外部 `CF_HDROP` → `sftp://45.32.49.125/home/linuxuser`：通過，報告位於 `build/clipboard-interop-sftp/report.json`。

ADB 與 SFTP 測試檔在驗證後由同一 headful 流程刪除；證據未記錄任何遠端密碼。
