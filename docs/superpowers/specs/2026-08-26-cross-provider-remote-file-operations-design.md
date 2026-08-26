# Local、ADB、SFTP 跨 Provider 檔案操作設計

## 目標

SuperExplorer 必須在 Local、ADB 與 SFTP 位置提供一致的新增資料夾、刪除、複製、剪下、貼上及拖放行為。支援的方向包含 Local ↔ ADB、Local ↔ SFTP，以及 ADB ↔ SFTP。遠端與遠端之間透過每次操作獨立、可自動清除的本機暫存目錄中轉。

目標路徑包括但不限於 `sftp://45.32.49.125/home/linuxuser` 與 `adb://emulator-5554/sdcard/Download`。實作依位置描述子的 provider 身分判斷能力，不針對上述字串寫死行為。

## 使用者操作

- 在可寫入的 ADB／SFTP 目錄背景按右鍵時，顯示「新增資料夾」與「貼上」。
- 對 ADB／SFTP 項目按右鍵時，顯示「刪除」、「複製」與「剪下」。
- `Ctrl+C`、`Ctrl+X`、`Ctrl+V` 與右鍵命令使用相同的 typed clipboard 與操作管線。
- Local、ADB、SFTP 之間的應用程式內拖放使用相同 Transfer Engine。
- 與原生 Windows 檔案總管互動時，本機來源可拖入 ADB／SFTP；ADB／SFTP 項目拖出時，先匯出至受控暫存區，再交給 Shell data object。
- Remote 刪除顯示永久刪除確認。Local 刪除繼續使用 Windows 資源回收筒語意。

## 架構

### 位置與能力

操作資格由 `LocationDescriptor`、`NamespaceCapabilities` 與 provider registry 決定，不解析顯示文字或網址字串。ADB 與 SFTP provider 宣告 create-directory、delete、download、upload 與 rename 能力。UI 僅在目前目錄及選取項目具備相應能力時顯示或啟用命令。

### Typed clipboard

應用程式 clipboard 保存 Copy／Cut 模式及一組具型別的來源位置。遠端來源不轉換成虛構本機路徑。文字與圖片剪貼簿仍由既有原生資料格式處理；只有可辨識的檔案傳輸格式或 SuperExplorer typed clipboard 才會觸發檔案貼上，因此不與複製文字或圖片衝突。

### 統一 Transfer Engine

所有右鍵、鍵盤及應用程式內拖放最終建立同一種資料傳輸請求。Transfer Engine 依來源／目的組合選擇管線：

- Local → Local：保留 Windows Shell 檔案操作。
- Local → ADB／SFTP：provider upload。
- ADB／SFTP → Local：provider download。
- ADB／SFTP → ADB／SFTP：來源 provider download 至 scoped staging，再由目的 provider upload。

Transfer Engine 必須支援檔案與資料夾。資料夾以 bounded traversal 遞迴建立目的子目錄並傳輸檔案，保留來源相對結構，不追蹤會造成循環的符號連結目標。

### 移動一致性

Cut／Move 使用 copy-then-delete。只有某一來源項目的完整目的樹成功建立後，才刪除該來源。若目的寫入成功但來源刪除失敗，結果為 Partial，保留來源並明確回報；不得假稱移動成功。取消時不得開始尚未必要的來源刪除。

### 暫存生命週期

每個跨遠端操作建立獨立的 `tempfile::TempDir`，名稱不含使用者名稱、主機、裝置序號或遠端路徑。操作完成、取消或失敗時由 RAII 清除。暫存根目錄與目的路徑必須經過邊界檢查，禁止使用廣泛目錄或未驗證的相對路徑作為清除目標。

拖出至 Windows Explorer 的暫存資料需要延長至 Shell 完成資料消費；其 lease 由 data object／drag session 擁有，結束後清除，不使用永久快取。

## Provider 操作

ADB 使用既有 client 的 shell／push／pull 能力：建立資料夾採安全參數編碼的 `mkdir`，刪除依項目類型選擇遞迴語意，傳輸保持取消與 deadline。SFTP 使用既有 session 的 mkdir、unlink／rmdir、upload／download。兩者都必須拒絕無效名稱、父路徑跳脫及 provider／authority 不一致的來源或目的。

重新命名可供同 provider 的快速移動使用，但跨 provider 與需要一致錯誤語意的 Cut 仍以統一 copy-then-delete 為權威路徑。

## 衝突與錯誤

- 同名目的項目沿用目前 FileOperation 的衝突決策，不靜默覆寫。
- 每個來源項目產生獨立結果：Succeeded、Partial、Failed 或 Cancelled。
- 資料夾部分上傳失敗時不得刪除來源；已建立的目的資料保留並回報實際狀態，避免以不安全的遞迴 rollback 刪除使用者既有資料。
- 操作完成後重新整理受影響的來源及目的分頁；既有 generation／cancellation 機制拒絕過期結果。
- 診斷不得包含 SFTP 密碼、完整私密遠端內容或暫存檔案內容。

## UI 與原生互動

右鍵命令、工具列、快捷鍵與拖放均查詢同一個 capability predicate。背景選單和項目選單分開判斷，以避免在不可寫目錄顯示新增／貼上。Remote 永久刪除確認需顯示項目數量與不可復原提示。

Windows Explorer 拖入時讀取檔案 data object 並轉為 Local typed sources。拖出時建立延遲可用的本機 staged representation；若 staging 失敗，取消 drag 並顯示單一操作錯誤，不把不完整路徑交給 Explorer。

## 驗證

實作完成後一次執行聚焦檢查，不在每一步跑完整測試：

- Provider 測試：ADB／SFTP 建立資料夾、檔案／資料夾刪除、取消、無效路徑。
- Transfer 測試：六種跨邊界方向、遞迴資料夾、Copy、Cut、Partial、取消、staging 清理。
- UI／state 測試：背景與項目右鍵能力、Ctrl+C/X/V、Remote 永久刪除確認、文字／圖片剪貼簿互不干擾。
- Drag/drop 測試：Explorer 拖入／拖出 lease 與 Local／ADB／SFTP 應用程式內拖放。
- 相關 crates 編譯與 OpenSpec strict validation。

不執行完整 workspace 迴歸測試。
