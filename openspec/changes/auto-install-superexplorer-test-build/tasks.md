# SuperExplorer測試建置自動安裝完整實作計畫

## 1. 固定現況與契約邊界

### 1.1 建立舊版部署失敗基線

**目的：** 證明現行預設流程只啟動installer而未等待安裝，並固定release／installed身分差異的可重現證據。
**輸入：** 核准設計、現行batch／Lua／NSIS流程、release與installed binaries。
**產出：** `evidence/baseline.md`、流程與hash基線。
**依賴：** 無。
**Owner／Wave：** Primary／Wave 1。
**Gate／Evidence：** G1；`evidence/index.jsonl` task 1.1.*。
**完成門檻：** 可指出detached launch位置、安裝位置及三個binary的before hash；證據不含credential。

- [x] 1.1.1 記錄`build_test_install.bat`目前的參數轉送、成功訊息與退出碼行為
- [x] 1.1.2 記錄`build_install.lua`發布後detached installer launch及不等待terminal status的程式路徑
- [x] 1.1.3 解析SuperExplorer test NSIS實際安裝位置與silent參數
- [x] 1.1.4 記錄release與installed三個必要binary的存在、大小及SHA-256基線

### 1.2 凍結入口隔離與選項矩陣

**目的：** 防止共用orchestrator變更外溢到正式或SuperDesktop入口，並明確排序選項優先權。
**輸入：** proposal、design、既有component options與entry points。
**產出：** 選項矩陣、隔離測試案例及實作契約。
**依賴：** 1.1。
**Owner／Wave：** Primary／Wave 1。
**Gate／Evidence：** G2；`evidence/option-matrix.md`。
**完成門檻：** default、check、no-launch、skip-build及其他entry point皆有唯一預期副作用與terminal state。

- [x] 1.2.1 定義SuperExplorer測試入口顯式auto-install內部選項，不以component隱式推測
- [x] 1.2.2 定義`--check`不發布不安裝不啟動的優先語意
- [x] 1.2.3 定義`--no-launch`發布後不安裝不啟動的優先語意
- [x] 1.2.4 定義`--skip-build`只略過編譯且保留安裝、hash與啟動gate
- [x] 1.2.5 定義正式combined與SuperDesktop-only入口維持既有行為的隔離案例

## 2. 實作同步安裝與身分gate

### 2.1 建立可測試的同步process與安裝位置契約

**目的：** 提供等待installer退出並精確定位installed binaries的共用基礎，而不依賴detached launch。
**輸入：** 1.2矩陣、`build/lib/process.lua`、NSIS install definitions。
**產出：** Lua同步執行／位置resolver變更及聚焦測試。
**依賴：** 1.2。
**Owner／Wave：** Primary／Wave 2。
**Gate／Evidence：** G3；產品diff與`evidence/focused-tests.md` task 2.1.*。
**完成門檻：** exit 0／nonzero可區分；含空白路徑安全；resolver與NSIS test位置一致。

- [x] 2.1.1 擴充同步process helper支援參數、cwd、log與terminal exit status
- [x] 2.1.2 為同步helper新增成功、nonzero與含空白參數的聚焦測試
- [x] 2.1.3 建立SuperExplorer test installer安裝位置resolver並避免重複硬編碼
- [x] 2.1.4 為per-user安裝位置與三個必要binary路徑新增聚焦測試

### 2.2 實作silent install與SHA-256 blocking gate

**目的：** 讓預設SuperExplorer測試流程只有在NSIS安裝完成且installed身分等於release時才能成功。
**輸入：** 2.1 helper、已發布installer、release輸入清單。
**產出：** orchestrator auto-install分支、hash verifier與具體錯誤。
**依賴：** 2.1。
**Owner／Wave：** Primary／Wave 3。
**Gate／Evidence：** G4 blocking；task 2.2.*測試與安裝log。
**完成門檻：** 三檔一致才通過；installer failure、missing及mismatch皆nonzero且不顯示成功。

- [x] 2.2.1 解析並轉送SuperExplorer測試入口的顯式auto-install內部選項
- [x] 2.2.2 在發布後以`/S`同步執行本次產生的NSIS installer並保存log
- [x] 2.2.3 計算release與installed `SuperExplorer.exe` SHA-256並阻擋缺失或不符
- [x] 2.2.4 計算release與installed broker SHA-256並阻擋缺失或不符
- [x] 2.2.5 計算release與installed worker SHA-256並阻擋缺失或不符
- [x] 2.2.6 對installer nonzero、missing與hash mismatch輸出stage及檔名並回傳非零
- [x] 2.2.7 驗證錯誤與log不包含credential、URI userinfo或未清理命令內容

### 2.3 驗證後啟動與batch真實訊息

**目的：** 只啟動通過身分gate的installed主程式，讓batch訊息與實際terminal outcome一致。
**輸入：** 2.2 gate、installed executable、現行batch訊息。
**產出：** 安裝版啟動流程、更新batch訊息與失敗傳播。
**依賴：** 2.2。
**Owner／Wave：** Primary／Wave 3。
**Gate／Evidence：** G5；task 2.3.*測試與default run log。
**完成門檻：** gate通過才啟動；啟動要求失敗會讓batch失敗；成功文字不再稱為只launch installer。

