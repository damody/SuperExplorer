## Why

目前整理工作區改動、依功能拆分中文提交、處理 submodules 並推送，需要反覆向 Codex 說明相同規則。新增一個專案內的批次入口，可將這套提交流程固定化並降低誤收建置暫存檔的風險。

## What Changes

- 新增根目錄 `commit.bat`，以非互動模式啟動 Codex CLI。
- 固定使用 `gpt-5.3-codex-spark` 與 low reasoning effort。
- 內嵌中文提交提示詞，要求只整理既有改動、不修改專案內容、排除暫存檔、按功能分批提交並推送。
- 涵蓋主倉庫與 submodules，並保留 Codex CLI 的退出狀態供呼叫端判斷。

## Capabilities

### New Capabilities

- `automated-git-commit`: 以批次檔驅動 Codex，自動審查、分類、提交並推送主倉庫及 submodules 的既有改動。

### Modified Capabilities

無。

## Impact

- 新增根目錄批次檔 `commit.bat`。
- 執行環境需要已安裝並登入 Codex CLI，且 Git remote 允許推送。
- 不變更應用程式、建置流程或既有 API。
