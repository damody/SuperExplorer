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
