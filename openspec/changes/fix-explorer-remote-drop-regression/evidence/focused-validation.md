# 集中驗證證據

日期：2026-09-01（Asia/Taipei）

## 根因與修正

- 修正前 ADB headful run 只有 `UpdateExternalDrag`，沒有 `DropExternal`，目的檔 oracle 失敗。
- 根因是 `pointer_drag_capture_listener` 在沒有 marquee、scrollbar、side-pane resize 或 details-column resize session 時，仍於 capture phase 攔截所有左鍵 MouseUp。Windows OLE terminal Drop 會被 GPUI轉成該 MouseUp，導致 bubble-phase `on_drop` 永遠收不到。
- 修正後 idle capture listener直接放行 MouseMove／MouseUp；只有真正持有 capture 的 transient session會 stop propagation。

## Headful Windows Explorer matrix

- Local Ctrl Copy：2/2（檔案、資料夾），目的存在、來源保留；`build/drag-regression-local-after/report.json`。
- ADB Ctrl Copy：2/2（檔案、資料夾），目的存在、來源保留；`build/drag-regression-adb-after/report.json`。
- ADB Shift Move：1/1，目的存在、來源移除；`build/drag-regression-adb-move-after/report.json`。
- SFTP Ctrl Copy：2/2（檔案、資料夾），目的存在、來源保留；`build/drag-regression-sftp-after/report.json`。
- SFTP Shift Move：1/1，目的存在、來源移除；`build/drag-regression-sftp-move-after/report.json`。
- ADB cleanup對emulated `/sdcard` 的暫時性ETXTBSY使用bounded exact-path rmdir fallback；SFTP controlled names由credential-interactive cleanup probe移除。

## 聚焦檢查

- `cargo test -p explorer-model drag_drop`：5 passed。
- `cargo test -p explorer-shell-win drag_drop -- --test-threads=1`：5 passed、1 interactive-only ignored。
- `cargo test -p explorer-ui drag`：16 passed。
- idle OLE MouseUp regression test：passed。
- `cargo test -p explorer-app remote_service`：19 passed。
- `cargo check -p explorer-app`、`cargo fmt --all`、`git diff --check`：passed。
- credential scan未發現密碼或URI userinfo；登入資料只透過互動輸入。
