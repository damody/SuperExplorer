## ADDED Requirements

### Requirement: Collision-safe Explorer-style default folder name
系統 SHALL 依目前目錄以不區分大小寫方式選用 `New folder` 或第一個可用的 `New folder (N)`。

#### Scenario: Existing numbered names
- **WHEN** `New folder` 與 `New folder (2)` 已存在
- **THEN** 暫存列使用 `New folder (3)`

### Requirement: New folder remains provisional while naming
系統 SHALL 在名稱確認前只顯示 UI 暫存資料夾，不向 Local、ADB 或 SFTP provider 提交建立操作。

#### Scenario: Editor opens
- **WHEN** 使用者執行新增資料夾
- **THEN** 暫存列立即出現、完整名稱被選取，且實體路徑尚不存在

#### Scenario: User cancels
- **WHEN** 使用者按 Esc
- **THEN** 暫存列消失且不建立實體資料夾

### Requirement: Rename confirmation creates the folder
系統 SHALL 在 Enter 或失焦確認有效名稱後，使用最終文字提交一次 Folder 建立操作。

#### Scenario: Default name is accepted
- **WHEN** 使用者不修改 `New folder` 並按 Enter
- **THEN** 系統建立名為 `New folder` 的資料夾

#### Scenario: Custom name is accepted
- **WHEN** 使用者輸入有效新名稱並確認
- **THEN** Local、ADB 或 SFTP 在目前父目錄建立該名稱

#### Scenario: Invalid or colliding name
- **WHEN** 名稱無效或目前 snapshot 已有同名項目
- **THEN** editor 保留並顯示錯誤，且不提交建立操作

### Requirement: Draft is context-safe and isolated
系統 MUST 在導航或分頁 context 不相符時清除草稿，且 SHALL 不改變既有 F2、滑鼠 rename 或批次建立語意。

#### Scenario: User leaves the directory
- **WHEN** 草稿仍在編輯而 active location 已改變
- **THEN** 新位置不顯示該草稿，也不建立資料夾

#### Scenario: Extension batch creation
- **WHEN** 擴充套件執行批次建立
- **THEN** 操作直接依既有流程執行，不開啟暫存 rename editor
