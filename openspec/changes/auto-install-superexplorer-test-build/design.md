## Context

目前SuperExplorer測試入口將`--component superexplorer --allow-superexplorer-dirty`轉交Lua orchestrator。orchestrator驗證release輸入、封裝NSIS並用detached `start`開啟安裝器，因此父流程不知道安裝是否完成或是否成功。使用者實際遇到安裝目錄仍是舊版，導致工作樹已修正的OLE拖放未生效。

此變更只針對SuperExplorer測試入口；正式combined installer與SuperDesktop-only測試建置維持既有行為。NSIS需要管理執行中應用與MFT service，不能以直接複製binary繞過。

## Goals / Non-Goals

**Goals:**

- 預設測試建置成功代表本次release已由NSIS安裝且必要binary身分一致。
- 同步取得silent installer、hash verification及已安裝版啟動的terminal結果。
- 保留`--check`、`--no-launch`、`--skip-build`既有邊界。
- 以真實Explorer→ADB／SFTP dotfile拖放驗證安裝結果。

**Non-Goals:**

- 不改正式或SuperDesktop-only installer的預設啟動語意。
- 不直接複製檔案到Program Files或LocalAppData安裝目錄。
- 不改NSIS產品內容、Rust公開API、登入資料或傳輸架構。
- 不執行完整專案迴歸。

## Decisions

### 1. SuperExplorer測試模式採同步silent install

Lua orchestrator在成功發布測試installer後，以共用同步process helper執行`installer.exe /S`並等待退出。這讓installer exit code成為父批次檔的blocking gate。拒絕沿用detached GUI，因為它只能證明程序啟動；也拒絕直接覆寫binary，因為會繞過既有程序、服務、外掛與registry協調。

### 2. 自動安裝只由test-superexplorer entry顯式啟用

新增內部選項或mode contract，由`build_test_install.bat`顯式要求「install and verify」。共用`build_install.lua`不得僅依`component == superexplorer`推測，以免正式combined或其他呼叫者意外產生外部變更。`--no-launch`優先，關閉安裝與啟動；`--check`不發布；`--skip-build`只略過編譯。

### 3. 安裝後以三個必要binary SHA-256做身分gate

驗證`SuperExplorer.exe`、`explorer-extension-broker.exe`、`explorer-extension-worker.exe`。release與installed兩側皆須存在，並以Windows可用的可信hash工具或既有Lua helper計算SHA-256。任一缺失或不符即失敗，訊息只含stage、檔名及hash，不含credential。這比版本字串或時間戳可直接證明執行內容一致。

### 4. 只啟動驗證後的已安裝主程式

hash gate通過後，以安裝目錄的`SuperExplorer.exe`啟動。啟動API失敗須回傳非零。父流程不以release工作樹binary取代安裝版，避免驗收錯誤目標。

### 5. 安裝位置沿用installer定義並由單一helper解析

test installer可能依既有NSIS定義使用per-user位置；驗證helper不得硬編碼另一個目錄。安裝目錄來源須與installer輸出定義一致，並在測試中覆蓋含空白路徑。若未來installer位置更動，只更新單一契約。

### 6. 實證調整分級

- **A—任務精修：** 可調整helper位置、命令拆分或runner等待，但不得改選項、gate或安裝範圍。
- **B—設計／規格修正：** 若NSIS silent行為或安裝位置假設錯誤，在既定範圍內同步更新design/spec/tasks並重驗。
- **C—實質變更：** 若需改正式installer、降低hash gate、直接覆寫安裝檔或擴大外部寫入，須先取得使用者核准。

任何B/C調整推翻已完成任務時須reopen並保留舊證據沿革，不得靜默降低blocking gate。

## Risks / Trade-offs

- **[需要系統權限]** → 沿用`RequestExecutionLevel admin`；UAC取消或安裝器失敗即明確失敗，不顯示成功。
- **[執行中程序阻擋更新]** → 由既有NSIS安全關閉協調處理；不得強制直接覆寫。
- **[silent finish page不會啟動應用]** → orchestrator在hash gate後明確啟動安裝版。
- **[安裝位置漂移]** → 由共用resolver與fixture測試綁定installer定義。
- **[headful UIA競態]** → 以遠端ADB／SFTP內容oracle作產品結果，UIA只負責真實輸入驅動。
- **[SFTP credential洩漏]** → credential只經interactive stdin，不寫入命令列、log或evidence。

## Migration Plan

先加入可測試的同步安裝／hash helper，再將SuperExplorer測試入口切換為顯式auto-install mode。以`--check`及`--no-launch`證明無外部安裝，以預設入口完成NSIS安裝、hash gate和安裝版啟動。失敗可回復本次入口與orchestrator變更，恢復互動式installer launch；已安裝產品仍由既有NSIS管理。

## Open Questions

無。安裝位置與silent參數由既有NSIS實證決定，屬A級實作定位；不得據此放寬核准gate。
