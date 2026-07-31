## ADDED Requirements

### Requirement: 正式 parity matrix
專案 SHALL 維護 `docs/PARITY_MATRIX.md`，每個納入範圍的 foundation、UI、multi-tab/navigation、file-operation、Clipboard/OLE/context-menu 與 search capability MUST 記錄 milestone、狀態、自動或手動驗收方法、已知差異與 Windows API 限制；沒有證據的項目不得標記完成。

#### Scenario: 更新 capability 狀態
- **WHEN** 開發者將 parity 項目改為完成
- **THEN** 同一列必須引用可重現的測試或已執行手動結果，且已知差異欄不得以空白隱藏未解決差異

### Requirement: 實作與狀態文件
專案 SHALL 維護 `docs/IMPLEMENTATION_PLAN.md`、`docs/STATUS.md` 與 `docs/MANUAL_TESTS.md`，內容 MUST 與目前 change、實際程式行為及 milestone exit criteria 一致。

#### Scenario: Milestone handoff
- **WHEN** M0 或 M1 準備標記完成
- **THEN** plan 必須標示已完成 task、status 必須列出 gates 與限制、manual tests 必須包含可逐步執行的 Windows 驗收流程

### Requirement: 受控視覺基準
專案 SHALL 以本機 Windows 11 25H2 Explorer 建立固定 window size、DPI、theme 與 font configuration 的視覺基準，並記錄 OS build、Explorer version、app commit 與 capture 條件。

#### Scenario: 建立 baseline
- **WHEN** 開發者擷取 M1 light 或 dark baseline
- **THEN** metadata 必須足以重現環境，且比較範圍涵蓋區域高度、間距、字級、色彩及 focus/hover/pressed/disabled/selected 狀態

### Requirement: 可診斷的 visual regression
visual comparison SHALL 輸出 baseline、actual、diff 與 token/layout diagnostics；文字 antialiasing 與動態區域 MUST 使用明確 mask/tolerance，layout bounds 與 semantic colors MUST 使用較嚴格門檻。測試不得自動覆寫 baseline。

#### Scenario: 視覺差異超過門檻
- **WHEN** actual 與 baseline 的 layout 或 color 差異超過設定門檻
- **THEN** 測試必須失敗並保留四類診斷產物，baseline 必須維持不變直到人工 review

### Requirement: Windows 手動驗收矩陣
驗收 SHALL 至少涵蓋啟動/resize/關閉、light/dark、keyboard focus、100/125/150/200% DPI、多分頁真實資料夾、檔案操作、與 Explorer 雙向 Clipboard/drag-drop、context menu 及 search；high contrast、多螢幕與 caption/Snap capability 的實際狀態 MUST 明確記錄。

#### Scenario: 無法自動執行的 case
- **WHEN** CI 或目前環境無法執行有視窗或特定 DPI case
- **THEN** 文件必須記錄未驗證原因、手動步驟與待執行環境，不得填入假成功結果

### Requirement: Milestone exit evidence
M0 只有在可啟動/resize/關閉、panic diagnostics、四個 Cargo gates 與 handle snapshot 有證據時 SHALL 完成；M1 只有在靜態 chrome、theme、actions/focus、DPI 與 visual baseline 有證據時 SHALL 完成。

#### Scenario: Exit criteria 缺少證據
- **WHEN** 任一必要 gate、manual case、visual baseline 或文件更新尚未完成
- **THEN** 對應 milestone 必須保持進行中或受阻狀態，且 parity matrix 不得宣告完成

### Requirement: 效能與資源基線
專案 SHALL 記錄冷/暖啟動、資料夾 first-item/first-viewport、搜尋 first-result、檔案操作 progress latency、一般 UI callback、memory、thread、GDI/User handle、queue depth 與關閉後資源狀態，並將數字視為 regression baseline 而非未量測承諾。

#### Scenario: 產生基線報告
- **WHEN** 在固定本機環境執行 release benchmark 與啟停 smoke test
- **THEN** 報告必須區分冷暖啟動、提供多次樣本的 median/p95，並記錄 debugger、hardware 與 OS build 條件
