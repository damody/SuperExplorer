# Dotfile JSON 外部拖放完整實作計畫

## 1. 建立真實失敗基線與分層根因

### 1.1 指定來源與環境基線

**目的：** 固定不修改的真實來源與兩個遠端目的，建立可重現、可稽核的前置條件。
**輸入：** 核准設計、`D:\SuperExplorer\.tmp-full-meta.json`、ADB emulator、SFTP測試主機。
**產出：** `evidence/baseline.md`及來源metadata/hash、provider可用性記錄。
**依賴：** 無。
**Owner／Wave：** Primary／Wave 1。
**Gate／Evidence：** G1；`evidence/index.jsonl` task 1.1.*。
**完成門檻：** 來源存在且為34,629 bytes；兩個目的可列舉；證據不含credential。

- [x] 1.1.1 記錄指定來源的絕對路徑、Windows屬性、大小與SHA-256
- [x] 1.1.2 驗證ADB裝置`emulator-5554`與`/sdcard/Download`可存取
- [x] 1.1.3 以interactive credential驗證SFTP`/home/linuxuser`可存取且不持久化secret
- [x] 1.1.4 掃描基線證據，確認不含密碼、URI userinfo或來源內容

### 1.2 真實Windows拖放分層重現

**目的：** 找出指定dotfile在OLE decode、UI target、state validation或transfer dispatch的第一個失敗階段。
**輸入：** 1.1基線、既有headful runner與應用程式日誌。
**產出：** ADB/SFTP before reports、階段分類與根因判定。
**依賴：** 1.1。
**Owner／Wave：** Primary／Wave 2。
**Gate／Evidence：** G2；`build/dotfile-drag-*-before/report.json`與`evidence/root-cause.md`。
**完成門檻：** 兩條路徑均有可重現結果，且根因由事件與遠端oracle支持。

- [x] 1.2.1 讓runner直接選取既有`.tmp-full-meta.json`而非複製改名fixture
- [x] 1.2.2 對ADB目的執行一次真實Explorer左鍵Copy並保存事件與disk oracle
- [x] 1.2.3 對SFTP目的執行一次真實Explorer左鍵Copy並保存事件與remote oracle
- [x] 1.2.4 將最後成功階段與第一個拒絕階段分類為OLE、UI、state或dispatch
- [x] 1.2.5 以程式碼路徑對照實證並記錄最小共用根因

## 2. 修正共用流程與失敗可觀測性

### 2.1 無損來源與basename處理

**目的：** 修正第一個有實證的共用失敗點，讓合法dotfile無損進入provider-neutral傳輸。
**輸入：** 1.2根因、現有OLE/UI/state/remote external drop契約。
**產出：** 最小產品程式變更與程式內契約註解。
**依賴：** 1.2。
**Owner／Wave：** Primary／Wave 3。
**Gate／Evidence：** G3；產品diff及task 2.1.*聚焦測試。
**完成門檻：** 不含`.json`或provider特判；dotfile basename完整；一般來源行為不變。

- [x] 2.1.1 在實證失敗層加入或修正合法本機dotfile來源的共用判定
- [x] 2.1.2 使用平台路徑filename語意無損保留`.tmp-full-meta.json`basename
- [x] 2.1.3 確認ADB與SFTP沿相同prepared external drop契約，不新增provider分支
- [x] 2.1.4 保留空、非絕對、不存在與無filename來源的fail-closed行為
- [x] 2.1.5 保留Copy完成前後本機來源不被刪除的安全語意

### 2.2 分層診斷與安全錯誤

**目的：** 讓未產生傳輸或provider失敗時可判定階段與原因而不洩漏secret。
**輸入：** 1.2分類、既有operation message與logging契約。
**產出：** 必要的結構化診斷、使用者錯誤與credential掃描證據。
**依賴：** 1.2；可與2.1同Wave但shared file變更須串行整合。
**Owner／Wave：** Primary／Wave 3。
**Gate／Evidence：** G4；聚焦測試與`evidence/security-scan.txt`。
**完成門檻：** 可區分四個階段；訊息含sanitised目的與provider reason；無secret。

- [x] 2.2.1 為指定來源DragOver無terminal request補齊最後成功與拒絕階段診斷
- [x] 2.2.2 為ADB/SFTP dispatch失敗保留sanitised目的與provider reason
- [x] 2.2.3 確保診斷不輸出密碼、URI userinfo或完整敏感來源清單
- [x] 2.2.4 驗證既有泛用`Internal`不會覆蓋可取得的具體失敗原因

## 3. 建立自動化回歸與headful oracle

### 3.1 聚焦單元與整合回歸

