# Code Lines Limit 狀態與完整原因 Tooltip 設計

## 目標

Code Lines 類型欄位因資料夾准入限制而未啟動時，儲存格只顯示簡短、醒目的紅色 `Limit`。使用者將滑鼠移到文字或儲存格上時，tooltip 顯示該次未啟動的完整原因。

## 狀態呈現

- `Pending` 維持既有「等待 File Count…」文字與一般狀態色彩。
- `Unavailable` 顯示紅色 `Limit`，tooltip 為「依賴 File Count，因此未啟動」。
- `OverLimit` 顯示紅色 `Limit`，tooltip 為「File Count 超過限制，因此未啟動」。
- 無障礙名稱保留欄位名稱與完整原因，不能只朗讀 `Limit`。

## 元件邊界

`FolderAdmissionStateV1` 分別提供簡短顯示文字、完整原因與是否為限制狀態。Details 欄位 cell renderer 負責套用紅色語意色彩、aria label 與 GPUI hover tooltip；准入判斷與 MFT 查詢流程不變。

## Tooltip 行為

使用 GPUI 既有 tooltip 生命週期，tooltip 內容採目前的 UI tokens 與 tooltip typography。滑鼠離開後由框架關閉，不新增全域狀態或自製計時器。

## 驗證

- 單元測試驗證三種准入狀態的短標籤、完整原因與限制旗標。
- renderer 測試或契約檢查驗證限制狀態使用警示色、aria label 使用完整原因且 cell 安裝 tooltip。
- 既有 File Count 可見性、上限判斷與 Code Lines dispatch 測試必須維持通過。

## 非目標

- 不變更 File Count／Folder Count 查詢與計算方式。
- 不新增本地化資源。
- 不改變一般 extension 錯誤或 Pending 狀態的呈現。
