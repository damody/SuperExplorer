## Why

`build_test_install.bat`目前只建置、封裝並非同步開啟安裝GUI，批次檔會在安裝完成前回報成功；未完成GUI時系統仍執行舊版，造成已修正的Windows Explorer外部拖放在實際安裝版仍失敗。測試入口需要把「本次release確實成為已安裝版本」納入成功條件。

## What Changes

- 將SuperExplorer測試建置的預設流程改為同步silent install、安裝後binary hash gate及啟動已驗證版本。
- 保留`--no-launch`只建置封裝、`--check`只檢查及`--skip-build`重用release輸入的既有語意。
- 對安裝器退出、必要檔案缺失、hash不符與啟動失敗回傳非零退出碼及具體階段。
- 不改正式combined installer與SuperDesktop-only測試入口。
- 以指定dotfile JSON驗證安裝版Explorer→ADB／SFTP真實拖放，並安全清理受控遠端副本。

## Capabilities

### New Capabilities

- `verified-test-installer-deployment`: 規範SuperExplorer測試建置入口的同步安裝、release／installed身分驗證、選項邊界、錯誤語意與安裝版拖放驗收。

### Modified Capabilities

無。

## Impact

- 影響`build_test_install.bat`、`build/build_install.lua`、共用Lua process／installer helper及其聚焦測試。
- 預設測試建置將執行具系統可見效果的既有NSIS測試安裝器，並等待完成；`--no-launch`仍完全不安裝。
- 不新增公開Rust API、外部相依或資料格式遷移。
- 驗收影響既有安裝目錄、SuperExplorer程序與MFT service lifecycle；由既有NSIS安全協調處理。
