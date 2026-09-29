# 本機分欄檢視進度與驗收狀態（2026-09-28）

## 範圍與判定

第一版只支援本機固定或卸除式磁碟的磁碟機代號路徑；其他位置回退到詳細資料檢視。以 `specs/local-column-view/spec.md` 與先前的 `ACCEPTANCE_REVIEW_2026-09-27.md` 為驗收依據。此文件只列實際跑過的結果；未驗證的情境不視為完成。

工作樹在本變更前已有大量未提交修改，分支仍為 `master`，基準 HEAD 為 `7e33dcc0da2b5e86c37231eac58de669466588b2`。沒有重設、清除或提交其他修改。
目前 `tasks.md` 為 14／22 項完成；未勾選項目保留實機 SKIP 或全庫 gate 失敗的事實，並在下文列出下一步。

## 已驗證

| 項目 | 結果與證據 |
|---|---|
| 模式、session、分支、列舉與預覽偏好契約 | `local-column-view-contract` PASS：`target/column-view-after-drag-local-column-view-contract-20260928`。 |
| 本機導覽、子欄與 Alt+P | `local-column-view-headful` PASS：`target/column-view-after-drag-local-column-view-headful-20260928`。 |
| JPEG 整合預覽與舊圖清除 | `local-column-view-jpeg-headful` PASS：`target/column-view-after-drag-local-column-view-jpeg-headful-20260928`。 |
| 窄視窗水平捲軸及預覽分隔線 | `local-column-view-horizontal-headful` PASS：`target/column-view-after-drag-local-column-view-horizontal-headful-20260928`。 |
| F2 欄內重新命名 | `local-column-view-rename-headful` PASS：`target/column-view-after-drag-local-column-view-rename-headful-20260928`。 |
| PDF Preview Handler 的基本邊界及生命週期 | `local-column-view-handler-headful` Quick PASS：`target/column-view-after-drag-local-column-view-handler-headful-20260928`。 |
| 空資料夾狀態、父欄保留與重試 | `local-column-view-empty-headful` PASS：`target/column-view-after-drag-local-column-view-empty-headful-20260928`。 |
| 祖先欄 Delete | Runner 擁有的 `delete-me.txt` 被移除，其他檔保留：`target/column-view-command-delete-codex-v2-20260928/report.json` PASS。 |
| 貼上到目的資料夾 | 複製 `paste-me.txt` 後點入 `paste-dest`，只在該資料夾新增副本：`target/column-view-command-paste-codex-v5-20260928/report.json` PASS。單選資料夾但不導覽的滑鼠路徑另有 reducer 測試，未宣稱實機通過。 |
| 祖先欄右鍵 | 真實選單出現在祖先檔案旁，含剪下、刪除、重新命名，檔案未變更：`target/column-view-command-context-codex-v2-20260928/report.json` PASS。 |
| 四項命令矩陣 | `local-column-view-commands-headful` PASS：`target/column-view-commands-uitest-20260928`；Delete、目的資料夾貼上、跨欄 Move、祖先欄右鍵均有獨立夾具與結果。 |
| 跨欄拖放 | `drag-me.txt` 從 `drag-src` 移到祖先欄 `drag-dst`，`not-dragged.txt` 保留；`target/column-view-command-drag-codex-v4-20260928/report.json` PASS。互動紀錄有 `column_drop ... effect=Move paths=1`。 |
| 100,000 項、五層路徑 | 聚焦模型測試 PASS；初次投影與模型建構約 54 ms，穩態單次可見列建構最高約 57 µs，只建立可見列。這是本機 debug profile 測值，不是端到端影格保證。詳見 `.ai-collab/review.md`。 |
| PDF 預覽已就緒文字 | 補齊 `column-preview-handler-ready` 在其餘 18 個語系的翻譯；20 個語系的 `menus.ftl` 均有此鍵，`git diff --check` PASS。 |
| 鍵盤與 UIA | `local-column-view-keyboard-headful` PASS：`target/column-view-keyboard-uitest-final-20260928`。五個子案例分別驗證鍵盤開啟分欄、Down/Right/Left 與位址、Shift/Ctrl 欄內選取、欄／列／預覽／狀態的名稱角色，以及 Tab 後的焦點返回。選單勾選狀態因 UIA 沒有 TogglePattern，明確 SKIP。 |
| 鍵盤修補後回歸 | `local-column-view-commands-headful` PASS：`target/column-view-commands-after-keyboard-20260928`；基本導覽 PASS：`target/column-view-headful-after-keyboard-20260928`；聚焦模型 23 項、UI 32 項及新增的兩項鍵盤單元測試均 PASS。 |

