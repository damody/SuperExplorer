## 1. 模型與持久化

### 1.1 定義書籤契約

**目的：** 建立型別化且可還原的書籤模型。
**輸入：** 已核准設計與既有 session 模型。
**產出：** Bookmark 模型、集合 mutation 與序列化支援。
**依賴：** 無。
**Owner／Wave：** Primary integrator／wave 0。
**Gate／Evidence：** G1；evidence index `1.1.*`。
**完成門檻：** 三類書籤可穩定序列化、重排與 rollback。

- [x] 1.1.1 建立具穩定 ID、名稱、排序與 Folder/File/LuaScript payload 的 Bookmark 模型。
- [x] 1.1.2 實作新增、編輯、刪除、重排與 rollback 的集合操作及單元測試。
- [x] 1.1.3 將書籤 payload 納入 session，並測試舊 session 載入為空集合與無效資料恢復。

### 1.2 接入 session 協調器

**目的：** 將書籤 mutation 接入既有背景持久化。
**輸入：** 1.1 模型、UI state 與 app session lifecycle。
**產出：** 還原、投影與背景儲存接線。
**依賴：** 1.1。
**Owner／Wave：** Primary integrator／wave 0。
**Gate／Evidence：** G2；evidence index `1.2.*`。
**完成門檻：** 所有 mutation 在重啟後還原，且不影響 Quick Access。

- [x] 1.2.1 在 root UI state 提供書籤 hydration、讀取與 mutation API。
- [x] 1.2.2 將書籤納入 app session snapshot 與還原組裝。
- [x] 1.2.3 以聚焦測試驗證成功持久化、失敗回復與既有 session 欄位保持不變。

## 2. 工具列與管理介面

### 2.1 呈現與啟用書籤

**目的：** 顯示 Firefox 式工具列並正確啟用檔案系統目標。
**輸入：** 1.2 state、chrome layout、圖示與 Shell 開啟介面。
**產出：** 工具列、溢位選單與資料夾／檔案 dispatch。
**依賴：** 1.2。
**Owner／Wave：** Primary integrator／wave 1。
**Gate／Evidence：** G3；evidence index `2.1.*`。
**完成門檻：** 型別圖示、排序、overflow 與目標錯誤均符合規格。

- [x] 2.1.1 在位址列下渲染有不同資料夾、檔案與 Lua 圖示的書籤工具列。
- [x] 2.1.2 將空間不足項目依序放入 More Bookmarks 選單並測試順序不遺失。
- [x] 2.1.3 分派資料夾導航、檔案 Shell 開啟與缺失目標非阻塞錯誤。

### 2.2 建立與管理書籤

**目的：** 提供快速加入及完整 CRUD/排序管理。
**輸入：** 1.2 mutation、2.1 工具列與既有 context menu。
**產出：** 加入操作、Lua 建立器與書籤管理員。
**依賴：** 2.1。
**Owner／Wave：** Primary integrator／wave 1。
**Gate／Evidence：** G4；evidence index `2.2.*`。
**完成門檻：** 使用者可建立、編輯、重排及刪除所有書籤型別。

- [x] 2.2.1 對檔案系統檔案與資料夾新增「加入書籤」右鍵操作。
- [x] 2.2.2 新增工具列 `+` 的 Lua 名稱／原始碼建立與驗證流程。
- [ ] 2.2.3 實作管理員的型別化編輯、拖曳重排、刪除與持久化互動測試。
- [x] 2.2.4 新增單一檔案系統選取項目的空心／實心星號切換，並以 UITEST 驗證加入、取消與再加入。

## 3. Lua 與舊自動化移除

### 3.1 執行受限 Lua 書籤

**目的：** 安全地在背景執行使用者點選的 Lua 指令。
**輸入：** 2.1 Lua dispatch、既有 Lua runtime 與工作排程。
**產出：** 最小 bookmark runtime adapter 與 terminal result。
**依賴：** 2.1。
**Owner／Wave：** Primary integrator／wave 2。
**Gate／Evidence：** G5；evidence index `3.1.*`。
**完成門檻：** 僅提供不可寫 `current_folder`，且所有 terminal 結果非阻塞。

- [x] 3.1.1 定義具有固定 timeout 的按需 Lua request/result 契約並建立背景排程接線。
- [x] 3.1.2 建立只注入唯讀 `current_folder` 的 runtime adapter，禁止所有其他 Explorer host API。
- [ ] 3.1.3 測試實體資料夾成功、非檔案系統拒絕、重新指派、例外、啟動失敗與 timeout。

### 3.2 移除 `.explorer.lua` 自動化

**目的：** 停止任何進入資料夾即執行腳本的行為。
**輸入：** 3.1 adapter、app automation composition 與 UI folder-script 接線。
**產出：** 移除舊組裝與負向回歸測試。
**依賴：** 3.1。
**Owner／Wave：** Primary integrator／wave 2。
**Gate／Evidence：** G6；evidence index `3.2.*`。
**完成門檻：** `.explorer.lua` 不被讀取、執行、修改或刪除。

- [x] 3.2.1 移除 `AutomationComposition`、`FolderScriptHandle` 與目錄切換的 `enter_directory` 接線。
- [x] 3.2.2 移除僅服務自動探索的 API、fixture 與測試，保留 3.1 所需共用 runtime。
- [x] 3.2.3 新增負向整合測試與 source search evidence，證明進入含 `.explorer.lua` 的資料夾不產生工作。

## 4. 驗收與交接

### 4.1 UI 與 session 端到端驗收

**目的：** 驗證可見互動、還原和 Lua 結果。
**輸入：** 1 至 3 階段實作與 UITEST 慣例。
**產出：** 書籤 UITEST、截圖與報告。
**依賴：** 2.2、3.2。
**Owner／Wave：** Primary integrator／wave 3。
**Gate／Evidence：** G7；evidence index `4.1.*`。
**完成門檻：** 需求情境以自動化或聚焦測試取得可重現證據。

- [x] 4.1.1 登錄書籤工具列 UITEST 並驗證圖示、排序、overflow 與管理操作。
- [ ] 4.1.2 驗證重啟還原，以及 Lua 成功、拒絕、例外和 timeout 的通知。
- [x] 4.1.3 保存測試報告、截圖與 session fixture 雜湊至 evidence index。

### 4.2 最終驗證

**目的：** 完成需求到證據的追溯與 scoped diff 審查。
**輸入：** 前置 gates、測試輸出與 OpenSpec artifacts。
**產出：** 嚴格驗證、追溯矩陣與交接報告。
**依賴：** 4.1。
**Owner／Wave：** Primary integrator／wave 4。
**Gate／Evidence：** G8；evidence index `4.2.*`。
**完成門檻：** 所有規格情境皆有 passing evidence，且沒有未解決 P0/P1。

- [x] 4.2.1 執行格式化與 explorer-model、explorer-automation、explorer-ui、explorer-app 聚焦測試。
- [x] 4.2.2 執行 UITEST、建立 requirement/scenario 到 task/gate/evidence 追溯矩陣並索引雜湊。
- [ ] 4.2.3 執行嚴格 OpenSpec 與 detailed task 驗證，審查 scoped diff 並真實更新證據與任務狀態。
