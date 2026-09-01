# Dotfile JSON 外部拖放修正設計

## 目標

修正 Windows 原生檔案總管無法將 `D:\SuperExplorer\.tmp-full-meta.json` 拖入以下 SuperExplorer 遠端目錄的問題：

- `adb://emulator-5554/sdcard/Download`
- `sftp://45.32.49.125/home/linuxuser`

修正必須適用於一般本機檔案，不能以 `.json` 副檔名建立特例。來源檔名以 `.` 開頭時，遠端目的名稱仍須完整保留為 `.tmp-full-meta.json`。

## 已知條件

- 指定來源是存在的普通檔案，大小為 34,629 bytes，Windows 屬性只有 `Archive`。
- 既有實機矩陣驗證過一般 `.txt` 檔與資料夾，但未涵蓋 dotfile。
- 既有修正已讓 Windows OLE 的合成 `MouseUp` 傳遞至 bubble-phase drop target；本次須確認 dotfile 是否在 OLE 解碼、目的命中、來源驗證或遠端傳輸階段被拒絕。

## 方案選擇

採用以真實指定檔案驅動的端到端診斷：保留來源檔不變，逐層觀察外部拖放事件與傳輸結果，修正第一個有實證的共用失敗點。

不採用下列方案：

- 放寬所有來源驗證：可能讓虛擬 Shell 項目或不存在的來源進入傳輸層。
- 對 `.json` 或 dotfile 寫特判：會掩蓋相同的路徑或檔名處理缺陷。

## 資料流與責任邊界

1. Windows Explorer 透過 OLE `FileDrop` 提供完整本機來源路徑。
2. UI drop target 保留來源路徑與檔名，解析目前背景或資料夾列目的地。
3. 狀態層只拒絕空集合、不存在來源、不支援來源或不可寫目的地；不得因檔名以 `.` 開頭而拒絕。
4. 遠端服務將本機檔案送往 ADB 或 SFTP，目的 basename 必須原樣保留。
5. 結果以遠端存在、長度 34,629 bytes 與內容雜湊一致為準。

每一層只負責自己的轉換與驗證；不得在 UI 層加入遠端協定專屬分支。

## 錯誤處理

- OLE 沒有提供實體路徑時，維持 fail-closed，並記錄拒絕階段與原因。
- 目的地不可寫或來源不存在時，顯示包含來源與目的路徑的詳細失敗訊息。
- ADB/SFTP 傳輸失敗時，保留協定回傳的可行動錯誤，不折疊成泛用 `Internal`。
- 測試清理只處理本次受控目的路徑下的精確 fixture 名稱，不做廣泛遞迴刪除。

## 驗證策略

### 自動化回歸

- 單元測試驗證 `.tmp-full-meta.json` 被接受並保持 basename。
- 現有一般檔案、資料夾與 copy/move 語意測試必須維持通過。

### 實機驗證

- 從 Windows Explorer 將同一個 `D:\SuperExplorer\.tmp-full-meta.json` 左鍵拖入 ADB 與 SFTP 指定目錄。
- 確認 UI 產生 `DropExternal`，傳輸完成後遠端檔名完整。
- 以遠端 stat/read 驗證大小與內容雜湊等於本機來源。
- 驗證完成後刪除兩個遠端受控副本，本機來源檔保持不變。

### 最終集中檢查

- 執行受影響 crate 的聚焦測試與 `cargo check`。
- 執行格式、差異、敏感資訊與 OpenSpec strict 檢查。
- 任一檢查失敗時修正後重跑，直到全部通過。

## 非目標

- 不重構整個拖放或傳輸架構。
- 不變更剪貼簿、右鍵選單或遠端登入流程。
- 不執行完整專案回歸測試。
