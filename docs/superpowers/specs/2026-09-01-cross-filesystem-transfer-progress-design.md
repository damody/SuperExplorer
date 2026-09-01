# Local、ADB、SFTP 真實傳輸進度設計

## 目標

SuperExplorer 在 Local、ADB、SFTP 任意兩端之間複製或移動檔案與資料夾時，必須依實際傳輸的位元組與項目完成狀態持續更新下方進度列。進度不得只停在 0% 後直接跳到 100%，也不得用經過時間偽造百分比。

## 範圍

涵蓋 Local→Local、Local↔ADB、Local↔SFTP、ADB↔SFTP，以及同 provider 的 ADB／SFTP 跨路徑傳輸。檔案與遞迴資料夾、Copy、Move、取消、失敗與部分成功都使用相同進度契約。跨遠端傳輸仍可經本機 request-scoped staging，但 UI 顯示為單一連續 operation。

不在本次範圍內重做操作中心外觀、加入速度歷史圖表或剩餘時間預估，也不執行完整產品迴歸測試。

## Domain contract

既有 `OperationProgress` 保留項目數欄位，並加入 byte progress：

- `completed_items`／`total_items`：已完成根項目與總根項目。
- `completed_bytes`：已成功讀取並交付至目前 pipeline 階段的實際位元組。
- `total_bytes: Option<u64>`：可預掃描時提供總量；無法可靠取得時為 `None`。
- `phase`：Preparing、Transferring、Finalizing，用於說明目前狀態，不用來偽造百分比。
- `current_item`：目前處理項目的可公開位置或名稱。

確定型百分比只在 `total_bytes > 0` 時由 bytes 計算，並限制在 terminal 前最多 99%。零位元組 operation 以項目完成度顯示。未知總量使用不確定進度條與已傳輸 byte 數，不顯示虛假百分比。完整 Finished terminal 才設定 100%；Cancelled、Failed、Partial 保留最後真實值。

## Progress reporter

新增 request-scoped `TransferProgressReporter`，由 transfer engine 與 provider adapter 共用。它負責：

- 原子累計 byte 與 item delta，拒絕倒退及 overflow。
- 將高頻 stream callback 節流／合併，避免塞滿 bounded progress lane。
- 每次輸出攜帶原 request context，讓既有 generation 與 terminal gate 排除 stale／late progress。
- terminal 關閉後不再發布更新。
- 不在 diagnostic 中記錄 credential 或 private authority。

Reporter 不依賴 GPUI、Shell COM 或特定 provider，因此 Local、ADB 與 SFTP 可使用相同契約。

## 大小預掃描

operation 開始時，以 provider metadata 遞迴計算來源樹：

- 可讀取且有可靠大小的檔案加入 `total_bytes`。
- 空檔與空資料夾仍加入 item 工作量，但不增加 bytes。
- 任一來源無法取得可靠大小時，整體 `total_bytes` 為未知，避免顯示錯誤分母。
- 預掃描可取消；取消時不得開始後續 destructive Move cleanup。
- 實際讀取量超過預掃描值時，降級為未知總量，百分比不得倒退。

## 各 provider 串流

Local stream 在每次成功 read/write 後回報實際交付 bytes。ADB push/pull 與 SFTP upload/download 的 copy loop 也在成功寫入目的端後回報 delta，不以檔案宣告大小取代實際傳輸量。

同 provider 若使用 server-side rename 或 provider-native copy，只有在 provider 能回報真實 byte progress 時才顯示確定百分比；否則使用項目／不確定進度，完成前不得假裝已傳輸全部 bytes。

## 跨遠端兩階段傳輸

ADB↔SFTP 或不同遠端 session 透過 staging 時，下載與上傳形成一個 operation：

- 已知來源總量為 `N` 時，總工作量為 `2N`，下載佔前半、上傳佔後半。
- 每個階段只計入成功交付的 bytes，因此 UI 單調由 0% 前進至 100%，不在 staging 邊界重設。
- staging 建立、finalize 與 Move cleanup 使用 phase／item progress，不額外偽造 byte delta。
- 第二階段失敗時保留未完成來源；只有目的端成功的項目才允許 Move cleanup。

## UI 行為

下方 operation message 使用 byte progress 優先：顯示操作、完整來源／目的、目前項目、已傳輸／總 bytes 與百分比。未知總量顯示已傳輸 bytes 與不確定進度條。多項 operation 同時保留項目數摘要。進度更新沿用既有 bounded/coalesced event 路徑，不能阻塞 UI thread。

## 錯誤與取消

每個 provider stage 的錯誤保留來源、目的、phase 與 provider 原因，但經 credential redaction。取消立即關閉 reporter，停止新工作並拒絕 late progress。Partial terminal 列出成功、失敗與未嘗試項目；進度列不得被 terminal 強制改為 100%。

## 驗證

聚焦測試涵蓋 reporter 單調性、節流、未知總量、大小超出、terminal barrier、取消與 partial。整合測試涵蓋 Local、ADB、SFTP 六個跨端方向、單一大檔、遞迴資料夾與兩階段 staging。Headful 測試使用可觀察的大型 fixture，證明 operation 在完成前至少產生一個介於 0% 與 100% 的進度，並驗證完成、取消和錯誤 UI。最後集中執行格式、相關 crate 編譯、聚焦測試、實際 ADB／SFTP 傳輸、diff check 與 OpenSpec strict validation。
