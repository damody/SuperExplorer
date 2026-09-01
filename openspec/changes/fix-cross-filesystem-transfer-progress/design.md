## Context

`OperationProgress` 已是 Shell STA、service 與 UI 的 typed event，但 remote transfer 只發布 operation 起點與 terminal。`explorer-remote::TransferEngine` 執行 Local／ADB／SFTP 的實際遞迴 upload/download，`explorer-app::RemoteExplorerService` 負責 request routing、staging 與 terminal，UI 則只依 item counters 算百分比。這使大型遠端工作沒有中間狀態。

本變更以已核准的 `docs/superpowers/specs/2026-09-01-cross-filesystem-transfer-progress-design.md` 為權威來源。既有 dirty worktree 與其他 OpenSpec change 不得被回復；驗證只針對相關傳輸路徑，不執行完整迴歸。

## Goals / Non-Goals

**Goals:**

- 以實際成功交付的 bytes 持續發布 Local／ADB／SFTP 傳輸進度。
- 讓檔案、遞迴資料夾、Copy、Move、取消、Partial 與未知大小使用一致 contract。
- 讓跨遠端 staging 形成單調、不重設的兩階段進度。
- 保持 bounded/coalesced publication、request correlation、terminal barrier 與 credential-safe diagnostics。
- 以聚焦測試及真實 ADB／SFTP 大型 fixture 證明完成前存在 0% 與 100% 之間的進度。

**Non-Goals:**

- 不加入速度曲線、ETA、bandwidth throttling 或傳輸佇列新 UI。
- 不改 public extension ABI。
- 不做完整產品迴歸或無關重構。

## Decisions

### 1. 擴充既有 OperationProgress，而非建立平行事件

內部 model 加入 `completed_bytes`、`total_bytes: Option<u64>`、phase 與 current item。Shell 與 remote producer 共用同一事件，UI 不需判斷 provider。替代方案是新增 remote-only event，但會複製 terminal/stale handling 並讓 operation center 分裂，因此不採用。

### 2. Reporter 位於 transfer 層，publisher 由 app 注入

`TransferProgressReporter` 接收小型 sink/callback，不依賴 GPUI 或 COM。它持有單調 counters、節流時間／byte threshold、closed flag 與 request-owned publication closure。Provider 的 streaming loop 只回報成功交付 delta。這讓 mock provider 可決定性測試，也避免每個 provider 自行建構 `ExplorerEvent`。

### 3. 預掃描只承諾可靠總量

Transfer engine 以 provider metadata 遞迴估算來源。任一節點大小未知、overflow 或實際 bytes 超過 estimate 時，整體改為 indeterminate；不得調大分母造成視覺倒退。空檔／空資料夾以 item progress 收斂。預掃描可取消且不啟動 destructive cleanup。

### 4. Bytes 在成功寫入目的 stage 後計入

read 成功但 write 失敗的 bytes 不算完成。Local file copy、ADB sync/stream 與 SFTP stream 都在目的寫入成功後回報。若既有 provider API 只提供整檔 operation，擴充內部 provider method 接受 progress callback；不更改 extension ABI。

### 5. 兩階段 staging 使用工作量 2N

remote→remote 已知來源 N bytes 時，download 與 upload 各佔 N，總量 2N；未知時全程 indeterminate。階段切換只改 phase，不重設 counters。Move cleanup 不增加 byte progress，只有目的項目完整成功才執行。

### 6. Producer 節流、consumer terminal gate

Reporter 以 byte threshold 與最短時間合併更新，但 stage/item/terminal 邊界強制 flush。事件使用既有 bounded progress lane；terminal 關閉 reporter，UI 依 request/generation 拒絕 late update。Finished 才將呈現設為 100%，Cancelled／Partial／Failed 保持最後實值。

### 7. UI 百分比優先順序

已知且非零 bytes 使用 byte ratio，terminal 前上限 99%。零-byte operation 使用 item ratio；未知 bytes 顯示 indeterminate 與 transferred bytes，不顯示百分比。訊息保留完整來源／目的、目前項目、item counts 與詳細 terminal。

### 8. Evidence-driven correction

- A 類：可調整 task 拆分、測試命令或 reporter 節流常數，不改 contract/gate。
- B 類：若 provider 實作證明大小或 callback 假設錯誤，可在核准範圍內同步修正 design/spec/tasks，重開受影響 evidence 並 strict validate。
- C 類：降低真實 byte gate、移除任何傳輸方向、改 public ABI、增加 dependency／權限／外部 destructive 範圍，必須取得使用者核准。

## Risks / Trade-offs

- **預掃描增加遠端往返** → 對 metadata 已在 list cache 的項目重用資料；無可靠資料時降級 indeterminate，不為百分比阻塞太久。
- **高頻 callback 塞滿 queue** → reporter coalescing、try-send 與 terminal 強制 flush；測試 queue saturation。
- **檔案傳輸中大小改變** → actual 超出 estimate 時降級未知，counter 保持單調。
- **兩階段失敗造成誤刪來源** → cleanup 仍依逐項 destination terminal；progress 不授權 deletion。
- **u64 overflow** → checked/saturating aggregation並降級未知，不 panic。
- **敏感路徑洩漏** → UI 可顯示使用者要求的 public location；persistent diagnostic 維持 credential redaction。

## Migration Plan

1. 先擴充 model constructors/tests，讓所有 producer/consumer 可編譯。
2. 實作 reporter 與 transfer engine callback，再逐一接入 Local、ADB、SFTP。
3. 接入 app event routing、staging weighting 與 terminal closure。
4. 更新 UI 與 automation semantics。
5. 最後集中跑聚焦與 headful gates。Rollback 可回復本 change 的 model／reporter／provider／UI edits；不含持久資料 migration。

## Open Questions

無。Provider 若不能可靠取得總大小，已明確採用 indeterminate，而不是在 apply 階段重新決策。
