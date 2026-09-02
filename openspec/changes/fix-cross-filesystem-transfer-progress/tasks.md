## 1. Progress contract 與 reporter

### 1.0 立即可見operation lifecycle

**目的：** 讓Copy／Move提交後不等待provider preflight即可顯示Preparing，並以同一record收斂至明確terminal。
**輸入：** 核准即時狀態設計、既有request context與operation center。
**產出：** submit-time record、Preparing publisher、明確動詞與時序測試。
**依賴：** 無。
**Owner／Wave：** primary／1。
**Gate／Evidence：** G-IMMEDIATE；`evidence/index.jsonl`。
**完成門檻：** 慢速preflight在300ms內可見；小檔無人工延遲且顯示複製完成；submit failure不殘留Preparing。

- [x] 1.0.1 盤點拖放、貼上、鍵盤與右鍵Copy／Move的record插入與submission順序
- [x] 1.0.2 在具有效context的Copy／Move提交時立即插入Preparing operation record
- [x] 1.0.3 在remote metadata estimator前force emit同一request的Preparing progress
- [x] 1.0.4 讓第一個delivered-byte delta切換Transferring且不因preflight重建record
- [x] 1.0.5 將Copy／Move lifecycle文字改為準備、正在與複製完成／移動完成
- [x] 1.0.6 讓submission failure、panic與disconnect關閉既有Preparing record
- [x] 1.0.7 新增慢速preflight 300ms、瞬間小檔與同request terminal聚焦測試
- [x] 1.0.8 新增拖放／貼上及Local／ADB／SFTP入口共用狀態機測試

### 1.1 Domain contract

**目的：** 建立 Local／ADB／SFTP 共用且可向後更新所有 workspace consumers 的 byte/item progress 型別。
**輸入：** 核准設計、現有 `OperationProgress`、operation record 與 Shell producers。
**產出：** model 型別、constructors、format helpers 與 contract tests。
**依賴：** 無。
**Owner／Wave：** primary／1。
**Gate／Evidence：** G-CONTRACT；`evidence/index.jsonl`。
**完成門檻：** 所有 producer/consumer 編譯；已知、未知、零 bytes 與 terminal 百分比規則有測試。

- [x] 1.1.1 盤點 `OperationProgress` 的所有 constructors、publishers、records 與 render consumers
- [x] 1.1.2 擴充 model contract，加入 completed／total bytes、phase 與 current item
- [x] 1.1.3 實作確定、未知、零 bytes 與 terminal presentation 計算 helper
- [x] 1.1.4 更新既有 Shell、fake service、automation 與 UI constructors
- [x] 1.1.5 新增 model progress 單調與百分比邊界測試

### 1.2 TransferProgressReporter

**目的：** 提供 request-scoped、節流、單調、overflow-safe 且 terminal-safe 的共用 reporter。
**輸入：** 1.1 contract、既有 bounded progress event lane。
**產出：** reporter module、publisher adapter 與聚焦測試。
**依賴：** 1.1。
**Owner／Wave：** primary／2。
**Gate／Evidence：** G-REPORTER；`evidence/index.jsonl`。
**完成門檻：** delta 聚合正確；高頻更新受限；phase/item/terminal flush；terminal 後無 late progress。

- [x] 1.2.1 實作 checked byte/item aggregation 與已知／未知 total 狀態轉換
- [x] 1.2.2 實作時間與 byte threshold coalescing及強制 flush 邊界
- [x] 1.2.3 實作 close／cancel／terminal barrier 與 late callback rejection
- [x] 1.2.4 接入 request context publisher且保持 nonblocking bounded semantics
- [x] 1.2.5 新增 monotonic、overflow、throttle、flush 與 terminal barrier 測試

## 2. Transfer engine 與 provider 串流

### 2.1 Metadata preflight

