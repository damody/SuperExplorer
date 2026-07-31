## ADDED Requirements

### Requirement: 多分頁生命週期
系統 SHALL 支援建立、切換、重新排序與關閉多個分頁；每個分頁 MUST 擁有獨立 location、history、directory state、selection、view state、search state、generation 與 cancellation scope。

#### Scenario: 背景分頁完成列舉
- **WHEN** 使用者在分頁 A 導覽後切換到分頁 B，而 A 的列舉稍後完成
- **THEN** A 的結果只能更新 A，B 的 file view、status 與 selection 不得被改寫

#### Scenario: 關閉有進行中工作的分頁
- **WHEN** 使用者關閉仍在列舉或搜尋的分頁
- **THEN** 該分頁的所有 request 必須取消，後續 event 必須被拒絕，其他分頁維持可用

### Requirement: Per-tab 導覽 history
每個分頁 SHALL 維護獨立 Back/Forward history，包含 location、display title 與可重建 view anchor；Refresh MUST NOT 新增 history entry，失敗的導覽 MUST NOT 破壞目前 history。

#### Scenario: 不同分頁各自返回
- **WHEN** 分頁 A 與 B 分別瀏覽不同的兩層資料夾後，各自執行 Back
- **THEN** 每個分頁必須回到自己的上一個 location，不得使用另一分頁的 history

#### Scenario: 導覽失敗
- **WHEN** active tab 導覽到不存在或無權限的資料夾
- **THEN** 系統必須保留原 location/history 與可重試錯誤，Forward/Back 狀態不得被錯誤提交污染

### Requirement: 真實 location 解析與增量列舉
系統 SHALL 透過 Windows Shell API 解析本機 location，於專用 STA 增量列舉 children，以 bounded batches 將 owned domain entries 傳回 UI；一般 GPUI callback MUST NOT 執行同步目錄掃描或 apartment-affine COM call。

#### Scenario: 大型真實資料夾
- **WHEN** 使用者導覽到含 100,000 個實際項目的測試資料夾
- **THEN** 系統必須在完整列舉前顯示首批/首個 viewport、保持 UI 回應，並最終顯示真實總數

#### Scenario: 空資料夾
- **WHEN** 真實資料夾列舉成功且沒有 child
- **THEN** directory state 必須成為 ready-empty，而不是 loading、error 或含假項目

### Requirement: Request generation 與取消
每次 Navigate、Refresh 或重新列舉 SHALL 建立含 tab/request/generation 的 context；新 generation MUST 取消舊 request，model MUST 拒絕任何不符合目前 context 的 batch、error 或 terminal event。

#### Scenario: 快速連續導覽
- **WHEN** 同一分頁快速依序導覽 A、B、C 且 A/B 結果晚於 C
- **THEN** 最終只能顯示 C，A/B 的所有 late batches 與 errors 必須被拒絕

### Requirement: Stable item identity 與 selection
directory entries 與 selection SHALL 使用 stable `ShellItemId`；sort、watcher insertion/deletion/rename 與 incremental batches MUST NOT 以可變 row index 或 path string 作為唯一 identity。

#### Scenario: Rename 後保持選取
- **WHEN** 已選項目被本程式或 watcher 通知重新命名且 identity 可配對
- **THEN** selection 必須仍指向同一項目並更新 display name，不得任意移到相同行號的其他項目

### Requirement: 真實檔案開啟與資料夾進入
系統 SHALL 依 item capability 區分 container navigation 與 Shell open；資料夾在目前/新分頁開啟，檔案使用公開 Shell open 行為，錯誤 MUST 可觀測且不得卡住 UI。

#### Scenario: 進入資料夾
- **WHEN** 使用者啟動真實子資料夾項目
- **THEN** active tab 必須開始新 generation 的增量導覽並在成功後提交 history

#### Scenario: 開啟檔案失敗
- **WHEN** Shell 無法開啟真實檔案
- **THEN** UI 必須顯示安全的可採取行動錯誤，原 directory/selection 保持可用

### Requirement: Watcher merge 與 overflow recovery
系統 SHALL 監看 active/loaded 本機資料夾變更，coalesce notifications、盡量配對 rename、以 stable-ID diff 更新 snapshot；buffer overflow 或不完整通知 MUST 觸發重新列舉與 diff。

#### Scenario: 外部快速變更
- **WHEN** 測試在真實資料夾快速建立、rename、刪除大量項目
- **THEN** UI 必須最終收斂到磁碟真實內容，selection/history 不得被整體任意清除

#### Scenario: Watcher overflow
- **WHEN** 測試注入或造成 watcher overflow
- **THEN** 系統必須記錄 overflow、重新列舉、以新 generation/diff 收斂並發出可觀測 terminal state

### Requirement: 真實資料夾整合測試矩陣
專案 SHALL 使用測試唯一擁有的 temporary root 驗證小型、100,000 項目、Unicode、emoji、長名稱、hidden/system、reparse point、permission denied、rapid changes、rename storm 與 watcher overflow；破壞性 setup/cleanup MUST 驗證目標仍位於 temporary root。

#### Scenario: 測試安全邊界
- **WHEN** fixture 被要求在 workspace root、使用者 profile、drive root 或 temporary root 外建立/刪除資料
- **THEN** fixture 必須拒絕操作並以明確錯誤結束，不得修改非測試資料

#### Scenario: Fake 與 real contract parity
- **WHEN** 對 fake Shell service 與 real temporary-folder service 執行相同 navigation contract cases
- **THEN** command/event、cancellation 與 terminal semantics 必須一致，平台細節只能出現在 adapter evidence
