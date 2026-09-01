# SFTP 與 Windows 檔案總管雙向拖放互通設計

## 目標

SuperExplorer 的 SFTP 檔案檢視必須能與 Windows 原生檔案總管雙向拖放：

- 從檔案總管拖入 `sftp://45.32.49.125/home/linuxuser` 或其他可寫 SFTP 目錄時，上傳檔案與資料夾。
- 從 SuperExplorer SFTP 拖到檔案總管時，讓 Explorer 收到標準 Windows 檔案拖放資料並建立本機項目。
- 跨檔案系統預設為 Copy；明確按住 Shift 時才提出 Move。
- Move 只有在目的端完整建立成功後才刪除對應來源，取消與失敗不得刪除來源。

## 採用方案

沿用既有 OLE 拖放邊界與 remote transfer service。檔案總管到 SFTP 解析外部 `CF_HDROP` 後直接上傳；SFTP 到檔案總管先把遠端項目下載至受控本機暫存目錄，再以標準 Shell `IDataObject` 啟動 `DoDragDrop`。

不以 Copy/Paste 模擬拖放，因為那會失去原生游標效果、取消、drop target 與鍵盤修飾鍵語意。本次也不實作 `FileGroupDescriptor`/`FileContents` 延遲串流虛擬檔案，因為其 COM data object、非同步內容供應與 Explorer 相容性範圍顯著更大。

## 資料流

### Windows 檔案總管到 SFTP

1. GPUI/Windows drop target 在 DragEnter、DragOver 與 Drop 期間接受有效的檔案 `CF_HDROP`。
2. UI 將來源本機路徑、目的 SFTP 位置、按鍵狀態與協商後 effect 轉成 `DataTransferRequest::DropExternal`。
3. Remote service 將來源描述轉成本機 `ItemDescriptor`，使用現有 provider upload 與 staging lifecycle 寫入 SFTP。
4. 預設 effect 為 Copy。只有明確 Shift 且 drop target 接受 Move 時使用 Move。
5. Move 逐項在上傳成功後移除來源；部分失敗只移除已成功項目，失敗項目保留。

### SFTP 到 Windows 檔案總管

1. UI 對 SFTP 選取項目送出 `BeginDrag`，包含允許 effects 與滑鼠按鍵。
2. Remote service 建立唯一暫存根目錄，保持原始檔名與資料夾樹，並在背景完整下載選取項目。
3. 下載成功後，把 staging 的本機項目委派給 Shell STA；Shell 建立標準 `IDataObject`，設定 Preferred DropEffect，並執行原生 `DoDragDrop`。
4. 暫存目錄從 OLE 拖曳開始前一直存活到 `DoDragDrop` 終止、Explorer 完成同步讀取且 terminal event 已處理。
5. OLE 回報 Copy 時保留 SFTP 來源；回報 Move 且目的端成功時才透過 provider 刪除遠端來源。

## Effect 與安全語意

- 無修飾鍵：Copy。
- Shift：提出 Move；來源刪除仍以實際 performed effect 與成功 terminal 為準。
- Ctrl：Copy。
- 不支援 Link，若 Explorer 或 UI 提出 Link 則降級為 Copy 或拒絕，絕不當作 Move。
- 取消、`DROPEFFECT_NONE`、OLE 錯誤、下載失敗或目的端拒絕都不得刪除來源。
- 資料夾採遞迴傳輸，且根名稱只建立一次；不得把暫存根目錄本身暴露為多餘層級。

## 暫存生命週期

- 每次拖曳使用獨立、不可預測的暫存根目錄。
- staging ownership 綁定 request ID，不能被其他拖曳或導覽覆蓋。
- `DoDragDrop` 完成前不得清理。
- 成功、取消與失敗都必須進入可驗證的清理終點。
- 應用程式關閉時清理仍由 runtime ownership/drop guard 負責。

## 錯誤與可觀察性

- 詳細錯誤包含操作、來源、目的地、provider/OLE 原因與逐項結果。
- 遠端密碼、私鑰與 credential token 不得出現在狀態列、日誌或測試證據。
- 取消是正常 terminal，不記為 Internal 錯誤。
- UI 必須在 DragEnter/Drop 未進入路由時留下可診斷事件，避免「滑動有事件但放下沒事件」再次發生。

## 驗證

所有完整檢查集中在實作完成後：

1. 聚焦測試 external drop effect 協商、SFTP upload、部分失敗與 Move 刪除條件。
2. 聚焦測試 remote staging、標準 `CF_HDROP` data object、`DoDragDrop` terminal 與清理。
3. Headful 實測檔案總管拖入 `sftp://45.32.49.125/home/linuxuser`。
4. Headful 實測同一路徑的檔案拖到檔案總管。
5. 驗證預設 Copy、Shift Move、取消拖曳與來源保留。

## 不在本次範圍

- Windows 虛擬檔案延遲串流格式。
- 非檔案的文字、圖片或 URL 拖放。
- ADB 拖放行為的額外功能擴張；共用修正若自然同時改善 ADB，仍須避免回歸。
