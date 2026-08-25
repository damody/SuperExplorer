# MFT Service 資源群組設計

## 目的

Folder Options 的五個 MFT 預算列目前與其他快取列混排，使用者無法直接判斷它們是否由 MFT Service 管理。介面必須清楚表達資源擁有者，同時保留既有即時用量、上限編輯器與滑桿行為。

## 設計

- 將 `Persisted MFT index`、`Volume index memory`、`File data memory`、`Folder aggregates memory` 與 `MFT Service LRU` 包在單一有邊框、圓角的可存取群組中。
- 群組標題顯示 `MFT Service 資源`，並提供 `MFT Service resources` 的 aria label。
- 說明文字標明這些資源由所有 SuperExplorer 程序共用；磁碟索引跨服務重啟保留，記憶體快取在服務重啟後重建。
- `Folder size cache TTL` 留在群組外，因為它是查詢重用期限設定，不是上述五種服務資源用量。
- 不改變遙測來源、預算持久化、滑桿範圍或更新頻率。

## 驗證

- 聚焦 UI 契約測試確認群組 ID、可存取標籤、標題、說明與五個資源標籤仍存在。
- 執行 `explorer-ui` 的該項聚焦測試與 crate check；不執行完整 workspace tests。

