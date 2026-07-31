## Context

`build_install.bat` 啟動 bundled Lua 後只接收 exit code；真正的版本解析、release validation、NSIS invocation 與輸出路徑 ownership 全在 `build/build_install.lua`。既有 `process.run` 專供非互動子程序：關閉視窗、redirect stdout/stderr 並等待結束，不能直接拿來啟動安裝精靈。

工作區同時有其他未提交修改，本變更只能接觸 installer handoff 所需的 BAT、Lua process helper、測試及自身 OpenSpec，不得整理或提交其他模組。

## Goals / Non-Goals

**Goals:**

- 成功建置與驗證後啟動本次精確輸出的 installer 一次。
- 讓 interactive installer 與 build process 解耦，BAT 不等待安裝完成。
- 對 `--check` 和任何 build/package/validation/launch failure 維持 fail-closed。
- 保留 Unicode、空白路徑與原始 exit-code contract。

**Non-Goals:**

- 不自動安裝、傳入 silent switch、提升權限或控制安裝精靈。
- 不等待 installer exit，不把 install result 併入 build result。
- 不改版本、NSIS payload、clean-tree gate 或 output naming。

## Decisions

### 由 Lua 執行精確 handoff

在 `validate_executable(output)` 成功後立即呼叫 `process.start`，因為只有 Lua 同時擁有本次 version-derived output 與驗證結果。BAT 掃描 `dist` 的方案會允許 stale artifact；輸出暫存檔方案則增加不必要的跨程序狀態。

### 新增獨立的非等待 process primitive

保留 `process.run` 原行為，新增 `process.start(spec)`。它以現有 UTF-16LE/Base64 encoded PowerShell pattern 建立 `ProcessStartInfo`，設定 `UseShellExecute = true`、`FileName`、`WorkingDirectory` 與分離 arguments，呼叫 `Process.Start` 後立即 dispose process handle 並成功返回。PowerShell 只在 OS 拒絕 launch 時以非零碼失敗。

這比 `os.execute("start ...")` 更安全，因為沒有未跳脫的 cmd metacharacter，也能沿用 Unicode literal encoding。`process.start` 不 redirect streams、無 `CreateNoWindow`，因此 installer 能顯示正常 Windows UI。

### BAT 不再阻塞

BAT 原樣傳遞參數與 Lua exit code。成功訊息表達「建置完成並已啟動安裝程式」；失敗訊息保留非零碼。移除所有路徑的 `pause`，因為它既不是錯誤恢復機制，也會卡住 CI/terminal invocation。

### 測試避免真正安裝

Lua process smoke 以 `powershell.exe` 作為受控 child，要求它在暫存 owned directory 寫 marker；`process.start` 返回後以有界輪詢驗證 marker。靜態 contract test 讀取 `build_install.lua` 與 BAT，驗證 launch 位於 output validation 後、`--check` early return 之前沒有 launch、BAT 不掃描 dist 且不 pause。真實 installer 的產生與內容仍由既有 NSIS/installer smoke 負責，本變更測試不開啟會修改系統的精靈。

## Risks / Trade-offs

- [installer process 在 launch 後立即崩潰，build 仍已成功] → handoff contract 只保證 OS 接受啟動；installer runtime 由既有 installer smoke 驗證。
- [PowerShell policy 或 process creation 失敗] → encoded command 不依賴 script file，錯誤轉為 structured nonzero build failure。
- [BAT 從 Explorer 雙擊時視窗太快關閉] → installer 已獨立顯示；失敗細節仍寫入既有 build logs，且 console invocation 可直接看到摘要。
- [測試誤開真正 installer] → automated tests 只呼叫受控 PowerShell marker child，真 installer path 僅做靜態/PE validation。

## Migration Plan

1. 增加並測試 `process.start`，不改 `process.run`。
2. 在 installer output validation 後接線 launch。
3. 更新 BAT completion/failure flow 並移除 pause。
4. 執行 Lua smoke、contract tests、`--check` 與 OpenSpec/UITEST gates。
5. 回滾時移除單一 launch call 與 BAT 訊息即可；沒有資料或安裝狀態遷移。

## Open Questions

無；正常/`--skip-build` 啟動、`--check` 不啟動及非等待式 handoff 均已核准。

