## ADDED Requirements

### Requirement: 主程式使用一致的 SuperExplorer 產品識別
系統 SHALL 將 Cargo-built 與 packaged Windows 主程式輸出為 `SuperExplorer.exe`，且 Windows `ProductName`、`FileDescription`、`InternalName` 與 `OriginalFilename` MUST 使用 `SuperExplorer` 對應值；內部 composition-root package MUST 保留 `explorer-app` 名稱。

#### Scenario: 建置開發版主程式
- **WHEN** 開發者執行 `cargo build -p explorer-app`
- **THEN** 系統在 target profile 目錄產生可執行的 `SuperExplorer.exe`，且不要求 `explorer-app.exe` 才能啟動或驗證 UI

#### Scenario: 檢查 Windows 檔案資訊
- **WHEN** 驗證工具讀取完成建置的 `SuperExplorer.exe` VERSIONINFO
- **THEN** ProductName 與 FileDescription 顯示 `SuperExplorer`，InternalName 為 `SuperExplorer`，OriginalFilename 為 `SuperExplorer.exe`

#### Scenario: 以 Cargo package 執行測試
- **WHEN** 開發者執行 `cargo test -p explorer-app`
- **THEN** 既有 package、integration tests 與 composition-root dependency graph 維持可用

### Requirement: 視窗與工作列標題反映作用中瀏覽位置
系統 SHALL 將 native window title 設為作用中分頁目前成功導覽的位置；檔案系統位置 MUST 使用完整 Windows 路徑，且標題 MUST 隨成功導覽、作用中分頁切換、關閉作用中分頁及 session restore 更新。

#### Scenario: 啟動於檔案系統資料夾
- **WHEN** SuperExplorer 的初始作用中分頁位於 `D:\test`
- **THEN** native window 與 Windows 工作列所見標題為 `D:\test`

#### Scenario: 在磁碟間成功導覽
- **WHEN** 作用中分頁從 C: 的測試資料夾成功導覽至 D: 的測試資料夾
- **THEN** 標題只顯示 D: 目的地的完整路徑，且不保留先前 C: 路徑或附加產品後綴

#### Scenario: 切換作用中分頁
- **WHEN** 使用者從瀏覽 `C:\fixture-a` 的分頁切換到瀏覽 `D:\fixture-b` 的分頁
- **THEN** 標題同步改為 `D:\fixture-b`

#### Scenario: 背景分頁完成載入
- **WHEN** 非作用中分頁收到 directory terminal event 或 display metadata 更新
- **THEN** 系統維持作用中分頁的標題，不以背景分頁位置覆寫

#### Scenario: 導覽失敗或正在編輯網址
- **WHEN** 使用者編輯網址但尚未成功導覽，或目的地導覽失敗
- **THEN** 標題維持最後成功作用中位置，不顯示未確認的輸入文字

### Requirement: 虛擬位置具有可讀且安全的標題回退
作用中位置沒有檔案系統路徑時，系統 SHALL 使用該位置非空的 Shell／navigation 顯示名稱；若沒有可讀名稱，系統 MUST 回退為 `SuperExplorer`，MUST NOT 顯示空白、內部 parsing token 或 stale background-tab title。

#### Scenario: 瀏覽本機虛擬位置
- **WHEN** 作用中分頁瀏覽具有顯示名稱「本機」但沒有一般檔案系統路徑的 Shell 位置
- **THEN** native window 標題顯示「本機」

#### Scenario: 虛擬位置缺少顯示名稱
- **WHEN** 作用中虛擬位置的 display title 為空白或尚未解析
- **THEN** native window 標題顯示 `SuperExplorer`

### Requirement: 封裝與驗證工具只依賴新的主程式產物
installer、artifact finalization、production headful smoke、UITEST prerequisites 與交付文件 SHALL 使用 `SuperExplorer.exe` 作為主程式檔名；Cargo package selector與內部 crate 路徑 SHALL 繼續使用 `explorer-app`。

#### Scenario: 建立 NSIS 安裝程式
- **WHEN** release artifact 經 finalize 與 NSIS packaging
- **THEN** installer 輸入、安裝檔、捷徑、DisplayIcon、啟動與解除安裝 ownership 均指向 `SuperExplorer.exe`

#### Scenario: 執行 headful 與 UITEST
- **WHEN** 測試 runner 檢查 prerequisite 並啟動 production UI
- **THEN** runner 尋找並啟動 profile 目錄中的 `SuperExplorer.exe`，且失敗訊息回報相同的新路徑

### Requirement: 改名不得遺失既有使用者資料或降低自我程序保護
系統 MUST 保留既有 `%LOCALAPPDATA%\RustGpuiExplorer` 資料根、session schema、cache layout 與 Shell parsing identities，且涉及 locked-file recovery 的自我程序辨識 MUST 同時拒絕關閉新 `SuperExplorer.exe` 與舊 `explorer-app.exe` 程序。

#### Scenario: 使用既有設定啟動改名後版本
- **WHEN** 使用者已有 RustGpuiExplorer 根目錄下的 session、搜尋索引或圖示快取
- **THEN** `SuperExplorer.exe` 沿用既有資料，不因產品改名建立空白替代根或遺失狀態

#### Scenario: Restart Manager 回報新舊主程式 owner
- **WHEN** locked-file owner 名稱為 `SuperExplorer.exe` 或仍在執行的舊 `explorer-app.exe`
- **THEN** recovery policy 將該 owner 視為應用程式自身並拒絕要求關閉

