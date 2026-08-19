# build_install 忽略 SuperDesktop 證據 log 設計

## 背景

直接執行 `build_install.bat` 會以 `--component all` 建置正式的 SuperExplorer 與 SuperDesktop 組合安裝程式。正式模式目前要求 SuperDesktop submodule 的 HEAD、parent gitlink、remote URL 與整個工作樹完全一致。這項檢查會把 OpenSpec headful 驗證留下的未追蹤 `.log` 視為 product/build source 變更，導致建置在編譯與 NSIS 階段之前中止。

已確認的失敗案例只有下列未追蹤檔案：

- `openspec/changes/align-start-footer-windows11-parity/evidence/**/report.log`
- `openspec/changes/align-start-footer-windows11-parity/evidence/**/stderr.log`
- `openspec/changes/align-start-footer-windows11-parity/evidence/**/stdout.log`

這些檔案是可重新產生的測試輸出，不是安裝程式輸入，也不應弱化原始碼、建置設定、gitlink 或供應來源檢查。

## 目標

- 直接執行 `build_install.bat` 時，未追蹤的 `openspec/**/evidence/**/*.log` 不再阻擋正式安裝程式建置。
- 任何已追蹤檔案變更、未追蹤原始碼、未追蹤建置設定或 evidence 以外的 log 仍必須阻擋正式建置。
- 保留 SuperDesktop HEAD、parent gitlink、declared/configured/origin URL 與 initialized 狀態驗證。
- 不刪除、搬移、提交或改寫任何 evidence log。
- 成功的無參數執行仍產生當次版本的正式安裝檔並依既有契約啟動它。

## 非目標

- 不允許正式模式接受任意 dirty SuperDesktop 工作樹。
- 不改變 `build_desktop_test_install.bat` 的測試建置語意。
- 不放寬 SuperExplorer Rust source cleanliness gate。
- 不清理既有 `build/logs`、`dist` 或 OpenSpec evidence。
- 不變更版本計算、release build、PE 驗證、NSIS 打包或 artifact publication 流程。

## 方案

### 批次入口

`build_install.bat` 在既有 `--component all` 後加入一個專用內部旗標，例如 `--ignore-superdesktop-evidence-logs`。使用者提供的參數仍照原順序轉送，既有 `--check`、`--skip-build`、`--no-launch` 與退出碼行為不變。

旗標由正式批次入口明確傳入，避免直接呼叫 Lua 的其他流程在不知情下改變潔淨檢查語意。

### 參數解析

`build/lib/installer_components.lua` 新增對應布林選項，且只允許搭配 `--component all`。若在 `superexplorer` 或 `superdesktop` 模式使用，參數解析立即失敗。這能防止旗標成為一般用途的 dirty bypass。

### SuperDesktop 狀態分類

`build/build_install.lua` 仍取得完整的 `git status --porcelain=v1 --untracked-files=all`。只有在正式批次明確開啟旗標時，才對每一列做分類：

- 僅忽略狀態為 `??` 的未追蹤路徑；
- 路徑必須位於 `openspec/` 下任一 `evidence/` 目錄；
- 副檔名必須精確為 `.log`；
- 路徑分隔符正規化為 `/` 後再比對；
- 已追蹤的 modified/deleted/renamed log 不得忽略；
- evidence 外的 log、任何其他副檔名及任何 source/build 檔案不得忽略。

分類後仍有任何狀態列時，沿用既有 `validate_submodule_identity` 失敗路徑並列出阻擋項目。gitlink、HEAD 與 URL 驗證不經過此過濾器。

## 錯誤處理

- 未知旗標或錯誤 component 組合：在進入建置前以非零碼失敗。
- porcelain 列無法安全分類：採 fail-closed，保留該列並阻擋正式建置。
- SuperDesktop HEAD、gitlink 或 URL 不一致：維持原本錯誤，不因 evidence log 旗標放行。
- Lua、Cargo、PE、NSIS 或啟動失敗：維持原有 exit code 與最終 `[FAILURE]` 摘要。

## 測試與驗收

擴充既有 installer component 與 handoff 測試，至少涵蓋：

1. `build_install.bat` 以 `--component all --ignore-superdesktop-evidence-logs` 呼叫 Lua。
2. `all` 模式接受新旗標；其他 component 模式拒絕。
3. 只有未追蹤 `openspec/**/evidence/**/*.log` 時，正式 identity 驗證通過。
4. 未追蹤 `.rs`、`.toml`、evidence 外 `.log`、evidence 內非 `.log` 仍失敗。
5. 已追蹤 evidence log 的修改或刪除仍失敗。
6. dirty 工作樹與 gitlink、HEAD、URL 負向案例維持通過。
7. `build_install.bat --check` 成功且不建立或啟動安裝檔。
8. 無參數正式執行產生新的 `dist/SuperExplorer-Setup-<version>-x64.exe`，通過 PE 驗證並依既有行為啟動。

## 安全性與相容性

此設計只豁免明確、未追蹤、位於 OpenSpec evidence 下的 `.log`。它不允許任意 dirty source、不依賴刪除檔案，也不改變安裝內容或 component selection。現有 CI 與直接 Lua 呼叫若未傳入旗標，會保持目前的嚴格行為。