- [x] 2.3.1 在三個hash通過後啟動installed `SuperExplorer.exe`
- [x] 2.3.2 將啟動API失敗轉為具體stage及非零退出碼
- [x] 2.3.3 更新`build_test_install.bat`預設成功訊息為已建置、安裝、驗證及啟動最新版
- [x] 2.3.4 保留`--no-launch`與`--check`既有成功訊息及batch退出碼

## 3. 選項隔離與安裝整合驗證

### 3.1 聚焦選項與負向測試

**目的：** 固化所有選項分支、fail-closed語意與其他入口隔離。
**輸入：** 2.*實作、可替換process/hash fixture。
**產出：** 聚焦Lua／batch測試及結果記錄。
**依賴：** 2.3。
**Owner／Wave：** Primary／Wave 4。
**Gate／Evidence：** G6 blocking；`evidence/focused-tests.md`。
**完成門檻：** 每個規格scenario有獨立斷言；所有聚焦測試exit 0。

- [x] 3.1.1 驗證default分支依序發布、同步安裝、三檔hash、啟動
- [x] 3.1.2 驗證`--no-launch`不呼叫installer、installed verifier或application launch
- [x] 3.1.3 驗證`--check`不建置、發布、安裝或啟動
- [x] 3.1.4 驗證`--skip-build`略過cargo但仍執行default安裝gate
- [x] 3.1.5 驗證installer nonzero時不執行hash或application launch
- [x] 3.1.6 驗證任一binary missing或mismatch時不啟動且回傳非零
- [x] 3.1.7 驗證正式combined與SuperDesktop-only入口未啟用auto-install

### 3.2 真實預設入口安裝驗收

**目的：** 以使用者實際命令證明最新版確實安裝，而非只產生installer。
**輸入：** 3.1通過、可用NSIS／權限、目前release來源。
**產出：** default run log、installer artifact、installed hash與啟動證據。
**依賴：** 3.1。
**Owner／Wave：** Primary／Wave 5；系統可見安裝只由Primary執行。
**Gate／Evidence：** G7 blocking；`evidence/installed-acceptance.md`。
**完成門檻：** `build_test_install.bat` exit 0；三檔hash一致；啟動程序path指向installed executable。

- [x] 3.2.1 執行`build_test_install.bat --no-launch`驗證只發布及installer PE內容
- [x] 3.2.2 執行不帶參數的`build_test_install.bat`並記錄同步安裝terminal結果
- [x] 3.2.3 比對release／installed主程式SHA-256與實際啟動path
- [x] 3.2.4 比對release／installed broker SHA-256
- [x] 3.2.5 比對release／installed worker SHA-256
- [x] 3.2.6 驗證default成功訊息、退出碼與安裝／啟動事實一致

## 4. 安裝版拖放與最終集中檢查

### 4.1 ADB與SFTP真實dotfile blocking matrix

**目的：** 證明由修正後batch安裝的binary具備使用者要求的Explorer外部拖放。
**輸入：** 3.2已驗證installed app、`.tmp-full-meta.json`、ADB與SFTP provider。
**產出：** ADB／SFTP headful reports、remote oracle及安全cleanup證據。
**依賴：** 3.2。
**Owner／Wave：** Primary／Wave 6；credentialed SFTP只由Primary執行。
**Gate／Evidence：** G8 blocking；`evidence/drag-acceptance.md`。
**完成門檻：** 兩個目的basename、34,629 bytes及hash一致；來源保留；遠端受控副本清理。

- [x] 4.1.1 確認指定來源存在、大小34,629 bytes並記錄SHA-256
- [x] 4.1.2 從Windows Explorer拖入`adb://emulator-5554/sdcard/Download`
- [x] 4.1.3 以ADB remote oracle驗證精確basename、大小與SHA-256
- [x] 4.1.4 從Windows Explorer拖入`sftp://45.32.49.125/home/linuxuser`
- [x] 4.1.5 以interactive SFTP remote oracle驗證精確basename、大小與內容
- [x] 4.1.6 驗證兩次Copy後本機來源存在且SHA-256未變
- [x] 4.1.7 僅在ownership驗證後清理ADB精確副本
- [x] 4.1.8 僅在ownership驗證後清理SFTP精確副本

### 4.2 最終品質與證據一致性

**目的：** 集中執行使用者要求的最後檢查，任何失敗都回到對應package補完並重跑。
**輸入：** 所有程式、測試、安裝與headful結果。
**產出：** 最終驗證記錄、52筆唯一evidence index及完成tasks。
**依賴：** 4.1。
**Owner／Wave：** Primary／Wave 7。
**Gate／Evidence：** G9 blocking；`evidence/final-validation.md`、`evidence/index.jsonl`。
**完成門檻：** 所有命令exit 0、52個leaf皆有唯一證據、OpenSpec strict valid且無P0/P1缺漏。

- [x] 4.2.1 執行受影響Lua／batch聚焦測試並保存exit status
- [x] 4.2.2 執行`git diff --check`與新增行credential literal掃描
- [x] 4.2.3 驗證installer與三個release／installed binary的最終SHA-256記錄
- [x] 4.2.4 建立每個leaf唯一`task_id`的evidence index並驗證筆數與唯一性
- [x] 4.2.5 執行詳細tasks validator與`openspec validate --strict`
- [x] 4.2.6 人工核對proposal→design→scenario→gate→task→evidence追溯
- [x] 4.2.7 將全部通過leaf標記完成；任一失敗項reopen並補完後重新驗證
