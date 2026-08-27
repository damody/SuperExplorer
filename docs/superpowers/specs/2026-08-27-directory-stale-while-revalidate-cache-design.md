# 資料夾 Stale-While-Revalidate 快取設計

## 目標

Local、ADB、SFTP 資料夾一旦成功瀏覽過，再透過 Back、Forward、Backspace 上一層、網址列或雙擊資料夾回到相同位置時，必須立即顯示上次成功快照，不等待重新列舉。畫面顯示快取的同時，系統仍在背景讀取最新資料並自動收斂更新。

指定的 SFTP 驗證路徑為：

- `sftp://45.32.49.125/home/linuxuser/test`
- `sftp://45.32.49.125/home/linuxuser`

## 現況

一般導覽由 `TabState::begin_navigation_request()` 啟動。它會呼叫 `DirectoryState::begin(..., false)`，清空上一個位置的 snapshot。Back、Forward、Backspace 上一層及一般 Navigate 都會經過這條路徑，因此 Local、ADB、SFTP 必須等第一批列舉結果抵達後才重新出現內容。

既有 Refresh 使用 `DirectoryState::begin(..., true)` 保留同一位置的 snapshot；其 merge/finish 語意已能在新 batch 抵達時 upsert，並在完成時依 `seen` 移除消失項目。本設計重用這套收斂機制，但快照來源改成目標位置的 LRU cache，而不是目前位置。

## 架構

### 視窗層 DirectorySnapshotCache

在 UI 視窗狀態持有一個記憶體型 `DirectorySnapshotCache`，供同一視窗的所有分頁共用。每筆包含：

- 正規化後的 `LocationDescriptor` key。
- 最近一次成功完成的 `DirectorySnapshot`。
- LRU 存取序號及項目數。

快取最多保存 64 個資料夾，且所有快照合計最多 100,000 個項目。插入或命中會更新 LRU；超出任一限制時，從最久未使用項目開始淘汰。單一超過 100,000 項的快照不進入快取，避免它清空其他有效項目。

快取只存在於記憶體，關閉程式後不保存，也不加入 session persistence。

### Location key

- Local 使用完整 `LocationDescriptor::FileSystem` 路徑，依現有 Windows 路徑比較策略正規化大小寫及尾端分隔符。
- ADB、SFTP 使用 provider ID、public authority 與 canonical components；移除只對單次列舉有效的 entry ID／container generation 差異，使相同 URI 可以命中。
- Shell namespace、Known Folder 與 synthetic root 不列入本次新增快取，維持既有行為。

### 導覽資料流

1. 導覽動作在建立新 request 前取得目標 location。
2. 以目標 location 查詢快取。
3. 命中時，以快照啟動新的 `DirectoryState::Loading`；未命中則以空 snapshot 啟動。
4. 無論是否命中，都照常提交背景 `Navigate`。
5. `LocationResolved`、`DirectoryBatch`、`DirectoryFinished` 仍用既有 tab ID、generation、request ID 與 cancellation 驗證。
6. 新 batch upsert 快取畫面；完成時依 `seen` 移除舊項目，再將成功 snapshot 寫回 LRU。
7. 失敗或取消保留畫面中的 cached snapshot，但不得把失敗／部分資料寫回快取。

此流程同時套用 Back、Forward、多步 history、Backspace 上一層、網址列、書籤及雙擊資料夾，避免各入口自行實作快取邏輯。

## UI 行為

- 快取命中後第一個 render 就顯示完整舊快照，不顯示空白檔案區。
- `DirectoryState` 維持 Loading，因此既有載入狀態可表示背景更新；不得用遮罩阻擋檔案互動。
- selection 在開始目標導覽時清除，避免把前一資料夾 selection 套到 cached rows；新完成 snapshot 仍由既有 reconcile 處理。
- 排序、欄位、view mode 仍使用分頁設定，快取只保存原始 directory snapshot。
- 背景完成後若資料沒有變化，不製造額外可見閃動。

## 一致性與失敗處理

- Cache snapshot 是 stale presentation，不是 authoritative storage；每次命中都必須提交背景 revalidation。
- 舊 generation、錯誤 request ID、已取消 request 的 batch／finished 事件不能更新目前畫面或快取。
- 檔案操作與 watcher 不強制清空整個快取；重訪可能短暫看到舊項目，但背景列舉會移除它。
- 背景讀取失敗時顯示既有錯誤狀態並保留 cached rows，讓使用者仍能看到最近成功內容。
- 未成功完成的空白或部分快照不能覆蓋最近成功快照。

## 驗證

- Model/UI 純狀態測試：命中立即顯示、未命中空 loading、完成後更新、失敗保留、stale event 拒絕。
- LRU 測試：64 個位置與 100,000 項上限、命中提升最近使用順序、超大型單筆拒絕。
- 導覽入口測試：Back、Forward、Backspace 上一層及一般導覽使用同一快取流程。
- Location key 測試：Local、ADB、SFTP 相同 canonical location 命中，其他 authority/path 不誤命中。
- 真實視窗測試：在指定兩個 SFTP 路徑來回切換，證明返回時先顯示 cached rows，背景完成後更新。
- 僅執行相關聚焦測試、相關 crate compile check、格式與 diff check；不執行完整迴歸測試。
