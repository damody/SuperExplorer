## ADDED Requirements

### Requirement: Typed 原生檔案操作
系統 SHALL 以 typed request 將 create、rename、copy、move、recycle delete 與 permanent delete 送至 Shell STA，並使用 `IFileOperation` 或等價公開 Shell API；UI MUST NOT 直接執行同步 destructive filesystem call。

#### Scenario: 建立資料夾
- **WHEN** 使用者在可寫入的真實測試資料夾建立新資料夾
- **THEN** operation 必須回報 queued/running/finished、磁碟上出現項目，watcher/model 最終只顯示一個對應 entry

#### Scenario: Rename 驗證失敗
- **WHEN** 使用者輸入 Windows 不允許或會衝突的名稱
- **THEN** UI 必須保留 rename editor 與文字、顯示可修正錯誤，且不得提交部分 rename

### Requirement: Copy 與 move
系統 SHALL 支援單一與多選項目 copy/move 到真實資料夾，保留 per-item outcome，並依 Shell capability 處理跨 volume、reparse point 與 namespace 限制。

#### Scenario: 多項 copy 部分失敗
- **WHEN** 一批 copy 中部分來源成功、部分因權限或衝突失敗
- **THEN** terminal outcome 必須逐項區分成功/失敗，UI 不得將整批誤報為完全成功或完全失敗

#### Scenario: 跨 volume move
- **WHEN** 測試環境提供兩個受控 volume 且使用者執行 move
- **THEN** 系統必須依 Shell semantics 完成 copy/delete 或回報限制，並保留可驗證的 per-item outcome

### Requirement: 回收刪除與永久刪除
系統 SHALL 明確區分 recycle delete 與 permanent delete，永久刪除 MUST 要求符合產品規則的明確確認，且兩者的 UI、journal capability 與結果不得混淆。

#### Scenario: 回收刪除
- **WHEN** 使用者對受控測試檔案執行一般 Delete
- **THEN** 系統必須使用回收語意、回報完成或限制，並從目前 snapshot 移除對應 identity

#### Scenario: 永久刪除取消
- **WHEN** 使用者啟動永久刪除但在確認或執行中取消
- **THEN** outcome 必須是 cancelled 或逐項 partial outcome，不得顯示成功，也不得建立不安全 undo entry

### Requirement: Progress、取消與 terminal semantics
每個 operation SHALL 發出可關聯的 queued、running、progress 與恰好一個 terminal outcome；取消 MUST 傳遞到可取消的 Shell operation，UI thread 不得因 progress callback 被阻塞。

#### Scenario: 取消大型 copy
- **WHEN** 使用者在大型真實測試 copy 顯示進度後取消
- **THEN** operation center 必須最終顯示 cancelled/partial outcome，列出已完成項目，且不再接受該 request 的 late progress

### Requirement: 衝突決策
系統 SHALL 將 name collision、destination changed、access denied 等 conflict 轉為 typed decision，支援適用時的 replace、skip、rename 或 cancel；沒有使用者決策時 MUST NOT 靜默覆寫。

#### Scenario: 同名目的地
- **WHEN** copy/move 目的地已有同名項目
- **THEN** UI 必須顯示可用決策及影響，並只依使用者選擇繼續，不得自行覆寫

### Requirement: 安全 operation journal 與 undo/redo
系統 SHALL 只為已完成且具有可重新驗證 inverse 的 operation 建立 journal entry；外部變更、identity 不符或 Windows API 不支援時 MUST 停用 undo/redo 並說明原因。

#### Scenario: 安全 rename undo
- **WHEN** rename 已完成且原 parent/name 仍可安全恢復
- **THEN** Undo 必須恢復原名稱並建立可用 Redo，兩者都產生完整 operation outcome

#### Scenario: 外部變更使 inverse 失效
- **WHEN** journal 建立後外部程序占用原名稱或移除目標 identity
- **THEN** Undo 必須在執行前失效並顯示原因，不得覆寫外部資料

### Requirement: Destructive integration test 安全性
所有 destructive integration tests SHALL 只操作測試建立、canonicalize 後仍位於唯一 fixture root 的項目；test harness MUST 拒絕 unresolved path、drive root、workspace root 與 user profile。

#### Scenario: Cleanup target 越界
- **WHEN** fixture cleanup 收到指向 fixture root 外的 path 或 reparse escape
- **THEN** cleanup 必須停止並回報安全錯誤，不得遞迴刪除該目標
