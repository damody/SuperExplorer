# 書籤工具列與 Lua 指令驗證證據

## 聚焦驗證

- `cargo check -p explorer-app`：通過。
- `cargo test -p explorer-model bookmark --lib`：4 項通過，包含依型別化目標查找既有書籤。
- `cargo test -p explorer-automation --lib bookmark --no-fail-fast`：4 項通過，涵蓋唯讀 `current_folder`、禁止其他 host API、例外與 timeout。
- `cargo test -p explorer-ui --lib bookmark --no-fail-fast`：通過工具列溢位聚焦測試。
- production source 負向搜尋：`AutomationComposition`、`FolderScriptHandle`、`enter_directory` 與 `.explorer.lua` 均不存在。
- `openspec validate add-bookmark-toolbar-lua-commands --strict`：通過。

## 驗證說明

- `superexplorer-mft-helper` 的工作區編譯問題已由目前工作樹解除，app、broker 與正式 UI binary 均可建置。
- 全域 UITEST coverage gate 仍會列出其他同時進行中 OpenSpec changes 的未登錄 requirements；本變更的 7 個 requirements 已全部映射至 `bookmark-toolbar-headful`。

## 2026-08-05 Headful UITEST

- 正式 case：`bookmark-toolbar-headful`，full/visual suites，結果 PASS（19.3 秒）。
- 執行位置：`target/uitest-runs/bookmark-toolbar-star-left-v12`。
- 實機檢查確認：原生右鍵「加入書籤」、單選項目的實心／空心星號切換、檔案／資料夾／Lua 不同圖示、More Bookmarks overlay 與管理員均可辨識且無遮擋。
- 星號切換自動化涵蓋「加入 → 取消 → 再加入」，且切換後保留原檔案選取；位置斷言確認星號永遠位於第一個書籤左側。

## 主要產物 SHA-256

- `report.json`：`9c7773b259c26eb8a4d0b465782154c6775e982d8cfe35427014ed25daed403c`
- `bookmark-star-on.png`：`59e55e0a2c527f58619204eb316e925ee592d16ade21daeed84ba9f1aaa5d3e9`
- `bookmark-star-off.png`：`816a82038a61928f68e0565b68dc445b6d4b5f9da41e1dac8e3f091c4d9b6104`
- `bookmark-toolbar.png`：`6ced5020735265aed02c4097d7d83d2070279cceb4f4004062c826c0991c1c6c`
- `bookmark-overflow.png`：`bf73352e60398f05b57e535a0ca36e2a7465496058d491e385905e8fdd0a0c68`
- `bookmark-manager.png`：`ec8793bd81e7528b84dfed46ad21477beba8e0beae41a3f108ea7855fdf69cbe`
- `bookmark-context-menu.png`：`acd4a5fcba7998249786298bf5329413d7f59e0e81fbe47ccbd1282f4badfefb`

完整 requirement／scenario 到 task／gate／evidence 對應見 `traceability.md`。

## 2026-08-14 completion verification

- Bookmark manager typed edit, delete, persistence, and native row drag/drop reorder are covered by focused UI state and render-contract tests.
- Lua coverage now includes physical-folder success, non-filesystem rejection, read-only reassignment failure, script exception, injectable runtime startup failure, timeout, and the corresponding user notices.
- Session restart round-trip preserves typed bookmark payloads, stable IDs, and order.
- Focused results: explorer-model 5/5 plus restart 1/1; explorer-automation 5/5; explorer-ui bookmark 7/7; `cargo build -p explorer-app` passed.
- Fresh production-binary headful run passed at `evidence/headful-2026-08-14`; strict OpenSpec validation, UITEST manifest JSON parsing, retired-source negative search, formatting, and `git diff --check` passed.

### Fresh headful SHA-256

- `report.json`: `9c7773b259c26eb8a4d0b465782154c6775e982d8cfe35427014ed25daed403c`
- `bookmark-context-menu.png`: `99af900a1d327c314c09d4b032ff2ecbfc00f097a4e8e515406992f23340ebf5`
- `bookmark-manager.png`: `04945376d7c6be06ffad45372ce567feab2c3bf19c7e9c9e43ef2ccb6931b4b4`
- `bookmark-overflow.png`: `4cc06e6f170407d81fd3660f84a6031120d0e3a9ae2a8d337dd6fda25728f5ba`
- `bookmark-star-off.png`: `08ee536a9639c8ea5187e8dbf4b298c6a77635a85856dda53c84ad3da71918e0`
- `bookmark-star-on.png`: `98ba67b9d07a23dc973e340697e5fb07674ee802f2dee51370d68ab3656d36f9`
- `bookmark-toolbar.png`: `f3ed79cb3520ca96e7c28a29f46ebbb05bd6dffd562bd8852455eed4ec335e2b`
