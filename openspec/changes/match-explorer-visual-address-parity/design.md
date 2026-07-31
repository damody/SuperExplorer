## 2026-07-27 Explorer 輸入與選取狀態機

所有滑鼠與鍵盤操作共用 typed interaction reducer。`SelectionModel` 以 stable Shell item id 保存 selected、focused、anchor；排序／篩選只提供 range 的 presentation order。普通 click 取代選取並更新 anchor，Ctrl-click toggle，Shift-click 取代為 anchor range，Ctrl+Shift-click 加入 range。

框選使用 window-scoped transient session 與既有 Win32 pointer capture，保存 content-space 起點、游標、原選取與 modifier。renderer 依檢視模式提供 item bounds；mouse-up、Esc、blur、capture-lost、tab/view switch 共用 idempotent terminal path。

context menu 滑鼠 anchor 只做一次 client-to-screen 轉換；鍵盤 anchor 由 focused item 推導。網址列與搜尋列共用 Explorer typography、line-height、selection foreground/background；任何非 editor command、navigation、tab activation 或 window deactivation 都先恢復 resolved breadcrumb。EditableText 保留文字編輯鍵；FileView 接收選取與檔案鍵；global scope 才接導航、分頁、網址與搜尋。

## Context

既有程式已完成 GPUI-CE 視窗、真實資料夾、多分頁、檔案操作、Clipboard、OLE drag-and-drop、context menu 與 search，但目前 chrome 是早期功能導向版：`LayoutTokens` 只有粗粒度高度與間距，網址列是固定寬度的單一 `EditableTextState`，工具列仍有 Unicode 暫代 glyph。現有 Explorer/app 證據的整體視窗尺寸只差 1 physical px，但區域結構、命令位置、側欄比例、內容欄位與視覺資產差異仍大，不能以全圖 pixel diff 證明 10% 座標要求。

