# 分欄檢視邏輯修正與驗收

參考：`COLUMN_VIEW_LOGIC_AUDIT_2026-09-27.md`。原文件保留為當時的審查快照，本文件記錄後續修正與驗收，避免把舊行號或已修問題重新當成現行缺陷。

目前狀態：本輪 C05／C14／C15／C16 限縮修正已由 Codex 獨立審查與驗證接受。Grok 完成實作和補修，Codex 補上背景 terminal 守衛及實機 harness 修復。全功能尚有未完成門檻，詳見文末。

工作樹基準：開始時為 `master`、`7e33dcc0da2b5e86c37231eac58de669466588b2`，含先前未提交變更。Grok 讀碼期間，外部提交將 HEAD 更新為 `c984e91f0f4a8f1d9a94b534d9eb69ad36cd7dd2`。Codex 已確認保存的 12 個起始原始碼快照與該 HEAD 逐項相同；Grok 執行紀錄沒有提交命令。本輪保留外部提交，不提交、不推送、不重設其他變更。

## 本輪再次確認的問題

| 稽核 ID | 現行問題 | 修正與驗收要求 |
| --- | --- | --- |
| C05 | 協調器把其他分頁的載入一併視為過期；切換分頁會取消有效請求。 | A→B→A 保留有效載入；同分頁過期、關閉或離開 Columns 僅取消相應請求；總併發上限仍為 2，終結事件才釋放槽。 |
| C14 | 正式選取仍傳入未解析路徑；測試注入祖先 key 無法證明 junction 真的被攔截。 | 在服務端解析實際檔案系統身分，經正式事件送回 UI；回指祖先時不繼續導航，保留可用祖先；正常 junction 可以進入。 |
| C15 | 父目錄已修正，但查重使用可見投影，漏掉隱藏項目；祖先資料缺失時仍可能借用最右欄名稱。 | CreateFolder／CreateItem 的目的地與完整名稱快照來自同一欄；缺快照時交由後端 KeepBoth 處理，不能誤用別的目錄。 |
| C16 | 不支援位置的 Columns 仍可透過選單或鍵盤啟用，保存偏好與畫面指示容易不一致。 | 視覺停用、滑鼠／鍵盤／直接 action 均阻擋新選擇；已保存偏好仍可在回到本機後恢復，Details fallback 指示必須準確。 |

### 驗收方式

1. 由 Grok 修正限縮的程式與回歸測試；Codex 獨立審查本輪差異。
2. 模型／UI 測試驗證跨分頁請求、過期事件、建立父目錄及完整名稱來源、不可用選單與偏好恢復。
3. Shell 測試使用 Codex 建立的真實 Windows junction：`a/b/back` 回指 `a`；`a/b/elsewhere` 指向 `outside`。不能以注入字串 key 作為唯一循環證據。
4. 重新建置後，以獨立程序實際點擊循環及正常 junction，檢查地址、狀態與可見內容。
5. 執行 workspace check、格式、聚焦回歸、OpenSpec 驗證，記錄實際結果。

## 舊稽核其他項目的後續狀態

以下為本輪啟動前已完成的修正與既有證據，不代表本輪重跑了全部情境。

| ID | 後續修正與證據 |
| --- | --- |
| C01 | 命令／預覽／狀態使用活動欄選取，取消選取會同步清空；選取與 stale preview 聚焦測試已通過。 |
| C02 | 已接上列右鍵、拖放與背景目的地；四項命令視窗矩陣通過，包含祖先 Delete、paste、跨欄 Move、原生右鍵。 |
| C03 | F2 使用活動欄真實項目身分；視窗測試以實際檔案重新命名結果驗證。 |
| C04 | batch 不再釋放併發槽，terminal 才釋放；重複 terminal 無效，聚焦測試通過。 |
| C06 | batches／terminal 使用有序有界傳送；佇列滿回報資源失敗，不冒稱使用者取消或成功；Shell 壓力聚焦測試通過。 |
| C07 | Shift 使用整欄顯示順序，Ctrl 切換與跨欄重設已測試；真實 Shift／Ctrl 鍵盤案例通過。 |
| C08 | 祖先及最右欄共用排序／隱藏／資料夾優先投影，選取與鍵盤使用相同順序；混合資料聚焦測試通過。 |
| C09 | 已有水平捲輪／捲軸與實際 viewport reveal；深路徑、thumb／track／divider 視窗案例通過。 |
| C10 | 垂直 offset 依列數／高度 clamp，鍵盤會 reveal 焦點；聚焦測試及鍵盤視窗案例通過。 |
| C11 | 已接上預覽 divider 拖曳；JPEG／水平及 PDF Quick 測試驗證寬度變化。 |
| C12 | 在建立 row model 前限定可見範圍，重用投影；100,000 項／五層測試有界列配置通過，數字是本機 debug 測量。 |
| C13 | 分別顯示 Loading、Empty、Error／Retry；空目錄視窗案例通過。不可存取資料夾的真實視窗證據仍未完成。 |
| C17 | `column-preview-handler-ready` 已補齊全部 20 語系；其他既有 i18n 缺鍵另列。 |
| E01 | 基礎視窗案例已斷言真實夾具列、地址、子欄、預覽像素／清除；另外補上命令及鍵盤矩陣。 |
| E02 | Columns 新需求已逐項映射，驗證輸出沒有未映射的 `add-local-column-view` 需求；全庫其他需求仍有缺口。 |
| E03 | 文件按已觀察的操作更新；PDF 的完整 DPI／崩潰等案例保留為未完成，不當成 PASS。 |

