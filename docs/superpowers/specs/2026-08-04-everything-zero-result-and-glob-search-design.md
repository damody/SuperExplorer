# Everything 零結果與 Glob 搜尋修正設計

## 目標

修正兩個搜尋一致性問題：Everything SDK 成功回傳零筆時不得啟動較慢的本機 fallback；`*.rs`、`foo*.rs`、`*test*` 與 `?` 等 glob 查詢必須在 Everything、SQLite LocalIndex 與 filesystem fallback 得到相同結果。

## Backend 選擇

Everything DLL 可載入、IPC/database 可用且查詢成功時，該 backend 對零筆或多筆結果都具有決定性，直接完成請求。只有 DLL、ABI、IPC、逾時或查詢錯誤才切換至既有 LocalIndex/filesystem fallback。使用者取消直接結束，不觸發 fallback。查詢途中失效時保留已發布結果，fallback 結果沿用既有 identity/path 去重與 exactly-one terminal event 契約。

## Glob 語意

未加欄位的一般文字若含未逸出的 `*` 或 `?`，即視為對完整檔名、不分大小寫的 glob：`*.rs` 匹配 Rust 副檔名、`foo*.rs` 匹配指定前後綴、`*test*` 匹配任意位置、`?` 匹配一個 Unicode scalar。沒有 wildcard 的一般文字維持既有 substring 行為。`type:rs` 與 `ext:rs` 維持既有精確副檔名語意。

Parser 保存 glob 意圖，並由 `explorer-search` 提供單一 matcher，供 LocalIndex、filesystem fallback 以及 Everything 候選結果的最終過濾共同使用。Everything query renderer 將 glob 傳成安全的 Everything filename expression；folder scope、引號、反斜線與 Everything 語法字元仍須 escape，避免放寬搜尋範圍或造成語法注入。

## 錯誤與效能

Glob matcher 不使用可能產生災難性回溯的正規表示式，採線性／有界動態匹配。搜尋仍在背景 worker 執行，保留分頁、結果上限、逾時與 cancellation checks。診斷記錄實際 backend 與錯誤分類，不記錄使用者查詢或完整私人路徑。

## 測試與驗證

單元測試涵蓋零結果不 fallback、不可用時 fallback、取消不 fallback，以及 `*.rs`、`foo*.rs`、`*test*`、`file?.rs`、Unicode、大小寫與無 wildcard substring。跨 backend contract tests 對相同 fixture 驗證相同可見結果，並驗證 query escaping、途中 IPC 失效、去重及 exactly-one terminal。最後執行 formatting、相關 crate tests、Clippy、workspace tests 與 release build。

## 非目標

不安裝、啟動或設定 Everything；不改變搜尋 UI、索引資料格式或現有進階欄位語法；不讓 service-only 狀態被誤認為 SDK IPC 可用。
