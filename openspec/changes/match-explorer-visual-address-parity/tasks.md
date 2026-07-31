核准來源：[`docs/superpowers/specs/2026-07-27-explorer-visual-address-parity-design.md`](../../../docs/superpowers/specs/2026-07-27-explorer-visual-address-parity-design.md)。契約：[`proposal.md`](proposal.md)、[`design.md`](design.md)、[`explorer-visual-parity`](specs/explorer-visual-parity/spec.md)、[`interactive-breadcrumb-address`](specs/interactive-breadcrumb-address/spec.md)。實作前盤點：[`docs/EXPLORER_VISUAL_ADDRESS_BASELINE.md`](../../../docs/EXPLORER_VISUAL_ADDRESS_BASELINE.md)。

## 13. Explorer 滑鼠、選取、焦點與快捷鍵一致性

### 右鍵選單座標
- [x] 13.1 測試 GPUI client 座標、視窗外框座標與 DPI，保證右鍵選單不得重複加上 non-client origin。
- [x] 13.2 背景、單項與多選原生 context menu 共用一次 client-to-screen 轉換；鍵盤叫出則錨定 focused row。
- [x] 13.3 驗證多螢幕負座標與 100/125/150/175/200% DPI 的 Win32 `POINT` 語意。

### 網址列與搜尋列視覺、焦點生命週期
- [x] 13.4 新增 Explorer input typography、line-height、selection 色彩契約測試並提高字級。
- [x] 13.5 修正網址列全選的前景、背景與 caret 顏色，涵蓋 light/dark/high-contrast。
- [x] 13.6 對齊兩個 input 的內容高度、padding、圖示位置、文字 baseline 與垂直置中。
- [x] 13.7 點擊檔案、資料夾、空白、導覽、工具列、搜尋列、分頁時取消 draft/error 並恢復 breadcrumb。
- [x] 13.8 window deactivate、tab switch 與 navigation submit 關閉網址編輯/menu，不保留舊 draft。

### Shift/Ctrl 選取模型
- [x] 13.9 `SelectionModel` 擴充 stable-id selected/focused/anchor，測試 clear/replace/toggle/range/additive/reconcile。
- [x] 13.10 Shift-click 依排序／篩選後 presentation order 連選；Ctrl+Shift 加入範圍；click 更新 anchor；Ctrl-click toggle。
- [x] 13.11 右鍵已選項保留多選、未選項單選、空白清除，對齊 Explorer。

### 滑鼠框選與 pointer capture
- [x] 13.12 建立 window-scoped typed marquee session，保存起點、目前點、原選取、modifier 與 terminal reason。
- [x] 13.13 空白左鍵超過 drag threshold 後顯示 Explorer 色彩的半透明 selection rectangle。
- [x] 13.14 Details/List/Icon/Tiles/Content 依 item bounds 相交更新 stable-id selection；Ctrl 框選保留原選取。
- [x] 13.15 共用 Win32 pointer capture，mouse-up/Esc/blur/capture-lost/tab/view switch exactly-once 結束。
- [x] 13.16 接近上下邊界時自動捲動並重算相交項目。

### Explorer 鍵盤與滑鼠矩陣
- [x] 13.17 Arrow/Home/End/PageUp/PageDown 支援普通、Shift、Ctrl、Ctrl+Shift 四種選取語意。
- [x] 13.18 實作 Space/Ctrl+Space、Ctrl+A/Ctrl+I、Enter/Ctrl+Enter、F2、Delete/Shift+Delete、Menu/Shift+F10、Alt+Enter。
- [x] 13.19 對齊 Backspace、Alt+Left/Right/Up、F5、Ctrl+L/Alt+D、Ctrl+F/F3、Ctrl+T/W/Tab/Shift+Tab、Escape。
- [x] 13.20 雙擊、空白 click、drag threshold、右鍵與快捷鍵導入同一 typed dispatcher。
- [x] 13.21 補齊 tracing、accessibility selected/focused 與 EditableText scope 測試。

### 驗證
- [x] 13.22 新增 model/UI/render tests，涵蓋座標、焦點、文字色、range、marquee terminal 與快捷鍵。
- [x] 13.23 使用真實 `D:\test` 在 Details 與圖示檢視執行 headful smoke 並保存證據。
- [x] 13.24 執行 fmt、check、Clippy、tests、OpenSpec strict、diff-check，只提交本 change 追蹤檔。

## 1. 參考基準與現況盤點