核准設計來源為 `docs/superpowers/specs/2026-07-27-explorer-visual-address-parity-design.md`。主要參考環境固定為 Windows 11 build 26200、Explorer `10.0.26100.8875`、繁體中文、淺色、175% DPI、`D:\` Details view；其他 DPI 與主題負責驗證縮放及可用性，不取代主要基準。

需求契約見 [`specs/explorer-visual-parity/spec.md`](specs/explorer-visual-parity/spec.md) 與 [`specs/interactive-breadcrumb-address/spec.md`](specs/interactive-breadcrumb-address/spec.md)，執行順序見 [`tasks.md`](tasks.md)，實作前版本與程式盤點見 [`docs/EXPLORER_VISUAL_ADDRESS_BASELINE.md`](../../../docs/EXPLORER_VISUAL_ADDRESS_BASELINE.md)。

## Goals / Non-Goals

**Goals:**

- 使具名 Explorer 區域、控制項矩形、中心、間距及尺寸誤差不超過 10%。
- 讓按鈕名稱、順序、啟用狀態、icon、平坦色塊、字級、字重與行高可量測並對齊。
- 實作 per-tab 麵包屑／完整路徑 editor 雙模式網址列。
- 讓 segment 名稱與 `>` 都可操作，且 Shell 列舉不阻塞 GPUI thread。
- 維持既有真實檔案、Clipboard、OLE、context menu、search 與多分頁行為。
- 產生可重現的 region-level 視覺證據與失敗診斷。

**Non-Goals:**

- 不嵌入、自動控制或重新散佈 Windows Explorer。
- 不擷取 Explorer 私有二進位資產，也不宣稱 ClearType/GPU rasterization 逐像素相同。
- 不要求最愛／釘選項目的實際內容相同。
- 不以這次 change 擴充 preview handler、完整 Home/Gallery provider 或第三方 extension broker。
- 不對真實 `D:\` 執行破壞性測試。

## Decisions

### 0.1 2026-07-27 捲軸 pointer capture 補強

核准設計見 [`docs/superpowers/specs/2026-07-27-scrollbar-pointer-capture-design.md`](../../../docs/superpowers/specs/2026-07-27-scrollbar-pointer-capture-design.md)。左右垂直 scrollbar 共用 typed drag session；只有從 thumb 按下才開始，track click 保留 page-up/page-down。GPUI window-level capture-phase listener 負責 client area 內跨元素 move/up，Win32 `SetCapture`/`ReleaseCapture` RAII boundary 負責 HWND 外的 mouse move/up。offset 每次依最新 `ScrollHandle` bounds/max offset 與初始 grab offset 重算並 clamp，Mouse Up、release outside、Esc、window deactivate、capture lost、tab switch、close 走同一 idempotent terminal path。

### 0. 2026-07-27 補強：原生圖示跨啟動快取與欄位互動

核准補強設計見 [`docs/superpowers/specs/2026-07-27-explorer-native-icon-cache-sort-columns-overlay-design.md`](../../../docs/superpowers/specs/2026-07-27-explorer-native-icon-cache-sort-columns-overlay-design.md)。檔案／資料夾像素維持 Windows Shell 原始 RGBA 與 overlay，不重新著色；若 GPUI CE Windows texture backend 需要 BGRA byte order，只能在 `RenderImage` 邊界轉換，不能污染 memory/disk cache。Shell STA 使用 bounded memory LRU，並在 `%LOCALAPPDATA%\RustGpuiExplorer\icon-cache\v1` 建立具 schema、Windows build、identity、size、DPI、theme、association/overlay generation 與 checksum 的 lazy disk cache。真實存在的 filesystem item 在每個 process 首次載入時必須優先向 live Shell 重新合成 overlay 並覆寫 disk entry，避免前次啟動時尚未就緒或已過期的 TortoiseGit／OneDrive 狀態永久遮蔽現況；只有 live Shell 暫時失敗才讀 disk fallback。同 process 後續請求仍命中 memory LRU。cache corruption／I/O error 必須回退 Shell load，不得成為功能失敗。

breadcrumb menu 使用 GPUI `deferred(anchored(...))` 進入最上層 paint/hit-test；caption button 的視覺、pointer、accessibility 與 native `WindowControlArea` 共用單一 layout rect。Details 排序與四欄寬度納入 per-tab `ViewSettings`，header、command bar 與 rows 共用 typed reducer；排序只改 presentation order，selection 仍依 stable identity。

### 1. 使用具名區域契約，而非全圖差異作主要 gate

capture diagnostics 對 title/tab、navigation、command、pane、divider、details header、rows、status、search、caption controls 與每個 icon 輸出 logical/physical rectangles。比較器逐欄計算 reference/actual delta；10% 門檻套用於邊界、中心、寬高與間距，小於 10 logical px 的值允許 1 logical px rounding。

全圖 diff 仍輸出供人工檢視，但不把動態文字、不同 favorites 內容或反鋸齒誤差混成單一不具行動性的比例。

替代方案是只調低 pixel tolerance；它無法指出是哪個控制項位移，也會被大量白色內容區稀釋，因此不採用。

### 2. 拆分 Explorer chrome tokens，保持單次 DPI scaling

`LayoutTokens` 擴充為按區域命名的高度、寬度、padding、gap、glyph、column 與 typography tokens。token 保存 logical px，只有 GPUI/Windows boundary 轉 physical px。固定控制項保持參考尺寸，網址列與搜尋框依 Explorer 優先序使用剩餘空間；窄視窗才啟用明確 overflow。

### 3. 原生 Shell icon 與集中式 chrome icon renderer

檔案、資料夾、磁碟及 namespace icon 由 `explorer-shell-win` 透過 Shell image API 取得，轉為 owned pixel payload 後才跨執行緒。`HICON`／`HBITMAP`／COM interface 使用 RAII 且在正確 apartment 釋放。cache key 包含 stable identity、size bucket、DPI、theme 與 association generation。

導覽與 command icon 由 `ExplorerIcon` enum 集中指定 Fluent glyph 或量測向量路徑；production chrome 禁止再以 Unicode 箭頭、剪刀或省略字元冒充最終圖示。若公開資源無法取得 Explorer 私有 icon，使用幾何相符的 Fluent 等價圖示並記錄差異。

### 4. 系統主題來源加 Explorer typography tokens

theme service 把 light/dark/high-contrast 系統狀態映射到 semantic colors；平坦區域每一 sRGB channel 差值不得超過 12。typography 依 tab、command、address、search、navigation、details header、file row、status 分開定義 family fallback、size、weight 與 line height。繁中使用 Windows UI 字型 fallback，不硬編碼單一英文字型。

### 5. 網址列使用明確 per-tab 狀態機

新增 `AddressBarState`：`Browsing`、`Editing`、`EnumeratingMenu` 與 `NavigationError`。每個 tab 保存 resolved ancestry、editor draft 與 menu generation。background event 可更新該 tab 的 resolved location，但不得覆寫 active tab 的 draft。

點空白、`Ctrl+L` 或 `Alt+D` 進入 editor 並全選 parsing path；Enter 使用既有 address parser 提交；Esc 回復 resolved breadcrumb。失敗不提交 history，保留文字及錯誤。segment 點擊轉成既有 typed navigation command。

### 6. Breadcrumb 使用 Shell identity，不以字串切路徑為唯一真相

`BreadcrumbSegment` 保存 stable id、display name、`LocationDescriptor`、icon hint 與 container capability。filesystem 可先產生 ancestry，再由 Shell metadata 補 display name；This PC、磁碟、UNC、ZIP、Libraries 與 namespace location 皆以 Shell ancestry 為準。

### 7. Chevron menu 使用可取消的批次列舉

每次點 `>` 產生含 tab/request/generation 的 child-container request。Shell STA 只回傳可導覽直接 children，以 bounded batches 更新 loading/partial/empty/error menu。關閉、切 tab、導覽、再次開啟或關窗會取消；所有 late events 由 request context 拒絕。

### 8. 漸進遷移而非一次替換所有 chrome

先建立量測與 model contracts，再接 Shell ancestry/menu/icon，最後替換 render tree。每個階段維持可編譯及既有回歸測試。舊 editor 在新 breadcrumb 通過 keyboard、IME、accessibility 與真實資料夾測試後才移除。

## 2026-07-28 More 選單與資料夾選項

- `command-more-menu` 與 Sort／View 相同，必須是 `relative` semantic button；`command-more-popup` 是按鈕的 direct child，以 `absolute top = minimum_hit_target, right = 0` 錨定並以 deferred layer 繪製，禁止回退到 command bar 或視窗原點。
- 選單固定依 Windows 11 繁中 Explorer 排列為：復原、壓縮成 ZIP 檔案、加到我的最愛、複製路徑、分隔線、全選、全部不選、反向選擇、分隔線、內容、選項。每個項目具有 stable id、MenuItem role、繁中 accessibility name、enabled state 與鍵盤索引。
- 復原、加入常用與內容沿用 `ShowContextMenu` 的 canonical verb 邊界；壓縮先解析 `Windows.CompressToZip`，若目前 Windows build 的傳統 `IContextMenu` 未公開該 verb，則在 Shell boundary 以 Windows 內建 `tar.exe -a` 的逐參數呼叫建立 collision-safe ZIP，不經 command string 或 shell interpolation。所有失敗都走既有可恢復錯誤與 tracing，不得假裝成功。複製路徑以選取項目的 filesystem path／Shell parsing name產生 Explorer 相容文字並寫入系統 clipboard。選取三命令沿用 stable-id selection reducer。
- 資料夾選項為 app 內 modal，只提供「一般」與「檢視」兩頁；本次明確不建立「搜尋」頁或搜尋設定。檢視頁直接編輯既有 per-tab `ViewSettings` 的副本，套用／確定時一次提交，取消時不得改變 view。
- modal 的一般頁提供開啟資料夾方式與單／雙擊模式；檢視頁提供套用目前資料夾、重設與進階設定，至少覆蓋項目核取方塊、顯示副檔名、顯示隱藏項目、縮小項目間距、詳細資料窗格與預覽窗格。未接上實際 reducer 的控制項不得偽裝為可用。
- UITest 以 More 按鈕 physical bounds 為 oracle，驗證 popup top/right、項目順序、selection reducers、Options modal 的兩頁與 Search tab 缺席；真實 temporary folder 驗證 Copy Path、Properties、Pin to Home 與 Compress to ZIP 的 command contract及 terminal error 可恢復性。

## 2026-07-28 大／中／小圖示空間排列

- 根因是 wrapped item 仍無條件使用 `w_full()`，使 flex-wrap 永遠只有一欄；同時 Small Icons 被當成垂直 stacked icon view，造成與 Explorer 的水平 icon-label cell 不同，selection 也因此橫跨 viewport。
- 新增純值 `SpatialGridMetrics` 作唯一幾何來源。Small Icons 為 row-major、水平排列、20 logical px icon、32px cell height、240px cell width；Medium／Large／ExtraLarge 為 row-major stacked tiles，分別使用48／64／96px icon與104／120／144px cell width。實測若超過10%再只調整這組 profile token。
- FileViewHost 的 item root在wrapped modes使用固定cell width；Details與Content仍為full row。內層 name/icon layout依 `stacked` 決定flex direction，selected／hover／drop cue自然只覆蓋cell。
- Marquee與鍵盤導航以相同 `columns = floor(viewport/cell_width)` 計算row-major座標；resize只改columns，不改presentation order、stable selection、focus或anchor。
- 切換mode後依metrics icon size重新要求Shell payload；cache key包含size bucket，late result仍由既有generation/key約束隔離。

### 2026-07-31 command menu pointer focus

Sort, View, and More menu rows dispatch bounded `Set*MenuFocus` actions on pointer movement, matching the existing breadcrumb and navigation-history menus. The focused row and CSS hover both use the neutral `selected_inactive` gray, so keyboard focus and pointer focus cannot leave a stale blue row behind. Extensions retains the same row renderer and hover palette for its single command. Popup occlusion and existing mouse-down propagation boundaries remain authoritative.

### 2026-07-31 stable drive breadcrumb names

Local filesystem drive segments use the canonical uppercase drive designator (`D:`) as their stable breadcrumb text. Shell ancestry and icon metadata may arrive asynchronously, but volume titles such as `新增磁碟區 (D:)` must not replace that text or change the segment width. The Shell producer skips drive-title enrichment and the UI state normalizes every incoming drive segment as a defensive boundary; folder, archive, UNC, and namespace names retain their Shell display names.

### 2026-07-31 Explorer tab-strip pointer and focus parity

Every rendered tab owns its complete pointer hit target. A physical middle-button release over that target dispatches the existing typed `CloseTab` action for the hit tab without activating a different tab or falling through to the native window drag region. Tab-strip model focus remains accessible but does not draw a private blue line across the active tab; active/inactive surface colors remain the focus cue. The new-tab control retains its full accessible hit target while its idle fill equals the strip and its glyph uses the official Fluent `Add` artwork instead of the circled `New` command artwork.

### 2026-07-31 selected navigation drive icon stability

Navigation-pane Shell icons and file-view Shell icons share the bounded texture cache. A file-view drive presentation may replace the navigation request's exact key with a newer association or overlay generation for the same location. Navigation snapshots therefore resolve the newest compatible key by location, theme, and DPI and carry that actual key into rendering. Selection changes only the row surface; it never changes or suppresses the resolved Windows Shell drive texture.

### 2026-07-31 This PC view-mode parity

This PC keeps one stable directory presentation and interaction identity while selecting a view-specific visual profile. Details uses the Explorer columns Name, Type, Total size, and Free space. Small, Medium, and Large icons use bounded horizontal drive cards with a Shell icon, name, capacity bar, and localized free/total text. Content uses full-width rows with the filesystem name, a wider capacity bar, and localized capacity text. `GetVolumeInformationW` supplies the filesystem name alongside the existing label and capacity metadata. The group heading is localized as `裝置和磁碟機`; selection, activation, context menus, keyboard focus, and sorting continue through the existing item reducer.

### 2026-07-31 Content rows and continuous wheel zoom

Ordinary-folder Content view uses a fixed-height, full-width two-column row. The left column owns the Shell icon, name, and file-only type line; the right column owns the localized modified-date line and a file-only size line. Containers therefore do not repeat a generic folder type on the right. Every row draws the shared divider token at its bottom and uses the same vertical centering contract.

Ctrl+wheel operates on one ordered per-tab ladder rather than only switching named modes: Content, Tiles, Details, List, then Small 24/32/48, Medium 64/72/84, Large 96/108/128, and Extra Large 256/384/512 logical pixels. Direct View-menu selection enters the middle notch of an icon category. The selected notch drives both Shell icon requests and rendered cell geometry; old persisted sessions restore the category's default notch without changing the durable schema.

### 2026-07-31 aspect-preserving thumbnail cells

Stacked icon views divide every item into a bounded square image host, an 8 logical-pixel gap, and an independent 48 logical-pixel filename region. Thumbnail pixels are fit into the image host with a single uniform scale factor and centered on the unused axis. The host clips all painting as a defensive boundary, so portrait, landscape, square, malformed, and late-arriving Shell thumbnails cannot paint over the filename. Shell icons use the same containment path, while list, Details, Tiles, and Content retain their existing row geometry.

### 2026-07-31 bounded multi-line icon filenames

Stacked icon filenames use a block formatting context whose width and maximum width equal the owning cell's content width, with a zero minimum width so long unbroken names cannot impose their intrinsic width on adjacent cells. Normal items wrap and ellipsize after two lines; the selected item may reveal a third line. Every stacked cell permanently reserves three line boxes, keeping row-major geometry stable when selection changes. The full filename remains in the model and accessibility name; only its presentation is truncated.

### 2026-07-31 breadcrumb overlay round-trip and localized search hint

Breadcrumb, overflow, and breadcrumb-child icons resolve the newest compatible texture for the exact Shell location, theme, and DPI instead of requiring the original generation-zero request key. This preserves a TortoiseGit-composited folder icon when a later file-view request replaces the earlier breadcrumb cache entry, including Git folder → drive/root → Back or direct return navigation. The generic Shell folder texture remains only a geometry-stable fallback when no compatible concrete texture exists.

The idle search field derives its localized hint from the last resolved breadcrumb segment and renders `搜尋 {current folder}`. It falls back to the current history display title only when ancestry is not yet available, so an address-edit draft or delayed metadata event cannot replace the folder name shown by the search field.

### 2026-07-31 adaptive wrapped-icon columns

Wrapped icon views treat the configured cell width as a preferred Explorer profile rather than an immutable width. A shared pure layout solver chooses the nearest complete-row column count, prefers candidates whose width remains within plus or minus ten percent of the profile, and distributes the usable file viewport exactly across that row. The usable width already excludes the overlay scrollbar track, so the selected or focused rightmost item's complete border remains visible. If there are too few items to fill a row, cells retain their profile width instead of stretching sparse content.

Renderer sizing, wrapped virtualization, marquee intersection, keyboard movement, and viewport icon scheduling consume this same fitted geometry. Resize changes only the fitted column geometry; presentation order, stable selection, focus, and Shell icon size remain unchanged.

### 2026-07-31 native-resolution Shell icons while zooming

Filesystem icons no longer use the 16/32px `SHGFI_ICON` result as the visible raster for every zoom level. `SHGetFileInfoW` resolves the system image index and overlay index, then the Shell small, large, extra-large, or jumbo image list supplies pixels at or above the requested physical size through 256px. Requests above 256px without an overlay use `IShellItemImageFactory` at the exact DPI-derived physical size. Overlay-bearing items retain the live Shell-composited jumbo icon rather than dropping TortoiseGit or cloud status artwork. File-view cache keys preserve the complete 16..1024 physical-pixel range required by the 512 logical-pixel zoom notch at 200% DPI; navigation chrome retains its bounded 20px profile.

## Risks / Trade-offs

- [Explorer 私有資產無公開 API] → 使用 Windows Shell/Fluent 公開來源，對例外建立具名差異，不以截圖切圖規避。
- [不同 Windows build 尺寸或 icon 改變] → metadata 固定主基準 build，新增 build 時建立獨立 profile，不靜默覆寫 baseline。
- [Shell provider 列舉緩慢或重入] → STA typed queue、bounded batch、generation cancellation 與可恢復 menu error。
- [icon cache 造成 GDI/COM leak] → owned payload 邊界、RAII、容量限制、theme/DPI invalidation 與 soak handle snapshot。
- [寬度不足造成 breadcrumb/search 擠壓] → 明確 flex 優先序、segment elision/overflow 與窄視窗 geometry tests。
- [重構 chrome 破壞 OLE hit target] → 在每個 UI milestone 重跑 Explorer→app 與 app→Explorer drag fixtures。
- [文字 rasterization 無法逐像素一致] → 比較 family/size/weight/line-height/baseline，mask 文字邊緣但不 mask layout。

## Migration Plan

1. 新增 reference profile、region schema、diagnostics 與 comparator，不改 production render。
2. 新增 layout/theme/typography/icon contracts 與測試。
3. 新增 breadcrumb/address model、actions、typed Shell ancestry/menu events 與 fake service tests。
4. 接上真實 Shell ancestry、child enumeration 與 icon pipeline。
5. 以新 navigation/command/body/status components 逐區替換舊 chrome，保持 fallback editor 至新流程穩定。
6. 執行真實 `D:\`、DPI/theme、keyboard/IME/accessibility、Clipboard/OLE/context menu/search 回歸。
7. 更新 reference evidence、parity/status/manual/handoff 文件後移除舊 placeholder paths。

若需 rollback，保留新 model/service contracts但由 feature switch 回到舊 address renderer；baseline 不自動更新，避免把失敗畫面寫成新標準。

## Open Questions

無阻擋問題。實作遇到 Explorer 私有或 build-specific 行為時，依「公開 Windows API 優先、10% 幾何契約、記錄已知差異」原則自行決定。
## 2026-07-27 一般權限 UI、按需 UAC 與背景日誌決策

- application、diagnostics、Shell STA 與 GPUI 全程維持 `asInvoker`，不在 startup 自我提升；因此一般權限的 AutoHotkey／快捷鍵工具仍可在視窗取得焦點時攔截 F1。原生 `IFileOperation` 保留 `FOF_NOERRORUI` 並加入 `FOFX_SHOWELEVATIONPROMPT`，只有操作受保護位置而需要較高權限時才由 Windows Shell 顯示 UAC；取消 UAC 由既有 operation terminal/error pipeline 回報。
- process-wide tracing formatter 固定停用 ANSI。這個應用程式的 stdout/stderr 可能由背景 log viewer 或檔案重新導向接收，純文字輸出可保證 timestamp、level、message 與 structured fields 可讀且不出現 escape bytes。
- startup integrity、file-operation flags 與 formatter 都建立 source/output contract test；GPUI manifest 仍保留 PerMonitorV2、Common Controls v6 與 `uiAccess=false`。
## 2026-07-27 網址列文字垂直置中校準

- 網址列與搜尋列的共用 Explorer input typography 由 13/20 logical px 調為 14/22，baseline 調為 17；外框仍維持 32 logical px。
- editable field 的上下 padding 由 `(minimum_hit_target - line_height) / 2` 推導，在 32 px 外框內各為 5 px，使文字、selection 與 caret 共用同一垂直中心，不使用與 DPI 綁定的固定 physical offset。

## 2026-07-27 命令列資料收斂與指標索引校正

- 所有 file-row pointer／keyboard action 的 `row_index` 統一代表排序、隱藏篩選及 view mode 套用後的 presentation index；模型以同一排序器解析 stable item，捲動也只接受 presentation index，禁止再混用原始 `DirectorySnapshot` index。
- command、sort/view menu 與 Details header 在 mouse-down 階段停止向檔案背景傳播，避免工具列點擊誤啟動 marquee、row selection 或 scrollbar paging。切換排序／檢視直接更新目前 tab 的 presentation，不等待磁碟 I/O。
- Create/Rename/Paste/Delete 或可變更檔案的 Shell context command 成功或部分成功後，若 terminal context 仍符合目前 tab generation，主動提交一次 refresh；若 watcher 已先推進 generation 則不得重複刷新。
- Rename 按鈕在單選且可寫時進入既有 inline editor；Share 按鈕對選取項目解析並直接執行 Windows Shell canonical `Windows.Share` verb，找不到 verb 時回報可恢復錯誤，不假裝成功。

## 2026-07-27 Details header 固定捲動修正

Details header 從 scroll host 的內容樹移出，成為固定 viewport wrapper 的 sibling absolute layer；資料列與保留標題高度的 spacer 才進入 scroll host。如此 `top` 永遠為 `0`，不依賴 wheel compositor offset 與 render 更新的先後順序；只有 `left` 跟隨 `offset.x` 以維持水平 overflow 的 header/rows 同欄對齊。headful 測試直接量測 UIA 螢幕座標，避免只依賴 render tree 推論。

## 2026-07-27 搜尋框比例校準

使用者提供的同類型 Explorer 畫面在1867 physical px、175% DPI下，搜尋框約435 physical px，即視窗寬度的23.3%；既有2688px Explorer baseline則約為25%。application原本固定384 logical px，在使用者視窗顯示為約672 physical px並明顯壓縮網址列。新配置使用視窗logical width的23.5%，再以既有120/384 logical px作compact／寬螢幕上下限；網址列維持`flex_1`並自動取得其餘空間。驗收同時涵蓋兩個reference寬度、DPI單次縮放、compact無重疊與headful region bounds，不只比對單張截圖。

## 2026-07-27 F2 inline rename 高度與置中校準

Details row維持32 logical px，但inline rename field由整列高縮為24 logical px，並由名稱欄的flex cross-axis置中。檔名字型沿用file-row 12/16 metrics，垂直padding以`(field height - line height) / 2`推導為上下各4 logical px，使glyph、selection與caret共用同一line box；focus border仍為1 logical px。錯誤提示的anchor改用24px field底部，避免保留舊32px位移。相同置中helper亦供address/search使用，防止兩套公式漂移。

## 2026-07-28 排序／檢視選單錨點修正

- 排序與檢視 popup 成為各自 `relative` semantic button 的 direct child，不能再掛在整條 command bar 尾端，也不增加會改變 hit-test 的外層 wrapper。實機追查確認 GPUI-CE 的 deferred `AnchoredPositionMode::Local` 在此 flex/absolute 組合只保留水平 static position、垂直 origin 退回 client top，因此此處不再依賴該不完整 origin。
- popup 由按鈕內的 `absolute top = minimum_hit_target, right = 0` 直接定位：右緣與按鈕右緣對齊、垂直位移等於按鈕高度，因此第一列位於按鈕下方；popup 固定寬度小於 compact window 可用寬度，右對齊可避免靠近視窗右緣溢出。`deferred` 保留並提高 paint priority，確保選單顯示於檔案列與左側導覽之上。
- 驗收不把單一固定螢幕座標寫死，而檢查 popup 與觸發按鈕水平相交、第一個 menu item 位於按鈕底部附近、整個 item 位於視窗範圍，並禁止 `(0,0)` 原點回歸。
- UITest 分別啟用 Sort 與 View，取得 UIA physical bounding rectangles 後套用相同 oracle；既有 Sort mouse smoke 保留真實 pointer 路徑，專用 anchor case 使用 InvokePattern 排除游標／焦點競態。報告保存 button、popup、first item、window bounds 與 top/right delta，讓 DPI 下的錯位可重現。
