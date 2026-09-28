# 本機分欄檢視驗收與修正建議（2026-09-27）

## 結論與範圍

**目前不能驗收通過。** 已有可編譯的分欄模式、分欄選單、部分分支模型、圖示與 session 欄位，但使用者最需要的檔案預覽、檔案開啟、連續欄位捲動及欄內操作仍有明確缺口。`tasks.md` 有 17/22 項勾選完成；其中若干項的勾選與實際 UI 接線不符，應重開至真正驗證通過。

驗收依據是本變更的 `proposal.md`、`specs/local-column-view/spec.md`、`design.md` 與 `tasks.md`。本次只讀程式、執行驗證與撰寫此報告，沒有修改功能程式碼。工作樹早在本變更前就有大量未提交修改；下列全域回歸失敗**不能直接歸因於分欄變更**，須做基線比對。

## 本次實測

| 驗證 | 實際結果 | 判讀 |
|---|---|---|
| `openspec validate add-local-column-view --strict` | PASS | 規劃語法有效，不能證明功能完成。 |
| `cargo check --workspace` | PASS | 工作區可編譯。 |
| `cargo fmt --all -- --check` | FAIL | `crates/explorer-ui/src/chrome.rs`、`column_view.rs`、`lib.rs` 有格式差異。 |
| `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8` | PASS，模型 16 項、UI 6 項 | 多為模型／靜態路由測試，未證明真實預覽與滑鼠操作。 |
| `cargo test -p explorer-model -p explorer-ui --lib -- --test-threads=8` | FAIL，`explorer-ui` 541 PASS、10 FAIL、2 ignored | 包含鍵綁定數量、背景右鍵契約、版面、快取等；需與既有 dirty 基線區分。 |
| `cargo run -p explorer-uitest -- --validate-only` | 無法啟動 | 套件有多個 binary；指令須加 `--bin explorer-uitest`。`GROK_IMPLEMENTATION_PROMPT.md` 內也需同步修正。 |
| `cargo run -p explorer-uitest --bin explorer-uitest -- --validate-only` | FAIL | 全庫有 548 個未覆蓋 requirement；輸出包含大量其他 change，不能視為本功能單獨失敗。新變更目前用一個萬用 `covers` 宣稱所有 requirement。 |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test_local_column_view_headful.ps1 -OutputDirectory D:\SuperExplorer\target\column-view-acceptance-20260927` | FAIL | 找到「檢視」、「分欄」與分欄表面；找不到夾具 `b` 列。詳見 `target/column-view-acceptance-20260927/report.json`。紀錄顯示初始視窗在 `C:\`，腳本沒有先證明已進入夾具。 |

既有 `openspec/changes/add-local-column-view/evidence/icons-2026-09-27/column-icons.png` 可看出第一欄有圖示，但右側預覽只有文字，畫面不能作為影像／文件預覽成功的證據。較早的 `headful-2026-09-26-rerun/report.json` 只驗證選單與分欄表面存在，沒有驗證子欄、預覽、鍵盤或檔案命令。

## 必修問題與具體修改

### P0-1：整合預覽沒有顯示影像，也沒有可信的處理器畫面

**證據：** `crates/explorer-ui/src/column_view.rs:555-605` 的 `column_preview()` 只畫名稱、路徑與狀態，參數是 `_show_image`，沒有使用 texture 或 `img()`。`crates/explorer-ui/src/lib.rs:10720-10753` 雖然產生 `preview_texture` 並傳給 `ExplorerWindow`，`chrome.rs:1132-1155` 建立 `ColumnStrip` 時未把它傳入。`state.rs:9561-9598` 對任何有本機路徑的單一項目固定顯示「載入中」，也未區分資料夾、離線項目或真正的失敗。現有 `preview_aspect_fit()` 只有單元測試，未接到實際渲染。

**修法：** 把 `preview_texture`、失敗狀態及 broker lifecycle 結果傳入 `ColumnStrip`；影像以 `ObjectFit::Contain` 畫在有尺寸的預覽區。非影像檔要有正確的 broker host 子區域、載入／可用／失敗／重試狀態；資料夾與離線項目不能永遠顯示載入中。以一張 JPEG 與一個有 Preview Handler 的本機檔做 headful 斷言：影像像素或 handler HWND 真的位於右側預覽框，切換選取後過期內容消失。

### P0-2：檔案雙擊與 Enter 沒有送出開啟命令

**證據：** `column_view.rs:397-443` 對檔案雙擊產生 `OpenColumnItem`；`lib.rs:8970-8983` 卻和單擊一樣呼叫 `activate_column_item()`。`state.rs:9634-9689` 只有選取或資料夾導覽，對檔案回傳 `None`。`lib.rs:9609` 把 Enter 映射成向右；模型 `column_view.rs:493-521` 對檔案產生的 `open_file: true` 未被 UI 消費。因此雙擊與 Enter 不會走既有 `ExplorerCommand::OpenItem`。

**修法：** 區分「選取」與「開啟」action；檔案雙擊／Enter 直接以該列 `ItemDescriptor` 建立 `OpenItem`，資料夾維持正式導覽。測試應攔截實際提交的 command，斷言檔案 ID、位置與 disposition，而不是只測 `ColumnSelectEffect`。

### P0-3：欄內右鍵、拖放、重新命名等命令未真正接入

**證據：** `column_view.rs:407-445` 的列只有左鍵處理，沒有右鍵或拖放事件。`state.rs:7495-7520` 的右鍵 item 流程要求 `tab.selection.contains(item_id)`，但祖先欄位選取存在私有 `ColumnBranch`；`state.rs:7053-7074` 的 F2 重新命名仍依目前分頁 `presentation` 的 row index。`selected_items()` 雖嘗試讀欄位選取，尚不足以讓所有既有命令知道祖先欄的正確列、父資料夾與焦點。

**修法：** 將欄位命中結果統一成 `{tab, column directory, item descriptor, selected IDs}`，再接既有右鍵、拖曳、貼上、刪除與 rename 命令。F2 必須作用在活動欄的真實項目；若原有 inline editor 只能顯示在單一 `FileViewHost`，需為分欄列提供定位／提交路徑。用 runner 擁有的夾具逐項驗證：祖先欄右鍵開正確選單、F2 只改該檔、跨欄拖放與貼上落在指定資料夾、刪除後相應欄更新。

### P1-4：水平捲動沒有使用者輸入路徑，揭露位置用了固定 960 px

**證據：** `SetColumnHorizontalOffset` 只在 `actions.rs` 定義、`lib.rs:9006` 消費，沒有 UI 事件會發出；`column_view.rs:146-190` 只用負左邊距畫出 offset，沒有水平滾輪／觸控板／捲軸。`state.rs:9676` 用固定 `viewport = 960.0` 計算新欄揭露，與實際檔案區寬度無關。`SetColumnPreviewWidth` 也沒有 UI 發送端。

**修法：** 從實際 file-surface layout 取得 viewport，對橫向滾輪、Shift+wheel、觸控板與可操作的水平捲軸建立輸入路徑；當視窗、導覽欄或預覽寬度改變時重新 clamp/reveal。預覽分隔線須能真正拖曳並發出 `SetColumnPreviewWidth`。以窄視窗＋四層路徑測試每個祖先可回捲，末欄與預覽可到達。

### P1-5：垂直捲動無上界且沒有自動捲入焦點列

**證據：** `column_view.rs:252-269` 的滾輪直接以舊 offset 加減 delta；`explorer-model/src/column_view.rs:533-537` 只限制 offset 大於零，沒有以列數和 viewport clamp。模型的 `move_vertical()` 更新選取但沒有調整 `vertical_offset`；UI 列表只有 leading spacer，焦點可落在視窗外。

**修法：** 用 `max(0, row_count * row_height - viewport_height)` clamp 每欄 offset，列高與實際可用高度要一致；鍵盤上下移動、滑鼠選取與資料夾展開後呼叫 ensure-visible。加入第一列／末列／空欄／極短視窗測試，確認不會捲成整欄空白。

### P1-6：大量資料夾只虛擬化「畫出哪些列」，仍每次建立全部列模型

**證據：** `state.rs:9438-9499` 每次產生 `column_strip_model()` 都對每一欄 snapshot 做 `to_vec()` 再 `rows_from_entries()`，後者在 `column_view.rs:75-95` 複製每筆 ID、名稱、位置及 metadata；真正的 `fixed_virtual_range()` 到 `column_view.rs:218-284` 才套用。所有祖先欄也一起建立，不是只處理可見欄。10 萬筆資料夾的單元測試只測 range 函式，未測 render/model 建構成本。

**修法：** 先由 viewport 算可見欄與列的 range，再只 materialize 可見列（含小量 overscan）；避免每次 paint 複製全量 snapshot，使用共享 snapshot／Arc 與索引。以 100,000 項、至少五層路徑量測模型建構時間、配置量與互動延遲，並測滾動時才要求對應圖示。

### P1-7：祖先欄沒有沿用隱藏檔設定與排序

**證據：** `state.rs:9458-9473` 的活動欄使用 `FilePresentation`，祖先欄直接拿 `snapshot.entries().to_vec()`；`column_view.rs:75-95` 原樣轉列，沒有 `hidden_items`、sort 或資料夾優先投影。規格要求每欄遵守現有顯示規則。

**修法：** 對每一欄使用同一套 `FilePresentation::build_filtered`（或等價共享投影），並保存投影索引到原始 item ID。用混合隱藏檔、資料夾與檔案的兩層夾具，切換顯示隱藏項目與排序方向，斷言每欄順序一致。

### P1-8：輔助列舉的併發與背壓契約有漏洞

**證據：** `state.rs:9853-9870` 收到第一個 `ColumnDirectoryBatch` 就呼叫 `column_loads.complete(request_id)`，但終結事件尚未到；下一次規劃可超過 `COLUMN_LOAD_CONCURRENCY = 2`。`explorer-shell-win/src/sta.rs:2747-2785` 用 `try_send` 發 batch；通道滿時把 `false` 回給列舉器，列舉改走取消／Pending，沒有明確背壓或可重試錯誤。`state.rs:9810` 把所有欄視為 visible，沒有真正的可見欄優先權。

**修法：** 請求只在 terminal 時釋放 in-flight 槽位；在取消與舊版結果路徑也恰好釋放一次。批次發送使用有界等待／背壓或明確的 resource-limited terminal，不要把滿通道當使用者取消；只優先載入 viewport 內與緊鄰的祖先欄。加入慢 consumer、連續多 batch、快速切換、取消後終結事件測試，斷言完整筆數與最大同時請求數。

### P1-9：選取與預覽／命令狀態可能分歧

**證據：** `state.rs:9663` 的 Shift 範圍選取只傳 `std::slice::from_ref(&entry)`，無法取得錨點與目標之間的整欄順序。Ctrl/Shift 分支只更新 `ColumnBranch`，活動最右欄的 `tab.selection` 沒有同步；`state.rs:9602-9618` 對最右欄回傳 `None`，使 `selected_items()`、`lib.rs:10705-10720` 預覽來源與畫面上的欄選取可能不一致。`column_preview_text()` 對資料夾 `ItemDescriptor` 因具有 path 而誤判為檔案載入中。

**修法：** 以活動欄的完整排序投影計算 Shift 範圍；建立單一 selection bridge，讓欄列顯示、命令、狀態列、預覽調度都讀同一組 item ID。資料夾／檔案／離線判斷應從 `FileEntry` 而非只有 path 的 `ItemDescriptor` 取得。測單選、多選、跨欄切換、空選與延遲預覽，並斷言 UI 與 command payload 相同。

### P1-10：Preview Handler 的幾何與鍵盤焦點未驗證

**證據：** `column_view.rs:567-605` 的預覽根容器沒有 `.relative()`，卻直接掛上 `chrome::preview_host_boundary_probe()`；該 probe 在 `chrome.rs:8160-8200` 是 `.absolute().inset_0()`。舊版預覽窗格在 `chrome.rs:8111-8130` 把 probe 放入 `.relative()` 的預覽內容區。`state.rs:8793-8805` 的 Tab 焦點可達判斷只看普通 `preview_pane`，不看 `column_preview_visible`。

**修法：** 提供明確的相對定位預覽內容框，將 probe/handler 限制在該框內，更新 resize/DPI 時的 bounds；讓分欄預覽加入焦點順序與加速鍵轉送。以 UIA 與真實 handler HWND 檢查預覽不會遮蓋欄列，Alt+P 關閉後卸載 handler，Tab 可進出且不奪走全域快捷鍵。

### P2-11：空資料夾與失敗狀態沒有正確呈現

**證據：** `state.rs:9446-9460` 的最右欄在非 Loading、非 `level.phase == Error` 的情況下一律標為 Ready；空 `DirectoryState::Ready` 因此沒有空狀態，導航失敗也可能被顯示成空白 Ready。`column_view.rs:295-309` 只有非 Ready 才渲染狀態文字。

**修法：** 從活動 directory 的 `Idle/Loading/Ready(empty)/Ready(nonempty)/Failed/Partial` 推導欄狀態，提供可重試控制與可讀錯誤；不可把失敗當空資料夾。使用空目錄及權限不足夾具做實際 UIA/headful 斷言。

### P2-12：驗收腳本與任務勾選過早宣稱完成

**證據：** `scripts/test_local_column_view_headful.ps1:74-77` 設定夾具作啟動位置，但執行後畫面／紀錄顯示 `C:\`，腳本沒有先 assert 地址已到夾具。腳本找到 `b` 後也只點擊、送 Alt+P，再寫 PASS；沒有 assert 子欄、地址、預覽內容、關閉狀態或檔案命令。`uitest/manifest.json:58-78` 用 `add-local-column-view/*` 把所有 requirement 都歸給 22 個局部測試；headful 案沒有具體 covers，且腳本描述仍稱只記 SKIP。`tasks.md` 的 3.1、3.2、3.3、4.1、4.2、4.4、5.1、5.5 勾選需依真正證據重審。`docs/COLUMN_VIEW.md` 目前把尚未驗證的行為寫成已可用。

**修法：** 先修啟動隔離與地址 assert（參考 `scripts/smoke_inline_rename_capture.ps1`），再讓 runner 逐步驗證 folder click → 下一欄子項目 → breadcrumb/history → JPEG/handler 預覽 → Alt+P → 鍵盤／右鍵／檔案開啟，對每一步記錄實際結果與截圖。把 UITEST covers 分配到能證明該 requirement 的案例，對環境不具備的 handler 或 DPI 明確 SKIP。依驗證結果重開／完成 checkbox，文件只描述實際可用行為。

## 建議修復順序與驗收門檻

1. **恢復基礎品質門檻：** 執行 `cargo fmt --all`，再跑 `--check`；分類 10 項全量 UI 測試失敗並與原有工作樹基線比較。不可直接改測試預期來掩蓋缺陷。
2. **完成核心互動：** P0-2、P0-3、P1-9。先確定每個欄列的正式 command、選取與父資料夾一致，再接預覽。
3. **完成可見預覽：** P0-1、P1-10。用真實影像與 handler 驗證畫面、幾何、取消和故障後復原。
4. **完成瀏覽與效能：** P1-4 至 P1-8、P2-11。驗證深層路徑、排序、空／錯誤欄、100,000 項與請求背壓。
5. **重做證據：** P2-12。修正命令為 `cargo run -p explorer-uitest --bin explorer-uitest -- ...`，執行完整 headful/visual 場景；對全域 548 個未覆蓋 requirement 另立基線處理，不把此變更的測試冒充全庫覆蓋。

**通過條件：** 規格每個情境都有能觀察到結果的測試或真實 Windows 證據；`cargo fmt --check`、相關單元／整合測試與本功能 headful 測試通過；全庫失敗的歸屬、差異與剩餘限制被如實列出。最終再逐項更新 `tasks.md`，並附上 build revision、測試時間與證據路徑。
