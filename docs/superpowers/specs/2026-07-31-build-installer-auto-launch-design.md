# build_install 成功後自動啟動安裝程式設計

## 目標

使用者從 `build_install.bat` 執行正式建置時，只要 release build、artifact validation 與 NSIS 打包全部成功，就立即啟動該次產生的 SuperExplorer 安裝檔。失敗、純檢查模式或無法確認輸出產物時不得啟動任何舊安裝檔。

## 行為契約

- 一般執行 `build_install.bat`：完成建置及驗證後，啟動本次 `dist\SuperExplorer-Setup-<version>-x64.exe`，再以成功碼結束 BAT。
- `build_install.bat --skip-build`：使用已存在且通過 PE 驗證的 release binaries 重新打包；打包成功後同樣啟動本次輸出。
- `build_install.bat --check`：只檢查工具與輸入，不建立也不啟動安裝程式。
- release build、NSIS、輸出驗證或啟動程序任何一步失敗：回傳非零 exit code，不掃描 `dist`，不嘗試開啟其他安裝檔。
- 成功啟動安裝程式後 BAT 不顯示「按任意鍵」；失敗時保留錯誤摘要，但也不阻塞自動化環境等待按鍵。

## 架構

`build_install.lua` 是唯一知道版本與確切 output path 的元件，因此由它在 `validate_executable(output)` 成功後呼叫新的 `process.start`。`process.start` 使用 PowerShell encoded command 建立 `ProcessStartInfo`，以 Shell execute 啟動互動式 `.exe`，不轉送 stdout/stderr、不等待安裝流程結束；它只負責確認 Windows 接受啟動要求。

`build_install.bat` 繼續只負責定位 bundled Lua、傳遞參數與傳回 Lua exit code。成功訊息改成已完成建置並啟動安裝程式；移除無條件 `pause`，避免安裝程式已出現後仍留下命令視窗，也讓 CI 不會卡住。

## 安全與錯誤處理

- 不以時間戳、萬用字元或「最新檔案」選擇 installer；只能使用此次版本計算出的 output path。
- `process.start` 接收分離的 executable 與 working directory，透過 encoded PowerShell 傳遞 Unicode／空白路徑，不拼接未跳脫的 shell command。
- 啟動前重用既有 `validate_executable`，確保輸出存在、具 `MZ` signature 且大小合理。
- 啟動失敗納入既有 structured failure，包含階段、命令、工作目錄與 exit code；不得把啟動失敗誤報為建置成功。
- `--check` 必須在任何產物建立、刪除或啟動之前返回。

## 測試

- Lua contract test 驗證 `process.start` 的 executable、working directory、Unicode quoting 與成功／失敗傳播，不真正啟動安裝 UI。
- build-install contract test 以 injectable launcher 驗證正常模式恰好啟動一次且路徑等於本次 output，`--check` 與各種 failure path 啟動零次。
- BAT 靜態測試驗證 exit code 原樣傳回、成功路徑沒有 `pause`，且沒有掃描 `dist` 或 `start` 舊檔的 fallback。
- 真實 smoke 使用隔離 mock installer executable 或 `--skip-build` 受控產物驗證 launch handoff；自動測試不得真的進入會修改系統的安裝精靈。

## 非目標

- 不自動執行 silent install、不自動要求 UAC、不自動按安裝精靈按鈕。
- 不等待安裝完成，也不把 installer exit code 當成 build exit code。
- 不改變版本規則、NSIS 內容、安裝目錄或現有 clean-tree release policy。
