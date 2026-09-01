## 1. 外部 Explorer 拖入 SFTP

### 1.1 UI external drop 契約

**目的：** 讓 DragEnter、DragOver 與 Drop 對 SFTP file view／folder row 產生一致且可診斷的命令。
**輸入：** 核准 design、現有 GPUI external metadata 與 `DropExternal` model。
**產出：** UI effect／target 路由修正與聚焦測試。
**依賴：** 無。
**Owner／Wave：** primary／1。
**Gate／Evidence：** G-UI-DROP；`evidence/index.jsonl`。
**完成門檻：** 有效 `CF_HDROP` 在 SFTP 空白處與資料夾列均命中，拒絕時包含原因。

- [x] 1.1.1 盤點 SFTP file view、folder row 與 navigation target 的 DragEnter／DragOver／Drop 分派斷點
- [x] 1.1.2 修正 external file metadata 到 `DropExternal` 的來源、target 與 effect 轉換
- [x] 1.1.3 加入 Drop 未命中及 effect 拒絕的結構化診斷
- [x] 1.1.4 新增 UI external drop target 與修飾鍵聚焦測試

### 1.2 Remote upload 與 Move 安全

**目的：** 把 external local sources 正確上傳至 SFTP，並安全處理 Copy／Shift Move。
**輸入：** 1.1 命令契約、remote provider upload/delete 能力。
**產出：** remote transfer 路由、逐項結果與來源刪除規則。
**依賴：** 1.1。
**Owner／Wave：** primary／2。
**Gate／Evidence：** G-UPLOAD-MOVE；`evidence/index.jsonl`。
**完成門檻：** 檔案與資料夾 Copy 成功；Move 只刪成功來源；部分失敗保留失敗來源。

- [x] 1.2.1 修正 `DropExternal` 到 SFTP provider upload 的命令路由
- [x] 1.2.2 補齊本機資料夾遞迴上傳與單一根名稱語意
- [x] 1.2.3 實作 Copy／Ctrl Copy／Shift Move 的 effect 與成功後逐項來源刪除
- [x] 1.2.4 補齊 upload、來源刪除與部分成功的詳細 terminal
- [x] 1.2.5 新增 remote upload、遞迴、Move 與部分失敗聚焦測試

## 2. SFTP 拖到 Windows Explorer

### 2.1 Request-scoped staging

**目的：** 在 OLE 開始前建立完整、隔離且不提早清理的本機 staging 樹。
**輸入：** `BeginDrag` 選取項目、SFTP provider download、request ID。
**產出：** staging builder、ownership map 與清理 terminal。
**依賴：** 無，可與 1.1 同 wave。
**Owner／Wave：** primary／1。
**Gate／Evidence：** G-STAGING；`evidence/index.jsonl`。
**完成門檻：** 檔案／資料夾 staging 結構正確，連續 request 互不覆蓋，所有 terminal 可清理。

- [x] 2.1.1 盤點 remote `BeginDrag`、download 與 `active_drag_staging` ownership 生命週期
- [x] 2.1.2 修正檔案與資料夾遞迴下載到單一根名稱的 staging builder
- [x] 2.1.3 將 staging ownership 綁定 request ID 並保持到 Shell terminal
- [x] 2.1.4 修正成功、取消、失敗及應用程式關閉的 staging 清理
- [x] 2.1.5 新增 staging 結構、隔離、失敗與清理聚焦測試

### 2.2 Shell OLE data object 與 performed effect

**目的：** 讓 Explorer 接受 staging 項目並把實際 Copy／Move／Cancel 結果安全回傳。
**輸入：** 2.1 staging items、現有 Shell STA `DoDragDrop`。
**產出：** 標準 `CF_HDROP` data object、effect terminal 與 remote source completion。
**依賴：** 2.1。
**Owner／Wave：** primary／2。
**Gate／Evidence：** G-OLE-OUT；`evidence/index.jsonl`。
**完成門檻：** Explorer 可建立本機檔案／資料夾；Copy 保留遠端；成功 Move 才刪遠端；Cancel 保留遠端。

- [x] 2.2.1 驗證並修正 staging items 建立標準 Shell `IDataObject`／`CF_HDROP`
- [x] 2.2.2 修正 Preferred DropEffect、allowed effects 與 Ctrl／Shift negotiation
- [x] 2.2.3 將 `DoDragDrop` performed effect 與 Cancelled terminal 回傳 remote service
- [x] 2.2.4 實作成功 Move 後逐項刪除 SFTP 來源，Copy／None／失敗保留來源
- [x] 2.2.5 新增 Shell data object、effect、取消與 remote completion 聚焦測試

## 3. 整合與診斷

### 3.1 跨層 terminal 與安全

**目的：** 統一 UI、remote、Shell 的 request、錯誤與 credential-safe 診斷。
**輸入：** 1.x、2.x 的 terminal 與錯誤。
**產出：** 詳細訊息、sanitised log 與可追蹤 request lifecycle。
**依賴：** 1.2、2.2。
**Owner／Wave：** primary／3。
**Gate／Evidence：** G-DIAGNOSTICS；`evidence/index.jsonl`。
**完成門檻：** 每個失敗 stage 可識別來源、目的、effect 與原因，且無 credential 洩漏。

- [x] 3.1.1 統一 external drop、staging、OLE、upload/download/delete 的錯誤 stage 與逐項結果
- [x] 3.1.2 確保取消不記為 Internal 且不顯示成功
- [x] 3.1.3 新增 credential redaction 與診斷 contract 測試

## 4. 最後集中驗證

### 4.1 自動與 headful gate

**目的：** 以指定 SFTP 路徑證明雙向行為，並確認共用 local／ADB 拖放未回歸。
**輸入：** 全部實作、可連線 SFTP profile、Windows Explorer 與 ADB fixture。
**產出：** 測試輸出、截圖／報告與 `evidence/index.jsonl`。
**依賴：** 3.1。
**Owner／Wave：** primary／4。
**Gate／Evidence：** G-FOCUSED、G-HEADFUL、G-FINAL；`evidence/index.jsonl`。
**完成門檻：** 全部 blocking gate 通過；失敗必須修正並重跑，未執行或失敗不得勾選。

- [x] 4.1.1 執行格式化、Shell／remote／UI 聚焦測試與相關 crate 編譯檢查
- [x] 4.1.2 Headful 驗證 Explorer→`sftp://45.32.49.125/home/linuxuser` 的檔案與資料夾 Copy
- [x] 4.1.3 Headful 驗證同一路徑 SFTP→Explorer 的檔案與資料夾 Copy
- [x] 4.1.4 Headful 驗證 Shift Move、取消與來源保留／刪除門檻
- [x] 4.1.5 執行 local／ADB 共用拖放聚焦回歸
- [x] 4.1.6 建立逐 task evidence index、審閱 diff 並 strict 驗證 OpenSpec
