# Lua 批次資料夾產生器

受限 Lua 僅宣告按鈕、host form 與 typed plan；支援 1–100,000 個名稱、超過 1,000 二次確認、取消後真實 partial 狀態，以及只刪除仍為空白且由本 plan 建立之目錄的保守 undo。實際檔案操作一律留在 host executor。