既有完整證據：`openspec/changes/add-local-column-view/ACCEPTANCE_STATUS_2026-09-28.md` 及 `.ai-collab/review.md`。

## 本輪 Grok 作業與 Codex 驗收

第一輪 Grok 作業 `run-mulcvmwa-xhyv2j` 完成，回報模型 24／UI 36／Shell 5 項聚焦測試通過。這些是 Grok 的結果，尚未代替 Codex 獨立驗證。

Codex 原始碼審查找出兩項具體缺口，已在同一 Grok thread 委派限縮補修：

- **R1／C14：舊身分回覆仍可能導航。** Ctrl／Shift 改選、取消選取或切換 Details 不一定改變 branch revision；目前 consumer 沒有確認選取意圖、模式或取消 token。取消終結又會被一般 Failed fallback 當成可導航。要求明確取消待處理意圖、獨立拒收過期回覆，並以真實狀態操作重現。
- **R2／測試入口：一般 Shell 測試依賴本機 junction 夾具。** 要改成明確的 opt-in 實機測試；一般測試不依賴固定目錄，明確執行實機測試時仍須檢查環境變數與夾具，不能默默 PASS。

補修作業：`run-muldy5v8-cwjd9a`，thread `88f634a4-b412-46fc-893e-8b76465f3f09`。補修已完成；Codex 獨立測試確認改選、切換模式／分頁、關閉、重新整理、取消及重複回覆不再引發舊導航。

### C05：請求按分頁失效，背景回覆只更新自己的分支

- 協調器以 `tab_id + generation + revision` 判斷過期，只取消同一分頁失效的請求。其他分頁的有效請求保持運行。
- 關閉分頁或離開 Columns 時，僅取消該分頁的輔助載入。取消仍須等 terminal 到達後才釋放併發槽，維持全域最多兩個請求。
- Codex 另重現一個整合缺陷：背景 A 的 Empty／Finished 回覆會呼叫針對活動 B 的分支修復；兩者開啟相同路徑時，可能截掉 B 尚在載入的子欄並排入導航。
- Codex 加入活動分頁守衛。背景回覆仍更新 A 的資料及釋放其槽，但不修復 B。新測試 `column_view_inactive_terminal_preserves_the_active_tab_branch` 修正前確實 FAIL、修正後 PASS，再接受 B 的後續 batch／terminal。
- 修改入口：`crates/explorer-model/src/column_view.rs` 的 `supersede_stale`／`supersede_tab`；`crates/explorer-ui/src/state.rs` 的載入、關閉分頁及 `apply_column_event`。

### C14：以真實檔案系統身分攔截回指，拒收失效意圖

- 服務端透過既有 Windows 檔案身分 API 解析資料夾及祖先；跟隨 junction 目標，以 volume／file ID 比較，而非只比較路徑字串。
- 正式 `ResolveColumnCycle` 命令及 `ColumnCycleResolved` 終結事件連接 Shell、模型和 UI；身分查詢在服務執行緒進行，不在 UI render 路徑同步操作檔案系統。
- 回指祖先時保留目前有效地址與父欄，顯示「此資料夾連結回先前的欄位」；不同身分的普通資料夾及 junction 正常導航。
- 待處理請求要同時符合活動分頁、generation、revision、有效 Columns 模式、活動欄及單一選取身分才可生效。Ctrl／Shift 改選、清空、鍵盤移動、模式／分頁切換、刷新及關閉均取消舊意圖。
- Cancellation 結果不導航；不支援的位置拒絕。身分 API 的一般讀取／可用性失敗仍沿用文字路徑導航，以維持既有行為；因此不能宣稱所有身分解析失敗時也能阻擋循環。
- 修改入口：模型的 `ColumnFilesystemIdentity`／`ColumnCycleOutcome`、`protocol.rs`、Shell `sta.rs`、UI `cancel_pending_column_cycle`／`column_cycle_intent_matches`／事件 consumer。

