## 1. 重現與事件所有權

### 1.1 真實回歸重現

**目的：** 以目前 binary 證明 Explorer→ADB／SFTP 的實際失敗階段並保存可比較基線。
**輸入：** 核准設計、既有 headful runner、Windows Explorer、指定 ADB/SFTP 路徑。
**產出：** failure report、事件/磁碟 oracle與根因分類。
**依賴：** 無。
**Owner／Wave：** primary／1。
**Gate／Evidence：** G-REPRO；`evidence/index.jsonl`。
**完成門檻：** 至少一個遠端目的地穩定重現，且能區分 UI target、state validation或transfer dispatch。

- [x] 1.1.1 盤點現行 background、folder-row與ordinary-row external drop handler及近期差異
- [x] 1.1.2 以 Windows Explorer→ADB 受控 Copy fixture重現並保存 terminal event與檔案存在性
- [x] 1.1.3 以 Windows Explorer→SFTP 受控 Copy fixture重現並保存 terminal event與檔案存在性
- [x] 1.1.4 將實證根因對應至 UI target、state validation或transfer dispatch之一

### 1.2 Drop ownership contract

**目的：** 把三類檔案檢視元素的 external drop ownership收斂成互斥規則。
**輸入：** 1.1 根因、GPUI external path API、presentation resolver。
**產出：** ownership helper／render wiring與contract tests。
**依賴：** 1.1。
**Owner／Wave：** primary／2。
**Gate／Evidence：** G-OWNERSHIP；`evidence/index.jsonl`。
**完成門檻：** background與writable folder可接收；ordinary file透明；folder terminal不重複bubble。

- [x] 1.2.1 建立或收斂 background／folder／ordinary-row ownership predicate
- [x] 1.2.2 修正 file-view background external can-drop、drag-move與terminal drop wiring
- [x] 1.2.3 修正 writable folder-row handler並保持 terminal stop-propagation
- [x] 1.2.4 移除ordinary file-row會攔截external drop的拒絕型child target
- [x] 1.2.5 新增filled viewport、folder row與ordinary row ownership聚焦測試

## 2. 目的地、effect與terminal安全

### 2.1 Generation-safe destination

**目的：** 讓背景與資料夾目的地固定於drop當下的stable identity與generation。
**輸入：** 1.2 ownership、tab/location state、filesystem destination validator。
**產出：** destination resolver/state validation與stale tests。
**依賴：** 1.2。
**Owner／Wave：** primary／3。
**Gate／Evidence：** G-DESTINATION；`evidence/index.jsonl`。
**完成門檻：** 導覽/切tab/row重排後不得誤投新目錄；不可寫folder不得降級background。

- [x] 2.1.1 盤點DropExternal action攜帶的row、tab、generation與destination重建路徑
- [x] 2.1.2 固定background目前目錄與folder stable item目的地
- [x] 2.1.3 對stale generation、不可寫provider與失效folder fail closed
- [x] 2.1.4 新增navigation、tab switch、presentation reorder與capability變更測試

### 2.2 Payload／effect與Move安全

**目的：** 保持標準CF_HDROP的Copy／Move語意並拒絕不支援payload。
**輸入：** 2.1目的地、既有effect negotiation與TransferEngine。
**產出：** validated request與effect/terminal regression tests。
**依賴：** 2.1。
**Owner／Wave：** primary／4。
**Gate／Evidence：** G-EFFECT；`evidence/index.jsonl`。
**完成門檻：** file/folder、多選、Copy/Move皆正確；None/Link/空白/非本機不產生命令。

- [x] 2.2.1 驗證ExternalPaths非空、全為本機絕對路徑且effect為Copy或Move
- [x] 2.2.2 確保Drop只排入一個DataTransfer command並保留allowed/performed effect
- [x] 2.2.3 驗證Move僅由success terminal刪除對應成功來源
- [x] 2.2.4 新增多選file/folder、Copy、Move、None、Link與invalid source測試
- [x] 2.2.5 驗證取消、部分失敗與late callback不誤刪來源或顯示成功

## 3. 診斷與跨層整合

### 3.1 Credential-safe drag diagnostics

