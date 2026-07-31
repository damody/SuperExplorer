# Everything、按需 SQLite 搜尋與詳細資料欄位設計

## 範圍

這個變更同時完成兩個檔案總管操作缺口：搜尋框自動選擇 Everything 或本機按需索引，以及詳細資料檢視的欄位標題右鍵選單與可讀檔案大小。兩者共用既有 per-tab view/search state、取消權杖、背景 Shell 工作邊界與 session persistence，不建立第二套 UI 狀態。

## 搜尋方案比較與決策

考慮三種 Everything 整合方式：

1. **動態載入官方 Everything SDK DLL（採用）**：API 小、結果欄位完整，且不需要編譯期匯入程式庫。缺點是必須隨程式提供正確架構的 DLL，並且目前使用者工作階段內要有 Everything 背景 client 提供 IPC。
2. **直接實作 WM_COPYDATA IPC**：不需 DLL，但要自行維護隱藏視窗、結構版本、訊息生命週期與取消／逾時，Windows UI 整合風險較高。
3. **啟動 `es.exe` 並解析輸出**：隔離簡單，但程序啟動延遲、輸出 escaping、取消與部署都較差。

程式啟動時與每次 backend 失效後，以 SDK 的 IPC 能力探測判定 Everything 是否可用；只偵測 Windows service 名稱並不足以證明 IPC 可查詢。程式不主動啟動、安裝或修改 Everything。IPC 可用時使用 Everything，否則使用 SQLite。Everything 在查詢途中失效時，該次查詢保留已送出的結果並轉入 SQLite fallback。

## Everything adapter

新增獨立 adapter，從程式目錄或已驗證的 bundled dependency 位置動態載入 `Everything64.dll`。所有函式指標與字串結果在 adapter 內轉成 owned Rust 值，不把 SDK 指標傳出邊界。查詢在背景執行，設定：

- Unicode API、完整路徑、檔案／資料夾屬性、大小與修改日期 request flags。
- 將目前資料夾 canonical path 加入 Everything 查詢 scope，只傳回該資料夾以下結果。
- 將既有 parser 的文字、名稱、類型、大小、修改日期及布林運算轉成 escaped Everything 語法。
- 分頁取得結果並在每批之前檢查 cancellation；設定總結果與每批上限，避免一次配置無界記憶體。
- 失敗、逾時、IPC 消失或 DLL 不相容時回報 truthful backend status，再切換 SQLite。

Everything SDK 是對背景 Everything client 的 IPC wrapper；偵測與查詢行為依官方 SDK 文件，不假設 service process 可跨 Windows session 直接接受 IPC。

## SQLite 按需索引

索引位於 `%LOCALAPPDATA%\RustGpuiExplorer\search-index\v1\index.sqlite3`；不可用時退到程式既有可寫資料目錄策略。資料庫採 WAL、busy timeout、版本化 schema 與 bounded prepared statements。核心資料包含 canonical path、parent path、名稱、是否資料夾、大小、建立／修改時間、檔案類型及最後觀察時間；不保存檔案內容。

索引來源只有兩種：

1. **看過的資料夾**：成功列舉資料夾後，只 upsert 該次已取得的直接子項目，不遞迴。
2. **正在搜尋的 scope**：先查詢 SQLite 中已有且位於目前 scope 的結果，再在同一個 active request 內按 breadth-first traversal 補索引並串流新結果。只有明確展開的搜尋根目錄會遞迴。