### C15：建立目的地與查重快照來自同一欄

- CreateFolder／CreateItem 從活動欄取得目的地，並從該欄未篩選的完整快照收集名稱。隱藏／系統檔即使不顯示也參與查重。
- 祖先欄缺少快照時，不借用最右欄名稱；交給後端 `KeepBoth` 處理真實目的地衝突。
- 定向測試覆蓋祖先／最右欄不同內容、隱藏名稱、大小寫及後綴、快照缺失，以及 Details 的既有行為。
- 修改入口：UI `command_parent_names`、兩項建立命令及對應 reducer 測試。

### C16：不可用位置停用新選擇，保留已有偏好

- `ColumnsMenuPresentation` 以目前位置產生 enabled／checked／名稱；不支援位置顯示「分欄：無法使用」，Details 為有效 fallback。
- 不可用項目灰色顯示，移除 click／focus／tab handlers；選單鍵盤跳過該項，Enter／Space、直接 action 和 reducer 均有守衛。
- 若使用者之前已選 Columns，進入常用頁時暫時顯示 Details；回到本機恢復 Columns。常用頁上的無效點擊不會寫入新的 Columns 偏好。
- zh-TW／zh-CN 的 unavailable 訊息改為中文。未增加新的翻譯鍵。
- 修改入口：UI `chrome.rs` 的停用列、`actions.rs`／`lib.rs` 的派送守衛、`state.rs` 的模式設定與選單呈現。
- 可及性限制：本輪 UIA 仍回報 Button 的 `IsEnabled=True`，且沒有 TogglePattern。操作阻擋已由狀態測試與實際視窗驗證；原生 disabled／checked 語意仍需另行修補 GPUI 可及性層，不能當成已完成。

## Codex 獨立驗證紀錄

所有定向結果使用本輪最後的 Rust 原始碼，包含 Codex 的背景 terminal 守衛；不只採用 Grok 自報。

| 驗證 | 結果 | 證據 |
| --- | --- | --- |
| `cargo check --workspace` | PASS | `target/column-logic-check-codex-20260928.txt` |
| `cargo fmt --all -- --check` | PASS | `target/column-logic-fmt-codex-20260928.txt` |
| OpenSpec strict validation | PASS | `target/column-logic-openspec-codex-20260928.txt` |
| 模型／UI `column_view` 定向測試 | 模型 25、UI 38 PASS | `target/column-logic-tests-codex-20260928.txt` |
| 原有鍵盤派送及 Shift／Ctrl 選取兩項回歸 | 各 1 PASS | `target/column-logic-key-routing-codex-20260928.txt`、`target/column-logic-key-selection-codex-20260928.txt` |
| Shell 預設列舉測試，不設夾具環境變數 | 4 PASS、1 ignored | `target/column-logic-shell-default-codex-20260928.txt` |
| 真實 junction 身分與正式事件 consumer | 1 PASS，明確 `--ignored` | `target/column-logic-shell-fixture-codex-20260928.txt` |
| i18n library tests | 9 PASS | `target/column-logic-i18n-codex-20260928.txt` |
| 背景 terminal 缺陷重現 | 修正前 FAIL、修正後 PASS | `target/column-logic-inactive-terminal-before-fix-20260928.txt`、`target/column-logic-inactive-terminal-after-fix-20260928.txt` |
| `SuperExplorer.exe` 重建 | PASS | `target/column-logic-build-codex-20260928.txt` |
| 九項鍵盤／邏輯視窗矩陣 | 9 PASS；勾選值 UIA 子斷言 1 SKIP | `target/column-logic-keyboard-uitest-final-v4-20260928/report.json` |
| 基本導航／子欄／Alt+P 回歸 | PASS | `target/column-logic-basic-headful-20260928/report.json` |
| 四項命令回歸，本輪最新矩陣 | 1 PASS、2 FAIL、1 SKIP；桌面前景干擾，需空閒桌面補跑 | `target/column-logic-commands-headful-v2-20260928/report.json` |

