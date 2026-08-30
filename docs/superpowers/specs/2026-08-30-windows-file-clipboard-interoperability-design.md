# Windows 檔案剪貼簿雙向互通設計

## 目標

SuperExplorer 與 Windows 原生檔案總管必須透過標準 Shell 檔案剪貼簿雙向互通：

- 在檔案總管複製或剪下檔案後，可貼到 SuperExplorer 的 local、ADB 與 SFTP 目錄。
- 在 SuperExplorer local 目錄複製或剪下檔案後，可貼到檔案總管或其他支援標準 Windows 檔案剪貼簿的程式。
- 文字、圖片、HTML 等非檔案剪貼簿內容不得被誤判為檔案，也不得被檔案操作覆寫或消耗。

## 採用方案

沿用既有 Windows Shell STA 與 OLE `IDataObject` 邊界，以 `CF_HDROP` 表示本機檔案清單，並用 Shell 的 Preferred DropEffect 區分複製與剪下。應用程式不建立第二套互不相容的 Windows 檔案剪貼簿協定。

相較於只在使用者按下貼上時臨時讀取系統剪貼簿，此方案可讓工具列、快捷鍵與右鍵選單持續取得正確的貼上可用狀態。相較於全面導入延遲渲染或虛擬檔案格式，本方案直接滿足目前的本機檔案互通需求，且不擴張遠端項目直接暴露給外部程式的範圍。

## 元件責任

### Windows Shell 剪貼簿介面

- 只在 Shell STA 讀寫 OLE 剪貼簿。
- 監看 clipboard sequence，辨識剪貼簿所有權是否已由外部程式取代。
- 外部剪貼簿包含 `CF_HDROP` 時，解析實體本機路徑與 Preferred DropEffect。
- SuperExplorer local 複製或剪下時，發布標準 Shell `IDataObject`，包含 `CF_HDROP` 與 Preferred DropEffect。
- 不解析、不清除且不重新發布純文字、圖片或 HTML 格式。

### 應用程式剪貼簿狀態

- 將外部標準檔案剪貼簿表示為可貼上的 external 狀態，並保留 copy/cut 模式與項目數量。
- 系統剪貼簿改成非檔案內容時，清除檔案貼上能力，但不更動系統剪貼簿內容。
- UI 的貼上啟用狀態以最新剪貼簿狀態為準；真正提交貼上時再次讀取並驗證原始 OLE 資料物件，以避免陳舊狀態。

### 貼上路由

- local 目的地：交給現有 Windows Shell file operation。
- ADB 或 SFTP 目的地：將外部 `CF_HDROP` 的本機來源交給既有 remote transfer service；服務透過本地暫存與既有 provider 上傳流程完成操作。
- copy 成功後保留剪貼簿；cut 只有在來源移除與目的地建立都成功後才完成移動語意。部分失敗沿用詳細檔案操作結果，不宣告整批成功。

## 鍵盤與編輯欄位隔離

- 檔案檢視取得焦點且有本機項目選取時，`Ctrl+C`／`Ctrl+X` 執行檔案剪貼簿操作。
- 位址列、搜尋欄、重新命名輸入框或其他文字編輯器取得焦點時，快捷鍵維持文字剪貼簿語意。
- `Ctrl+V` 只有在檔案檢視為操作表面且系統剪貼簿包含可傳輸檔案時才執行檔案貼上。

## 錯誤處理

- 剪貼簿暫時忙碌時使用現有有界重試，不阻塞 UI 執行緒。
- 不支援或無效的 Shell 資料物件回報為不可貼上的剪貼簿狀態，不造成崩潰。
- 來源遺失、權限不足、ADB/SFTP 上傳失敗或本地暫存失敗時，狀態列顯示來源、目的地、操作類型及底層詳細原因。
- 不在日誌或狀態列輸出 SFTP 密碼或其他認證資訊。

## 驗證

完整檢查集中於實作完成後：

1. 聚焦測試 Shell clipboard 的 `CF_HDROP` 讀寫、Preferred DropEffect、外部 sequence 變更與非檔案格式隔離。
2. 聚焦測試 UI 在檔案檢視與文字輸入焦點下的 `Ctrl+C`／`Ctrl+V` 路由。
3. Headful 驗證檔案總管複製至 SuperExplorer local、ADB、SFTP。
4. Headful 驗證 SuperExplorer local 複製後可在檔案總管貼上。
5. 確認純文字與圖片剪貼簿不啟用檔案貼上，也不被清除。

## 不在本次範圍

- 將 ADB 或 SFTP 遠端項目直接以 Windows 虛擬檔案資料物件發布給外部程式。
- 新增檔案總管未採用的私有剪貼簿格式作為主要互通協定。
- 改寫既有衝突處理 UI 或遠端認證流程。