每個目錄／小批次寫入使用短 transaction。搜尋取消、輸入被替換、清除搜尋、切換位置、關閉分頁或應用程式關閉時，既有 cancellation token 立即停止排隊新目錄，內圈每個 entry 都再次檢查。取消後不得有背景 crawler 繼續擴張 scope；已提交的小批次可保留，未提交 transaction 回滾。即使使用者從 `C:\` 搜尋，也只在該 request 存活期間前進。

SQLite 查詢必須綁定參數並 escape LIKE wildcard；scope 比對使用 canonical parent 邊界，禁止 `C:\foo` 誤包含 `C:\foobar`。資料庫損壞時隔離舊檔、建立新 schema，搜尋仍可繼續。索引大小、結果數、待處理目錄與單次 traversal 項目數均有設定上限。

## Backend 狀態與資料流

`SearchBackend` 擴充為 Everything 與 LocalIndex，保留 WindowsIndex 僅供既有相容／遷移測試。正式選擇順序固定為：

1. Everything IPC 可用：只執行 scoped Everything 查詢。
2. Everything 不可用或失效：讀取 SQLite 快取，再以 cancellation-aware traversal 更新目前 scope。

UI 繼續接收既有 `SearchStatus`、`SearchBatch`、`SearchFinished`，所以 generation rejection、跨分頁隔離、結果 dedupe 與取消呈現沿用現有模型。搜尋框不暴露後端實作細節；診斷事件記錄選用 backend、切換原因、visited/indexed/matched 數與取消延遲，但不得記錄查詢文字或私人完整路徑。

## 詳細資料欄位右鍵選單

詳細資料 header 成為獨立的右鍵 hit-test surface。右鍵不選取或啟動底下檔案列，並開啟有專用 focus 的互斥 popup。選單提供：

- 調整目前欄位至最適大小。
- 調整所有可見欄位至最適大小。
- 名稱、修改日期、類型、大小、建立日期、作者、標籤、標題的顯示核取。
- 「其他…」開啟可存取的欄位選擇 modal，容納未來 dynamic columns。

名稱永遠可見；其餘欄位可切換。至少保留一欄。欄位順序、可見集合與寬度是 per-tab view settings，透過現有 session snapshot 持久化。新增欄位缺少 metadata 時顯示空白，不在 UI thread 讀檔；作者、標籤與標題只顯示已由 Shell enumeration 提供的 owned metadata。

選單支援 hover highlight、Up/Down/Home/End、Enter/Space、Escape、外部點擊關閉與原 focus 還原；popup 必須遮蔽底下 header 與 file rows。

## 檔案大小格式

建立唯一 `format_file_size` pure helper，所有 Details、Tiles、Content、Preview metadata 與自動欄寬都使用同一結果。採 1024 進位：

- 0 到 1023 bytes：顯示 `0 KB` 或向上取整為 `1 KB`。
- KB 到 9.9 KB：最多一位小數；10 KB 以上顯示整數。
- 依門檻提升為 MB、GB、TB，避免顯示 `1024 KB` 或 `1024 MB`。
- 使用目前 locale 的數字小數／千分位格式；單位維持 `KB / MB / GB / TB`。
- 資料夾或未知大小顯示空白。

## 錯誤處理與安全性

- SDK DLL 缺少、簽章／架構不符、函式缺少或 IPC 不可用都視為 capability unavailable，不影響啟動。
- DLL 路徑只允許程式 owned/bundled 位置，避免目前工作目錄 DLL hijacking。
- SQLite path 必須位於 owned data root；migration、quarantine 與刪除都驗證 resolved containment。
- 所有搜尋來源最多發出一個 terminal event，遲到批次由 request generation 拒絕。
- 不索引檔案內容、不追蹤 reparse point、不跨出搜尋 root，不在取消後繼續 crawl。

## 測試與驗收

- Everything fake DLL/API contract：可用探測、scope escaping、分頁、取消、IPC 中斷與 SQLite failover。
- SQLite temporary-directory tests：shallow observed-folder upsert、cached-first query、active-scope traversal、WAL/schema migration、corruption recovery、path boundary、reparse exclusion與取消後資料庫列數不再增加。
- Model/UI tests：backend status、generation rejection、header right-click occlusion、focus、keyboard、check state、名稱不可隱藏、per-tab persistence 與「其他…」modal。
- 大小 formatter boundary table：0、1、1023、1024、KB/MB/GB/TB 門檻與未知／資料夾。
- Headful UITEST：有 Everything 與無 Everything 兩條路徑；建立隔離 fixture，驗證搜尋結果、取消後 crawler 停止；欄位選單每個 action 可點擊且不高亮檔案列。
- 使用十次循環而非大型無界 soak，涵蓋兩個分頁與至少兩個不同磁碟路徑。

## 非目標

- 不安裝、啟動、設定或重建 Everything 資料庫。
- 不主動索引整顆磁碟、不建立常駐全機 watcher。
- 不做檔案內容全文索引。
- 不在這個變更中實作任意第三方 property handler；只消費既有 owned metadata。
