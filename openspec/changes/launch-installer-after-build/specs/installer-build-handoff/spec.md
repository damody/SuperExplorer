## ADDED Requirements

### Requirement: 成功打包後啟動本次安裝檔
系統 SHALL 在 release inputs、NSIS packaging 與 output executable validation 全部成功後，精確啟動本次 version-derived `SuperExplorer-Setup-<version>-x64.exe` 一次；MUST NOT 以目錄掃描、時間戳或舊產物 fallback 選擇 installer。

#### Scenario: 一般 build 成功
- **WHEN** 使用者執行 `build_install.bat` 且 release build、NSIS 與輸出驗證全部成功
- **THEN** 系統啟動本次計算出的 installer 一次，並以 build exit code 0 結束而不等待安裝完成

#### Scenario: skip-build 打包成功
- **WHEN** 使用者執行 `build_install.bat --skip-build` 且既有 release binaries 與新打包輸出皆通過驗證
- **THEN** 系統啟動該次重新產生的 installer 一次

### Requirement: 檢查與失敗路徑禁止啟動
系統 MUST 在 `--check` 模式或任何 build、packaging、validation、launch 前置失敗時啟動零個 installer，且 MUST 保留非零錯誤傳播；不得改開其他 dist 產物。

#### Scenario: check-only 模式
- **WHEN** 使用者執行 `build_install.bat --check`
- **THEN** 系統只驗證工具與輸入並成功返回，不建立、刪除或啟動 installer

#### Scenario: 打包或驗證失敗
- **WHEN** release build、NSIS invocation 或 output validation 任一步失敗
- **THEN** 系統回傳非零 exit code，啟動零個 installer，且錯誤摘要保留失敗階段

#### Scenario: Windows 拒絕啟動
- **WHEN** installer 已通過檔案驗證但 Windows process launch 失敗
- **THEN** 系統將 launch 視為 build handoff failure、回傳非零碼，且不嘗試任何替代 installer

### Requirement: 互動式啟動支援安全路徑並立即交還控制
process launcher SHALL 使用分離的 executable、arguments 與 working directory，MUST 支援 Unicode 與空白路徑，MUST 允許 installer 顯示互動 UI，且 MUST NOT redirect installer streams 或等待 installer 結束。

#### Scenario: Unicode 與空白安裝檔路徑
- **WHEN** validated installer 位於含 Unicode 或空白字元的 owned 路徑
- **THEN** launcher 將完整路徑作為單一 executable identity 傳給 Windows，不經 cmd tokenization 並成功返回

#### Scenario: installer 持續執行
- **WHEN** Windows 已接受啟動且 installer UI 仍開啟
- **THEN** Lua/BAT build process 可結束並回傳 0，不等待 installer process terminal state

### Requirement: BAT 完成流程不得等待按鍵
`build_install.bat` SHALL 原樣傳遞 Lua 參數與 exit code，成功訊息 SHALL 表達 installer 已啟動，且所有成功或失敗路徑 MUST NOT 使用 `pause` 或其他互動式關閉提示。

#### Scenario: 從命令列成功執行
- **WHEN** Lua 完成打包並接受 installer launch
- **THEN** BAT 顯示繁體中文成功訊息並立即以 0 結束

#### Scenario: 從自動化環境失敗
- **WHEN** Lua 回傳非零碼且 stdin 不可互動
- **THEN** BAT 顯示包含該 exit code 的失敗訊息並立即回傳相同非零碼，不等待鍵盤輸入