**目的：** 固化dotfile合法邊界、basename無損及一般檔案隔離。
**輸入：** 2.1/2.2產品變更與現有external drop測試模組。
**產出：** 聚焦單元/整合測試及通過記錄。
**依賴：** 2.1、2.2。
**Owner／Wave：** Primary／Wave 4。
**Gate／Evidence：** G5；`evidence/focused-validation.md`。
**完成門檻：** 每個規格邊界有獨立斷言；受影響測試全部通過。

- [x] 3.1.1 新增合法`.tmp-full-meta.json`來源被接受的回歸測試
- [x] 3.1.2 新增前導點basename在目的URI或prepared item中保持完整的回歸測試
- [x] 3.1.3 新增不存在或無filename來源仍fail-closed的邊界測試
- [x] 3.1.4 重跑一般檔案與資料夾external drop既有測試以驗證隔離
- [x] 3.1.5 重跑Copy來源保留與provider-neutral routing既有測試

### 3.2 真實檔案headful runner與安全清理

**目的：** 讓runner可重跑指定使用者檔案並用basename/size/hash判定ADB與SFTP結果。
**輸入：** 1.2 runner基線、2.*修正、來源SHA-256。
**產出：** runner更新、after reports與精確遠端cleanup結果。
**依賴：** 2.1、2.2；與3.1可同Wave但最終執行在產品build後。
**Owner／Wave：** Primary／Wave 4。
**Gate／Evidence：** G6；`build/dotfile-drag-*-after/report.json`。
**完成門檻：** ADB/SFTP各1/1通過；名稱、大小、hash、來源保留均成立；遠端副本清理。

- [x] 3.2.1 擴充runner接受既有來源檔與預期basename/size/hash
- [x] 3.2.2 增加ADB遠端stat/read/hash oracle與精確fixture cleanup
- [x] 3.2.3 增加SFTP遠端stat/read/hash oracle與interactive精確fixture cleanup
- [x] 3.2.4 確保runner拒絕覆寫或清理無法證明屬於本次測試的同名資料
- [x] 3.2.5 確保report不含credential、URI userinfo或來源內容

## 4. 實機驗收與最終集中檢查

### 4.1 ADB與SFTP blocking matrix

**目的：** 以使用者指定檔案和目的路徑證明產品行為已恢復。
**輸入：** 3.2 runner、修正後build、可用ADB/SFTP環境。
**產出：** 兩份after report、日誌節錄與清理證據。
**依賴：** 3.1、3.2。
**Owner／Wave：** Primary／Wave 5；credentialed SFTP操作只由Primary執行。
**Gate／Evidence：** G7 blocking；`evidence/headful-acceptance.md`與index task 4.1.*。
**完成門檻：** 所有leaf通過；任一失敗須回到對應根因或實作package補完後重跑。

- [x] 4.1.1 建置修正後SuperExplorer並確認啟動使用本次binary
- [x] 4.1.2 從Explorer將指定JSON拖入`adb://emulator-5554/sdcard/Download`
- [x] 4.1.3 驗證ADB目的basename、34,629 bytes與SHA-256一致
- [x] 4.1.4 從Explorer將指定JSON拖入`sftp://45.32.49.125/home/linuxuser`
- [x] 4.1.5 驗證SFTP目的basename、34,629 bytes與SHA-256一致
- [x] 4.1.6 驗證兩次Copy後本機來源存在、大小與SHA-256未變
- [x] 4.1.7 驗證日誌含`DropExternal`與terminal provider結果且無泛用無原因失敗
- [x] 4.1.8 驗證受控ownership後清理ADB與SFTP精確遠端副本

### 4.2 最終品質、規格與證據一致性

**目的：** 集中執行使用者要求的最後檢查，失敗時持續修正並重跑到通過。
**輸入：** 所有產品、測試、runner與headful結果。
**產出：** 最終驗證記錄、44筆唯一evidence index、完成tasks與strict-valid OpenSpec。
**依賴：** 4.1。
**Owner／Wave：** Primary／Wave 6。
**Gate／Evidence：** G8 blocking；`evidence/focused-validation.md`、`evidence/index.jsonl`。
**完成門檻：** 所有命令exit 0、44個leaf皆有唯一證據、無P0/P1缺漏。

- [x] 4.2.1 執行`cargo fmt --all`並確認格式完成
- [x] 4.2.2 執行受影響crate的external drag聚焦測試
- [x] 4.2.3 執行`cargo check -p explorer-app`驗證整合編譯
- [x] 4.2.4 執行`git diff --check`與敏感資訊掃描
- [x] 4.2.5 建立每個leaf唯一`task_id`的evidence index並驗證筆數
- [x] 4.2.6 執行詳細任務結構validator與`openspec validate --strict`
- [x] 4.2.7 人工核對proposal→design→scenario→task→evidence追溯與leaf atomicity
- [x] 4.2.8 將全部通過的leaf標記完成；任何失敗項回開並補完後重新驗證
