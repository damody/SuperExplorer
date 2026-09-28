# 分欄檢視程式邏輯審查清單

審查快照：2026-09-27 23:28（Asia/Taipei），`HEAD 7e33dcc0` 加上當時未提交的 `add-local-column-view` 工作樹。此清單聚焦本次分欄變更及其直接呼叫的既有功能；`SuperDesktop`、其他外掛與整個產品的既有邏輯不在本次逐行審查範圍。只記錄現象、觸發路徑與證據，供人工 review；此文件沒有修正計畫。

## 已由現行程式確認

| ID | 嚴重度 | 異常邏輯與可觀察影響 | 證據 |
| --- | --- | --- | --- |
| C01 | 高 | **取消選取後，命令仍可能作用於舊檔案。** 最右欄單擊 A 會同步 `tab.selection`；再 Ctrl 點 A，分欄的 `selected` 變空，但 `tab.selection` 未同步清空。`selected_items()` 回退讀舊的 `tab.selection`，刪除／剪下仍可能指向 A；預覽來源也會回退到 A。 | `crates/explorer-ui/src/state.rs:9717-9734`、`:9516-9548`、`:7887-7905`、`:8305-8310` |
| C02 | 高 | **分欄列沒有右鍵或拖放入口。** `column_row()` 只監聽左鍵；既有右鍵流程還要求 item 在 `tab.selection`。因此祖先欄的選取即使能用於部分命令，也無法以該列的滑鼠右鍵打開正確檔案選單，拖放亦沒有接線。 | `crates/explorer-ui/src/column_view.rs:405-488`、`crates/explorer-ui/src/state.rs:7481-7514` |
| C03 | 高 | **F2 仍以目前資料夾的列表列號定位。** 分欄焦點可在祖先欄，但 `begin_focused_inline_rename()` 透過 `focused_row_index()` 與 `presentation_entry()` 取得目前分頁的列，可能無反應或改到另一項目。 | `crates/explorer-ui/src/state.rs:7030-7069`、`:9662-9691` |
| C04 | 高 | **輔助列舉收到第一批資料便提前釋放併發槽。** `apply_column_event()` 在 batch 而非 terminal 呼叫 `column_loads.complete()`。多批資料夾的列舉還在執行時，後續 render 可再發新請求，突破 `COLUMN_LOAD_CONCURRENCY = 2` 的約束。 | `crates/explorer-ui/src/state.rs:10000-10030`、`crates/explorer-model/src/column_view.rs:25`、`:835-865` |
| C05 | 高 | **同一載入協調器會取消別的分頁的請求。** `plan(tab_id, ...)` 的 `retain` 只留下目前 tab 的 request；切換分頁、重新規劃時，其他 tab 尚在跑的祖先列舉全部進入取消清單。這會造成重做工作，也可能留下先前分頁短暫的 Loading／Pending 狀態。 | `crates/explorer-model/src/column_view.rs:786-802`、`crates/explorer-ui/src/state.rs:9952-9978` |
| C06 | 高 | **佇列滿被當成列舉取消。** STA 以 `try_send` 發每一批；通道滿時回傳 `false` 給列舉器，最後產生取消終結。使用者沒有取消時，大目錄或慢 consumer 仍可能只看到部分資料。 | `crates/explorer-shell-win/src/sta.rs:2755-2782` |
| C07 | 中 | **Shift 範圍選取只拿到被點的一筆。** UI 呼叫 `select_range(..., std::slice::from_ref(&entry))`，模型卻需整欄有序 entries 才能找 anchor 到目標的區間。實際 Shift 點選無法建立預期範圍。 | `crates/explorer-ui/src/state.rs:9722-9724`、`crates/explorer-model/src/column_view.rs:436-465` |
| C08 | 中 | **祖先欄與鍵盤順序不遵守現有顯示規則。** 最右欄畫面使用 `FilePresentation`，祖先欄直接使用原始 snapshot；方向鍵也使用原始 snapshot。隱藏項目、排序與資料夾優先規則可與目前畫面不一致，鍵盤上下移動也可能跳到不同於視覺鄰列的項目。 | `crates/explorer-ui/src/state.rs:9439-9475`、`:9851-9871`、`crates/explorer-model/src/column_view.rs:467-483` |
| C09 | 中 | **水平瀏覽只具備狀態與負 margin，沒有手動操作入口。** `SetColumnHorizontalOffset` 只有 action 定義及消費端；分欄 surface 沒有水平捲輪、觸控板或捲軸發送端。點擊揭露欄位仍用固定 `960.0`，不是實際 viewport；窄視窗下的末欄／祖先欄可達性無法保證。 | `crates/explorer-ui/src/column_view.rs:182-215`、`crates/explorer-ui/src/state.rs:9736-9744`、`:9902-9922`、`crates/explorer-ui/src/lib.rs:8996-8999` |
| C10 | 中 | **垂直捲動無上界，也不把鍵盤焦點捲入畫面。** offset 只限制非負，沒有依列數及可用高度 clamp；上下鍵只改選取 ID。多捲幾次可得到整欄空白，鍵盤焦點也可停在可視區外。 | `crates/explorer-ui/src/column_view.rs:285-329`、`crates/explorer-model/src/column_view.rs:467-483`、`:533-537` |
| C11 | 中 | **預覽欄寬有 action 與保存欄位，但沒有拖曳發送端。** 目錄欄有 divider 事件；預覽欄沒有 `SetColumnPreviewWidth` 的 UI 事件，文件所述可調預覽寬度尚無實際操作路徑。 | `crates/explorer-ui/src/column_view.rs:343-401`、`:737-879`、`crates/explorer-ui/src/actions.rs:789-791` |
| C12 | 中 | **大量資料僅在畫列時虛擬化。** 每次建立 strip model 先複製每一欄全部 `FileEntry`，再為全部 entries 複製 row model；可視範圍最後才在 renderer 套用。大目錄與深路徑仍有全量配置及逐列工作。 | `crates/explorer-ui/src/state.rs:9425-9499`、`crates/explorer-ui/src/column_view.rs:69-86`、`:248-329` |
| C13 | 中 | **空目錄／列舉失敗可被畫成正常 Ready。** 最右欄只要不是 Loading 且沒有 `level.phase == Error`，就顯示 Ready；它沒有依 `tab.directory` 的 Ready(empty)／Failed／Partial 分類。失敗或空目錄可出現無文字的空白欄。 | `crates/explorer-ui/src/state.rs:9441-9461`、`crates/explorer-ui/src/column_view.rs:331-342` |
| C14 | 中 | **循環偵測測試使用了正式 UI 沒有提供的解析後 key。** 模型測試手動把已解析的祖先 key 傳入 `select_child`；正式點擊傳的是 `None`，改用未解析的字面路徑。junction／符號連結回指祖先時，測試通過不能證明正式路徑會偵測到循環。 | `crates/explorer-model/src/column_view.rs:347-359`、`:1266-1277`、`crates/explorer-ui/src/state.rs:9726` |
| C15 | 中 | **祖先欄建立項目時，父資料夾與查重來源不一致。** `create_folder_request()` 會將 parent 指到活動祖先欄，但 existing 名稱仍取目前分頁最右資料夾；`create_new_item_request()` 又只取歷史中的目前資料夾。從祖先欄執行建立命令，名稱衝突判斷及目的地可能不符合使用者所在欄位。 | `crates/explorer-ui/src/state.rs:6952-6997` |
| C16 | 低 | **不支援位置仍可點選 Columns，卻只顯示 Details 已勾選。** View 選單中的 Columns 一律可按；`set_view_mode()` 直接保存 Columns，實際畫面因位置不合格退回 Details。此時選單只勾 Details，沒有提示已保存的 Columns 偏好。 | `crates/explorer-ui/src/chrome.rs:10264-10282`、`crates/explorer-ui/src/state.rs:4304-4313`、`:9378-9385` |
| C17 | 低 | **新增預覽訊息未補齊語系。** `column-preview-handler-ready` 只存在英文、繁中，其他語系退回英文；完整性測試對 zh-CN 明確報此鍵缺失。 | `crates/explorer-ui/src/column_view.rs:670`、`crates/explorer-i18n/locales/en/menus.ftl:189`、`crates/explorer-i18n/locales/zh-TW/menus.ftl:189` |

