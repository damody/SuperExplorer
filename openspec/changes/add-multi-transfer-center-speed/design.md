## Context

OperationCenter 現在以 `HashMap` 保存記錄及一個 `latest` request ID，底部因此只知道最後插入的工作，最後工作先終止時不會回退。`OperationProgress` 已提供累積 bytes，但沒有時間樣本；通用遠端 reporter 已有 200 ms 節流，ADB runner 的原生輸出與 UI 可見更新仍需要端到端校正。工具列尚無多工作入口，Shift+Delete 則在提交時立即形成 Preparing 記錄。

限制：維持現有 provider 協定、RequestContext 取消語意、GPUI/Fluent 元件風格、八秒終止通知，不增加相依套件或跨重啟歷史。

## Goals / Non-Goals

**Goals:**

- 可信地顯示 Local、ADB、SFTP 的 bytes/second。
- 活動遠端狀態以 200 ms 節奏進入 UI，階段與終止事件立即送達。
- 穩定處理多工作排序、前景回退與逐工作取消。
- 提供底部單摘要及右上可展開的 Fluent 傳輸面板。
- Shift+Delete 僅在終止後顯示底部結果。
- 以正式安裝版完成端到端與使用者視角驗證。

**Non-Goals:**

- 不做 bandwidth throttling、排程器、暫停／續傳或速度圖表。
- 不將工作歷史寫入磁碟。
- 不改變 adb/sftp provider 公開 API 或 Windows Shell 原生操作語意。

## Decisions

### 1. 模型保存穩定插入順序

`OperationCenterState` 使用 request ID 索引加上插入順序向量，而不是依賴 `HashMap` iteration。新增 `active_transfer_count`、`foreground_record`、`records_newest_first` 等語意查詢。替代方案是 UI 自行排序 clone，但會讓狀態規則分散且難以單元測試，因此不採用。

### 2. 速度在 OperationRecord 由累積快照推導

每筆記錄保存上一個有效 `(completed_bytes, Instant)` 與 EMA bytes/second。新的單調 progress 到達時，以 bytes 差除以單調時間差，EMA 採近期樣本權重 0.35；第一筆、零增量、Preparing、倒退或遲到事件不產生速度。這避免擴充 provider protocol，也能讓 Local、ADB、SFTP 共用同一規則。測試透過注入樣本時間的內部方法保持決定性。

### 3. 200 ms 是發布節奏，不是偽造進度

通用 reporter 每 200 ms 最多發布一般 bytes 更新，階段／項目邊界／終止強制發布。ADB runner 持續 drain stdout/stderr 並以 200 ms tick 發布最新已知快照；若原生 adb 沒有新 bytes，只能發布相同快照，不得增加百分比。替代方案是輪詢遠端檔案大小，會增加額外 adb/sftp I/O 和競態，不採用。

### 4. B 混合式介面

底部只渲染 `foreground_record`：優先最後啟動的活動 Copy/Move；沒有活動傳輸時才顯示最後終止通知。右上工具列傳輸按鈕顯示活動數並切換 anchored Fluent panel，清單為本次執行期間記錄、最新在上。面板狀態保存在 `AppViewState`，Escape、外部點擊與重複按鈕關閉。

### 5. Shift+Delete 延後底部呈現

永久刪除記錄仍在模型中接收終止結果與保留歷史，但 queued/running 時被 `foreground_record` 排除；終止後才可成為八秒通知。這不延後實際刪除，也不破壞 request correlation。

### 6. 導向與取消沿用 typed actions

活動列取消沿用 request ID 精準取消。終止列由 typed `LocationDescriptor` 導向本機 parent 或遠端 destination/source，不拼接未驗證 shell command。無可導向位置時隱藏動作。

### 7. 實作中調整分級

- A：不改需求、門檻或 public contract 的任務拆分、順序與測試命令調整，可直接更新 tasks/evidence。
- B：核准範圍內的設計或規格錯誤，暫停受影響分支，同步修正 design/spec/tasks 並重新 strict validate；相關舊 evidence 標為 stale。
- C：範圍、公開承諾、200 ms 門檻、必需證據、平台、權限或外部／破壞性操作改變，必須取得使用者核准。

## Data Flow

Provider/native operation → correlated `OperationProgress` → `OperationCenterState::apply_event` → monotonic validation and speed sample → bottom foreground selector and toolbar panel render. Cancel action → existing UI command dispatch → remote or Shell request registry → cancellation token → unique terminal event → foreground fallback and panel row update.

## Failure Handling and Observability

- 遲到、倒退、重複終止事件不改變 record 或速度。
- 取消一筆工作不得改變其他 record；較新工作終止後重新選擇 foreground。
- ADB tick 不得阻塞 pipe drain；channel 飽和時保留最新單調狀態，終止事件仍必須送達。
- 現有詳細操作錯誤列持續保存來源、目的地、階段、錯誤碼及原因，不記錄 SFTP 密碼。

## Risks / Trade-offs

- [ADB 原生輸出可能比 200 ms 稀疏] → 200 ms tick 重送最新快照，UI 保持活躍但不虛構 bytes。
- [大量本次執行記錄使面板過長] → 面板設最大高度並捲動；不做磁碟持久化。
- [EMA 初期速度不穩] → 第一筆不顯示，後續以 0.35 權重平滑並在零增量時保留最近可信值。
- [右上面板與其他 overlay 衝突] → 沿用單一 overlay 關閉規則及 Escape/外部點擊測試。

## Migration Plan

模型與 UI 變更不涉及持久格式遷移。依序落地模型、provider 節奏、UI state/actions、chrome render；通過 focused tests 後由 `build_test_install.bat` 產生並安裝測試版。回滾可還原本 change 的模型／UI／runner 改動，既有單工作 OperationCenter 仍是相容基線。

## Open Questions

無。使用者已選定 B 混合式、記錄只保留本次程式執行期間，並授權其餘細節由實作者決定。