## 邏輯稽核後續修正（本輪）

參考 `docs/COLUMN_VIEW_LOGIC_AUDIT_2026-09-27.md`，本輪接受 C05／C14／C15／C16 的限縮修正：按分頁取消祖先列舉；正式 Shell 檔案身分事件攔截 junction 回指；拒收改選／模式／分頁切換後的過期循環回覆；建立名稱查重使用活動欄完整快照；不可用位置阻擋新 Columns 選擇並恢復已存偏好。

Codex 另以修正前 FAIL／修正後 PASS 重現並修正背景 terminal 截斷活動分支。獨立 workspace check、fmt、OpenSpec strict、模型 25／UI 38 定向測試、兩項鍵盤回歸、Shell 預設 4 PASS／1 ignored、真實 junction opt-in 1 PASS、i18n library 9 PASS 及 binary 重建均通過。

最後九項鍵盤／邏輯 headful 矩陣 PASS：`target/column-logic-keyboard-uitest-final-v4-20260928`；基本導航回歸 PASS：`target/column-logic-basic-headful-20260928`。勾選 UIA 子斷言仍 SKIP，disabled Button 的 UIA `IsEnabled` 仍 true；操作阻擋與偏好恢復已實機驗證，可及性原生語意仍開放。

本輪四項命令回歸尚未全過：`target/column-logic-commands-headful-v2-20260928` 為祖先 Delete PASS、paste／drag FAIL、context SKIP；過程觀察到測試視窗被覆蓋／失去 foreground，需要空閒桌面補跑。先前的四項 PASS 保留為歷史證據，不能取代本輪結果。

HEAD 在 Grok 讀碼期間經外部提交變成 `c984e91f`；起始 12 個原始碼快照與此 HEAD 相同，保留外部提交。詳細修改、Grok 補修、失敗紀錄、重跑指令及剩餘門檻見 `docs/COLUMN_VIEW_LOGIC_FIXES_2026-09-28.md`。本輪不勾選額外的全功能 tasks。

## 仍需處理

1. **Preview Handler 擴充情境：SKIP。** 同步視窗 resize、真實 DPI 切換、handler 崩潰注入、UIA Tab 焦點讀回未經安全且穩定的實機驗證；目前只有相關狀態測試與 Quick handler 案例。
2. **權限不足資料夾：單元測試。** 沒有更改 ACL 來製造實機夾具；空資料夾實機已過，權限不足的實機情境仍缺證據。
3. **單選資料夾但不導覽的貼上：單元測試。** 實機貼上案例先進入目的資料夾；`column_paste_destination` 的單選資料夾路徑已由 reducer 測試覆蓋，但滑鼠路徑尚無獨立實機斷言。
4. **完整 workspace 回歸：未通過。** 最新 `cargo test -p explorer-model -p explorer-ui --lib` 的模型 191 項通過；UI 582 通過、10 失敗、2 ignored。10 個失敗名稱與修補前及先前 dirty-tree 驗收紀錄相同；完整輸出：`target/column-view-full-lib-tests-after-keyboard-20260928.txt`。尚未取得乾淨基線來證明歸屬。
5. **全庫 OpenSpec coverage gate：未通過。** `explorer-uitest --validate-only` 報 548 項其他變更未覆蓋要求；沒有 `add-local-column-view` 的未覆蓋項。輸出：`target/column-view-coverage-after-keyboard-20260928.txt`。
6. **全庫翻譯完整性測試：未通過。** 本次補齊 `column-preview-handler-ready` 後，`cargo test -p explorer-i18n --test catalog_complete every_locale_matches_english_message_id_set -- --exact` 仍因 `zh-CN` 缺少 7 個其他功能的鍵而失敗；另一項 `catalog_lookup_is_non_empty_and_not_the_key` 原先也在 `status-thumbnail-quota-full` 失敗。這些失敗不應被記成分欄翻譯已完整驗收。
7. **檢視選單勾選狀態：UIA SKIP。** GPUI 的「分欄」選單項暴露為 Button，沒有 TogglePattern；鍵盤啟用及顯示分欄已實機 PASS，但自動化沒有讀到勾選值。需用穩定的程式狀態測試或視覺斷言補證據，才可完成 OpenSpec 1.4 的完整門檻。

## 修正建議與驗收門檻

