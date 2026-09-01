## ADDED Requirements

### Requirement: External file-view drop ownership
系統 SHALL 讓可寫檔案檢視背景接收 Windows Explorer 的有效 external file drop，且 MUST 讓一般檔案列對該 drop 保持透明；只有可寫資料夾列 SHALL 擁有 child-folder external drop。

#### Scenario: Drop on populated remote background
- **WHEN** ADB 或 SFTP Details viewport 由一般檔案列填滿，而使用者把有效本機項目放到檔案檢視背景
- **THEN** terminal Drop MUST 抵達背景 target，且系統 MUST 以目前遠端目錄建立一次 transfer request

#### Scenario: Drop on writable folder row
- **WHEN** 使用者把有效本機項目放到可寫 ADB 或 SFTP 資料夾列
- **THEN** 資料夾列 MUST 擁有 terminal Drop、阻止背景重複處理，並以該子資料夾建立一次 transfer request

#### Scenario: Drop on ordinary file row
- **WHEN** pointer 位於一般檔案列而該位置下方的檔案檢視背景是有效目的地
- **THEN** 一般檔案列 MUST NOT 吞掉 external Drop，背景 target MUST 仍能解析目前目錄目的地

### Requirement: Standard Windows file payload and effect
系統 SHALL 接受 Windows Explorer 以標準 OLE `CF_HDROP` 提供的非空本機絕對路徑，並 MUST 保留已協商的 Copy 或 Move effect；Link、None、空白或無效來源 MUST fail closed。

#### Scenario: Copy files and folders to remote destination
- **WHEN** 使用者以 Copy effect 拖入一個或多個本機檔案或資料夾至可寫 ADB 或 SFTP 目的地
- **THEN** 系統 MUST 上傳全部可成功項目、保留本機來源並回報逐項 terminal 結果

#### Scenario: Move files and folders to remote destination
- **WHEN** 使用者以 Move effect 拖入本機檔案或資料夾至可寫 ADB 或 SFTP 目的地
- **THEN** 系統 MUST 僅在對應目的端項目成功完成後移除該本機來源，失敗或未完成項目的來源 MUST 保留

#### Scenario: Unsupported external payload
- **WHEN** external drop 的路徑集合為空、含非本機或非絕對來源，或 effect 為 None 或 Link
- **THEN** 系統 MUST NOT 建立 transfer request、MUST NOT 刪除任何來源，並 MUST 清除 transient drop cue

### Requirement: Generation-safe destination resolution
系統 MUST 以 drop 發生時的 tab、generation、stable item identity 與 provider capability解析目的地，不得從後續 selection 或已變更的 presentation row 猜測目的地。

#### Scenario: Navigation changes during drag
- **WHEN** drag 開始後 tab 導覽、切換或 generation 改變，使原目的地變成 stale
- **THEN** terminal Drop MUST 被拒絕、不得傳輸至新目錄，且所有 drag transient state MUST 恰好清除一次

#### Scenario: Folder row is no longer writable
- **WHEN** folder row 在 terminal Drop 前不再能解析成同一個可寫目的地
- **THEN** 系統 MUST fail closed，且不得降級成目前背景目錄 drop

### Requirement: Observable and credential-safe rejection
系統 SHALL 區分 UI target 未接收、state validation 拒絕與 transfer dispatch 失敗；相關診斷 MUST 包含 target/provider/effect/source-count/generation 類別資訊，但 MUST NOT 包含密碼、URI userinfo或完整敏感來源清單。

#### Scenario: Drag updates without terminal command
- **WHEN** 系統收到 external DragOver，但 terminal Drop 沒有建立 transfer command
- **THEN** 診斷 MUST 指出最後 target kind、provider kind與可判定的拒絕階段，且不得輸出 credential

#### Scenario: Transfer dispatch fails after accepted drop
- **WHEN** Drop 已成功排隊但 provider upload 或 transfer routing 失敗
- **THEN** 使用者訊息 MUST 顯示操作、sanitised 目的地、失敗階段與 provider reason，並保持來源安全語意

### Requirement: Interoperability regression isolation
修正 MUST 維持 Windows Explorer→SuperExplorer Local external drop、文字／圖片 clipboard隔離及 SuperExplorer拖出行為，且 MUST 通過真實 Windows Explorer→Local／ADB／SFTP blocking matrix。

#### Scenario: Local destination regression
- **WHEN** Windows Explorer 以 Copy effect 將本機檔案拖入 SuperExplorer Local 目錄
- **THEN** 目的檔 MUST 建立、來源 MUST 保留，且遠端 drop ownership 修正不得改變此結果

#### Scenario: Clipboard formats remain isolated
- **WHEN** clipboard 只包含文字或圖片而不含有效 `CF_HDROP`
- **THEN** 系統 MUST NOT 將其解讀成 external file drop或清除其原始內容

#### Scenario: Controlled remote headful matrix
- **WHEN** 受控 Windows headful runner 對指定 ADB 與 SFTP 目錄執行 Copy 與 Move
- **THEN** 每個目的項目存在性及來源保留／移除 MUST 符合 effect，報告 MUST 不含 credential，且 marker-owned fixture MUST 在驗證後清理