**目的：** 為檔案與遞迴資料夾建立可靠 total bytes，無法承諾時安全降級未知。
**輸入：** provider metadata/list contract、cancellation、1.2 reporter。
**產出：** source tree estimator 與邊界測試。
**依賴：** 1.2。
**Owner／Wave：** primary／3。
**Gate／Evidence：** G-PREFLIGHT；`evidence/index.jsonl`。
**完成門檻：** 檔案、巢狀資料夾、空節點、未知大小、overflow、取消均有決定性結果。

- [x] 2.1.1 盤點 Local／ADB／SFTP metadata 是否提供可靠 file size 與 cached list reuse
- [x] 2.1.2 實作可取消的遞迴 source tree byte/item estimator
- [x] 2.1.3 實作未知節點、overflow 與 actual-over-estimate 降級規則
- [x] 2.1.4 新增檔案、巢狀／空資料夾、未知、overflow 與取消測試

### 2.2 Local、ADB、SFTP delivered-byte callbacks

**目的：** 每個實際 stream 在目的 write 成功後回報真實 chunk delta。
**輸入：** 1.2 reporter、現有 provider upload/download 與 local stream。
**產出：** provider callback contract、三類 stream 接線與 fault tests。
**依賴：** 2.1。
**Owner／Wave：** primary／4。
**Gate／Evidence：** G-STREAMS；`evidence/index.jsonl`。
**完成門檻：** read-only success 不計入；write success 精確累計；callback 不含 credential。

- [x] 2.2.1 擴充內部 provider transfer API 接受 delivered-byte callback
- [x] 2.2.2 將 Local copy stream 接入成功 write delta
- [x] 2.2.3 將 ADB upload／download stream 接入成功 write delta
- [x] 2.2.4 將 SFTP upload／download stream 接入成功 write delta
- [x] 2.2.5 新增 chunked success、read failure、write failure 與 callback redaction 測試

### 2.3 TransferEngine aggregation

**目的：** 讓單階段與遞迴 operation 正確更新 phase、current item、bytes 與根項目完成數。
**輸入：** 2.1 estimator、2.2 callbacks、現有 conflict／partial semantics。
**產出：** transfer engine progress orchestration 與整合測試。
**依賴：** 2.2。
**Owner／Wave：** primary／5。
**Gate／Evidence：** G-ENGINE；`evidence/index.jsonl`。
**完成門檻：** item/byte counters 精確，conflict、skip、partial 與 Move cleanup 不扭曲 progress。

- [x] 2.3.1 將 reporter 生命週期與 Preparing／Transferring／Finalizing phase 接入 engine
- [x] 2.3.2 實作每個 root item 的 current item 與完成計數
- [x] 2.3.3 保持 conflict、skip、failure、partial 與 Move cleanup 的逐項語意
- [x] 2.3.4 新增多檔、遞迴資料夾、skip、partial 與 Move 聚焦測試

## 3. 跨遠端 staging 與應用程式事件

### 3.1 Two-stage weighting

**目的：** 將 remote→remote download/upload 合併為單一 2N 或 indeterminate progress。
**輸入：** 2.3 engine、request-scoped staging、remote routing。
**產出：** phase weight adapter、staging ownership 整合與測試。
**依賴：** 2.3。
**Owner／Wave：** primary／6。
**Gate／Evidence：** G-STAGED；`evidence/index.jsonl`。
**完成門檻：** ADB↔SFTP 兩方向不重設、不倒退；第二階段失敗保留來源。

- [x] 3.1.1 盤點所有 remote→remote staging branches 與 request ownership
- [x] 3.1.2 實作已知 N bytes 的 download/upload 2N 加權 adapter
- [x] 3.1.3 實作未知 total 的跨 phase indeterminate aggregation
- [x] 3.1.4 將 destination terminal 與 Move cleanup gate 綁定逐項成功結果
- [x] 3.1.5 新增 ADB→SFTP、SFTP→ADB、第二階段失敗與取消測試

### 3.2 Event routing 與 terminal

**目的：** 將 remote progress 經既有 bounded lane 送至 UI，並確保 exactly-one terminal 與無 late progress。
**輸入：** 3.1 progress、RemoteExplorerService event mux。
**產出：** event routing、terminal close 與 diagnostics tests。
**依賴：** 3.1。
**Owner／Wave：** primary／7。
**Gate／Evidence：** G-EVENTS；`evidence/index.jsonl`。
**完成門檻：** request/generation 保持；terminal 後 progress 被拒；錯誤詳細且 credential-safe。

