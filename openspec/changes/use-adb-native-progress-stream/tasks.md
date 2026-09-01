## 1. ADB progress parsing contract

### 1.1 Stateful frame parser

**目的：** 將任意 pipe chunks 還原成可驗證的 cumulative progress observations。
**輸入：** 核准設計、ADB 37.0.0 真實輸出、既有 bounded capture。
**產出：** parser 型別、format helpers與 unit tests。
**依賴：** 無。
**Owner／Wave：** primary／1。
**Gate／Evidence：** G-PARSER；`evidence/index.jsonl`。
**完成門檻：** delimiter、chunk boundary、percent、byte pair、invalid與regression cases全數通過。

- [x] 1.1.1 收集本機 ADB push/pull 原生 stdout/stderr frame fixture
- [x] 1.1.2 實作跨 read 殘片與 `\r`／`\n` frame tokenizer
- [x] 1.1.3 實作 checked percent 與 completed/total byte-pair parser
- [x] 1.1.4 實作重複、倒退、越界、溢位與未知格式拒絕規則
- [x] 1.1.5 新增 parser 正常、分段、惡意與版本容錯測試

### 1.2 Observation-to-delta adapter

**目的：** 將 parser observations 轉成 reporter 可接受的單調 byte delta。
**輸入：** 1.1 parser、來源 total、既有 delivered-byte callback。
**產出：** cumulative adapter與 terminal completion tests。
**依賴：** 1.1。
**Owner／Wave：** primary／2。
**Gate／Evidence：** G-ADAPTER；`evidence/index.jsonl`。
**完成門檻：** known/unknown、rounding、reset、failure與success補齊語意可決定性驗證。

- [x] 1.2.1 實作 percentage-to-byte checked mapping
- [x] 1.2.2 實作 cumulative observation 去重與 delta 計算
- [x] 1.2.3 實作未知 total 與 per-file reset 的 indeterminate 降級
- [x] 1.2.4 實作僅成功 terminal 可補齊剩餘 bytes
- [x] 1.2.5 新增 rounding、reset、failure、cancel與overflow測試

## 2. Runner 與 provider integration

### 2.1 Incremental process output

**目的：** 在 child terminal 前以隱藏pseudo-terminal安全取得並交付ADB原生progress chunks。
**輸入：** 1.1 parser、`SystemAdbCommandRunner` cancellation/timeout/capture contract。
**產出：** 向後相容runner method、Windows pseudo-terminal adapter、pipe fallback與fake fixtures。
**依賴：** 1.1。
**Owner／Wave：** primary／3。
**Gate／Evidence：** G-RUNNER；`evidence/index.jsonl`。
**完成門檻：** stdout/stderr不阻塞、capture有界、panic/取消/timeout均清理child。

- [x] 2.1.1 擴充內部 runner API並為既有fake保留預設行為
- [x] 2.1.2 實作Windows隱藏pseudo-terminal transfer adapter與bounded增量reader
- [x] 2.1.3 隔離 output callback panic並持續drain
- [x] 2.1.4 保持取消、timeout、kill/wait與hidden-window行為
- [x] 2.1.5 新增PTY/pipe fallback、capture上限、panic、取消與timeout測試

### 2.2 ADB provider native progress

**目的：** upload/download直接使用原生output，不再輪詢目的端。
**輸入：** 1.2 adapter、2.1 runner、既有provider callback contract。
**產出：** push/pull streaming integration與無polling檢查。
**依賴：** 1.2、2.1。
**Owner／Wave：** primary／4。
**Gate／Evidence：** G-PROVIDER；`evidence/index.jsonl`。
**完成門檻：** 兩方向發布中間delta；unknown安全降級；source code無progress-only stat/tree scan loop。

- [x] 2.2.1 將 AdbClient push 接入 progress-capable runner
- [x] 2.2.2 將 AdbClient pull 接入 progress-capable runner
- [x] 2.2.3 將 provider upload/download接入 cumulative adapter與成功補齊
- [x] 2.2.4 刪除 remote metadata與local tree scan polling
- [x] 2.2.5 新增成功、非零退出、取消、未知格式與無polling測試

## 3. Integration 與最後驗證

### 3.1 Application progress compatibility

**目的：** 證明 native ADB delta沿既有TransferEngine/reporter/UI路徑保持單調與terminal安全。
**輸入：** 2.2 provider、既有cross-filesystem progress implementation。
**產出：** focused integration tests與diagnostic evidence。
**依賴：** 2.2。
**Owner／Wave：** primary／5。
**Gate／Evidence：** G-INTEGRATION；`evidence/index.jsonl`。
**完成門檻：** local↔ADB與ADB↔SFTP不倒退，失敗/取消不跳100%，credential scan通過。

- [x] 3.1.1 驗證 Local→ADB與ADB→Local reporter events
- [x] 3.1.2 驗證 ADB→SFTP與SFTP→ADB two-stage aggregation
- [x] 3.1.3 驗證 Failed/Cancelled保留最後實值且無late progress
- [x] 3.1.4 驗證diagnostics與progress不含credential或新增路徑欄位

### 3.2 Automated and real-device gates

**目的：** 集中完成編譯、聚焦測試、實機fixture與OpenSpec closing gates。
**輸入：** 全部實作、`emulator-5554`、受控marker fixture。
**產出：** raw outputs、evidence index與final review。
**依賴：** 3.1。
**Owner／Wave：** primary／6。
**Gate／Evidence：** G-FINAL；`evidence/index.jsonl`。
**完成門檻：** 所有blocking commands成功；push/pull terminal前有中間progress；fixture清理；strict validation通過。

- [x] 3.2.1 執行 model/remote/app/UI progress聚焦測試
- [x] 3.2.2 執行 format、相關crate check與 `git diff --check`
- [x] 3.2.3 執行 emulator大檔push/pull原生中間進度與內容一致性fixture
- [x] 3.2.4 執行ADB↔SFTP雙向受控matrix並清理fixture
- [x] 3.2.5 建立每個leaf唯一task_id evidence index
- [x] 3.2.6 執行task validator、OpenSpec strict validation與relevant diff review