### 真實 junction 測試重跑方式

先建立自己的夾具：`a/b/back` junction 指向 `a`；`a/b/elsewhere` 指向同一夾具內的 `outside`；該目錄有普通檔案。不得使用或刪除使用者既有資料夾。

```powershell
$env:SUPEREXPLORER_COLUMN_CYCLE_FIXTURE = 'D:/SuperExplorer/target/column-logic-cycle-fixture-20260928-487f24cb'
cargo test -p explorer-shell-win --lib column_enumeration_column_view_cycle_fixture_identity_event_consumer -- --ignored --test-threads=1 --nocapture
Remove-Item Env:SUPEREXPLORER_COLUMN_CYCLE_FIXTURE
```

這個測試現在明確 opt-in。一般 library run 不依賴本機絕對路徑；明確執行卻缺少環境變數或正確 junction 時仍失敗，不默默通過。

### 視窗測試修復與證據保留

`scripts/test_local_column_view_keyboard_headful.ps1` 從五個增加為九個子案例；新增循環 junction、正常 junction、沒有已存偏好的不可用選單、已存 Columns 偏好的不可用選單。

首輪 `target/column-logic-keyboard-uitest-codex-20260928` 為 6 PASS／3 FAIL，失敗證據保留：箭頭案例停在選單開啟；兩個不可用案例停在常用頁導航，尚未到達產品斷言。Codex 修復 harness 的實際指標焦點、滑鼠按下／放開時序、導航窗格／上一頁操作、Unicode isolates 名稱查詢及 PowerShell `$HOME` 唯讀變數衝突。這些失敗沒有被改寫成產品 PASS。

中間兩輪 `target/column-logic-keyboard-uitest-final-20260928`、`target/column-logic-keyboard-uitest-final-v2-20260928` 也保留 FAIL，主要停在指標開啟選單。`final-v3` 為八項 PASS／一項 FAIL，焦點案例仍停在選單開啟。最後加入共用 DPI-aware 指標 helper、owned HWND／前景檢查、真正的滑鼠事件間隔，以及僅限「尚未開啟選單」的三次有界重試；不重試或放寬功能結果斷言。

最後正式 UITEST `target/column-logic-keyboard-uitest-final-v4-20260928/report.json` PASS，耗時約 84 秒，九個子案例如下。`view-menu-checked-state` 另報 SKIP，並非第十個完整通過案例。

| 子案例 | 結果／實際斷言 |
| --- | --- |
| `view-menu-keyboard` | PASS；用鍵盤啟用 Columns 並讀到真實 Pane。 |
| `arrow-address` | PASS；Down／Up、Right 進入 nest、Left 返回父欄焦點與地址符合既有契約。 |
| `shift-ctrl-selection` | PASS；Shift 範圍、Ctrl+Space 切換及 Ctrl+Down 保留活動欄選取。 |
| `accessible-names` | PASS；欄、列、預覽及狀態的 UIA 名稱／角色可讀。 |
| `focus-return` | PASS；跨欄與 Tab／Shift+Tab 後方向鍵作用於活動欄。 |
| `cycle-back` | PASS；真實 junction 回指夾具祖先，顯示循環訊息、地址保持在 nest，父欄仍在。 |
| `junction-normal` | PASS；正常 junction 進入 elsewhere，子檔 outside.txt 可見。 |
| `disabled-menu-new` | PASS；常用頁滑鼠／鍵盤操作不可用 Columns，返回本機仍非 Columns。 |
| `disabled-menu-saved` | PASS；本機先選 Columns，常用頁操作不能覆寫偏好，返回本機恢復 Columns。 |

Codex 已檢視不可用選單的實際截圖：Columns 灰色並顯示「無法使用」，Details 的視覺勾選正確。UIA `IsEnabled=True`／缺 TogglePattern 的限制仍保留。完整截圖位於相應子案例的 `disabled-menu.png`；返回本機證據為 `return-local.png`。

### 命令回歸的桌面阻礙

基本導航回歸已在最後 binary 上 PASS。四項命令矩陣首輪停在選單開啟；Codex 在命令腳本補上相同的 foreground／owned HWND／DPI-aware 指標與滑鼠間隔後，第二輪的祖先欄 Delete PASS，實際只移除 runner-owned `delete-me.txt`，其他檔案保留。

