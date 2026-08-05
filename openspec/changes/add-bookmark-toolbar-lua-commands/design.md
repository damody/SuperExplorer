## 背景

目前的 session 已持久化快速存取釘選、分頁與檢視設定；UI 亦具備位址列下方的 chrome 與 Windows Shell 檔案開啟路徑。Lua runtime 目前則被資料夾範圍的 `.explorer.lua` 自動化生命週期持有：進入目錄就可能探索與啟用腳本。此變更跨越模型、持久化、UI、應用程式組裝與 Lua 執行邊界，且會移除現有自動化模式。

使用者已核准的產品契約是：書籤工具列提供資料夾、檔案和 Lua 指令；Lua 只在點選時直接執行，且唯一的 Explorer 上下文是唯讀 `current_folder`。

## 目標與非目標

**目標：**

- 以既有 session 背景寫入機制持久化有順序的三類書籤。
- 提供 Firefox 式工具列、快速加入與可編輯、排序、刪除的管理員。
- 以非阻塞工作安全地執行 Lua，並在成功或失敗時提供可閱讀的結果。
- 徹底移除 `.explorer.lua` 的自動探索與執行，而不觸碰使用者磁碟上的檔案。

**非目標：**

- 不提供網頁 URL 書籤、書籤同步、匯入／匯出或資料夾群組。
- 不把目前選取項目、檔案操作、Shell、程序啟動或任意 Explorer API 暴露給 Lua 書籤。
- 不遷移、刪除或改寫既有 `.explorer.lua` 檔案。
- 不保留資料夾進入時的自動化相容模式。

## 決策

### 以型別化書籤集合擴充 session

`explorer-model` 新增可序列化的 bookmark 值物件與集合操作，並把其放入 `PersistedSessionEnvelope` payload。集合以穩定 UUID 與明確排序表示，而非以 UI 索引作為身分。

這比另建設定檔更符合既有重設、版本驗證與背景持久化流程；亦可沿用 Quick Access 的 mutation／rollback 模式。缺失目標不會在還原時被濾掉，避免意外遺失使用者資料。

替代方案是把 Lua 腳本另存為檔案並只收藏檔案路徑。它無法提供一致的內建編輯與可攜 session，因此不採用。

### 工具列與管理員共用同一個 UI 狀態集合

Root UI state 是書籤的唯一來源。工具列只投影可見範圍並把溢位放入 More Bookmarks；管理員則投影完整集合並發出新增、更新、重排、刪除 mutation。資料夾、檔案、Lua 三者皆有固定而不同的圖示語意。

這避免工具列與管理員產生雙重排序或競態。星號是工具列第一個固定控制項，不參與書籤排序或 overflow；單選檔案系統項目時，以空心／實心星號投影該型別化目標是否已存在，按下即可加入／移除；未單選實體項目時停用。右鍵快速加入仍僅適用於檔案系統檔案或資料夾；Lua 僅由 `+` 或管理員建立。

### Lua 書籤採用受限、一次性的執行請求

每次點選 Lua 書籤時，UI 先確認目前 tab 是已解析的實體資料夾，再建立包含 source、bookmark ID、工作目錄與 timeout 的背景請求。runtime 僅在 Lua global scope 注入不可重新指派的 `current_folder` 字串，並且不註冊任何其他 Explorer host function。完成狀態以既有非阻塞任務／通知呈現。

這比重用資料夾腳本 registry 更安全：registry 的生命週期、監看與 host action 語意是為自動化設計，會讓手動書籤取得不必要能力。若現有 runtime 不能在不帶自動化 host 的情況下建立，實作必須先建立最小 bookmark runtime adapter；不得退回到 `.explorer.lua` discovery。

### 直接移除資料夾自動化組裝

移除 app 的 `AutomationComposition`、UI 的 `FolderScriptHandle` 連接、目錄切換時的 `enter_directory` 呼叫，以及只服務該流程的探索／測試。保留可重用的 Lua runtime／排程程式碼，但僅由新的 bookmark adapter 持有。

此為刻意的 breaking change，沒有 feature flag 或回退自動執行。回退版本僅能透過回滾應用程式二進位檔達成，不能以讀取舊腳本補救。

## 風險與取捨

- [session schema 不相容或資料損毀] → 為新欄位提供向後相容預設值、限制數量與原始碼大小，並測試舊 session 解碼與無效項目恢復。
- [Lua 腳本造成 UI 卡頓或無限執行] → 固定在背景工作排程執行並套用 timeout；逾時為明確的 terminal failure。
- [腳本誤以為具備完整檔案 API] → 只注入 `current_folder`，在編輯介面與錯誤訊息明示限制，並用測試防止新增 host API。
- [移除自動化後留下隱性呼叫] → 對程式樹做負向搜尋與回歸測試，驗證進入含 `.explorer.lua` 的資料夾不會讀取或執行該檔案。
- [工具列寬度不足] → 保留首要書籤的順序並以單一 More Bookmarks 選單呈現溢位，不截斷或遺失項目。

## 遷移與回復計畫

1. 讀取舊 session 時以空書籤集合初始化；舊 Quick Access 保持不變。
2. 新版不掃描或變更任何 `.explorer.lua`，使用者若需命令須手動新增為 Lua 書籤。
3. 更新後若需回復，安裝前一版二進位檔即可恢復舊自動化行為；新版新增的 session 欄位必須可由舊版安全忽略或拒絕並採既有恢復路徑處理。
4. 不執行檔案系統資料遷移或刪除。

## 未解決問題

無。實作期間若現有 runtime 的受限 adapter 需要調整，僅可作為不改變已核准 scope 與安全契約的 A 級任務細化；若需要擴張 Lua 權限或恢復自動探索，必須視為需使用者核准的重大變更。
