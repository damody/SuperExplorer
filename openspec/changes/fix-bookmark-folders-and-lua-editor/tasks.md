## 1. 資料模型與相容遷移

### 1.1 樹狀書籤契約

**目的：** 以可回復的持久化樹狀集合取代扁平書籤。
**輸入：** 已完成的 bookmark-toolbar 契約與本變更 design/specs。
**產出：** 節點模型、tree mutation API 與 model tests。
**依賴：** 無。
**Owner／Wave：** Primary integrator／wave 0。
**Gate／Evidence：** G1；`evidence/index.json` 的 `1.1.*`。
**完成門檻：** 每項樹 mutation 和 rollback 可獨立通過。

- [ ] 1.1.1 定義 Folder/Bookmark 節點、穩定 ID、parent 與 sibling order。
- [ ] 1.1.2 實作建立、改名、移動及同層重排 mutation。
- [ ] 1.1.3 實作無子節點 folder 刪除與 mutation rollback。
- [ ] 1.1.4 實作非空 folder descendant-count 與確認後遞迴刪除 mutation。
- [ ] 1.1.5 對循環 parent、無效 parent、重複 ID 和非法 order 加入恢復測試。

### 1.2 Session 升級與耐久性

**目的：** 讓既有書籤 session 無損升級且保留失敗回復。
**輸入：** 1.1 tree 契約與現有 session envelope。
**產出：** 相容序列化、升級 fixture 與 lifecycle tests。
**依賴：** 1.1。
**Owner／Wave：** Primary integrator／wave 0。
**Gate／Evidence：** G2；`evidence/index.json` 的 `1.2.*`。
**完成門檻：** 舊 session 可重啟保存、無效樹不會遺失有效項目。

- [ ] 1.2.1 實作扁平 `Bookmarks` JSON 至根節點的向後相容解碼。
- [ ] 1.2.2 將新樹接入 session snapshot、hydration 與 last-known-good 恢復。
- [ ] 1.2.3 測試 legacy ID/name/target/Lua source/order 無損升級。
- [ ] 1.2.4 測試耐久寫入失敗時完整回復 pre-mutation tree。

## 2. Lua 編輯器回歸修復

### 2.1 可操作的表單元件

**目的：** 消除 `+` modal 的不可見／卡住狀態。
**輸入：** 現有 EditableTextState、UI tokens 和 1.2 state。
**產出：** 有樣式、可聚焦的名稱與 payload 控件。
**依賴：** 1.2。
**Owner／Wave：** Primary integrator／wave 1。
**Gate／Evidence：** G3；`evidence/index.json` 的 `2.1.*`。
**完成門檻：** 打開、輸入、取消、Escape 和成功儲存皆有獨立通過證據。

- [x] 2.1.1 建立使用 token 顏色、border、caret、selection 的可重用書籤表單欄位。
- [x] 2.1.2 將 Lua name/source 控件改用表單欄位並保留強 entity handle 至關閉。
- [ ] 2.1.3 在開啟時聚焦名稱欄，取消／Escape／遮罩安全關閉並清理 handles。
- [ ] 2.1.4 持久化失敗時保留 modal 與 draft，成功時才清除。
- [ ] 2.1.5 新增 UI state/chrome regression tests 及 headful Lua editor screenshot。

## 3. Firefox 式資料夾與目的地互動

### 3.1 書籤資料夾右鍵管理

**目的：** 在我的最愛提供安全的 folder CRUD。
**輸入：** 1.1 tree API、navigation pane 和 action routing。
**產出：** 資料夾 context menu、name draft、刪除確認。
**依賴：** 1.1、2.1。
**Owner／Wave：** Primary integrator／wave 2。
**Gate／Evidence：** G4；`evidence/index.json` 的 `3.1.*`。
**完成門檻：** 根／子資料夾可完整 CRUD，且絕不觸及實體檔案系統。

- [ ] 3.1.1 將 tree 資料夾投影到 favourites navigation，含展開/收合 state 與 UIA roles。
- [ ] 3.1.2 實作 root 空白區／資料夾節點的右鍵命令及 action dispatch。
- [ ] 3.1.3 實作新增 root/subfolder 和重新命名對話框。
- [ ] 3.1.4 實作 non-empty folder descendant-count 確認與持久化 rollback。
- [ ] 3.1.5 新增 keyboard、右鍵與不影響 filesystem 的聚焦 tests。

### 3.2 統一書籤目的地編輯器

**目的：** 讓星號和加入書籤可選資料夾並能安全移除。
**輸入：** 1.1 tree、2.1 modal foundation、3.1 navigation commands。
**產出：** destination picker 與 create/edit/remove draft flow。
**依賴：** 1.1、2.1、3.1。
**Owner／Wave：** Primary integrator／wave 2。
**Gate／Evidence：** G5；`evidence/index.json` 的 `3.2.*`。
**完成門檻：** 星號、selected add、Lua add 與 manager edit 都共用同一正確流程。

- [ ] 3.2.1 擴充 bookmark draft 以記錄 parent、模式與可選移除。
- [ ] 3.2.2 實作可存取 root/folder tree picker 及預選目的地。
- [ ] 3.2.3 將星號從立即 toggle 改為 create-or-edit draft，並維持 selection independence。
- [ ] 3.2.4 將 selected file/folder 加入書籤改接 destination draft。
- [ ] 3.2.5 在既有 bookmark editor 提供確認後移除操作。
- [ ] 3.2.6 測試 root、子資料夾、existing edit/remove、non-filesystem disabled 與 persistence failure。

## 4. 工具列、管理員與 Lua 回歸整合

### 4.1 樹狀投影整合

**目的：** 保留書籤的可見性與可開啟行為。
**輸入：** 1.1 tree、3.2 editor、既有 toolbar/overflow/manager。
**產出：** Root folder menu、nested manager 與順序保持行為。
**依賴：** 3.2。
**Owner／Wave：** Primary integrator／wave 3。
**Gate／Evidence：** G6；`evidence/index.json` 的 `4.1.*`。
**完成門檻：** 所有項目仍可導航、Shell 開啟或執行受限 Lua。

- [ ] 4.1.1 更新 toolbar 與 overflow 以顯示 root folder submenu 和 sibling order。
- [ ] 4.1.2 更新 manager 以顯示 nested folders、move destination 與 context commands。
- [ ] 4.1.3 維持 folder/file activation、missing target notice 與 Lua background dispatch。
- [ ] 4.1.4 執行 Lua host-boundary 與 `.explorer.lua` negative regression tests。

## 5. 驗收、證據與交接

### 5.1 完整驗證

**目的：** 證明所有需求在實際程式與持久化 session 中成立。
**輸入：** 1 至 4 的通過結果。
**產出：** Test logs、headful reports/screenshots 和 evidence index。
**依賴：** 4.1。
**Owner／Wave：** Primary integrator／wave 4。
**Gate／Evidence：** G7；`evidence/index.json` 的 `5.1.*`。
**完成門檻：** 每個 requirement/scenario 有 passing evidence，無 P0/P1。

- [ ] 5.1.1 執行 explorer-model、explorer-ui、explorer-app、explorer-automation 聚焦測試。
- [ ] 5.1.2 執行 formatter、clippy 與工作區相關編譯檢查。
- [ ] 5.1.3 執行/擴充 headful UITEST，保存 Lua editor、folder context、destination picker、star edit/remove screenshots。
- [ ] 5.1.4 將每個 leaf 的 command、result、exit status、hash、timestamp 與 gate 寫入 evidence index。
- [ ] 5.1.5 執行 strict OpenSpec validation、task validator、traceability review 與 scoped-diff review。