第二輪其餘結果保留為：paste FAIL（點擊前目標已被其他視窗蓋住）、drag FAIL（沒有完成子欄開啟，失敗畫面亦被其他視窗蓋住）、context menu SKIP（明確記錄 foreground HWND 不等於測試 HWND）。這些不能當成命令成功，也未據此判定產品回歸。沒有重跑完整產品測試來掩蓋失敗。

目前阻礙是互動桌面前景／游標被其他視窗使用。Codex 已詢問可否提供約 60 秒空閒桌面以補驗。命令腳本新增按鍵前 foreground 守衛；測試 HWND 非前景時不截取背後其他視窗畫面。尚未補跑的三項仍開放，不以先前日期的 PASS 取代本輪證據。

補跑指令（僅操作新建 runner-owned 夾具）：

```powershell
cargo run -p explorer-uitest --bin explorer-uitest -- --case local-column-view-commands-headful --output target/column-logic-commands-headful-final-20260928
```

### Grok 執行紀錄

| 作業 | 結果 | 模型／時間 | Bridge 回報成本 |
| --- | --- | --- | --- |
| `run-mulcit4s-h6qdjr` | Codex 停止，未採用修改 | 首次預設模型、約 9 分 58 秒 | 不併入後兩輪數字 |
| `run-mulcvmwa-xhyv2j` | 完成四項限縮實作 | `grok-4.7-build-fast`、26 分 12 秒 | USD 17.7976162 |
| `run-muldy5v8-cwjd9a` | 完成 R1／R2 補修 | 同 thread、8 分 43 秒 | USD 7.87848 |

後兩輪 Bridge 回報合計 USD 25.6760962；這是已回報的兩輪費用，不是整個對話／帳戶用量。首次安靜期間後來確認可能在原生讀碼工具工作；後續不以缺少文字進度作為停滯證據。

## 後續修改建議與具體門檻

本輪原始碼修正已限定在四項殘留邏輯；下面是仍需獨立處理的工作，不能以本輪 PASS 關閉。

| 優先度 | 修改／補驗方向 | 接受門檻 |
| --- | --- | --- |
| P2 | 在 GPUI 可及性層提供真正 disabled 與 checked 語意，對停用 Columns 設置 disabled property；檢視項目選擇適合的 checkable role／pattern。 | Home 上 UIA `IsEnabled=False`；Columns 已保存時能讀回偏好或明確說明 fallback；本機模式的勾選值可讀取。保留本輪 action／reducer 防護，九項矩陣不得回歸。 |
| P2 | 真實外部刪除目前／祖先資料夾後按 F5，以現有分支修復流程回到最近可存取祖先；先寫失敗重現再改 reducer。 | 活動地址、欄位、選取、預覽與歷史一致；不會由背景分頁回覆截斷活動分支；以 runner-owned 夾具驗證。 |
| P2 | 補足 PDF Handler 的視窗 resize、DPI、焦點與崩潰壓力案例，將同步 Win32／UIA 動作移入有界 worker，避免整套驗收掛住。 | 有真實 PDF handler、owned HWND 的邊界／卸載／焦點／恢復證據；每項單獨報 PASS／FAIL／SKIP，不能以 Quick 代替完整結果。 |
| P2 | 檢視身分解析 Availability 失敗時的路徑 fallback 是否符合產品預期；若改成拒絕，須給可理解的重試訊息。 | 分別覆蓋取消、unsupported、讀取失敗、普通資料夾、正常 junction、回指 junction；不把取消視為可導航失敗，也不造成普通資料夾全部不能開。 |
| Gate | 對先前全庫 UI／翻譯／coverage 失敗取得可比較的乾淨基線，再逐項指派相應模組修正。 | 完整測試列出一致的 commit／命令／失敗名稱，修正前後可歸因；分欄定向 PASS 不取代全庫 gate。 |

## 仍有的全功能驗收界線

- PDF Preview Handler 的實際視窗 resize／DPI／focus／crash 完整壓力測試尚未全數通過；現有 Quick 有明確 SKIP。
- 外部刪除資料夾後的 F5 分支修復等情境仍需獨立驗證。
- 全庫 UI library tests、全庫 UITEST coverage 與 i18n completeness 有先前已記錄的失敗；沒有乾淨基線歸因，不能宣稱全庫全綠。

本輪四項邏輯修正的接受，不等於整個 Columns 功能的所有驗收已完成。