- 拖放失敗的根因是導航窗格的 drag-move 監聽器在游標不在窗格內時仍以目前開啟資料夾覆寫 OLE effect，讓原本有效的兄弟資料夾 Move 變成 `None`。修補後目標欄在游標範圍內決定 effect，實機 Move 與回歸皆 PASS。保留 `target/column-view-command-drag-codex-v3-20260928` 作失敗對照，不以純 source-string 測試代替真實 drop。
- **P1，選單狀態：** `chrome.rs::view_menu` 已在不支援位置把 Details 畫成勾選，但 Columns 的按鈕目前仍可啟用，且自訂 Button 不提供 TogglePattern。先為本機、UNC／WSL／虛擬位置寫一個直接檢查選單狀態與 Enter 行為的測試；再讓不支援位置的 Columns 無法由滑鼠、UIA 或鍵盤啟用，並在 GPUI／AccessKit 支援時暴露真正的 disabled/checked 語意。返回本機後必須保留原分欄偏好。若短期內無法暴露 TogglePattern，保留 UIA SKIP，使用程式狀態加截圖驗證顯示的選中標記。OpenSpec 1.4 在此之前不勾選。
- **P1，預覽處理常式：** `scripts/test_local_column_view_handler_headful.ps1 -Quick` 已通過初始 PDF handler、分隔線拖曳、Alt+P 卸載／重開及切換選取。完整腳本曾卡在同步 resize/UIA。把視窗 resize、真實 DPI 變更、焦點 Tab 讀回和 handler 崩潰注入拆成各自帶有外層 watchdog 的程序；只停止 runner 建立的 app／handler，並用實際 HWND 邊界、狀態文字與未失去導覽能力作斷言。未跑前不將模型測試寫成實機 PASS。
- **P1，更新與錯誤：** 為 `tasks.md` 2.5 補一個 runner 擁有的樹：開啟子資料夾後由測試程序移除該子夾，再按 F5；斷言失效的子欄與預覽消失、位址回到可導覽祖先、其他檔仍可選。另以不變更真實使用者 ACL 的隔離測試夾具驗證不可讀資料夾；若 Windows 權限無法穩定製造，保留單元測試與 SKIP。junction cycle 已有模型測試，實機 fixture 應確認不向同一祖先無限延伸。
- **P2，命令細節：** 補「單選資料夾但不進入」後 Ctrl+V 的獨立實機案例，確認只貼入被選資料夾；另以 runner 擁有的檔案驗證 Enter／雙擊遵循既有開啟策略，而不是只依 reducer 測試。這些案例應與現有命令矩陣一樣各有獨立 fixture 和 watchdog。
- **P2，驗收基線：** 全庫翻譯測試仍缺其他 7 個 zh-CN 鍵，UI 庫完整測試仍有相同 10 個失敗，全庫 UITEST coverage 仍有其他變更的 548 項未覆蓋。修復或建立可重現的乾淨基線後再關閉 5.2／5.3；分欄案例的 PASS 不代表這些全庫 gate 已通過。
- `tasks.md` 勾選只依各項完整驗收條件更新。真實 DPI、handler 崩潰或權限不足案例若無安全夾具，保留 SKIP 與原因。

完整逐輪 Grok 工作、Codex 審查與測試紀錄在 `.ai-collab/review.md`。

## 交接與工作樹

- 核心模型／狀態：`crates/explorer-model/src/column_view.rs`、`crates/explorer-ui/src/state.rs`、`actions.rs`、`lib.rs`、`column_view.rs`、`chrome.rs`；涵蓋分欄分支、列舉、預覽、拖放與鍵盤命令。
- 驗證：`scripts/test_local_column_view_*`、`uitest/manifest.json`、`docs/UITEST.md`、`docs/COLUMN_VIEW.md`；鍵盤腳本使用帶 BOM 的 UTF-8 供 Windows PowerShell 5.1 正確讀取繁體中文。
- 本輪另補 `crates/explorer-i18n/locales/*/menus.ftl` 的 PDF handler 就緒文字。Grok job `run-muka3uil-v2rjlc` 已結束，Codex 已獨立審查、修正測試器並執行正式 UITEST；沒有待執行的 Grok 工作。
- `master` 與基準 HEAD `7e33dcc0da2b5e86c37231eac58de669466588b2` 未移動；原有大量未提交修改保持在工作樹。沒有 commit、push、reset、clean 或遞迴刪除。測試只建立自身 `target/` 下的夾具及報告；已確認沒有測試留下的 `SuperExplorer.exe` 程序，原先安裝版程序未受影響。
