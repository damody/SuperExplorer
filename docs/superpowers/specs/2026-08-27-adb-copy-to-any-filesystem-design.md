# ADB 複製到任意可寫檔案系統設計

## 目標

使用者在 ADB 位置複製檔案或資料夾後，可以在目前的 Local、SFTP、ADB 或其他已註冊且可寫入的檔案系統資料夾貼上。右鍵命中背景、檔案或資料夾時，貼上目的地都固定為目前瀏覽的資料夾。

## 架構

沿用現有 application-owned clipboard、provider registry 與 `TransferEngine`，不建立 ADB 專用貼上分支。來源與目的地皆使用 `LocationDescriptor`；Local 與遠端互傳分別呼叫 download 或 upload，遠端與遠端則由來源 provider 下載至每次操作獨立的受限暫存目錄，再交由目的 provider 上傳。

選單可用性由有效剪貼簿、目前位置可寫入以及 provider 能力共同決定，不依賴顯示字串或只列舉 ADB／SFTP。未知或缺少必要能力的 provider 預設拒絕。

## 資料流

1. ADB Copy 同步建立 internal clipboard，保留來源的完整 typed descriptor。
2. 使用者在目的地執行 Paste；目的地取自 active tab current location。
3. `RemoteExplorerService` 優先讀取 internal clipboard，不等待 Windows `CF_HDROP` staging。
4. `TransferEngine` 依來源與目的地類型選擇 local copy、download、upload 或 download→staging→upload。
5. 每個來源回報成功、略過、取消、部分完成或失敗；Cut 只有在目的寫入成功後才刪除來源。

## 錯誤與安全

- 保留既有衝突決策、取消、deadline、暫存容量與路徑 containment 檢查。
- 下載或上傳失敗時回報真實單項結果；暫存目錄由作用域生命週期清理。
- 唯讀位置、無效剪貼簿、找不到 provider 或缺少 upload/download 能力時不提供可用 Paste 或明確失敗。
- 不改變外部 Windows 剪貼簿格式、憑證處理或 provider API。

## 測試

- ADB→Local：確認檔案內容寫入目前本機資料夾。
- ADB→SFTP：以兩個可觀察 fake provider 確認來源 download、目的 upload、內容與目的 descriptor。
- ADB→另一遠端 provider：證明 routing 依 provider registry，而不是 SFTP 特例。
- 右鍵背景、檔案、資料夾都產生相同目前資料夾目的地。
- 唯讀、無效 clipboard、download/upload 失敗與衝突情境保持既有語意，且不留下非作用域暫存資料。

## 非目標

- 不新增遠端 provider 或遠端直接串流介面。
- 不變更 Move/Cut 的刪除承諾。
- 不提供跨程序的遠端 provider descriptor 給其他應用程式。
