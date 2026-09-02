## ADDED Requirements

### Requirement: Legal local dotfile source acceptance
系統 SHALL 接受Windows Explorer透過標準OLE `CF_HDROP`提供的非空、絕對、存在本機dotfile路徑，且MUST NOT僅因basename以`.`開頭或副檔名為`.json`而拒絕來源。

#### Scenario: Existing dotfile JSON is dropped
- **WHEN** `D:\SuperExplorer\.tmp-full-meta.json`存在且Windows Explorer以Copy effect拖入可寫SuperExplorer目的地
- **THEN** 系統MUST建立一次包含完整來源路徑的external drop transfer request

#### Scenario: Invalid source remains rejected
- **WHEN** OLE來源為空、非絕對、不存在、虛擬Shell項目或無法取得filename
- **THEN** 系統MUST fail closed、不得建立transfer request，且MUST記錄不含credential的拒絕階段與原因

### Requirement: Lossless dotfile basename preservation
系統 SHALL 在OLE decode、UI action、state validation與provider dispatch之間無損保留來源basename，ADB與SFTP目的項目MUST命名為`.tmp-full-meta.json`，不得截斷前導點、改用空名稱或只保留副檔名。

#### Scenario: Dotfile is uploaded to ADB
- **WHEN**指定來源以Copy effect放到`adb://emulator-5554/sdcard/Download`
- **THEN**目的端MUST建立`/sdcard/Download/.tmp-full-meta.json`且本機來源MUST保留

#### Scenario: Dotfile is uploaded to SFTP
- **WHEN**指定來源以Copy effect放到`sftp://45.32.49.125/home/linuxuser`
- **THEN**目的端MUST建立`/home/linuxuser/.tmp-full-meta.json`且本機來源MUST保留

### Requirement: Provider-neutral content integrity
合法dotfile外部拖放 SHALL 使用既有provider-neutral transfer準備契約；修正MUST NOT依副檔名或特定provider建立產品分支，且成功結果MUST保持來源內容完整。

#### Scenario: ADB content integrity
- **WHEN**ADB terminal結果回報成功
- **THEN**遠端檔案大小MUST為34,629 bytes、SHA-256 MUST等於本機來源，且來源檔案的大小與SHA-256 MUST保持不變

#### Scenario: SFTP content integrity
- **WHEN**SFTP terminal結果回報成功
- **THEN**遠端檔案大小MUST為34,629 bytes、SHA-256 MUST等於本機來源，且來源檔案的大小與SHA-256 MUST保持不變

#### Scenario: Ordinary file regression isolation
- **WHEN**一般非dotfile本機檔案沿相同流程拖入Local、ADB或SFTP
- **THEN**其既有Copy行為與basename MUST維持不變

### Requirement: Observable staged failure
系統 SHALL 將合法外部來源未完成傳輸的原因分類為OLE decode、UI target、state validation或transfer dispatch，使用者與診斷訊息MUST包含sanitised來源名稱、目的provider與可行動原因，且MUST NOT包含密碼或URI userinfo。

#### Scenario: DragOver occurs without terminal request
- **WHEN**系統收到指定來源的external DragOver但沒有建立terminal transfer request
- **THEN**診斷MUST指出最後成功階段與第一個拒絕階段，且不得將錯誤折疊成沒有原因的`Internal`

#### Scenario: Provider upload fails
- **WHEN**DropExternal已排隊但ADB或SFTP上傳失敗
- **THEN**使用者訊息MUST顯示sanitised目的路徑、操作與provider reason，本機來源MUST保留

### Requirement: Controlled real-file regression evidence
修正 SHALL 通過以指定真實檔案執行的Windows headful ADB與SFTP blocking matrix，並MUST以精確路徑安全清理遠端測試副本。

#### Scenario: ADB headful copy oracle
- **WHEN**runner從Windows Explorer將指定來源拖入ADB指定目錄
- **THEN**報告MUST證明`DropExternal`發生、遠端basename/size/hash正確、來源保留，且遠端受控副本在驗證後清理

#### Scenario: SFTP headful copy oracle
- **WHEN**runner從Windows Explorer將指定來源拖入SFTP指定目錄
- **THEN**報告MUST證明`DropExternal`發生、遠端basename/size/hash正確、來源保留，且遠端受控副本在驗證後清理

#### Scenario: Credential-safe evidence
- **WHEN**保存headful報告、日誌與evidence index
- **THEN**所有持久證據MUST不含SFTP密碼、URI userinfo或其他登入secret
