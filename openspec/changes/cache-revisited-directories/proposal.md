## Why

重訪 Local、ADB、SFTP 資料夾時，目前導覽會先清空檔案清單並等待 provider 重新列舉，遠端路徑尤其容易產生明顯停頓。已成功瀏覽的資料應立即重用，同時在背景取得最新結果，讓 Backspace 與相鄰資料夾切換保持流暢且最終一致。

## What Changes

- 新增視窗共用、容量有界的記憶體 directory snapshot LRU cache。
- 將 Local、ADB、SFTP location 正規化成穩定 cache key，不受遠端暫態 entry ID 或 generation 影響。
- Back、Forward、多步 history、Backspace 上一層、網址列、書籤及資料夾開啟統一採 stale-while-revalidate 導覽。
- 快取命中後立即顯示舊快照，同時照常提交背景 Navigate；成功完成後更新快取與畫面。
- 失敗、取消及 stale request 不能污染快取，背景失敗時保留最近成功 rows。
- 快取上限為 64 個資料夾與合計 100,000 個項目；不持久化至 session。

## Capabilities

### New Capabilities

- `directory-stale-while-revalidate-cache`: Local、ADB、SFTP 重訪資料夾的即時快照顯示、背景收斂、canonical key 與容量淘汰規則。

### Modified Capabilities

無。

## Impact

- `explorer-model`：DirectoryState／TabState 以指定 snapshot 開始 navigation loading 的內部契約。
- `explorer-ui`：視窗層 cache、所有導覽入口及 DirectoryFinished 寫回流程。
- 導覽 request correlation、selection 與錯誤顯示維持既有公開行為。
- 不新增外部依賴，不變更 session 格式或 provider protocol。
