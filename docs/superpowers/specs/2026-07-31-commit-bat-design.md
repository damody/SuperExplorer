# `commit.bat` 設計規格

## 目標

在專案根目錄新增單一 `commit.bat`，透過 Codex CLI 的非互動模式，自動整理目前主倉庫與 submodules 的既有改動，依功能分類建立中文提交，最後推送遠端。

## 執行介面

- 批次檔以自身所在目錄作為 Codex 工作目錄，避免從其他路徑啟動時操作錯誤倉庫。
- 使用 `codex exec` 執行，不開啟互動式介面。
- 模型指定為 `gpt-5.3-codex-spark`。
- 將 `model_reasoning_effort` 設為 `low`，對應快思模式。
- 使用 `danger-full-access` sandbox、`never` approval policy，讓代理可執行 Git commit 與 push，不因等待互動核准而中斷。

## 提示詞要求

內嵌中文提示詞必須要求代理：

1. 不修改任何現有專案檔案，只處理提交與推送。
2. 檢查主倉庫與所有 submodules 的目前改動。
3. 辨識並排除編譯、建置、測試或工具產生的暫存檔，不將它們加入提交。
4. 依功能與關聯性將改動分批提交，不把不相干的內容塞入同一筆提交。
5. 使用中文 commit subject，並在 commit body 詳細說明內容與理由。
6. 正確處理 submodule 內部提交與主倉庫的 submodule 指標更新。
7. 提交完成後推送相關分支；若無改動、缺少遠端、驗證失敗或 push 失敗，明確回報狀態。

## 錯誤處理

- `commit.bat` 回傳 Codex CLI 的原始退出碼。
- CLI 執行失敗時顯示簡短錯誤訊息，成功時顯示完成訊息。
- 不在批次檔內自行執行額外 Git 指令；所有判斷與 Git 操作由 Codex 根據提示詞完成。

## 驗證

- 以靜態檢查確認批次語法、模型名稱、推理設定、工作目錄及提示詞完整。
- 使用不會執行 Codex 的方式檢查檔案內容，避免測試時意外提交或推送目前工作區。
- 不實際執行 `commit.bat`，因為它的預期效果包含不可逆的 commit 與外部 push。

## 範圍限制

- 僅新增 `commit.bat`；不調整其他專案程式或建置設定。
- 不額外建立提示詞檔、PowerShell 腳本或設定檔。
