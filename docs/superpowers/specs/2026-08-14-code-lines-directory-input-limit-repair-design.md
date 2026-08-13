# Code Lines 資料夾輸入上限修正設計

## 問題

Host 目前用 `MAX_BATCH_COLUMN_INPUT_BYTES_V1`（64 MiB）限制資料夾封包，卻在建立單一 `HostInputStreamSourceV1` 時套用較小的 `MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1`（8 MiB）。資料夾介於兩者之間時會通過讀取階段，但在 dispatch 準備階段失敗，整批列都顯示 `Code lines input could not be prepared`。

此外，資料夾封包會先讀取所有一般檔案，再由 provider 判斷是否為支援的原始碼。這會把 `.git` 物件、圖片、安裝程式及其他二進位內容計入封包預算，讓實際原始碼不大的專案也容易超過限制。

## 決策

Host 在建立資料夾封包前，使用與官方 Rust Code Lines provider 相同的 `tokei::LanguageType::from_path` 判斷，只讀取並封裝可辨識的原始碼檔案。資料夾封包的硬上限改為單一 Host input stream 上限 8 MiB，確保成功讀取的封包一定能建立 stream source。

不提高公開 ABI 或 Host stream 上限，也不增加路徑存取權限。Provider 仍只收到 Host 建立的不可變快照。

## 資料流程

1. Host 遞迴列舉已通過 File Count admission 的資料夾。
2. 跳過 symlink、非一般檔案，以及 tokei 無法由相對路徑辨識的檔案。
3. 對支援的檔案套用既有單檔 8 MiB 限制並加入 `SECLDIR1` 封包。
4. 加入每筆 record 前檢查整個封包是否仍不超過單一 stream 的 8 MiB 上限。
5. 可建立封包的列正常送往 Rust/Lua provider；無支援來源或超過上限的列回報 `Unsupported source`。
6. 某列準備失敗不得讓同批其他列一起失敗。

## 錯誤處理

- 無法讀取資料夾本身：維持 `Source unavailable`。
- 個別子項目在列舉期間消失或無法取得 metadata：跳過該項目。
- 支援的單檔或資料夾封包超過 stream 上限：回報 `Unsupported source`，不進入 dispatch。
- canonical path、檔名或 stream source 等單列準備失敗：只終止該列，不把通用錯誤套用到整批。

## 測試

- 大量不支援二進位檔不佔用資料夾封包預算，支援的原始碼仍可得到正確統計。
- 封包永遠不超過 `MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1`。
- 一個無法準備的列不會使同批其他列失敗。
- 保留現有遞迴檔名、Rust/Lua provider 與 999/1000 admission 邊界測試。
- 對 `D:\code\file_explorer` 的直接子資料夾執行實際封包診斷，確認不再產生 dispatch preparation mismatch。

## 非目標

- 不提高 ABI 上限。
- 不讓 extension 直接讀取任意檔案系統路徑。
- 不改變 File Count 可見性與少於 1000 個檔案的 admission 規則。
- 不處理本地化。
