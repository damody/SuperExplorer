# 目前資料夾書籤星號修正證據

- 日期：2026-08-10
- UITEST：`bookmark-star-toggle-headful`
- 結果：PASS
- 證據目錄：`target/uitest-runs/bookmark-current-folder-20260810-v2`

## 已驗證行為

- 未選取任何子項目時，目前實體資料夾顯示可操作的 `☆`。
- 點擊後新增目前資料夾書籤並顯示 `★`；再次點擊移除；第三次點擊可再加入。
- 選取 `File bookmark.txt` 後仍顯示目前資料夾的 `★`，selection 不改變星號目標。
- 星號固定在書籤工具列最左側，文字尺寸為 20 px，UI Automation 高度至少 24 px。
- 人工檢查三張 PNG，確認截圖內容為 SuperExplorer 視窗而非其他前景程式。

## Artifacts

- `report.json`
- `bookmark-star-off.png`
- `bookmark-star-on.png`
- `bookmark-star-selected.png`