- [x] 1.1 將核准設計文件、OpenSpec proposal、design 與兩份 capability specs 互相連結，確認主要參考環境均為 Windows build 26200、Explorer `10.0.26100.8875`、繁中淺色、175% DPI、`D:\` Details view。
- [x] 1.2 保存目前 app commit、GPUI-CE gitlink、Cargo.lock hash、Windows/Explorer version、實際 window DPI、theme、font、視窗 client bounds 與 `D:\` location metadata，建立本 change 的 before snapshot。
- [x] 1.3 盤點 `explorer-ui` title/tab、navigation、command、pane、details header、rows、status、search 與 caption controls 的 render tree、stable IDs、actions、focus handles 與現有 logical geometry。
- [x] 1.4 盤點 production chrome 內所有 Unicode／文字暫代 icon，逐項記錄目前字元、預期 Explorer icon、公開來源候選與可見尺寸。
- [x] 1.5 盤點現有 light/dark/high-contrast theme tokens 與直接固定 RGB，將每個使用點映射到 Explorer semantic region。
- [x] 1.6 盤點現有字型 family、字級、字重、行高與 GPUI text rendering 設定，找出直接固定值及缺少 diagnostics 的 surface。
- [x] 1.7 盤點網址列 editor entity、FocusAddress、Ctrl+L/Alt+D、Enter/Esc、IME 與 active/background tab 更新路徑，建立目前行為測試清單。
- [x] 1.8 盤點 common/model/Shell typed contracts 是否已能表達 Shell ancestry、container capability、child-only enumeration 與 native icon payload；缺口逐項對應後續任務。
- [x] 1.9 驗證既有 Explorer 與 app reference screenshot 可重現，將無法重現、過期或 metadata 不完整的 artifact 標記為 evidence-only，不自動覆寫。
- [x] 1.10 執行變更前 `cargo fmt --check`、workspace check、Clippy、tests 與 headful startup smoke，保存結果作回歸基準。

## 2. 具名區域診斷與比較工具

- [x] 2.1 先為 region diagnostics schema 撰寫 parser/validation tests，涵蓋唯一 region id、logical/physical rect、DPI、parent、state、icon bounds 與 typography reference。
- [x] 2.2 定義版本化 `ExplorerRegionDiagnostics` schema，包含 window client area、title/tab、navigation、command、pane、divider、details header、rows、status、search、caption controls 與動態子控制項。
- [x] 2.3 在 GPUI root 建立 geometry collection boundary，確保 component 只回報自己 layout 後的 bounds，不以重複手算取代實際 geometry。
- [x] 2.4 為每個現有 chrome stable ID 接上 region diagnostics，測試同一 frame 不得遺漏、重複或產生非有限座標。
- [x] 2.5 在 capture metadata 同時輸出 logical rect、physical rect、實際 scale factor 與 rounding method，測試 physical 值只縮放一次。
- [x] 2.6 先為 10% comparator 撰寫單元測試，涵蓋 left/top/right/bottom、center、width/height、gap、零尺寸與 reference 小於 10 logical px 的 1 px tolerance。
- [x] 2.7 實作 region comparator，逐欄輸出 reference、actual、absolute delta、relative delta、threshold 與 pass/fail，不以全圖 changed-pixel ratio替代。
- [x] 2.8 新增 favorites 動態內容 mask 設定，只忽略內容文字/數量，不得忽略 navigation pane 起點、縮排、列高、分隔線或 selected row geometry。
- [x] 2.9 新增 typography edge mask 與動態時間/狀態 mask，將 mask rectangle、理由與來源寫入 metadata，禁止 mask layout bounds。
- [x] 2.10 新增 overlay renderer，將 reference/actual 具名矩形、中心點與超標邊界以不同顏色畫在輸出影像上。
- [x] 2.11 擴充 comparison report，輸出 region summary、最差誤差排序、icon report、color samples、typography report 與可重現命令。
- [x] 2.12 建立 comparator CLI/PowerShell wrapper 的成功、超標、schema mismatch、image size mismatch 與缺 metadata tests。
- [x] 2.13 確保所有 compare scripts 預設只讀 baseline，只有明確 baseline update 命令且通過人工確認標記才可更新。
- [x] 2.14 用現有 `light-diff-175` Explorer/app artifact 跑新版 region pipeline，保存 before report 並確認目前差距會被具名指出。

## 3. Explorer layout、色彩與 typography token 系統

- [x] 3.1 先建立 Explorer layout profile contract tests，列舉 title/tab、navigation、command、pane、divider、details header、row、status、search、caption 與各控制 padding/gap 必要鍵。
- [x] 3.2 將粗粒度 `LayoutTokens` 擴充為區域化 tokens，所有值維持 logical px 且附 reference profile 名稱，避免 component 散落主要尺寸。
- [x] 3.3 以 reference screenshot 與 diagnostics 校準主環境的區域高度、navigation pane 寬度、details columns、row height、status height、搜尋框寬度及 chrome gaps。
- [x] 3.4 為固定控制項與 flex 區域定義寬度優先序：navigation buttons、address、search、command overflow、caption controls，並測試窄視窗不重疊。
- [x] 3.5 建立 100/125/150/175/200% logical-to-physical geometry table tests，驗證每個新增 token 只套用一次 scale factor。
- [x] 3.6 建立 resize/maximize/restore tests，驗證 navigation pane 比例、address/search 分配、details columns 與 status controls 不漂移。
- [x] 3.7 先擴充 semantic theme contract tests，涵蓋 Explorer surface、toolbar fill、address fill、search fill、divider、row hover/selected、disabled、focus、menu 與 caption states。
- [x] 3.8 將 light theme 平坦色塊校準至 Explorer reference，每個 sRGB channel 差值不超過 12，保留 sample region 與量測日期。
- [x] 3.9 校準 dark theme 對應 semantic colors，不以 light 反相產生，並擷取相同 states 的 app-only regression baseline。
- [x] 3.10 將 high-contrast mapping 改為 Windows system semantic colors，測試文字、選取、focus 與 disabled control 的對比及可辨識性。
- [x] 3.11 定義 `TypographyTokens`，至少包含 tab、command、address、search、navigation、details header、file row、menu、tooltip、status 的 family fallback、size、weight、line height。
- [x] 3.12 為繁中 Windows UI 字型 fallback 撰寫測試，確認 Microsoft JhengHei UI／系統 UI fallback 可用且不把單一英文 family 套到所有語系。
- [x] 3.13 依 Explorer reference 校準各 surface 字級、字重、line height 與 baseline，字級誤差不超過 1 logical px。
- [x] 3.14 將 theme/layout/typography tokens 經單一 root context 注入，加入 architecture/source test 防止 feature component 建立第二份來源。
- [x] 3.15 掃描新增與既有 production UI 的固定主要尺寸、固定 RGB 與未 tokenized 字級，逐項遷移或加入具體例外理由。

## 4. Chrome 與 Shell icon pipeline

- [x] 4.1 先定義 `ExplorerIcon` enum contract tests，涵蓋 Back、Forward、Up、Refresh、New、Cut、Copy、Paste、Rename、Share、Delete、Sort、View、More、Details、Search、Close 與 chevron。
- [x] 4.2 為每個 chrome icon 定義穩定 id、公開來源、view box、stroke/fill mode、logical glyph size、hit target、enabled/disabled/hover/pressed color。
- [x] 4.3 實作集中式 GPUI chrome icon renderer，確保 icon 的可見 bounds 可寫入 region diagnostics。
- [x] 4.4 將 navigation bar 的 Unicode Back/Forward/Up 與其他 placeholder glyph 替換為 `ExplorerIcon`，保持 action、tooltip、accessibility name 不變。
- [x] 4.5 將 command bar、view/status 與 tab/caption controls 的文字暫代 icon 逐一替換，完成後加入 source test 禁止 production chrome 回歸 Unicode placeholder。
- [x] 4.6 先為 Shell icon request、result、cache key 與 cancellation 撰寫 common/model contract tests。
- [x] 4.7 定義不攜帶 apartment-affine interface 的 owned Shell icon payload，包含 stable identity、size bucket、DPI、theme、association generation、RGBA/stride 與 fallback reason。
- [x] 4.8 在 `explorer-shell-win` 實作 `IShellItemImageFactory`／Shell image list 取得路徑，明確處理 filesystem、folder、drive、ZIP/RAR association 與 namespace item。
- [x] 4.9 為 `HICON`、`HBITMAP`、image list 與 COM interface 建立 RAII wrappers，在每個 unsafe block 記錄 ownership、apartment、buffer 與 cleanup invariant。
- [x] 4.10 將 Windows bitmap/icon 轉為 alpha 正確的 owned pixel payload，測試 premultiplied alpha、透明背景、stride、尺寸與失敗 cleanup。
- [x] 4.11 建立 bounded icon request queue 與 viewport priority，測試 100k folder 不會為所有 offscreen rows 同時建立請求。
- [x] 4.12 建立 icon cache 容量、LRU/eviction、DPI/theme/association invalidation 與 cancellation，測試 stale result 不覆寫新 generation。
- [x] 4.13 在 GPUI UI layer 建立 texture cache，僅接收 owned payload，不直接依賴 `explorer-shell-win` 或持有 Win32 handles。
- [x] 4.14 將 file rows、navigation items、breadcrumb root/drive 與 menu children 接上真實 Shell icon，loading/failure fallback 必須維持相同 geometry。
- [x] 4.15 以真實 `D:\` 混合檔案比對 folder、drive、ZIP、RAR 與一般檔案 icon 的來源、尺寸、中心及可見 bounds。
- [x] 4.16 執行重複 scroll、theme switch、DPI switch 與 navigation soak，量測 GDI/User handles、COM refs、memory 與 cache size不持續成長。

## 5. Per-tab AddressBarState 與 actions

- [x] 5.1 先為 `AddressBarState` 撰寫狀態轉移測試，涵蓋 Browsing、Editing、EnumeratingMenu、NavigationError 及所有合法/非法轉移。
- [x] 5.2 定義 `BreadcrumbSegment` stable id、display name、`LocationDescriptor`、icon hint、container capability 與 ancestry ordering invariants。
- [x] 5.3 定義 per-tab address draft、resolved ancestry、active menu request/generation、error 與 focus restoration state，不以 row index 或 display text 作 identity。
- [x] 5.4 將 address state 納入 tab 建立、clone/default、activate、close 與 generation cancellation 流程，加入 tab invariants。
- [x] 5.5 撰寫兩分頁隔離測試：第一分頁保留 draft、第二分頁導覽/search/watch 更新後，第一分頁 draft 與 selection 必須不變。
- [x] 5.6 新增 typed actions：EnterAddressEdit、UpdateAddressDraft、SubmitAddress、CancelAddressEdit、ActivateBreadcrumbSegment、OpenBreadcrumbChildren、CloseBreadcrumbMenu、ActivateBreadcrumbChild。
- [x] 5.7 將 `Ctrl+L` 與 `Alt+D` 綁定到相同 EnterAddressEdit action，加入 scope/IME/key-binding conflict tests。
- [x] 5.8 定義點擊網址列未占用空白區的 action 與 hit-test contract，測試不能誤觸最後 segment 或 chevron。
- [x] 5.9 進入 editor 時以 current location parsing path 初始化並全選；沒有 filesystem parsing path 時使用 Shell 可解析名稱，不以 display title 取代。
- [x] 5.10 SubmitAddress 沿用既有 address parser 與 navigation pipeline，測試 query 不轉 search、invalid address 不提交 history。
- [x] 5.11 CancelAddressEdit 回復 resolved breadcrumb、清除未提交 error 並還原前一 focus；測試不建立 navigation request。
- [x] 5.12 導覽失敗時保留 draft、error 與 editor focus，成功後才提交 history、清除 error 並切回 Browsing。
- [x] 5.13 背景 tab location metadata/watcher/search events 只能更新對應 tab resolved state，不得更新 active editor entity。
- [x] 5.14 將 address actions 納入 tracing，只記 action、tab、request、mode 與 outcome，不記錄使用者完整路徑內容。

## 6. Shell ancestry 與 child-container typed protocol

- [x] 6.1 先為 filesystem root、drive、nested path、UNC、This PC、ZIP、Libraries 與 fake namespace 建立 ancestry contract fixtures。
- [x] 6.2 定義 `ResolveAncestry` command 與 batch/terminal events，所有事件包含 request id、tab id、generation、cancellation 與 stable descriptors。
- [x] 6.3 定義 `EnumerateChildContainers` command 與 loading/batch/empty/error/cancelled terminal semantics，只允許直接可導覽 children。
- [x] 6.4 擴充 fake Shell service，支援 ancestry 延遲 metadata、批次 children、partial failure、slow provider、cancel、channel close 與 stale events。
- [x] 6.5 在 model reducer 實作 ancestry insert/update，display metadata 補齊不得更改 segment identity、ordering 或 history。
- [x] 6.6 在 model reducer 實作 menu batch merge、stable dedupe、Explorer-compatible sort 與 partial/error 保留。
- [x] 6.7 實作 request context validator，tab/request/generation 任一不符即拒絕 ancestry/menu event並記錄安全診斷。
- [x] 6.8 在 Shell STA 以 Shell item parent chain 解析 ancestry，保存 owned descriptors，不把 PIDL/COM interface裸跨執行緒。
- [x] 6.9 為純 filesystem path 提供早期 ancestry，再用 Shell display metadata補齊；測試 path 長度、Unicode、reparse 與根目錄。
- [x] 6.10 在 Shell STA 實作 child-container enumeration，使用 folder/container attributes 過濾一般檔案，不遞迴走訪 descendants。
- [x] 6.11 對大量 children 使用 bounded batch/backpressure，量測 first-menu-item 與 cancel latency，禁止同步阻塞 GPUI callback。
- [x] 6.12 處理 access denied、disconnected drive、unavailable namespace、provider hang/error，回傳可恢復 outcome而不是假 empty。
- [x] 6.13 將新 commands/events 接入 app composition root、Shell queue、event pump 與 shutdown ordering。
- [x] 6.14 建立 fake/real Shell 共用 contract suite，驗證 ancestry identity、direct-child filtering、terminal exactly-once 與 cleanup。

## 7. Breadcrumb、editor 與 overflow GPUI 元件

- [x] 7.1 先建立 render structure tests，驗證 Browsing 只顯示 breadcrumb、Editing 只顯示 editor、menu/error state 具有唯一 stable IDs。
- [x] 7.2 建立 `BreadcrumbAddressBar` container，使用剩餘寬度而非現有固定 272 px，接上 region diagnostics 與 Explorer address fill/border/radius。
- [x] 7.3 建立 segment component，顯示 Shell icon/名稱、正確 padding、hover/pressed/focus states、button role、accessible name 與 stable identity。
- [x] 7.4 建立 chevron component，使用集中 icon renderer、獨立 hit target、expanded/busy accessibility state，點擊不得觸發 segment action。
- [x] 7.5 將 active ancestry render 成 segment/chevron 序列，測試 `D:\` 顯示「本機」與磁碟 display name且每個 target 正確。
- [x] 7.6 點擊 segment 時 dispatch ActivateBreadcrumbSegment，成功導覽沿用 history、Back/Forward/Up availability與 generation cancellation。
- [x] 7.7 建立 address blank-area hit layer，只覆蓋 breadcrumb 未使用區域；測試右側空白、padding、邊界與最后 segment周圍不誤判。
- [x] 7.8 將現有 `EditableTextState` 改為 Editing mode 才顯示，進入時同步 draft、取得 focus、全選並保留 IME composition支援。
- [x] 7.9 實作 Enter/Esc/focus change 行為，測試成功、invalid、permission denied、cancel與錯誤後再次編輯。
- [x] 7.10 建立 address error presentation與 accessibility description，錯誤不得改變 row geometry、轉 search或清空 draft。
- [x] 7.11 定義 breadcrumb 可用寬度計算與優先序，固定 navigation buttons/search/caption後再分配 address寬度。
- [x] 7.12 實作長 ancestry 收合，優先保留目前 segment與必要 root，較舊 segments放入可操作 overflow menu。
- [x] 7.13 為窄視窗、長 Unicode、UNC、ZIP、This PC與無 parsing path案例撰寫 overflow geometry/interaction tests。
- [x] 7.14 實作 breadcrumb keyboard traversal：Tab/Shift+Tab、Left/Right、Enter/Space、Esc與 focus restoration。
- [x] 7.15 實作 breadcrumb/editor accessibility roles、names、selected/expanded/busy/error states與 accessibility actions。
- [x] 7.16 使用 Windows 繁中 IME headful smoke 驗證 composition、caret、selection、commit與 shortcut dispatcher不攔截。

## 8. Chevron child menu GPUI 元件

- [x] 8.1 先為 menu presentation reducer撰寫 loading、partial、ready、empty、cancelled、error與late-event tests。
- [x] 8.2 建立 anchored breadcrumb menu，位置以 chevron physical bounds及目前 monitor work area計算，支援上下翻轉且不離開螢幕。
- [x] 8.3 點 chevron 時建立新 request/generation並立即顯示 loading，第二次點同一 chevron依 Explorer行為關閉或重啟且不得產生雙 session。
- [x] 8.4 將 child batches增量渲染為可聚焦 menu items，顯示 Shell icon與display name，stable identity不使用row index。
- [x] 8.5 menu 只顯示直接可導覽 containers，加入一般檔案、hidden/system policy與重複 identity tests。
- [x] 8.6 選取 child dispatch ActivateBreadcrumbChild，先關閉menu再走既有navigation pipeline，不建立平行history。
- [x] 8.7 實作 loading、empty、partial/error與recoverable retry items；partial failure保留已收到可選children。
- [x] 8.8 實作 menu keyboard navigation：Up/Down、Home/End、Page、Enter、Esc、Left/Right返回breadcrumb及type-ahead（若Explorer參考支援）。
- [x] 8.9 實作 mouse hover、press、release-outside、wheel、click outside與window deactivation cleanup。
- [x] 8.10 切tab、導覽、關menu、開另一menu、關window時取消舊request並清除menu entity/focus exactly once。
- [x] 8.11 注入late batch、duplicate terminal與channel close，驗證不污染新menu、不panic且診斷含correlation。
- [x] 8.12 驗證slow/hanging provider不阻塞GPUI frame、address editor、Back/Forward/Up或window close。
- [x] 8.13 為menu建立accessibility role、expanded/busy/error、item count與invoke action，使用螢幕閱讀器/inspection smoke驗證。
- [ ] 8.14 在100/125/150/175/200% DPI及多螢幕work area驗證anchor、shadow、row height、icon bounds與click target。

## 9. Windows Explorer chrome 區域重構

- [x] 9.1 先依reference建立完整chrome區域順序與stable ID snapshot test，明確區分title/tab、navigation、command、body與status。
- [x] 9.2 校準title/tab strip高度、active tab形狀、close/new-tab位置、window drag region與caption controls，保持Snap/maximize hit tests。
- [x] 9.3 依Explorer順序重排navigation row為Back、Forward、Up、Refresh、address、search，校準每個button、gap、padding與enabled state。
- [x] 9.4 將新`BreadcrumbAddressBar`接入navigation row，移除舊固定寬度address placeholder render path但保留必要editor entity lifecycle。
- [x] 9.5 依Explorer reference校準search框位置、寬度、placeholder、search icon、clear/cancel state與per-tab search history行為。
- [x] 9.6 依Explorer繁中名稱與順序重建command row：新增、剪下、複製、貼上、重新命名、共用、刪除、排序、檢視、更多、詳細資料。
- [x] 9.7 將空間不足的command移入overflow menu，測試action availability、keyboard、tooltip與accessibility不因overflow改變。
- [x] 9.8 校準navigation pane寬度、section spacing、item row height、indent、icon、pin、selected state、divider與scrollbar位置；只有在可見 scrollbar、thumb 幾何與互動均通過後才可完成。
- [x] 9.9 保留favorites內容差異容許，但讓Home/Gallery/OneDrive/pinned/This PC/drives/network區域幾何與Explorer profile相符。
- [x] 9.10 建立Details header與columns，校準名稱、修改日期、類型、大小的起點、寬度、separator、sort indicator與resize hit target。
- [x] 9.11 校準file row高度、icon/text baseline、selection/hover/focus、cut pending、drop cue與virtualized viewport起點。
- [x] 9.12 校準status bar item count、selected count、view buttons、padding、top divider與視窗底部位置。
- [x] 9.13 驗證Back/Forward/Up/Refresh、command actions、search、pane selection、details sorting及status view controls仍連到真實state。
- [x] 9.14 驗證titlebar drag、caption minimize/maximize/restore/close、Snap hover與active/inactive window colors未因chrome改版回歸。
- [x] 9.15 為light/dark/high-contrast及hover/pressed/focused/disabled/selected/inactive states更新app-onlyvisual snapshots。

### 9A. 2026-07-27 實機差距盤點與補強計畫

#### 分頁位置複製

- [x] 9.16 為「+」新增分頁建立端到端契約：點擊前先保存 active tab 的 resolved `LocationDescriptor`、Shell display title 與 parsing text；新 tab 必須以同一 location 建立並成為 active，不得退回 `C:\\`、首頁或沿用未提交的 address draft。
- [x] 9.17 將新增分頁的 state mutation 與首次 `Navigate` command 合併成可測結果，避免 UI action 已切 tab、service command 卻仍引用舊 tab/request/generation；測試 click、`Ctrl+T` 與 accessibility invoke 三條路徑。
- [x] 9.18 建立兩分頁真實資料夾測試：由 `D:\\`、巢狀 Unicode 路徑及 Shell parsing-name location 各自新增 tab，驗證兩 tab location 相同但 history、selection、search、address draft 與 scroll anchor 互相隔離。

#### 檔案 metadata 與 Details 欄位

- [x] 9.19 擴充 owned `FileEntry` metadata，至少表達 modified time、logical size、display type、filesystem attributes 與 metadata unavailable 原因；資料不得攜帶 HANDLE、PIDL 或 COM interface 跨執行緒。
- [x] 9.20 在 Shell STA enumeration 以不阻塞 UI 的方式取得 `PKEY_DateModified`、`PKEY_Size`、`PKEY_ItemTypeText` 或等價公開 Shell/Win32 metadata；資料夾 size 依 Explorer Details 行為保持空白，零位元檔案顯示 `0 KB`。
- [x] 9.21 實作繁中 Explorer-compatible 日期與大小格式化：日期使用目前 Windows locale/time-zone；size 使用 1024-based KB、向上取整與群組分隔，不能把讀取失敗誤顯示為零。
- [x] 9.22 將 Name、修改日期、類型、大小欄位接到真實 metadata，補排序鍵與 stable fallback；以真實 `D:\\` 混合資料夾、一般檔案、ZIP、RAR、零位元及 Unicode 名稱比對 Explorer。

#### 左右 scrollbar

- [x] 9.23 為 navigation pane 建立獨立 `ScrollHandle`，只在 content extent 大於 viewport 時顯示 Explorer 風格垂直 track/thumb；正常、hover、pressed、inactive-window、dark/high-contrast 均使用 semantic tokens。
- [x] 9.24 為 file view 建立可見垂直 scrollbar，thumb 長度按 viewport/content 比例且有最小尺寸；details header 固定不隨 rows 捲動，scrollbar 不得覆蓋最後一欄內容。
- [x] 9.25 實作兩側 scrollbar 的 wheel、thumb drag、track page-up/page-down、Home/End 與 pointer capture/release-outside；offset 必須 clamp，resize 或資料量縮小後不得留下越界位置。
- [x] 9.26 建立 scrollbar render/interaction tests 與 headful smoke，分別使用超過一頁的 navigation fixture、真實 large folder 和空/短資料夾驗證顯示與隱藏條件。

#### 網址列 Explorer 互動對齊

- [x] 9.27 建立 address hit-test matrix：segment 只導覽自己、`>` 只開直接 children、右側未占用空白才進 editor；padding、最後 segment 周圍、menu overlay 與窄視窗不得事件穿透。
- [x] 9.28 點 address 空白、`Ctrl+L`、`Alt+D` 時以目前 resolved parsing text 初始化 editor、取得 focus 並全選；再次點 editor 只移動 caret，不得重設 selection 或 draft。
- [x] 9.29 Enter 以共用 address parser 提交 filesystem、UNC、`shell:` 與可解析 namespace；Esc 回復 resolved breadcrumb；invalid/denied 保留 draft、error 與 focus，且不得建立 history 或轉成 search。
- [x] 9.30 將 breadcrumb root、每個 segment 與 chevron 接到真實 Shell ancestry；segment navigation 必須維持 Back/Forward/Up 與 per-tab request cancellation。
- [x] 9.31 實作 chevron child-container menu 的 loading、batch、empty、partial/error、retry、click-outside、window deactivate 與 stale-generation rejection；只列直接可導覽 container，選取後走同一 navigation pipeline。
- [x] 9.32 實作 breadcrumb/menu keyboard 與 accessibility：Tab/Shift+Tab、Left/Right、Up/Down、Home/End、Enter/Space、Esc、type-ahead、role/name/expanded/busy/error及 focus restoration。

#### 完整「檢視」選單

- [x] 9.33 定義 per-tab `ViewSettings` typed state，包含八種 view mode（超大圖示、大圖示、中圖示、小圖示、清單、詳細資料、並排、內容）、details pane、preview pane、item check boxes、file-name extensions、hidden items 與 compact view；新 tab 複製目前設定但後續互相隔離。
- [x] 9.34 建立 Explorer 風格 anchored「檢視」主選單，依 reference 順序、分隔線、radio/check indicator、icon、hover/pressed/focus、螢幕 work-area 翻轉與 click-outside 行為呈現；目前模式與 pane 狀態必須即時反映。
- [x] 9.35 實作超大/大/中/小圖示模式的 grid layout、對應 Shell image size bucket、label wrapping、selection rectangle、keyboard spatial navigation、拖放與 context menu hit target。
- [x] 9.36 實作清單、詳細資料、並排與內容模式的 Explorer-compatible row/column layout；所有模式共用 stable item identity、selection、rename、open、Clipboard、OLE drag-and-drop、context menu 與 search results。
- [x] 9.37 實作詳細資料窗格與預覽窗格 toggle、互斥/共存規則、resize divider、selection updates、空 selection/multi-selection/error fallback；沒有公開 preview handler 時顯示明確可恢復狀態，不得假裝預覽成功。
- [x] 9.38 實作「顯示」子選單：項目核取方塊、檔案副檔名、隱藏的項目、精簡檢視；每項都要改變真實 presentation/hit target，並處理 protected system item policy。
- [x] 9.39 將 status bar 的 view buttons、command bar「檢視」按鈕與選單共用同一 actions/reducer；切換模式保留合理 selection/anchor，背景 tab 不得被 active tab 設定覆寫。
- [x] 9.40 為八種 view mode 與所有 pane/show combinations 建立 model、render structure、keyboard、accessibility、resize、DPI/theme及真實資料夾 smoke；禁止只切換選單勾選而沒有改變內容版面。

#### TortoiseGit Shell icon overlay

- [x] 9.41 盤點本機 `ShellIconOverlayIdentifiers` 與 TortoiseGit handler 排序/註冊狀態，保存 clean、modified、conflict、added、ignored、unversioned 可用性；若 handler 被 Windows overlay slot 上限排除須明確列為環境限制。
- [x] 9.42 在 Shell STA 以公開 Shell image-list/`SHGetFileInfoW` overlay flags 取得 base icon、overlay index 或已合成 `HICON`，建立 `HICON` RAII 與 alpha-correct RGBA conversion；不得自行猜測 Git status 或硬繪 TortoiseGit 私有圖示。
- [x] 9.43 將 overlay/association generation 納入 icon cache key/invalidation，確保 watcher 收斂後 clean↔modified↔unversioned 圖示會更新，stale icon result 不覆寫新狀態。
- [x] 9.44 建立 temporary real Git working tree 與 TortoiseGit smoke，逐一驗證 clean、modified、added/unversioned（及環境可產生的其他狀態）overlay 與 Explorer 相同；測試後只刪除專用 temporary fixture。
- [x] 9.45 執行反覆 status change、scroll、tab switch、DPI/theme 與 icon eviction soak，驗證 HICON/HBITMAP/HDC、COM refs、GDI/User handles及 cache size不持續成長。

### 9B. 2026-07-27 原生圖示快取、最上層選單、Caption、排序與欄寬

#### 核准設計與契約

- [x] 9.46 將核准的圖示來源、兩層快取、breadcrumb overlay、caption 單一矩形、typed sort 與 per-tab column widths 寫入 `docs/superpowers/specs/2026-07-27-explorer-native-icon-cache-sort-columns-overlay-design.md`，並連結 OpenSpec design/specs。
- [x] 9.47 為新增設計執行 placeholder、矛盾、scope 與歧義自我審查；明確記錄 `%LOCALAPPDATA%\RustGpuiExplorer\icon-cache\v1`、不重新著色、不讀 Explorer 私有資產與真實 `D:\` 只讀限制。

#### Breadcrumb deferred anchored overlay

- [x] 9.48 將 `breadcrumb_child_menu` 從普通 absolute child 改為 GPUI `deferred(anchored(...))`；deferred priority 必須高於主 scene 與其他普通選單，anchor 使用實際 chevron layout bounds而非手算全域座標。
- [x] 9.49 為 root drive chevron與一般 segment chevron共用 overlay renderer，處理下方空間不足時上下翻轉、左右視窗邊界 snap、DPI縮放與窄視窗，不得被command bar/file view裁切或覆蓋。
- [x] 9.50 建立全窗 click-outside backdrop、Esc、window deactivate、切tab、導覽與close exactly-once transition；overlay item hit-test 必須阻止事件穿透到address edit或底下row。
- [x] 9.51 建立 render/interaction/UIA tests：invoke `本機 >` 得到 C:/D:/E:、invoke `D:\test >` 得到真實直接子資料夾；逐項檢查menu item bounds、最上層hit-test、可點擊導覽與stale generation rejection。

#### Caption 單一顯示／互動矩形

- [ ] 9.52 盤點 caption button element、glyph child、`WindowControlArea`、pointer listener、diagnostics及UIA各自bounds，保存100%與175%目前偏差證據。
- [x] 9.53 重構 `caption_button`，使同一個外層 layout box同時持有背景、hover/pressed、glyph置中、pointer action、accessibility及Min/Max/Close native area；child不得建立額外或較小hit target。
- [x] 9.54 依window state切換maximize/restore可見glyph但維持相同矩形，驗證double-click title drag、Snap hover、maximize/restore後control bounds不漂移。
- [ ] 9.55 建立caption click-grid/UIA/headful matrix，涵蓋三按鈕四角、中心、邊界內外、100/125/150/175/200% DPI、active/inactive、minimize、maximize/restore與安全close。

#### Per-tab typed sorting

- [x] 9.56 在 model 定義 `SortColumn`、`SortDirection`、`SortDescriptor` 並納入 `ViewSettings`；新tab複製目前設定但後續隔離，serialization/default/invariant不得使用display label作identity。
- [x] 9.57 實作共用比較器：container優先；Name使用Windows-compatible不分大小寫fallback；DateModified、Type、Size使用typed metadata；missing固定置後；最後以display name與stable `ShellItemId`決定順序。
- [x] 9.58 排序只建立presentation index/order，不重排或重建`DirectorySnapshot`；selection、anchor、rename、open、Clipboard、drag/drop與context menu必須仍解析到正確stable item。
- [x] 9.59 新增header actions：點新欄採Explorer預設方向、再點同欄反轉；active欄顯示方向indicator及accessibility sort state，其他欄清除indicator。
- [x] 9.60 將command bar「排序」改為anchored menu，列出名稱、修改日期、類型、大小、遞增、遞減；與header共用同一reducer且立即反映目前radio/check狀態。
- [x] 9.61 建立四欄升降冪model/render tests，涵蓋folders/files、zero-byte、missing metadata、Unicode、大小寫、相同值stable tie、search results、watcher insert/remove及兩tab隔離。
- [x] 9.62 以真實`D:\`建立只讀排序oracle，逐欄點擊header及command menu，比對item count、前後順序、selection identity與Back/Forward/Refresh後設定保留。

#### Per-tab Details column resize

- [x] 9.63 在 model 定義`DetailsColumnWidths`與column identity，四欄使用logical px、具Explorer profile default、個別min/max及finite validation；納入per-tab `ViewSettings`與new-tab copy/isolation。
- [x] 9.64 將header與所有Details rows改為讀取同一`DetailsColumnWidths`，移除render path對固定`LayoutTokens.details_*_width`的直接依賴；非Details modes不受欄寬拖曳影響。
- [x] 9.65 為每個header separator建立可見1px semantic divider與較寬透明resize hit target，加入stable id、col-resize cursor、hover/pressed/focused及accessibility separator value。
- [x] 9.66 建立typed column resize session，保存tab、column、start pointer、start width；mouse move即時clamp，mouse up、release-outside、Esc、tab switch、view switch與window deactivate恰好結束一次。
- [x] 9.67 實作separator double-click auto-size，以header label與目前snapshot顯示字串／icon／padding估算並clamp；空資料夾只依header，不同步阻塞Shell或掃描磁碟。
- [x] 9.68 當總欄寬超過viewport時提供水平overflow/scroll且保留垂直scrollbar、固定header及最後一欄；resize/window/DPI改變不得留下非法offset。
- [x] 9.69 建立四欄drag、min/max、release-outside、double-click、header-row對齊、水平overflow、100–200%DPI、兩tab隔離及真實`D:\`headful tests。

#### Windows Shell 圖示兩層快取

- [x] 9.70 定義版本化disk cache key/header contract與tests，key至少包含schema、Windows build、Shell identity/path fingerprint、size bucket、DPI、theme、association generation與overlay generation；header含magic、digest、dimensions、stride、pixel format、length及checksum。
- [x] 9.71 在`explorer-shell-win`建立只保存owned bytes的`ShellIconDiskCache`，預設root為`%LOCALAPPDATA%\RustGpuiExplorer\icon-cache\v1`，測試可注入temporary root；不得保存PIDL、COM interface或native handle。
- [x] 9.72 實作load validation與fallback：path traversal防護、bounded file length、magic/schema/key/size/stride/checksum全驗證；corrupt/partial/unknown entry拒絕並可恢復重建，cache I/O錯誤不得使Shell icon request失敗。
- [x] 9.73 實作同目錄temporary write、flush及atomic replace，處理concurrent duplicate request、process crash殘留temp與唯讀/滿載；只在成功取得alpha-correct RGBA後寫入。
- [x] 9.74 將Shell STA load順序改為memory LRU→既存filesystem item的live Shell overlay refresh（成功後覆寫disk）→disk fallback；virtual／不存在項目維持memory→disk→Shell，warm hit仍回傳完整typed payload；加入memory/disk hit/miss/corrupt/write-failure/Shell refresh非敏感counters與tracing。
- [x] 9.75 實作disk容量/entry上限與LRU cleanup，不在startup解碼所有bitmap；cleanup不得刪除目前正在讀寫entry，超限時失敗只影響cache不影響UI。
- [x] 9.76 將association/overlay watcher generation接到memory與disk key/invalidation，確保TortoiseGit/OneDrive或副檔名關聯變更不讀舊像素；theme/DPI只失效相依variant。
- [x] 9.77 建立cold/warm跨process測試、corrupt/schema/build/DPI/theme/association/overlay invalidation、capacity eviction、concurrent load與shutdown cleanup；證明既存filesystem item跨process會重新取得live overlay並更新persistent fallback，而virtual／Shell暫時失敗路徑可還原完整RGBA。
- [x] 9.78 以真實`D:\`資料夾、drive、ZIP、RAR、一般檔案、TortoiseGit與OneDrive overlay執行Explorer/app圖示比對；確認無app tint、alpha/色彩hash穩定、cache重啟後畫面一致。

#### 本輪品質與提交

- [x] 9.79 執行fmt、workspace all-targets check、Clippy warnings-as-errors、model/UI/Shell tests、OpenSpec strict validation與git diff check；不得提交使用者未追蹤的`codex_gpui_win11_explorer_prompt.md`。
- [x] 9.80 建立兩個聚焦commit：先提交核准design/spec/tasks，再提交通過真實Windows驗證的implementation；handoff列出cache路徑、清除方式、證據、限制及剩餘Explorer parity差距。

#### Scrollbar pointer capture 補強

- [x] 9.81 將核准的 scrollbar pointer-capture 設計連結至 OpenSpec design/spec，新增離開 scrollbar/client/HWND、grab offset、clamp 與 exactly-once terminal scenarios。
- [x] 9.82 在 `interaction.rs` 定義 `ScrollbarKind`、typed `ScrollbarDragSession`、terminal reason與純函式 geometry；測試非有限值、零 range、grab offset、上下越界及resize後重算。
- [x] 9.83 新增 begin/update/end scrollbar actions及 reducer；同時只能有一個 session，track paging不得開始session，new tab copy不得複製 transient session。
- [x] 9.84 將 scrollbar thumb/track hit-test分離；thumb mouse-down保存pointer-to-thumb grab offset，track維持page-up/page-down，左右 scrollbar共用renderer且不建立第二套公式。
- [x] 9.85 在 GPUI root render期間註冊 window-level capture-phase mouse move/up listener，使游標進入任何其他client element後仍更新正確`ScrollHandle`並阻止底層row/drag操作。
- [x] 9.86 建立 audited Win32 `SetCapture`/`GetCapture`/`ReleaseCapture` RAII boundary，支援游標移出HWND；capture失敗退化成client capture且不得crash，capture lost走相同terminal path。
- [x] 9.87 將Mouse Up、Mouse Up Outside、Esc、window deactivate、capture lost、tab switch、view switch與close接到idempotent end；驗證重複terminal不重複release或改變offset。
- [x] 9.88 建立左右scrollbar unit/render/headful tests，以長navigation fixture及真實large folder把pointer移至內容中央與HWND外再放開；保存offset序列、capture ownership、release後不變與空/短folder隱藏證據。原9.25另含wheel及Home/End鍵盤範圍，維持獨立待辦，不以本次pointer-capture證據誤勾。

## 10. 真實資料夾與互通回歸

- [x] 10.1 建立只讀真實`D:\`驗收harness，啟動Explorer與app至相同location、Details view、排序及視窗bounds，不建立/刪除/重新命名D槽項目。
- [x] 10.2 驗證`D:\` ancestry、segment click、Back/Forward/Up/Refresh與status item count使用真實Shell結果。
- [x] 10.3 逐一點擊This PC、drive與nested folder segments的`>`，比對直接子資料夾oracle、排序、icon、empty/error及selection navigation。
- [x] 10.4 以temporary real-folder fixture測試Unicode、長路徑、無權限、empty、large-child-count、rapid rename與watcher convergence。
- [x] 10.5 建立多分頁網址列E2E：不同location、不同draft、同時menu enumeration、切換、關閉與late events隔離。
- [x] 10.6 重跑create/rename/copy/move/recycle/permanent-delete/cancel/conflict/undo/redo安全fixture，確認新row/chrome不改變磁碟oracle。
- [x] 10.7 重跑app內跨分頁及Explorer雙向Clipboard copy/cut/paste，驗證command availability、cut visual與watcher convergence。
- [ ] 10.8 重跑Explorer→app與app→Explorer左/右drag的copy/move/none matrix，驗證新pane/row/address/menu hit targets與drop effects。
- [x] 10.9 將`target/explorer-interop-evidence/20260727-drag-v26-explorer-to-app/fixture/explorer-source/explorer-left-copy.txt`所屬fixture納入回歸並確認原始證據檔hash不變。
- [x] 10.10 重跑background/single/multi及owner-draw `IContextMenu3`流程，驗證menu message routing與watcher收斂。
- [x] 10.11 重跑兩分頁search、快速replacement、navigation cancel、Windows Search/fallback與partial error，網址列draft不得被search state覆寫。
- [x] 10.12 執行keyboard-only及Windows IME headful流程，涵蓋breadcrumb、chevron menu、editor、search、pane、file rows、command與caption controls。
- [x] 10.13 執行accessibility inspection，驗證所有新增control的role/name/state/value/action、focus order與high-contrast可見性。
- [x] 10.14 執行large folder/rapid navigation/menu open-close/icon load soak，量測frame latency、queue depth、cancel latency、memory、threads、GDI/User handles及COM refs。

## 11. 視覺 parity 收斂與 DPI/theme 矩陣

- [x] 11.1 在主要參考環境重新擷取Explorer `D:\` reference，確認window client bounds與既有approved baseline一致；若OS/Explorer build不同則建立新profile而不覆寫舊檔。
- [x] 11.2 以相同window bounds、DPI、theme、font、location、sort與view擷取application screenshot及完整region diagnostics。
- [x] 11.3 執行具名region comparator，逐一修正title/tab、navigation、command、pane、details header、rows、status、search與caption超過10%的座標/尺寸。
- [x] 11.4 執行control名稱、順序、availability與accessibility label comparison，修正與繁中Explorer reference不一致項目。
- [x] 11.5 執行chrome icon bounds/center/size/stroke報告，修正每個超過10%或仍使用placeholder的icon。
- [x] 11.6 執行真實Shell item icon comparison，記錄association/build-specific差異並修正cache、size或alpha問題。
- [x] 11.7 執行平坦color samples比較，將每個light reference channel delta收斂至12內且不以擴大mask通過。
- [x] 11.8 執行typography family/size/weight/line-height/baseline比較，將字級差收斂至1logical px內。
- [x] 11.9 人工檢視reference、actual、overlay、raw diff與masked diff，確認favorites內容差異和文字AA是唯一允許mask類型。
- [x] 11.10 擷取dark profile與application，驗證geometry仍在10%、semantic colors正確且不引用淺色固定值。
- [x] 11.11 擷取high-contrast app evidence，驗證系統色彩、focus、selection、icon與文字可辨識；Explorer可比時保存同條件reference。
- [x] 11.12 執行100/125/150/175/200% DPI matrix；無法實際切換的case必須記錄未驗證，不得用模擬值宣告實機通過。
- [ ] 11.13 在可用多螢幕不同DPI間移動視窗並maximize/restore，驗證token/icon cache invalidation、caption hit test及breadcrumb menu anchor。
- [x] 11.14 產出最終region/color/icon/typography總報告，所有超標項目必須修正或以公開API/build限制列為未完成，不得靜默接受。

## 12. 品質閘門、文件與交付

- [x] 12.1 執行`cargo fmt --all --check`並修正格式，不將使用者未追蹤檔案納入變更。
- [x] 12.2 執行`cargo check --workspace --all-targets --locked`並修正所有新增typed contract、GPUI或Windows feature問題。
- [x] 12.3 執行workspace Clippy（all targets/features、warnings as errors）並逐項修正，不以寬泛allow隱藏可修正問題。
- [x] 12.4 執行workspace unit/integration/doc tests及所有新增fake/real Shell contract suites，保存失敗重現命令與最終結果。
- [x] 12.5 執行release build與headful startup/close smoke，確認resource manifest、DPI awareness、caption及Shell STA/OLE shutdown正常。
- [x] 12.6 執行architecture audit，確認`explorer-ui`不直接依賴`explorer-shell-win`、apartment-affine handles不跨執行緒、test-support不進production graph。
- [x] 12.7 掃描所有新增unsafe、HICON/HBITMAP/COM ownership、request terminal events、cancellation、timeout、cache eviction與window shutdown，完成安全review。
- [x] 12.8 更新`docs/PARITY_MATRIX.md`，新增geometry、icon、color、typography、breadcrumb、chevron menu列及每項自動/手動證據。
- [x] 12.9 更新`docs/MANUAL_TESTS.md`，記錄真實`D:\`、DPI/theme、keyboard/IME/accessibility與Explorer interop的實際結果及證據路徑。
- [x] 12.10 更新`docs/STATUS.md`與`docs/IMPLEMENTATION_PLAN.md`，列出已完成、未驗證、known differences、Windows/API限制及baseline profile。
- [x] 12.11 更新visual baseline/reference文件與schema說明，記錄10%幾何、channel 12、字級1px、mask規則及baseline更新流程。
- [x] 12.12 執行`openspec validate match-explorer-visual-address-parity --strict`，修正所有proposal/design/spec/tasks格式與coverage問題。
- [x] 12.13 檢查`git diff --check`、tracked/untracked邊界、submodule狀態及generated evidence policy，避免提交使用者檔案或巨大暫存產物。
- [x] 12.14 建立最終handoff，列出binary、啟動/測試命令、主要reference、region報告、真實資料夾與interop證據、已知限制及後續build profile維護方式。
## 14. 一般權限 UI、按需 UAC 與背景日誌

- [x] 14.1 移除 startup `runas` 與 administrator token 檢查，讓 diagnostics、Shell/OLE、GPUI 及 F1 快捷鍵維持一般使用者 integrity level。
- [x] 14.2 在原生 `IFileOperation` flags 加入 `FOFX_SHOWELEVATIONPROMPT`，保留 `FOF_NOERRORUI`，只有受保護操作需要時才顯示 UAC。
- [x] 14.3 將 process-wide tracing formatter改為無 ANSI 的純文字輸出，保留時間、level、message 與 structured fields。
- [x] 14.4 新增記憶體 writer 測試，驗證背景 log 包含事件欄位且不含 ESC byte。
- [x] 14.5 執行 fmt、workspace check/tests、Clippy、Windows executable build、OpenSpec strict 與 diff-check。
## 15. 網址列字級與垂直置中

- [x] 15.1 將 address/search 共用字級調為 14 logical px、line-height 22、baseline 17，保留 Windows UI 字型 fallback。
- [x] 15.2 依 32 px hit target 與 line-height 動態計算相等上下 padding，讓 glyph、selection、caret 垂直置中。
- [x] 15.3 新增 typography 契約測試，執行 fmt、check、Clippy、workspace tests、headful capture、OpenSpec strict 與 diff-check。

## 16. 命令列資料更新與錯誤捲動修正

- [x] 16.1 盤點 command bar 九個主要命令、sort/view menu、Details header 與 file background 的 mouse-down/click 傳播鏈，保存 presentation index 與 snapshot index 混用的根因。
- [x] 16.2 將 selection、range、focus、rename、open、context menu、drop target 及 keyboard focused row 全部改由共用 presentation-order resolver 解析 stable item。
- [x] 16.3 讓 `scroll_to_item` 僅接收 presentation index；替 command、sort/view menu 與 Details header 增加 mouse-down propagation boundary，防止點擊誤啟動 marquee 或向下捲動。
- [x] 16.4 將 Rename command 接到既有 inline editor，依單選與可寫狀態控制 availability；將 Share command 接到 Windows Shell canonical `Windows.Share` verb 與可恢復錯誤流程。
- [x] 16.5 在成功／部分成功的 file operation 或 invoked context command terminal 後，僅於 active tab/generation 仍相符時提交一次 refresh；watcher 已推進 generation 時拒絕重複刷新。
- [x] 16.6 新增 sorted presentation pointer identity、operation refresh generation、command availability、menu mouse isolation 與 Shell canonical verb tests。
- [x] 16.7 使用真實可寫 temporary folder 執行 Create/Rename/Copy/Cut/Paste/Delete、Sort/View 及 Win32 mouse smoke，驗證右側資料、stable selection、scroll offset 與 terminal log；再執行 fmt、workspace check/tests、Clippy、OpenSpec strict 及 diff-check。

## 17. Details header 固定捲動回歸

- [x] 17.1 追查 Details scroll host、absolute header、wheel compositor 與重新 render 的座標語意，確認 header 留在可捲動內容樹會產生一幀位置不同步。
- [x] 17.2 將 Details header 移為 scroll host 外的固定 sibling overlay：`top = 0`，僅讓 `left` 跟隨 `offset.x`，並保留四欄 resize、sort 與 hit-test。
- [x] 17.3 新增真實 240-item 長資料夾的 mouse-wheel headful regression與分軸座標 unit regression，驗證 header top 不變且水平 offset 契約維持欄位對齊。
- [x] 17.4 執行 fmt、UI tests、Clippy、OpenSpec strict、真實長資料夾 capture 與 diff-check，提交時排除使用者未追蹤檔。

## 18. 搜尋框比例校準

- [x] 18.1 量測使用者提供的 Explorer／application 畫面，確認 175% DPI 下 reference 約435 physical px／249 logical px，而 application 為672 physical px／384 logical px。
- [x] 18.2 將搜尋框改為視窗logical width的23.5%，以120/384 logical px clamp compact／寬螢幕，並讓breadcrumb address繼續以flex取得其餘空間。
- [x] 18.3 新增reference ratio、DPI與窄視窗無重疊tests，執行同寬headful capture確認搜尋框及網址列座標誤差在10%內。
- [x] 18.4 執行fmt、UI tests、Clippy、OpenSpec strict與diff-check，提交時排除使用者未追蹤檔。

## 19. F2 inline rename 高度與置中

- [x] 19.1 盤點Details row、file-row typography與rename text input，確認現況使用32px整列高度且沒有垂直padding。
- [x] 19.2 新增24 logical px inline rename高度token，以16px line-height推導上下各4px padding，並將container置中於32px row。
- [x] 19.3 新增高度、padding、DPI與錯誤提示anchor regression，使用真實可寫folder執行F2 headful capture。
- [x] 19.4 執行fmt、UI tests、Clippy、OpenSpec strict與diff-check，提交時排除使用者未追蹤檔。

## 20. 切換資料夾後的 F2 焦點一致性

- [x] 20.1 以真實可寫巢狀資料夾重現「先切換資料夾、再按 F2」流程，區分已點選項目與新資料夾尚無 focused row 兩種狀態。
- [x] 20.2 統一鍵盤 current-row fallback 與 rename 執行端契約：新資料夾無焦點時，F2 先選取第一個可見項目並建立 focus，再開啟 inline rename；既有 focused selection 優先。
- [x] 20.3 在 inline rename 入口補上目前位置可寫性檢查，避免唯讀 Shell 位置透過鍵盤繞過 command availability。
- [x] 20.4 新增 state regression 與先雙擊切換子資料夾的 headful smoke，驗證 F2 editor、選取、尺寸及垂直置中。
- [x] 20.5 執行 fmt、UI tests、Clippy、OpenSpec strict、真實視窗 capture 與 diff-check，提交時排除使用者未追蹤檔。

## 21. 跨磁碟切換後的 FileView focus

- [x] 21.1 以真實 C:\ 與 D:\ 重現「C:\ 選取項目、F2、左側切換 D:\、右側選取項目、再次 F2」完整操作序列。
- [x] 21.2 在 F2 binding log 記錄 focused surface、rename editor active 與 focused row，確認第二次 F2 被 NavigationPane model focus 阻擋。
- [x] 21.3 讓 SelectItem、additive/range selection、FocusItem、SelectAll、Invert、Clear、context menu、marquee 與 BeginRename 同步將 model focus 交給 FileView。
- [x] 21.4 新增 navigation pane 到 file view 的 pointer focus regression，並建立真實 C:\ → D:\ 雙 F2 headful smoke 與報告。
- [x] 21.5 執行 fmt、UI tests、Clippy、OpenSpec strict、真實跨磁碟視窗驗證與 diff-check，提交時排除使用者未追蹤檔。

## 22. Details 欄位拖曳座標 1:1

- [x] 22.1 追查 GPUI logical pointer 與 Win32 physical client pointer 的邊界，確認高 DPI 下 physical delta 被直接當 logical delta 使用。
- [x] 22.2 在 Details column resize 的 native capture 邊界依 `window.scale_factor()` 將 physical client x 轉為 logical x，begin/update 共用且只轉換一次。
- [x] 22.3 新增 100/125/150/175/200% table regression、負座標、無效 scale 與 40 logical px 等距拖曳測試。
- [x] 22.4 執行精準 UI tests、Clippy、OpenSpec strict、diff-check 與可用的真實欄位拖曳 headful smoke。
- [x] 22.5 只提交本節涉及檔案，保留同時進行的其他模組修改。

## 23. 排序／檢視選單定位與 UITest 回歸

- [x] 23.1 以使用者截圖與 render tree 追查 Sort/View popup 的 layout parent，確認 `AnchoredPositionMode::Local` 被掛在 command bar 尾端而取得錯誤 origin。
- [x] 23.2 讓 Sort 與 View popup 成為各自 `relative` semantic button 的 direct child，不新增會偏移 hit-test 的 wrapper；popup 以 `absolute top/right` 對齊按鈕底部／右緣並保留 deferred top-layer paint。
- [x] 23.3 以固定 popup 寬度小於 compact viewport及右緣對齊維持視窗內定位，新增 source/render contract，禁止 popup 回歸 command-bar 尾端、`TopRight` 魔術負位移或視窗 `(0,0)`。
- [x] 23.4 擴充既有 Sort/View smoke，並新增專用 `command-menu-anchor-headful` UITest；量測 window/button/popup/first-menu-item physical bounds，驗證 top/right 精準對齊、水平相交、視窗內及未落在原點，將座標與 delta 寫入 report.json。
- [x] 23.5 執行 fmt、explorer-ui／explorer-uitest 測試、Clippy、OpenSpec strict 與專用 headful smoke；只提交本節相關檔案，保留使用者同時修改的其他模組。既有 Sort/column smoke 的 menu anchor 已通過，後續欄寬拖曳因 concurrent workspace 行為出現 70→56 physical px 的獨立失敗，不影響本節專用座標 oracle。
## 24. More 選單與資料夾選項（搜尋排除）

- [x] 24.1 盤點既有 More placeholder、selection reducers、clipboard、Shell canonical verb、per-tab ViewSettings 與 modal render boundary；記錄可重用與缺少的 typed actions。
- [x] 24.2 將 More popup 移為 `command-more-menu` direct child，以 absolute top/right 與 deferred top layer 對齊按鈕，補上 mouse-down propagation isolation。
- [x] 24.3 依 Explorer 順序建立 9 個繁中 menu items、2 個 separators、stable IDs、MenuItem role、accessibility names、enabled/disabled 色彩與 hover/pressed/focused states。
- [x] 24.4 將鍵盤 Up/Down/Home/End/Enter/Space/Escape 改為跳過 separators並覆蓋全部 9 個命令；開啟其它 menu、視窗失焦或執行命令時 exactly-once 關閉。
- [x] 24.5 新增復原、壓縮成 ZIP、加入我的最愛、複製路徑與開啟資料夾選項 typed actions；既有全選、全部不選、反向選擇與內容不得另建平行 reducer。
- [x] 24.6 復原、Pin to Home 與 Properties 走 Shell canonical verb；本機 Shell 未公開 CompressToZip 時，Shell boundary 以無字串插值的 Windows tar fallback 建立 collision-safe ZIP，錯誤仍走 recoverable terminal path。
- [x] 24.7 複製路徑以 presentation-order 選取項目建立 Windows Explorer 相容 Unicode text，filesystem path 優先、Shell parsing name fallback，並寫入系統 clipboard。
- [x] 24.8 在 state 建立 FolderOptions draft、一般／檢視 active tab 與 open/apply/cancel/reset transitions；Cancel 不得改變 active tab，Apply/OK 一次更新 ViewSettings。
- [x] 24.9 實作 Explorer 比例的資料夾選項 modal、一般頁與檢視頁；本節不建立搜尋頁、搜尋 tab、搜尋設定或搜尋持久化。
- [x] 24.10 將檢視頁可用控制項接到 item check boxes、file name extensions、hidden items、compact view、details pane、preview pane，並讓套用後右側資料立即 rerender。
- [x] 24.11 新增 action/state/render unit tests，涵蓋順序、availability、keyboard index、canonical verbs、copy-path quoting、draft cancel/apply/reset 與 Search tab 缺席。
- [x] 24.12 擴充 headful UITest：驗證 More button/popup physical bounds與 Options 兩頁／Search 缺席；真實 temporary folder 測試 ZIP 內容建立與 collision-safe 命名，既有 Shell tests 覆蓋 canonical Properties 及失敗復原。
- [x] 24.13 執行 targeted fmt、explorer-ui/model/shell/uitest tests、Clippy、OpenSpec strict、headful smoke 與 diff-check；只提交本節檔案／hunks，保留使用者同步修改。More popup 實機 top/right delta 均為 0，Options 一般／檢視 Tab 可見且 Search Tab 缺席。
## 25. 大／中／小圖示 Explorer 空間排列

- [x] 25.1 以使用者提供的 Explorer／application 截圖量測 Small Icons 的橫向 icon-label cell、多欄 row-major flow，以及 Large／Medium Icons 的 fixed tile、換列與局部 selection bounds。
- [x] 25.2 建立 `SpatialGridMetrics` 純函式，依 ViewMode 回傳 flow、cell width/height、icon size、stacked label 與 columns；禁止 renderer、marquee、keyboard 各自手算不同幾何。
- [x] 25.3 修正 wrapped file item 不再無條件 `w_full()`：Large／Medium／ExtraLarge 採固定寬度 stacked tile，Small Icons 採固定寬度 horizontal icon-label cell，Details／Content 保持 full row。
- [x] 25.4 校準 Small Icons 為 20px icon、32px row、約240px cell及單行省略；Medium／Large 使用48／64px Shell icon、Explorer 比例 cell與最多兩行置中文字。
- [x] 25.5 讓 hover、selected、drop cue、inline rename、checkbox、context menu及drag hit-test只使用 item cell bounds，不得橫跨 viewport。
- [x] 25.6 讓 marquee intersection、Arrow/Home/End/PageUp/PageDown、Shift range及scroll extent共用 row-major columns；resize後重算 columns但保留stable-id selection/focus。
- [x] 25.7 驗證 view switch 會依新 size bucket重新請求48／64px Shell icon，stale 20px result不得覆蓋；fallback仍保持相同 icon slot幾何。
- [x] 25.8 新增純幾何、render source、selection/marquee、keyboard與icon request tests，涵蓋窄／寬viewport、1／N items及100–200% DPI。
- [x] 25.9 建立真實資料夾 headful UITest，切換小／中／大圖示後量測前兩列item rectangles、icon rectangles、selection rectangle與換列方向，保存screenshots/report。
- [x] 25.10 執行targeted fmt、explorer-ui tests、Clippy、OpenSpec strict、headful smoke與diff-check；保留使用者同時修改的其它模組。175% DPI 實機量得 Small／Medium／Large cell 為 419×56、182×154、210×182 physical px，對應 240×32、104×88、120×104 logical px，三模式 row-major、局部 selection及 screenshots/report 全數通過。

## 26. 導覽列與網址列 Shell 圖示修正

- [x] 26.1 將 Gallery 改為 Windows 11 可解析的 Shell namespace CLSID，Quick Access 無釘選及未設定 OneDrive 時顯示不可用禁止圖示且不 dispatch navigation。
- [x] 26.2 將展開後才發現的磁碟與資料夾位置加入 navigation Shell icon snapshot，避免點選本機後退回通用 fallback。
- [x] 26.3 擴充 breadcrumb icon hint，區分本機、磁碟、資料夾、壓縮檔與 namespace，載入期間不得使用 Details-list glyph。
- [x] 26.4 新增 model、UI、state 與真實 Windows Shell tests，涵蓋 Gallery identity、空 optional root、動態樹 icon location 與 archive ancestry。
- [x] 26.5 執行 breadcrumb headful UITest、targeted Clippy、OpenSpec strict validation 與 diff-check。

## 27. Command Menu Pointer Hover Parity

- [x] 27.1 Add bounded pointer-focus actions for Sort, View, and More menus and preserve existing keyboard activation semantics
- [x] 27.2 Render focused and hovered command rows with the breadcrumb menu neutral-gray palette, including the Extensions row
- [x] 27.3 Add reducer/render regression coverage and headful UTIT pixel evidence proving the highlight follows the pointer
- [x] 27.4 Run formatting, focused tests, Clippy, app build, registered UTIT, coverage validation, OpenSpec strict validation, and diff checks

## 28. Stable Drive Breadcrumb Text

- [x] 28.1 Define the local-drive breadcrumb contract as the canonical uppercase drive designator and preserve other Shell segment names
- [x] 28.2 Prevent Shell ancestry enrichment from replacing drive text with the volume title and normalize incoming drive batches in UI state
- [x] 28.3 Add model, state, real Shell, and headful UTIT coverage for initial and child-folder navigation
- [x] 28.4 Run formatting, focused tests, Clippy, app build, registered UTIT, coverage validation, OpenSpec strict validation, and diff checks

## 29. Explorer Tab Strip Pointer and Focus Parity

- [x] 29.1 Define middle-click close, focus-line removal, and plain Add new-tab visual contracts
- [x] 29.2 Route middle-button release on the hit tab through `CloseTab`, remove the active-tab top focus decoration, and use strip fill plus Fluent Add for New Tab
- [x] 29.3 Add renderer/icon regressions and physical-pointer/pixel UTIT coverage for all three behaviors
- [x] 29.4 Run formatting, focused tests, Clippy, app build, registered UTIT, coverage validation, OpenSpec strict validation, and diff checks

## 30. Selected navigation drive icon stability

- [x] 30.1 Define the selected-drive Shell icon stability contract and identify exact-key replacement by a newer compatible file-view presentation as the root cause
- [x] 30.2 Resolve navigation snapshots through the newest compatible location/theme/DPI key while preserving the actual cached key for rendering
- [x] 30.3 Extend registered UTIT coverage to physically select C: and verify C: plus sibling drive icon pixels remain visible
- [x] 30.4 Run formatting, focused tests, Clippy, app build, registered UTIT, coverage validation, OpenSpec strict validation, and diff checks

## 31. This PC view-mode parity

- [x] 31.1 Define Details, Small/Medium/Large icons, and Content presentation contracts from the supplied Explorer references
- [x] 31.2 Extend drive metadata with the filesystem name returned by `GetVolumeInformationW`
- [x] 31.3 Implement view-specific This PC geometry, localized group/capacity text, Details columns, drive cards, and Content rows on the shared stable item model
- [x] 31.4 Add model/render regressions and registered physical-pointer UTIT coverage for all five referenced view presentations
- [x] 31.5 Run formatting, targeted tests, Clippy, app build, registered UTIT, coverage validation, OpenSpec strict validation, and diff checks

## 33. Aspect-preserving thumbnails and filename separation

- [x] 33.1 Specify the Explorer-style bounded thumbnail host, proportional contain behavior, and independent filename region
- [x] 33.2 Render Shell thumbnails and icons with a tested aspect-fit calculation, centered containment, clipping, and fixed stacked-label geometry
- [x] 33.3 Add Rust coverage for portrait/landscape/square aspect fitting and all stacked icon sizes, plus headful UTIT bounds and screenshot evidence
- [x] 33.4 Run formatting, focused Rust tests, Clippy, build, the icon-view headful UTIT, and strict OpenSpec validation

## 34. Bounded multi-line icon filenames

- [x] 34.1 Specify two-line normal, three-line selected, ellipsis, full-name preservation, and adjacent-cell containment behavior
- [x] 34.2 Constrain stacked filename blocks to their cell width, remove intrinsic flex sizing, enable normal character wrapping, and reserve stable three-line geometry
- [x] 34.3 Add Rust source/geometry tests and a headful UTIT fixture covering spaced Latin, unbroken Latin, and long CJK names
- [x] 34.4 Run formatting, explorer-ui tests, Clippy, build, icon-view UTIT, screenshot review, and strict OpenSpec validation

## 32. Ordinary Content view and continuous Ctrl+wheel zoom

- [x] 32.1 Record the Explorer Content-row and continuous zoom ladder contracts in design/spec and register their UTIT requirement coverage
- [x] 32.2 Add a per-tab exact icon-size notch, compatible session restore defaults, and one bounded Content-to-512 Ctrl+wheel ladder
- [x] 32.3 Drive Shell icon requests and spatial geometry from the same exact notch and render equal-height divided Content rows with Explorer metadata placement
- [x] 32.4 Extend unit and physical-pointer UTIT coverage for Details→Tiles→Content and all twelve icon sizes
- [x] 32.5 Run formatting, targeted tests, Clippy, app build, registered UTIT, OpenSpec strict validation, and diff checks

## 35. Breadcrumb Git overlay round-trip and localized search hint

- [x] 35.1 Resolve breadcrumb, overflow, and child-menu icons through the newest compatible exact-location Shell texture while retaining the generic-folder fallback.
- [x] 35.2 Derive the idle search placeholder from the final resolved breadcrumb segment and render `搜尋 {current folder}` independently of address-edit drafts.
- [x] 35.3 Add Rust regressions and a registered headful UTIT covering Git folder → root → Git overlay persistence and localized search-hint updates.
- [x] 35.4 Run formatting, focused tests, Clippy, app build, the registered UTIT, coverage validation, OpenSpec strict validation, and diff checks.

## 36. Adaptive wrapped-icon columns and scrollbar-safe right edge

- [x] 36.1 Specify the Explorer-style preferred-width tolerance, complete-row distribution, sparse-row behavior, and scrollbar-safe right edge.
- [x] 36.2 Add a shared fitted spatial-grid solver and use it for rendering, virtualization, marquee selection, keyboard navigation, and icon scheduling.
- [x] 36.3 Add Rust geometry/source regressions and extend the registered icon-view UTIT with two five-column widths and a selected right-edge scrollbar assertion.
- [x] 36.4 Run formatting, explorer-ui tests, Clippy, app build, the registered icon-view UTIT, coverage validation, OpenSpec strict validation, and diff checks. The new requirement is mapped; the global coverage command remains blocked only by five unrelated `integrate-latest-gpui-explorer-fork` requirements.

## 37. Native-resolution Shell icons while zooming

- [x] 37.1 Replace fixed 16/32px filesystem icon acquisition with Shell system image-list selection that retains overlay indices.
- [x] 37.2 Preserve DPI-derived file-view requests through 1024 physical pixels and use `IShellItemImageFactory` for overlay-free requests above jumbo size.
- [x] 37.3 Add real Shell resolution tests, zoom/DPI cache-key tests, and registered headful UTIT screenshot evidence at the 128px notch.
- [x] 37.4 Run formatting, Shell/UI tests, Clippy, app build, registered icon-view UTIT, coverage validation, OpenSpec strict validation, screenshot review, and diff checks. The relevant Shell icon tests, full explorer-ui suite, build, format, direct registered headful script, screenshot, and strict validation pass. The complete Shell suite remains blocked by the existing unavailable Windows Search fixture followed by poisoned serial locks; Rust 1.97 Clippy reports only pre-existing diagnostics in context-menu, drag/drop, navigation, thumbnail, watcher, and STA test code. The UITest runner remains gated only by five unrelated `integrate-latest-gpui-explorer-fork` coverage entries.

## 38. Restore live TortoiseGit overlays after native-resolution icon upgrade

- [x] 38.1 Reproduce the regression with real clean, modified, added, and unversioned Git items and prove the high-resolution image-list path returns identical pixels.
- [x] 38.2 Request `SHGFI_ICON` together with `SHGFI_OVERLAYINDEX` so installed Shell overlay handlers evaluate the live item, while continuing to render the base from the requested high-resolution system image list.
- [x] 38.3 Retain and release the trigger `HICON` with the existing RAII wrapper, and keep breadcrumb compatible-key resolution for navigation round trips.
- [x] 38.4 Run the real TortoiseGit Shell regression, relevant UI tests, app build, registered breadcrumb Git-overlay headful UTIT, strict OpenSpec validation, and diff checks. The real handler test distinguishes clean, modified, and unversioned pixels; the headful Git-folder → C:\ → Git-folder round trip preserves the exact breadcrumb icon hash and remains distinct from a plain folder.
