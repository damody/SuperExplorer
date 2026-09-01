## ADDED Requirements

### Requirement: Windows 檔案可拖入 SFTP
系統 SHALL 接受 Windows Explorer 提供的有效 `CF_HDROP`，並把實體檔案與資料夾上傳至目前可寫的 SFTP drop target。

#### Scenario: 預設拖入檔案
- **WHEN** 使用者未按修飾鍵，從 Explorer 把檔案拖到 SFTP 目錄
- **THEN** 系統以 Copy 上傳檔案並保留本機來源

#### Scenario: 拖入資料夾
- **WHEN** 外部 `CF_HDROP` 包含資料夾
- **THEN** 系統在 SFTP 建立一次根資料夾並遞迴上傳其內容

#### Scenario: Shift 拖入
- **WHEN** 使用者按住 Shift 把本機項目拖到 SFTP
- **THEN** 系統只有在對應項目上傳成功後才移除該本機來源

### Requirement: SFTP 項目可拖到 Windows 檔案總管
系統 SHALL 將 SFTP 選取項目下載到 request-scoped staging，建立標準 Shell `IDataObject`，並以 OLE `DoDragDrop` 提供給 Explorer。

#### Scenario: 預設拖出檔案
- **WHEN** 使用者未按修飾鍵，把 SFTP 檔案拖到 Explorer 目錄
- **THEN** Explorer 建立本機副本，SFTP 來源保留

#### Scenario: 拖出資料夾
- **WHEN** 使用者把 SFTP 資料夾拖到 Explorer
- **THEN** Explorer 收到一個根資料夾及完整遞迴內容，沒有額外 staging 層

#### Scenario: Shift 拖出成功
- **WHEN** 使用者按住 Shift，且 OLE 實際回報成功 Move
- **THEN** 系統在本機目的建立完成後刪除對應 SFTP 來源

### Requirement: 拖放 effect 安全協商
系統 SHALL 對跨檔案系統拖放預設 Copy，Ctrl 為 Copy，Shift 才提出 Move，且 SHALL NOT 把 Link 當作 Move。

#### Scenario: Target 回報 None 或取消
- **WHEN** OLE target 回報 `DROPEFFECT_NONE` 或使用者取消拖曳
- **THEN** 系統回報 Cancelled、保留來源並清理 staging

#### Scenario: 部分失敗
- **WHEN** 多項目 upload、download 或來源刪除只有部分成功
- **THEN** 系統回報逐項結果，只對已符合成功門檻的項目套用 Move 刪除

### Requirement: Staging 生命週期受 request 管理
系統 SHALL 為每個遠端拖曳建立唯一 staging root，並保持至 OLE terminal 完成後才清理。

#### Scenario: 同時或連續拖曳
- **WHEN** 多個 drag request 同時或快速連續執行
- **THEN** 每個 request 的 staging ownership 互不覆蓋，且依各自 terminal 清理

#### Scenario: 下載失敗
- **WHEN** staging 下載在 OLE 開始前失敗
- **THEN** 系統不啟動 `DoDragDrop`、保留遠端來源並清理該 request staging

### Requirement: 拖放失敗可診斷且不洩漏認證
系統 SHALL 記錄 drag request、stage、來源、目的地、effect 與底層 provider/OLE 原因，並 SHALL NOT 記錄 SFTP 密碼、私鑰或 credential token。

#### Scenario: Drop 路由未命中
- **WHEN** UI 收到 DragEnter 或 Drop 但無法建立 `DropExternal` 命令
- **THEN** 系統留下可識別 target、metadata 與拒絕原因的診斷事件