**目的：** 讓有DragOver但無terminal command的狀況可定位且不洩漏敏感資訊。
**輸入：** 1.x事件路徑、2.x validation結果、既有tracing與operation message。
**產出：** typed rejection reason、sanitised logs與diagnostic tests。
**依賴：** 2.2。
**Owner／Wave：** primary／5。
**Gate／Evidence：** G-DIAGNOSTICS；`evidence/index.jsonl`。
**完成門檻：** UI reject、state reject、dispatch failure可區分；無password、userinfo或完整來源清單。

- [x] 3.1.1 定義UI target、state validation與transfer dispatch拒絕分類
- [x] 3.1.2 在terminal Drop與command submission邊界加入bounded結構化診斷
- [x] 3.1.3 讓使用者失敗訊息保留操作、sanitised目的地、stage與provider reason
- [x] 3.1.4 新增credential redaction、source-count與rejection-stage contract測試

### 3.2 Local與clipboard隔離

**目的：** 證明遠端ownership修正不影響Local drop、文字/圖片clipboard與拖出行為。
**輸入：** 3.1整合結果、既有clipboard/drag focused tests。
**產出：** 聚焦回歸結果與差異證據。
**依賴：** 3.1。
**Owner／Wave：** primary／6。
**Gate／Evidence：** G-ISOLATION；`evidence/index.jsonl`。
**完成門檻：** Local external Copy工作；非CF_HDROP內容不被解讀；既有drag source contract通過。

- [x] 3.2.1 驗證Windows Explorer→SuperExplorer Local background與folder目的地
- [x] 3.2.2 驗證text/image-only clipboard不觸發file drop或被清除
- [x] 3.2.3 驗證SuperExplorer→Explorer drag source與internal drag contracts未變

## 4. 集中實測與最後關門

### 4.1 Windows headful matrix

**目的：** 以真實Explorer pointer/OLE與磁碟oracle證明Local、ADB、SFTP行為。
**輸入：** 全部實作、Windows Explorer、`emulator-5554`、既有SFTP profile與marker fixture。
**產出：** headful report、terminal logs與fixture cleanup evidence。
**依賴：** 3.2。
**Owner／Wave：** primary／7。
**Gate／Evidence：** G-HEADFUL；`evidence/index.jsonl`。
**完成門檻：** Local Copy、ADB Copy/Move、SFTP Copy/Move全部通過；所有受控fixture清理。

- [x] 4.1.1 校準runner以UI Automation取得真實source bounds並驗證remote background target point
- [x] 4.1.2 執行Explorer→Local Ctrl Copy並驗證目的存在與來源保留
- [x] 4.1.3 執行Explorer→ADB Ctrl Copy並驗證目的存在與來源保留
- [x] 4.1.4 執行Explorer→ADB Shift Move並驗證目的存在與來源移除
- [x] 4.1.5 執行Explorer→SFTP Ctrl Copy並驗證目的存在與來源保留
- [x] 4.1.6 執行Explorer→SFTP Shift Move並驗證目的存在與來源移除
- [x] 4.1.7 驗證受控fixture名稱與來源後清理ADB、SFTP遠端項目並保留local evidence fixture

### 4.2 Automated and OpenSpec gates

**目的：** 集中完成格式化、相關測試、編譯、evidence與規格關門。
**輸入：** 4.1 reports、全部source與OpenSpec artifacts。
**產出：** raw outputs、唯一task evidence index與final review。
**依賴：** 4.1。
**Owner／Wave：** primary／8。
**Gate／Evidence：** G-FOCUSED、G-FINAL；`evidence/index.jsonl`。
**完成門檻：** 所有blocking command成功；每個leaf有唯一證據；strict validation與diff review通過。

- [x] 4.2.1 執行model、shell、UI與app external drag聚焦測試
- [x] 4.2.2 執行cargo fmt、相關crate check與git diff --check
- [x] 4.2.3 掃描舊拒絕型row target、credential與未清fixture
- [x] 4.2.4 建立每個leaf唯一task_id的evidence index與validation摘要
- [x] 4.2.5 執行task validator、OpenSpec strict validation與relevant diff review