## 驗收證據與宣稱的落差

| ID | 觀察 | 證據 |
| --- | --- | --- |
| E01 | `local-column-view-headful` 將找得到 View、Columns、分欄 surface，點一下資料夾再送 Alt+P 就回報 PASS；沒有斷言地址、子欄內容、預覽像素、檔案開啟或命令目的地。先前驗收執行實際找不到夾具 `b` 列。 | `scripts/test_local_column_view_headful.ps1:74-154`、`target/column-view-acceptance-20260927/report.json` |
| E02 | manifest 用 `add-local-column-view/*` 將所有 requirement 歸給局部單元測試；該測試雖通過，沒有覆蓋本清單的跨分頁、實際滑鼠右鍵、捲動與檔案操作。 | `uitest/manifest.json:58-78` |
| E03 | `docs/COLUMN_VIEW.md` 把水平捲動、預覽寬度調整、右鍵、拖放、F2、完整預覽等寫成現有操作。上述程式路徑尚不足以支持這些宣稱。 | `docs/COLUMN_VIEW.md:5-13` |

## 本次驗證與歸因界線

- `cargo check --workspace`：通過。
- `cargo fmt --all -- --check`：通過。
- `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`：模型 16 項、UI 9 項通過；不涵蓋上面多數真實交互路徑。
- `cargo test -p explorer-ui --lib -- --test-threads=8`：546 通過、10 失敗、2 ignored。失敗分散在鍵綁定、右鍵契約、版面、快取與 Home 等區域；沒有乾淨基線比對，不能全歸因於此次變更。
- `cargo test -p explorer-i18n --test catalog_complete -- --nocapture`：1 通過、2 失敗。其中一個失敗明確包含 C17；同一測試也列出其他缺鍵，另一個失敗是既有 `status-thumbnail-quota-full` 查找問題，歸因尚未確認。
- 舊審查 `openspec/changes/add-local-column-view/ACCEPTANCE_REVIEW_2026-09-27.md` 的「影像完全沒有渲染」、「檔案雙擊及 Enter 完全沒有開啟路徑」、「fmt 失敗」已與目前工作樹不符。現行程式已有 image renderer、`OpenColumnItem` 接線；這些路徑仍缺真實視窗驗證，但不應再作為已確認的缺陷列入。

工作樹在審查期間仍有修改（例如 `reveal_active_column()` 後來加入但尚無呼叫）；上列行號以審查快照為準。review 時應以各函式及實際 diff 再核對一次。
