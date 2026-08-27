# 書籤右鍵操作視窗設計

## 目標

書籤項目的右鍵互動改為獨立、可聚焦的原生操作視窗。右鍵不直接執行任何命令；使用者必須先選擇一項操作，再按「確認」才會開啟、編輯或進入刪除確認。

## 互動

操作視窗顯示目標書籤名稱、型別與單選操作清單。所有書籤提供「開啟」、「編輯名稱與路徑」及「刪除」；Folder／FolderPath 額外提供「在新分頁開啟」。初始選取「開啟」，按 Enter 或「確認」執行目前選項。Escape、「取消」及視窗關閉都只關閉操作視窗，不修改書籤。

「編輯」關閉操作視窗後開啟既有 `BookmarkEditorWindow`。「刪除」不立即移除資料，而是把操作視窗切換成刪除確認狀態；使用者再次按明確的「確認刪除」才產生 durable mutation。持久化失敗時沿用現有 rollback 並保留書籤。

## 架構

新增 `BookmarkActionWindow` 與快照，快照只包含目標書籤及顯示所需狀態。視窗本地保存目前單選 command 與是否處於刪除確認階段；命令透過 owner `ExplorerRoot` 的既有 action reducer dispatch。應用程式層保存單一 window handle：首次右鍵建立，後續右鍵更新 snapshot、重設預選動作並啟用既有視窗；handle 失效時安全重建。

`OpenBookmarkContextMenu` 保留為右鍵入口，但 reducer 不再建立主視窗座標 overlay state，而是呼叫 action-window observer。`chrome.rs` 移除書籤項目的 overlay menu；書籤列空白處及邏輯資料夾的新增／管理選單不在本次改動範圍。

## 錯誤與驗證

目標在視窗開啟前若已消失，入口不建立視窗並顯示通知。操作視窗開啟後若目標因其他投影變更而消失，確認命令不執行並關閉視窗。測試涵蓋型別化命令集合、預選與重設、取消零 mutation、編輯轉交、雙重刪除確認、單例 activate/update、舊 overlay 不再渲染，以及既有右鍵投影仍全部接線。
