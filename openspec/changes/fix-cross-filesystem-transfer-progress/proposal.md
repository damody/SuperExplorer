## Why

Local、ADB 與 SFTP 之間的傳輸目前只發布開始與 terminal 狀態，導致下方進度列長時間停在 0% 後直接跳到 100%。使用者無法判斷大型檔案或遞迴資料夾是否仍在傳輸，也無法從部分失敗或取消看出實際已完成的工作量。遠端連線與metadata preflight也可能在第一個可見狀態前阻塞數秒，使小檔案拖放看起來像完全沒有接受操作。

## What Changes

- 擴充核心 file-operation progress contract，加入真實 completed/total bytes、phase 與未知總量狀態。
- 建立 request-scoped、單調、節流且 terminal-safe 的 transfer progress reporter。
- 將實際 byte delta 接入 Local、ADB、SFTP 的 upload/download/copy stream。
- 將 ADB↔SFTP 等 staging 兩階段工作合併成單一加權進度，不在階段切換時重設。
- 讓下方進度列正確呈現確定百分比、不確定進度、目前項目、取消、部分成功與失敗。
- 在操作提交後300ms內顯示「準備複製／移動」，不等待provider preflight；小檔完成後顯示明確「複製完成／移動完成」。
- 補齊六個跨端方向、檔案／資料夾、未知大小、取消與錯誤的聚焦及 headful 證據。
- 不加入速度圖表、ETA，也不重做 operation center 外觀。

## Capabilities

### New Capabilities

- `cross-filesystem-transfer-progress`: 定義 Local、ADB、SFTP 任意兩端傳輸的真實 byte/item progress、staging 加權、UI 與 terminal 行為。

### Modified Capabilities

無。

## Impact

- `explorer-model` 的 `OperationProgress` domain contract。
- `explorer-remote` provider／transfer engine 的 streaming 與 metadata preflight。
- `explorer-app` remote routing、staging、事件合併與 terminal barrier。
- `explorer-shell-win` Local file-operation progress adapter（僅在需要統一 contract 時調整）。
- `explorer-ui` operation record、status message 與 progress bar。
- 既有 public extension ABI 不在本次範圍；若內部 model 欄位新增，所有 workspace constructors/tests 必須同步更新。
