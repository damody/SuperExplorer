## ADDED Requirements

### Requirement: Default test build installs synchronously
`build_test_install.bat`不帶停用參數執行時，系統 SHALL 建置並發布本次SuperExplorer測試安裝器，以NSIS silent模式同步安裝，且 SHALL 等待terminal exit status後才繼續。

#### Scenario: Successful default build and install
- **WHEN** 使用者不帶參數執行`build_test_install.bat`且建置與安裝成功
- **THEN** 批次檔等待安裝器退出，並進入安裝後身分驗證

#### Scenario: Installer exits unsuccessfully
- **WHEN** silent installer回傳非零退出碼或無法啟動
- **THEN** 整體流程回傳非零退出碼，指出安裝階段，且不得顯示成功或啟動應用

### Requirement: Installed binaries match the current release
自動安裝完成後，系統 MUST 比對`SuperExplorer.exe`、`explorer-extension-broker.exe`及`explorer-extension-worker.exe`在本次release輸入與實際安裝位置的SHA-256；三者皆存在且相符才可通過。

#### Scenario: All required hashes match
- **WHEN** silent installer成功且三個installed binary的SHA-256分別等於release輸入
- **THEN** 系統將安裝身分gate標記通過並允許啟動已安裝主程式

#### Scenario: Installed binary is missing
- **WHEN** 任一必要installed binary不存在
- **THEN** 系統回傳非零退出碼並指出缺失檔名，不得顯示完成

#### Scenario: Installed binary hash is stale
- **WHEN** 任一必要installed binary的SHA-256不等於本次release輸入
- **THEN** 系統回傳非零退出碼並指出不相符檔名，不得啟動未驗證版本

### Requirement: Installed application launches only after verification
系統 SHALL 僅在安裝器與三個binary hash gate通過後啟動安裝目錄中的`SuperExplorer.exe`，並 SHALL 將啟動API失敗視為整體失敗。

#### Scenario: Verified installed application starts
- **WHEN** 安裝及hash gate皆通過
- **THEN** 系統啟動installed `SuperExplorer.exe`並在啟動要求成功後回報整體成功

#### Scenario: Installed application cannot start
- **WHEN** 已驗證installed executable的啟動要求失敗
- **THEN** 系統回傳非零退出碼並指出啟動階段

### Requirement: Existing option boundaries remain stable
`--no-launch` SHALL 只建置及發布而不安裝、不驗證installed files及不啟動；`--check` SHALL 不編譯、不發布、不安裝及不啟動；`--skip-build` SHALL 只略過編譯並仍執行其餘預設安裝gate。

#### Scenario: No-launch build has no installation side effect
- **WHEN** 使用者執行`build_test_install.bat --no-launch`
- **THEN** 系統發布installer後成功結束，且不啟動installer或installed application

#### Scenario: Check mode is read-only
- **WHEN** 使用者執行`build_test_install.bat --check`
- **THEN** 系統只驗證工具、layout及必要輸入，不建立installer、不安裝且不啟動

#### Scenario: Skip-build still installs verified inputs
- **WHEN** 使用者執行`build_test_install.bat --skip-build`且既有release輸入有效
- **THEN** 系統封裝、同步安裝、比對hash並啟動已驗證installed application

### Requirement: Other installer entry points remain isolated
自動安裝行為 MUST 由SuperExplorer測試入口顯式啟用；正式combined installer及SuperDesktop-only測試入口 SHALL 保留既有發布／啟動語意。

#### Scenario: Formal build is not silently installed
- **WHEN** 呼叫正式combined installer入口且未顯式要求test auto-install mode
- **THEN** 共用orchestrator不得自動silent install該installer

#### Scenario: SuperDesktop test build is not changed
- **WHEN** 呼叫SuperDesktop-only測試入口
- **THEN** 其安裝器行為與本變更前一致

### Requirement: Installed drag acceptance proves deployed behavior
預設流程安裝的SuperExplorer MUST 接受Windows Explorer提供的合法本機`.tmp-full-meta.json`Copy至指定ADB與SFTP目的，且遠端basename、34,629 bytes與SHA-256 SHALL 與來源一致，來源 SHALL 保留。

#### Scenario: Installed application copies dotfile to ADB
- **WHEN** 從Windows Explorer將指定來源拖入`adb://emulator-5554/sdcard/Download`
- **THEN** ADB遠端oracle確認精確basename、大小與hash一致，且本機來源未變

#### Scenario: Installed application copies dotfile to SFTP
- **WHEN** 從Windows Explorer將指定來源拖入`sftp://45.32.49.125/home/linuxuser`
- **THEN** SFTP遠端oracle確認精確basename、大小與hash一致，且本機來源未變

#### Scenario: Remote cleanup is ownership constrained
- **WHEN** 驗收需要清理ADB或SFTP遠端副本
- **THEN** 系統只在精確名稱及內容證明屬於本次fixture後刪除，否則fail closed
