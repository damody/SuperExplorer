## Context

`SystemAdbCommandRunner` 現在以兩個 reader thread drain stdout/stderr，但只在 child 結束後交付完整 capture。ADB provider 因此無法取得即時 progress，上一輪暫以 download tree size polling 與 upload remote `stat` polling 補足。這些輪詢對目錄是 O(nodes) 或會反覆啟動 ADB shell process。

核准來源設計為 `docs/superpowers/specs/2026-09-01-adb-native-progress-stream-design.md`。既有 `OperationProgress`、`TransferProgressReporter` 與 SFTP/local delivered-byte callback 保持不變。

## Goals / Non-Goals

**Goals:**

- 即時解析 ADB 自身輸出的 progress frames。
- 只發布單調、可證明的 cumulative bytes，再轉為 delta。
- 移除 remote stat 與 local tree scan polling。
- 保持 bounded pipe drain、取消、timeout、terminal 與 credential-safe diagnostics。

**Non-Goals:**

- 不實作 ADB sync wire protocol。
- 不承諾所有歷史／未來 ADB 文案格式都能解析；未知格式必須安全降級。
- 不修改公開 extension ABI、UI layout、速度或 ETA。

## Decisions

### D1 — Transfer runner 使用隱藏 pseudo-terminal

實機證據顯示 ADB 37.0.0 在 stdout/stderr 為 pipe 時只輸出 terminal summary；只有連接 terminal 時才輸出中間百分比。因此 `AdbCommandRunner` 增加有預設實作的 progress-capable 方法，production transfer runner 以 Windows ConPTY／pseudo-terminal 執行 ADB並增量讀取合併輸出。普通非傳輸命令繼續使用既有雙 pipe runner。callback panic 以 `catch_unwind` 隔離。

替代方案一是一般 pipe parser，已由實機 fixture 否證；替代方案二是自行實作 ADB sync protocol，超出核准範圍；替代方案三是恢復 polling，效能不符合目標。因此選擇只對 push/pull 使用隱藏 pseudo-terminal adapter。

### D2 — Stateful delimiter parser

parser 保存跨 read 的殘片，以 `\r`／`\n` 完成 frame。先解析明確 `completed/total` byte pair，再解析 0–100 百分比。parser 不依賴檔名或固定英文句子。checked arithmetic、倒退與越界 observation 不發布。

### D3 — Provider 映射與降級

已知來源大小時百分比映射為 cumulative bytes；只把大於上一 observation 的差額交給 reporter。成功退出補齊剩餘可靠 bytes，非零退出／取消不補齊。未知大小或無法證明跨檔 cumulative 時維持 indeterminate。

### D4 — 移除輪詢

ADB download 不再週期性呼叫 `local_tree_bytes`，upload 不再週期性呼叫 `metadata`。metadata 僅可在傳輸前做一次可靠 file-size preflight，不得作為 progress timer。

### D5 — Evidence-driven correction

- A：可調 parser tokenization、測試拆分、內部函式名稱，不改需求或 gate。
- B：ADB 真實輸出證明核准格式假設錯誤時，暫停受影響工作並同步修正 design/spec/tasks。
- C：改用 sync protocol、放寬安全 gate、修改公開 ABI 或增加外部操作需使用者核准。

## Risks / Trade-offs

- [ADB 版本改變輸出] → parser 容錯且失敗時 indeterminate，不恢復輪詢。
- [ConPTY 不可用或建立失敗] → 傳輸回退既有 pipe runner並顯示 indeterminate，不恢復輪詢。
- [output callback 阻塞] → callback 只更新 parser/counter；UI 節流留在既有 reporter。
- [目錄每檔百分比重設] → 不把倒退值當 operation cumulative；沒有 byte pair 時不顯示假百分比。
- [callback panic] → reader 隔離 panic並繼續 drain，確保 child 不因 pipe 填滿而卡死。
- [診斷記憶體] → stdout/stderr capture 各維持既有上限。

## Migration Plan

1. 加入 parser 與 pseudo-terminal runner streaming contract及 tests。
2. provider 切換到 native stream並刪除 polling。
3. 跑 emulator fixture與跨 provider matrix。
4. 若 regression，回滾 provider 使用 native callback 的 commit；不回復高頻 polling，未知進度暫顯示 indeterminate。

## Open Questions

無；真實 ADB 37.0.0 output 將作為 blocking fixture，而不是未決設計問題。
