# 最終驗證

日期：2026-09-02

## 打包與安裝

- `build_test_install.bat`：通過。
- 安裝程式：`dist/SuperExplorer-Test-Setup-1.2026.9.2-x64.exe`。
- 安裝程式 SHA-256：`DAF928C918086DD279ADA3B761CDBB6B4FD26C78A4500605545D8A1FE6D94116`。
- release 與 installed `SuperExplorer.exe` SHA-256 均為 `D997A1E054CCEEF97F8DBF984E95AF2173246D1ACB8982E0F4FD208336876898`。

## 使用者視角

- ADB：1 GiB 檔案傳輸顯示非零速度、bytes、百分比；約 200 ms cadence；取消 UI 於 163.4 ms 內終止。證據：`user-perspective/adb-ui-final/`。
- SFTP：1 GiB 檔案傳輸顯示非零速度、bytes、百分比；約 200 ms cadence；取消 UI 於 163.4 ms 內終止。證據：`user-perspective/sftp-final/`。
- 工具列：右上顯示傳輸入口與活動數徽章；徽章本身亦可點擊。工作清單只保留本次程式執行期間，依 newest-first 顯示。
- 多工作：確定性模型測試驗證較新工作先完成後，底部自動回退到仍在執行的較舊工作；活動數只計 Copy／Move。
- Shift+Delete：foreground 選擇排除 queued／running 永久刪除，只在 terminal 後顯示結果；既有八秒 terminal 淡出路徑保持生效。

## 最後檢查

- `cargo fmt --all -- --check`：通過。
- model foreground／fallback 測試：通過。
- 200 ms reporter heartbeat 測試：通過（0.20 s）。
- UI 速度／session panel 測試：通過。
- `cargo check -p explorer-app`：通過。
- `openspec validate add-multi-transfer-center-speed --strict`：通過。

結論：此變更的 29 個工作項目均完成，正式安裝版本與 release hash 一致。
