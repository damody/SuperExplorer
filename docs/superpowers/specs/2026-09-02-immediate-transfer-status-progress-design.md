# 檔案操作立即狀態與真實進度設計

## 問題

目前遠端複製會在第一個可見進度事件前同步執行連線、metadata與總大小估算。Windows Explorer拖入`an.txt`至ADB或SFTP時，UI可能沉默約5秒，完成後才突然刷新，使用者無法判斷操作是否已接受或程式是否卡住。

## 核准行為

拖放或貼上成功提交Copy／Move後，狀態列必須立即建立同一筆operation並顯示操作、來源及目的。正常負載下最遲300ms內可見，不等待provider連線、metadata preflight或第一個byte callback。

狀態依序為：

1. `準備複製`／`準備移動`：request已接受，正在連線、列舉、估算或建立目的。
2. `正在複製`／`正在移動`：第一個實際delivered-byte callback後顯示。已知total顯示百分比與bytes；未知total顯示indeterminate bar及已傳輸bytes。
3. `複製完成`／`移動完成`：provider terminal成功後顯示，依既有8秒規則淡出。
4. 失敗、部分完成或取消：顯示具體原因與最後實值，不跳到100%。

小檔案即使在下一個render frame前完成，也必須讓使用者看到「準備複製」及「複製完成」；不強迫虛構中間百分比。大檔案必須在terminal前顯示至少一個1–99%的真實中間進度或未知total的持續bytes更新。

## 架構

- UI在提交具有效request context的file operation時立即插入`OperationRecord`，而非等待service第一個progress事件。
- Remote service在任何可能阻塞的metadata estimator前強制發布`Preparing` progress；估算移到可產生進度的operation生命週期內。
- `TransferProgressReporter`保有單調、節流及terminal barrier，但Preparing事件必須force emit；第一個byte delta切換至Transferring。
- Local、ADB、SFTP與跨provider staging共用`OperationProgress`契約，不加入檔案類型或provider UI特例。
- terminal更新同一request id的operation record，避免先沉默再出現另一筆完成訊息。

## 顯示與時序

- Operation center在record非terminal時始終顯示摘要；Preparing採indeterminate bar。
- 已知total且Transferring時顯示0–99%真實比例，只有Finished terminal可顯示100%。
- Finished文字使用明確動詞，例如`複製完成｜an.txt → adb://...`，而不是只有泛用`完成`。
- UI不得為了強制顯示Preparing而延遲真正傳輸；可見性以事件先發布及render測試保證，不加入人工sleep。

## 錯誤與安全

submit failure必須將已建立record轉成Failed，而不是留下永久Preparing。取消、panic、provider disconnect及partial均須exactly-one terminal；terminal後late progress被拒。狀態文字不得包含SFTP credential、URI userinfo或敏感來源內容。

## 驗證

- 使用慢速preflight fake驗證request提交後300ms內已有Preparing record。
- 使用小檔fixture驗證Preparing→Finished，同一request且無人工延遲。
- 使用大檔fixture驗證ADB與SFTP在terminal前至少有一個真實1–99% byte progress。
- 驗證Local↔ADB、Local↔SFTP、ADB↔SFTP與拖放／貼上共用狀態機。
- 驗證失敗／取消保留最後實值且無late progress。
- 最後只執行受影響聚焦測試與指定實機驗收，不做完整迴歸。
