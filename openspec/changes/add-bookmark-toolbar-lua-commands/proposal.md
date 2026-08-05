## 為什麼

SuperExplorer 目前缺少可快速開啟常用檔案與資料夾的工具列，也沒有可由使用者明確觸發、並以目前資料夾為上下文執行的 Lua 指令。現有 `.explorer.lua` 採取進入資料夾即自動載入的模式，和可預期、可管理的書籤指令需求不符。

## 變更內容

- 新增 Firefox 式的持久化書籤工具列，可收藏資料夾、檔案與內建 Lua 指令。
- 新增快速加入、Lua 書籤建立與完整書籤管理員，以支援編輯、刪除與排序。
- Lua 書籤僅在使用者點選時執行，並且只接收唯讀 `current_folder` 字串。
- 在工具列與管理員以明顯不同的資料夾、檔案與 Lua 指令圖示呈現各類書籤。
- **BREAKING** 移除進入資料夾時自動探索、載入或執行 `.explorer.lua` 的功能；既有使用者檔案不會被修改或刪除。

## 能力

### 新增能力

- `bookmark-toolbar`: 管理與顯示可持久化的資料夾、檔案及 Lua 書籤，並提供工具列與管理介面。
- `on-demand-lua-bookmarks`: 在目前檔案系統資料夾中安全且非阻塞地執行使用者點選的 Lua 書籤。

### 修改能力

- 無。

## 影響

- 受影響程式：`explorer-model` 的 session 模型、`explorer-ui` 的狀態與 chrome、`explorer-app` 的 session 組裝，以及 Lua runtime 的受限呼叫端。
- 移除 `explorer-app` 的資料夾自動化組裝、`explorer-ui` 的資料夾腳本掛接，以及 `explorer-automation` 中只供 `.explorer.lua` 自動探索使用的介面與測試。
- 不新增外部服務、網路權限或執行期相依套件；沿用內建 Lua runtime 與背景工作排程。