- [x] 3.2.1 接入 remote `OperationProgress` publisher 與 try_recv multiplexing
- [x] 3.2.2 在 Finished／Partial／Failed／Cancelled 前 flush 並關閉 reporter
- [x] 3.2.3 確保 submit failure、panic、disconnect 與 cancellation 都產生單一 terminal
- [x] 3.2.4 新增 saturation、late progress、terminal uniqueness 與 redaction 測試

## 4. 下方進度 UI

### 4.1 Operation record 與 render

**目的：** 依真實 contract 顯示百分比或 indeterminate bar及完整傳輸資訊。
**輸入：** 1.1 model、3.2 events、既有 operation message/fade UI。
**產出：** UI state、render、accessibility 與聚焦測試。
**依賴：** 3.2。
**Owner／Wave：** primary／8。
**Gate／Evidence：** G-UI；`evidence/index.jsonl`。
**完成門檻：** 已知 total、未知 total、零 bytes、multi-item 與所有 terminals 呈現符合 spec。

- [x] 4.1.1 擴充 operation record 保存 byte、phase 與 current item progress
- [x] 4.1.2 修正已知 bytes 與零 bytes 的百分比及 terminal 100% 規則
- [x] 4.1.3 實作未知 total 的 indeterminate bar 與 transferred-byte 文字
- [x] 4.1.4 更新完整來源／目的、目前項目、item/byte 摘要與 accessibility semantics
- [x] 4.1.5 確保 Cancelled／Partial／Failed 不跳 100% 且保留詳細原因
- [x] 4.1.6 新增 operation record、render structure 與 stale terminal 測試

## 5. 最後集中驗證

### 5.1 Automated gates

**目的：** 集中驗證相關 crates、六方向 contract 與無 late progress。
**輸入：** 全部實作與聚焦 fixtures。
**產出：** raw test outputs 與逐 task evidence index。
**依賴：** 4.1。
**Owner／Wave：** primary／9。
**Gate／Evidence：** G-AUTO；`evidence/index.jsonl`。
**完成門檻：** 所有 blocking 聚焦測試、編譯、格式與 diff gate 通過。

- [x] 5.1.1 執行 model／remote／app／UI progress 聚焦測試
- [x] 5.1.2 執行相關 crate 格式化與編譯檢查
- [x] 5.1.3 執行 Local↔ADB、Local↔SFTP、ADB↔SFTP 六方向整合矩陣
- [x] 5.1.4 建立每個 leaf 唯一 task_id 的 evidence index

### 5.2 Headful 與 final review

**目的：** 以真實大型 fixture 證明完成前有中間進度，並完成 OpenSpec closing gates。
**輸入：** 可連線 emulator-5554、已儲存 SFTP profile、5.1 artifacts。
**產出：** headful reports/screenshots、strict validation 與 final review。
**依賴：** 5.1。
**Owner／Wave：** primary／10。
**Gate／Evidence：** G-HEADFUL、G-FINAL；`evidence/index.jsonl`。
**完成門檻：** Local／ADB／SFTP 代表性大檔在 terminal 前觀察到 1–99%；取消不跳 100%；OpenSpec 及 diff 通過。

- [x] 5.2.1 Headful 驗證 Local→ADB 與 ADB→Local 中間 byte progress
- [x] 5.2.2 Headful 驗證 Local→SFTP 與 SFTP→Local 中間 byte progress
- [x] 5.2.3 Headful 驗證 ADB↔SFTP 兩階段不重設的中間 progress
- [x] 5.2.4 Headful 驗證取消／失敗保留最後實值且無 late progress
- [x] 5.2.5 審閱 relevant diff、credential scan 與 `git diff --check`
- [x] 5.2.6 執行 task validator、OpenSpec strict validation 並確認全部 task/evidence 完成
