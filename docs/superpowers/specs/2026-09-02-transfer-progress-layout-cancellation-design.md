# 傳輸進度版面與立即取消設計

## 目標

- 下方 operation surface 採水平配置：左側固定 250px 放置精簡 Cancel 控制，右側進度資訊填滿剩餘空間。
- active transfer 的進度文字與進度條最多每 200ms 更新一次，降低高頻 render 與 queue 壓力。
- Copy／Move 到 ADB、SFTP，以及 ADB↔SFTP staging 時，按下 Cancel 立即回應並停止可中斷的傳輸工作。

## 版面

Operation surface 保留完整摘要與既有 8 秒 terminal 淡出。active operation 的操作列拆成兩區：

1. 左側固定寬度 250px，放置不會撐滿整列的 Cancel 按鈕。按鈕維持 Close icon、可存取名稱與鍵盤操作。
2. 右側使用 flex-grow，顯示目前傳輸摘要，並以此區寬度作為 determinate 或 indeterminate progress bar 的基準。

Terminal operation 不顯示 Cancel；右側資訊可使用完整可用寬度。Partial failure rows 保留在主要操作列下方。

## 更新節奏

- delivered-byte 的一般 progress publication 以 200ms 為最短間隔。
- Preparing、total bytes 已知／未知切換、root item 切換、Finalizing、Finished、Partial、Failed、Cancelled 為強制邊界，立即發布。
- 不用人工 sleep 延遲傳輸；小檔仍可由 Preparing 直接到 Finished。
- UI 同時使用同一筆 operation record 的文字與 ratio，避免文字與進度條不同步。

## 立即取消

按下 Cancel 時，UI 立即 dispatch request-scoped cancellation，並立刻呈現「正在取消」。背景行為如下：

- Local streaming loop 在下一次 read/write 邊界停止。
- ADB push/pull 取消 token 後終止目前子程序／pipe，不能等待整個檔案傳完才返回。
- SFTP upload/download loop 在每個 chunk 與遞迴節點檢查 cancellation；可中止的 async operation 立即返回。
- ADB↔SFTP staging 在目前 stage 停止，且不得啟動下一 stage。
- Move 只有 destination 完整成功才允許 source cleanup；取消永遠不得刪除來源。
- provider callback、terminal 或 disconnect 晚到時，operation center 仍維持 exactly-one terminal 並拒絕 late progress。

若底層第三方 API 正位於不可中斷的單次系統呼叫，UI 仍須立即進入「正在取消」；該呼叫返回後不得開始下一個 chunk、項目或 stage。

## 錯誤與終止狀態

- Cancelled 保留最後真實 bytes/items，不跳到 100%。
- 取消與底層錯誤競合時，以第一個 request-correlated terminal 為準。
- Cancel action submission 失敗時顯示具體原因，不讓 UI 永久停留「正在取消」。
- Persistent diagnostics 不包含 SFTP credential。

## 驗證

- UI render structure：左側 250px、右側 flex-grow、progress bar 僅填滿右側。
- Reporter：一般 byte 更新間隔至少 200ms，強制 lifecycle boundary 不受節流。
- ADB：大型 push/pull 中途取消，子程序迅速退出、無 late progress。
- SFTP：大型 upload/download 中途取消，不再新增 remote bytes 或啟動下一項。
- ADB↔SFTP：第一 stage 與第二 stage 取消均不重啟、不刪來源。
- Cancelled/Failed/Partial 保留最後實值；Finished 才顯示 100%。
- 最後執行相關 crate 測試、真實 endpoint 驗證、`build_test_install.bat` 安裝與使用者視角 headful 檢查。

## 非目標

- 不加入 ETA、速度圖表或 pause/resume。
- 不改 operation center 的 8 秒淡出規則。
- 不修改 public extension ABI。
