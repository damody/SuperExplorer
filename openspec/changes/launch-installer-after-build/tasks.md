## 1. 非等待式互動程序啟動器

- [x] 1.1 在 `build/lib/process.lua` 抽出可重用的 argument encoding，確保既有 `process.run` 的 quoting、logging、等待與錯誤行為不變。
- [x] 1.2 新增 `process.start(spec)`，要求 stage/exe/cwd，支援分離 arguments，使用 encoded PowerShell 與 `UseShellExecute=true` 啟動互動程序。
- [x] 1.3 讓 `process.start` 在 Windows 拒絕 launch 時產生與 `process.run` 相容的 structured failure，包含 stage、display command、cwd 與 nonzero exit code。
- [x] 1.4 新增 Lua smoke，以 Unicode/空白 owned path 與受控 PowerShell child 寫入 marker，證明 launcher 非等待返回且沒有 cmd tokenization。

## 2. Installer build handoff

- [x] 2.1 在 `build/build_install.lua` 保留 `--check` 的 early return，確認其發生在 dist 建立、output 刪除與任何 launch 之前。
- [x] 2.2 在 NSIS 成功及 `validate_executable(output)` 後，以 output 為唯一 executable、dist 為 working directory 呼叫 `process.start` 恰好一次。
- [x] 2.3 更新成功輸出，明確顯示已驗證的 installer 路徑與已交接啟動；啟動失敗沿用既有 `format_failure` 並回傳非零碼。
- [x] 2.4 加入 contract test，驗證 launch call 位於 output validation 後，且原始碼不存在 dist glob、latest/timestamp selection 或替代 installer fallback。

## 3. BAT completion flow

- [x] 3.1 更新 `build_install.bat` 成功訊息為「建置完成並已啟動安裝程式」，失敗訊息保留 Lua exit code。
- [x] 3.2 移除成功與失敗路徑的 `pause`／按鍵提示，維持 `%*` 參數原樣傳遞及 `exit /b %BUILD_EXIT_CODE%`。
- [x] 3.3 加入 BAT 靜態 contract test，驗證無 pause、無 dist 掃描、成功／失敗訊息及 exit-code forwarding。

## 4. 回歸與 OpenSpec 驗證

- [x] 4.1 執行 bundled Lua syntax checks、process launcher marker smoke 與 installer/BAT contract tests，保存精確失敗原因。
- [x] 4.2 執行 `build_install.bat --check` 或等價 bundled-Lua check，證明不建立、刪除或啟動 installer 且 exit code 為 0。
- [x] 4.3 將新 contract case 加入 UITEST manifest，對應 `installer-build-handoff` requirements 並通過 `--validate-only` coverage gate。
- [x] 4.4 執行新 UITEST case，確認沒有真正 installer UI 或安裝副作用，並記錄 marker、exit code 與 report。
- [x] 4.5 執行 OpenSpec strict validation、placeholder/舊 fallback 掃描與 git diff check，完成 requirement-to-test traceability。
