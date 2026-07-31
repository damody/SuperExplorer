# 搜尋實作與驗證證據

## 實作邊界

- `explorer-search::parser` 以 UTF-8 byte span 產生 typed AST，涵蓋 quoted phrase、跳脫、`name`／`type`／`size`／`date`、comparison、`NOT > AND > OR` precedence 與隱含 AND。
- `explorer-search::address` 是獨立 address parser。磁碟機絕對路徑、UNC 與 `shell:` parsing name 才能成為 location；invalid address 不會 fallback 成 search，search property 也不會轉成 location。
- query binder 只輸出 placeholder template 與分離參數；Windows adapter 才依參數型別 escape AQS，接著 percent-encode `search-ms` URI，使用者文字不會直接成為 query structure。
- Windows adapter 實際建立 `CSearchManager`／`SystemIndex` catalog、檢查 crawl scope，並呼叫 `ISearchQueryHelper::GenerateSQLFromUserQuery`。indexed scope 以 Shell search namespace 增量列舉；unavailable／outside-scope 明確發布 source status 後走 fallback。
- filesystem fallback 在 Shell worker thread 執行，使用 cancellation、visited canonical directory set、預設不追蹤 reparse/symlink、64-item batch、directory/item hard limits 與 bounded `SyncSender` backpressure。
- 每個 tab 的搜尋有獨立 generation、cancellation、result snapshot、backend status 與 stable-ID source attribution；新 query、navigation、離開或關閉 tab 取消舊工作，完整 `RequestContext` 拒絕 late events。
- FileViewHost 呈現 search snapshot，但 directory snapshot/history 保留在下層；Esc/leave search 立即恢復原目錄。Ready-empty、loading、partial、error 與 cancelled 是不同 model/UI 狀態。

## 多分頁 end-to-end

`cargo test -p explorer-shell-win end_to_end_two_tab_search_replacement_navigation_cancel_and_partial_fallback -- --nocapture` 於 2026-07-26 本機通過（1.76 s）。兩個真實資料夾在不同 `TabId` 同時執行 `alpha`／`beta` query，各自只保留對應結果；快速 `name:never`→`beta` replacement 取消舊 token 並拒絕 late generation。搜尋中導覽到另一資料夾會取消搜尋並恢復 directory ownership。最後在真實 root 建立 4,100 個子目錄，超過 fallback 的 4,096 pending-directory hard limit，實際得到 `Partial` terminal 與 `FileSystemFallback` source status，沒有用 fake error 冒充。

## 真實資料夾 oracle

`cargo test -p explorer-search` 使用 OS temporary directory 與真實檔案驗證：

- Unicode：`專案 quarter four.txt`
- quoted phrase：`"quarter four"`
- `name`、`type`、`size`、`date` filters
- boolean 與括號
- zero-byte 與 zero-result
- cancellation、closed result channel、快速 query replacement
- stable identity 去重與 WindowsIndex/FileSystemFallback attribution 合併

`cargo test -p explorer-shell-win real_search_uses_typed_query_fallback_and_rejects_fast_replacement -- --nocapture` 通過，透過真實 Shell STA 搜尋 temporary folder，快速替換舊 query 後只保留 `專案 quarter four.txt`。

本機 `cargo test -p explorer-shell-win real_index_probe_is_truthful_for_temporary_scope -- --nocapture` 回報：

```text
temporary-folder Windows Search availability: Unavailable("HRESULT=0x80040154")
```

因此這台驗證機不把 index unavailable 假裝成空結果，而是發布 unavailable diagnostics 並由真實 filesystem fallback 完成 oracle。

## 100,000 項真實資料

命令：

```text
cargo test -p explorer-search measures_one_hundred_thousand_real_items -- --ignored --nocapture
```

2026-07-26 本機結果：

```text
first_result=9.2218ms
first_viewport=9.2219ms
terminal=2.7697217s
batches=1563
max_queue=0
memory_before=6451200
memory_after=6811648
memory_delta=360448
cancel_latency=21.3µs
```

Flat-directory fixture 沒有 pending child directories，因此 `max_queue=0`；result delivery 仍以 64-item batch 與 bounded channel 施加 backpressure。測試確認 100,000 個 stable results、terminal、取消小於一秒，並在結束後由 owned temporary fixture 清理資料。

## 完整回歸

- `cargo test -p explorer-search -p explorer-model -p explorer-ui`：通過。
- `cargo test -p explorer-shell-win -- --nocapture`：35 passed、1 個明確 ignored 的既有 100k navigation benchmark。
- `cargo clippy -p explorer-search -p explorer-model -p explorer-ui -p explorer-shell-win --all-targets -- -D warnings`：通過。
