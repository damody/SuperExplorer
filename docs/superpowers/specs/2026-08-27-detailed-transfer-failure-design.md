# 詳細傳輸失敗訊息設計

## 目標

當 Local、ADB、SFTP 之間的複製或移動失敗時，操作訊息列必須呈現足以定位問題的實際原因，不能只顯示泛用的 `A file could not be transferred.`。每筆失敗須能辨識來源、目的地、失敗階段與底層診斷，同時不得洩漏密碼或登入憑證。

## 範圍

- 涵蓋 Local、ADB、SFTP 彼此之間，經由本機暫存區執行的複製與移動。
- 改善單筆失敗、全部失敗及部分成功時的 OperationCenter 明細。
- 保留既有八秒訊息生命週期、進度、取消與最多五筆 partial outcome 顯示行為。
- 不改動成功傳輸行為、衝突處理策略或完整檔案操作架構。

## 診斷資料流

遠端傳輸層在每個項目失敗時，建立安全且具情境的診斷文字，內容依可用資訊包含：

1. 來源的 canonical Local／ADB／SFTP 路徑。
2. 目的地的 canonical 路徑，並在可推導時包含目標檔名。
3. 失敗階段，例如來源下載至暫存區、ADB push、SFTP upload、遠端重新命名或移動完成後刪除來源。
4. 底層 provider 回傳的錯誤或 stderr；若只有通用錯誤，仍保留原始 diagnostic。
5. 原生錯誤碼或 provider exit status（若存在）。

`TransferResult::Failed` 的 diagnostic 不再於轉換為 `OperationItemResult::Failed` 時被固定英文覆蓋。錯誤物件的 `user_message` 改為上述安全摘要，OperationCenter 直接顯示它。

## 顯示格式

終止摘要維持目前的操作總覽，例如：

`複製 3 個項目｜來源由系統剪貼簿提供 → adb://emulator-5554/sdcard/Download｜部分完成：0/3 成功`

其下每筆失敗改為具體內容，例如：

`失敗｜C:\Downloads\a.zip → adb://emulator-5554/sdcard/Download/a.zip｜ADB 上傳｜device offline`

當多筆項目因不同原因失敗時，各列顯示各自的路徑與原因。若結果數量超過既有五筆明細上限，維持目前上限，避免訊息區無限增高。

## 安全與降級

- SFTP URI 只使用 canonical authority 與路徑，禁止包含密碼、credential store token 或完整登入物件。
- 清理底層診斷中可能出現的密碼及 userinfo；無法安全清理的片段改成 `[已隱藏]`。
- 暫存目錄僅可作為失敗階段的內部資訊，不取代使用者看到的邏輯來源與目的地。
- 若來源、目的地或底層原因無法取得，分別顯示可用的項目名稱、目的資料夾及明確的「未提供底層錯誤」，不可退回原本固定英文。

## 驗證

- 純函式測試：Local／ADB／SFTP 路徑、不同傳輸階段、底層 diagnostic 與敏感資訊清理。
- 服務測試：`TransferResult::Failed` 轉換後保留安全詳細原因，且多項失敗互不覆蓋。
- UI 聚焦測試：失敗列包含來源、目的、階段與原因。
- 執行相關 crate 編譯檢查、格式與 diff check；依使用者要求不執行完整迴歸測試。
