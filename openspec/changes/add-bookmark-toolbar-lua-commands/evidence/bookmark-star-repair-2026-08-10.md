# 星號切換回歸修復證據

- 根因：星號動作錯誤依賴 Shell Pin 可用性，且新式 Shell 項目可能使用非路徑 descriptor，造成已有單一選取時仍被判定停用。
- 修復：改用獨立的單選判斷；位於實體父資料夾下的 Shell 項目會從父路徑與顯示名稱建立型別化檔案系統目標。
- UI：星號固定最左側，字級提高至 20 px。
- 新增 UITEST case：`bookmark-star-toggle-headful`，測試資料夾與檔案的「加入 → 取消 → 再加入」，結果 PASS。
- 執行位置：`target/uitest-runs/bookmark-star-repair-20260810-v7`。

## SHA-256

- `report.json`：`319801e11455fdaf5268a18040f58343a99e08cbf62f68eaa8778e22585a452f`
- `bookmark-star-off.png`：`d4e6776f9a86dbf20aa7ea0cd783dd2985f3b11ba2229298dc0020d07d48b180`
- `bookmark-star-on.png`：`70dd134c3faac019ccf89c5ea5bc84f21da0b235ffb707ef3d4a635c67a62f52`
