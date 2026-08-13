## ADDED Requirements

### Requirement: 持久化型別化書籤

系統 SHALL 在使用者 session 中持久化具有穩定識別碼與排序的資料夾、檔案及 Lua 書籤。每個書籤 MUST 僅包含其型別允許的 payload，且 session 還原 MUST 保留有效書籤的順序與 Lua 原始碼。

#### Scenario: 還原有序書籤

- **WHEN** 使用者重新啟動具有已儲存書籤的 SuperExplorer
- **THEN** 系統 SHALL 以儲存順序還原所有有效的資料夾、檔案與 Lua 書籤

#### Scenario: 還原不存在的目標

- **WHEN** 已儲存的檔案或資料夾目標已在檔案系統中不存在
- **THEN** 系統 MUST 保留該書籤並允許使用者在管理員修正或刪除它

#### Scenario: 舊 session 載入

- **WHEN** 系統載入不含書籤欄位的既有有效 session
- **THEN** 系統 SHALL 以空書籤集合完成載入而不改變既有 session 資料

### Requirement: 書籤工具列與型別圖示

系統 SHALL 在位址列下方顯示書籤工具列，並以可明確辨識的資料夾、檔案與 Lua 指令圖示呈現相應型別。工具列 MUST 依持久化順序顯示，且空間不足時 MUST 將未顯示項目放入 More Bookmarks 選單。

#### Scenario: 顯示型別化書籤

- **WHEN** 工具列含有資料夾、檔案與 Lua 書籤
- **THEN** 系統 SHALL 以不同型別圖示和持久化順序顯示它們

#### Scenario: 工具列溢位

- **WHEN** 工具列寬度不足以顯示全部書籤
- **THEN** 系統 MUST 以 More Bookmarks 選單提供每個溢位書籤且不得遺失其相對順序

### Requirement: 建立與管理書籤

系統 SHALL 在書籤工具列最左側固定提供醒目的星號，以切換目前分頁所顯示實體檔案系統資料夾的書籤狀態，並允許使用者從右鍵選單加入選取項目書籤；系統 SHALL 提供建立 Lua 書籤的工具列 `+` 操作與完整書籤管理員。管理員 MUST 支援改名、依型別編輯 payload、重排與刪除。

#### Scenario: 以星號切換目前資料夾的書籤狀態

- **WHEN** 目前分頁顯示尚未收藏的實體檔案系統資料夾，且使用者在未選取任何子項目的情況下按下空心星號
- **THEN** 系統 SHALL 立即加入目前資料夾的 Folder 書籤、持久化變更並在工具列最左側顯示實心星號
- **WHEN** 使用者再次按下實心星號
- **THEN** 系統 SHALL 立即移除相同型別化目標的書籤、持久化變更並恢復空心星號

#### Scenario: 星號不受檔案清單選取影響

- **WHEN** 使用者在目前資料夾中選取、切換或清除任一子項目
- **THEN** 系統 MUST 維持以目前資料夾為目標的星號狀態與切換行為

#### Scenario: 非檔案系統位置停用星號

- **WHEN** 目前分頁位置不是可解析的實體檔案系統資料夾
- **THEN** 系統 MUST 停用星號且不得建立書籤資料

#### Scenario: 從右鍵選單加入資料夾

- **WHEN** 使用者對檔案系統資料夾選取「加入書籤」
- **THEN** 系統 SHALL 新增指向該資料夾的書籤並持久化此變更

#### Scenario: 建立 Lua 書籤

- **WHEN** 使用者選擇工具列的 `+` 操作並儲存名稱與 Lua 原始碼
- **THEN** 系統 SHALL 新增 Lua 書籤並在工具列與管理員顯示腳本圖示

#### Scenario: 管理員重排書籤

- **WHEN** 使用者在書籤管理員重新排序一個書籤
- **THEN** 系統 MUST 立即以新順序更新工具列並在下一次啟動後維持該順序

### Requirement: 型別化目標啟用

系統 SHALL 對資料夾書籤在目前分頁導航，並 SHALL 對檔案書籤使用既有 Windows Shell 開啟行為。無法使用的檔案系統目標 MUST 顯示非阻塞且可理解的錯誤。

#### Scenario: 啟用資料夾書籤

- **WHEN** 使用者點選可存取的資料夾書籤
- **THEN** 系統 SHALL 在目前分頁導航至該資料夾

#### Scenario: 啟用檔案書籤

- **WHEN** 使用者點選可存取的檔案書籤
- **THEN** 系統 SHALL 交由既有 Windows Shell 行為開啟該檔案

#### Scenario: 啟用不存在的目標

- **WHEN** 使用者點選已不存在或無法開啟的資料夾或檔案書籤
- **THEN** 系統 MUST 保留書籤並顯示不阻塞使用者的失敗通知
