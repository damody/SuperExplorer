## ADDED Requirements

### Requirement: This PC adapts to Explorer view modes

The This PC surface SHALL use one stable item model with Explorer-specific presentation for each requested view. Details SHALL expose Name, Type, Total size, and Free space columns without Date modified. Small, Medium, and Large icons SHALL use bounded horizontal drive cards. Content SHALL use full-width rows with filesystem name, capacity bar, and localized free/total capacity text. Every view SHALL retain Shell icons, the `裝置和磁碟機` group, selection, activation, keyboard, and context-menu identity.

#### Scenario: Switch every referenced This PC view

- **WHEN** the user opens This PC and selects Details, Small icons, Medium icons, Large icons, and Content
- **THEN** each view uses its Explorer-specific columns or geometry
- **AND** every available drive exposes truthful total/free capacity and filesystem metadata
- **AND** physical-pointer UTIT captures and measures all five view presentations

### Requirement: Selected navigation drives retain their Windows Shell icon

The navigation pane SHALL keep the Windows Shell drive icon visible when a local drive row becomes selected. If a newer compatible icon generation for the same drive location has replaced the original navigation key in the bounded cache, the snapshot SHALL render that newer key instead of an empty icon slot. Selection styling SHALL affect only the row surface.

#### Scenario: Select C after opening This PC

- **WHEN** the user opens This PC and then activates the C: navigation row
- **THEN** C: remains selected with a visible Windows Shell drive icon
- **AND** other visible local-drive rows retain their icons
- **AND** the behavior is verified by a physical-pointer UTIT screenshot and pixel oracle

### Requirement: F2 rename SHALL remain available after directory navigation
After a successful directory navigation, the file view SHALL keep keyboard current-row and inline-rename target resolution consistent. If the new directory has visible items but no focused row, pressing F2 SHALL select and focus the first visible item before opening inline rename. If a visible focused item exists, that item SHALL remain the rename target. Read-only locations and empty directories SHALL NOT open an editor.

#### Scenario: F2 immediately after switching to a populated writable folder
- **WHEN** navigation finishes in a writable folder with visible entries and the previous directory selection has been cleared
- **AND** the user presses F2 while the file view owns focus
- **THEN** the first visible item becomes the single selected and focused item
- **AND** its inline rename editor opens

#### Scenario: Existing focus wins after switching folders
- **WHEN** the user selects a visible item after navigation
- **AND** presses F2
- **THEN** inline rename opens for that focused item rather than the first item

#### Scenario: Rename is unavailable without a writable target
- **WHEN** the active location is read-only or has no visible entries
- **AND** the user presses F2
- **THEN** no inline rename editor opens

#### Scenario: Selecting an item after navigation-pane drive switch restores file-view focus
- **WHEN** the user opens inline rename on an item in one drive
- **AND** uses the navigation pane to switch to another drive
- **AND** selects an item in the destination file view
- **THEN** the model and native focus owners SHALL both resolve to FileView
- **AND** pressing F2 SHALL open inline rename for the destination item

### Requirement: Explorer 輸入與選取互動一致性

應用程式 SHALL 使用 Explorer 相容的 pointer 座標、輸入框視覺、selection anchor、marquee 與 surface-aware keyboard commands。

#### Scenario: 滑鼠右鍵選單位於游標
- **WHEN** 使用者在項目或背景放開右鍵
- **THEN** native context menu 在對應 screen point 顯示且不重複轉換 origin 或 DPI

#### Scenario: 網址與搜尋文字清楚可見
- **WHEN** 網址文字反白或任一輸入框取得焦點
- **THEN** Explorer 相容的前景、selection 背景、字級、line-height 與置中保持文字可讀

#### Scenario: Shift 依畫面順序連選
- **WHEN** anchor 建立後 Shift-click 第二個項目
- **THEN** 目前 sort/filter order 中 anchor 到 target 的項目全部選取

#### Scenario: 框選可離開起始範圍
- **WHEN** 背景開始框選並按住滑鼠移出檔案檢視
- **THEN** pointer capture 持續更新矩形和相交選取直到 exactly-once terminal event

#### Scenario: 鍵盤與滑鼠共用語意
- **WHEN** 等價 Explorer 鍵盤或滑鼠命令操作 focus、anchor、selection、navigation、file、tab、address 或 search
- **THEN** 命令經過相同 typed transitions 並產生等價結果

### Requirement: 垂直捲軸拖曳捕捉

左右垂直 scrollbar SHALL 只在 pointer 從 thumb 開始按下時建立唯一 typed drag session，並 MUST 在 pointer 橫向離開 scrollbar、移至其他 client element 或暫時移出 application HWND 後，仍依 pointer Y 與原始 thumb grab offset 連續更新正確 `ScrollHandle`。track click SHALL 保留 page-up/page-down且不得建立 drag session。

#### Scenario: 離開捲軸範圍仍持續拖曳
- **WHEN** 使用者按住 file-view thumb 並將 pointer 移到檔案列中央或 HWND 外上下移動
- **THEN** thumb 與內容 offset 持續依垂直位置更新，橫向距離不影響進度，超出 track 時 clamp 至起點或終點

#### Scenario: 所有終止路徑只釋放一次
- **WHEN** drag session 收到 Mouse Up、Mouse Up Outside、Esc、window deactivate、capture lost、tab switch 或 window close
- **THEN** session 與 native capture SHALL 經同一 idempotent transition 恰好終止一次，放開後後續 pointer move不得改變offset

#### Scenario: 幾何在拖曳中改變
- **WHEN** viewport、content length、DPI 或 window size 在拖曳中改變
- **THEN** target offset SHALL 使用當下 handle bounds、maximum及thumb height重新計算，不得保留越界或使用過期幾何

追蹤來源：[`proposal.md`](../../proposal.md)、[`design.md`](../../design.md)、[`tasks.md`](../../tasks.md)、[`docs/EXPLORER_VISUAL_ADDRESS_BASELINE.md`](../../../../../docs/EXPLORER_VISUAL_ADDRESS_BASELINE.md)。

### Requirement: 固定且可重現的 Explorer 參考環境
系統 SHALL 維護 Windows 11 build、Explorer version、語系、主題、DPI、字型、視窗尺寸、location、排序與 view mode 的參考 metadata，且測試不得自動覆寫已核准 baseline。

#### Scenario: 擷取主要 D 槽基準
- **WHEN** 在主要環境擷取 `D:\` Details view 的 Explorer 與 application
- **THEN** 兩份 metadata 必須記錄相同視窗條件、175% DPI、繁中淺色主題與各自 commit/version

### Requirement: 具名區域 10% 幾何同等性
系統 MUST 對 title/tab、navigation、command、navigation pane、divider、details header、file rows、status、search 與 caption controls 建立具名矩形，且每個矩形的邊界、中心、寬高與相鄰間距相對誤差 MUST 不超過 10%。

#### Scenario: 區域超出座標門檻
- **WHEN** 任一具名區域相對 Explorer 的座標或尺寸誤差超過 10%
- **THEN** parity gate 必須失敗並列出 reference、actual、delta 與超標欄位

#### Scenario: 微小 rounding 數值
- **WHEN** 參考尺寸或間距小於 10 logical px
- **THEN** 比較器可接受最多 1 logical px 的 DPI rounding，但不得放寬其他區域

### Requirement: Explorer 控制項名稱、順序與狀態
application SHALL 在相同設定下呈現與 Explorer 對應的可見按鈕名稱、區域順序、tooltip/accessibility label 與 enabled/disabled 狀態；favorites 的內容可不同，但容器幾何仍 MUST 比較。

#### Scenario: D 槽無選取狀態
- **WHEN** 兩個應用程式都開啟 `D:\` 且沒有選取項目
- **THEN** navigation、command、view、details 與 status controls 的順序及 availability 必須符合參考 profile

### Requirement: Windows 原生檔案與 Shell icon
系統 SHALL 從 Windows Shell 取得檔案、資料夾、磁碟與 namespace icon，並以 stable identity、size、DPI、theme 與 association generation 快取；取得失敗時 MUST 使用同尺寸 fallback 且不得改變列幾何。

#### Scenario: 顯示真實 D 槽混合項目
- **WHEN** `D:\` 同時包含資料夾、ZIP、RAR 或其他已關聯檔案
- **THEN** 每列必須顯示 Shell 對應 icon，且 icon bounds/center 誤差不超過 10%

#### Scenario: theme 或 DPI 改變
- **WHEN** 視窗 DPI 或 Windows theme 在執行中改變
- **THEN** size/theme-dependent cache 必須失效並重新載入一次，不得重複 scaling 或洩漏 GDI/COM 資源

#### Scenario: 跨啟動暖快取
- **WHEN** 相同 Windows build、Shell identity、size、DPI、theme、association 與 overlay generation 的 item 在第二次啟動再次可見
- **THEN** 系統必須可從 `%LOCALAPPDATA%` 版本化 disk cache 還原相同 alpha-correct RGBA，且不得重新著色或持有前次 process 的 native handle

#### Scenario: 圖示快取損壞或過期
- **WHEN** disk entry 的 magic、schema、key digest、length 或 checksum 不符，或 association/overlay generation 已改變
- **THEN** 系統必須拒絕該 entry、回退 Shell load 並以原子方式重建，不得顯示錯誤關聯或錯色圖示

### Requirement: Details 欄位排序與調整寬度
Details view SHALL 讓名稱、修改日期、類型與大小欄位依 typed value 升／降冪排序，並讓使用者拖曳 separator 調整 per-tab logical width；header 與 rows MUST 共用同一排序／欬寬狀態。

#### Scenario: 點擊欄位標題排序
- **WHEN** 使用者點擊新欄或再次點擊目前欄
- **THEN** presentation 必須依 Explorer-compatible container、typed value、missing-value、name fallback 與 stable identity 規則排序，並更新方向 indicator但不得破壞 selection identity

#### Scenario: 拖曳欄位 separator
- **WHEN** 使用者拖曳 Details header separator或 release outside
- **THEN** 欄寬必須即時更新並 clamp，header/rows保持對齊，drag session恰好結束一次且背景 tab 不得被改動

### Requirement: Caption 顯示與互動矩形一致
最小化、最大化／還原與關閉按鈕的 visual bounds、pointer bounds、accessibility bounds 與 native Windows control area MUST 由同一個 layout rectangle產生。

#### Scenario: Caption click grid
- **WHEN** 在按鈕可見矩形內外各取邊界點執行 pointer與native hit-test
- **THEN** 內部點必須觸發正確控制、外部點不得誤觸，且最大化按鈕保留 Snap Layout 行為

### Requirement: 集中式 Explorer chrome icon
navigation、command、view、status 與 caption controls MUST 經集中式 icon contract 渲染，production UI SHALL NOT 使用 Unicode 箭頭或任意文字符號作最終 icon。

#### Scenario: 掃描 production chrome
- **WHEN** 執行 source contract test
- **THEN** 所有可見 chrome icon 必須有穩定 icon id、來源、logical size、可見 bounds 與狀態色彩

### Requirement: 系統配色同等性
application SHALL 由 Windows theme/system semantic colors 產生 light、dark 與 high-contrast tokens；主要參考環境的平坦色塊每一 sRGB channel 絕對差 MUST 不超過 12。

#### Scenario: 淺色平坦區域比較
- **WHEN** 比較 surface、control fill、divider、selected、hover、pressed 與 disabled 平坦區域
- **THEN** 每個未受文字反鋸齒影響的 sample 必須通過 channel 12 門檻

#### Scenario: 高對比啟用
- **WHEN** Windows 啟用 high contrast
- **THEN** application 必須使用系統 semantic colors 且維持文字、焦點與選取可辨識

### Requirement: Explorer typography 同等性
application SHALL 對 tab、command、address、search、navigation、details header、file row 與 status 使用具名 typography tokens；family fallback、size、weight、line height 與 baseline MUST 可診斷，字級誤差不得超過 1 logical px。

#### Scenario: 繁體中文文字比較
- **WHEN** 在繁中參考環境顯示相同按鈕、欄名與路徑
- **THEN** UI 必須使用 Windows UI 字型回退並通過字級、行高與 baseline 契約

### Requirement: 單次 DPI scaling 與 resize 行為
layout、icon 與 typography logical values MUST 僅在 GPUI/Windows 邊界縮放一次，並在 100%、125%、150%、175%、200% 及 resize/maximize/restore 下保持無裁切、無重疊與穩定 pane 比例。

#### Scenario: DPI matrix
- **WHEN** 在支援的 DPI case 擷取 geometry diagnostics
- **THEN** physical value 必須等於 logical value 乘單一 scale factor 加允許 rounding，且 region ordering 不變

### Requirement: Region-level 視覺證據
視覺工具 SHALL 輸出 reference、actual、overlay、raw diff、masked diff、region report、icon report、typography report 與 metadata；動態 favorites 與文字邊緣 mask MUST 明確記錄，layout bounds 不得被 mask。

#### Scenario: 視覺 gate 失敗
- **WHEN** geometry、color、icon 或 typography 任一 gate 失敗
- **THEN** artifact 必須指出具名 region、實際值、門檻與可重現 capture command

### Requirement: 既有 Explorer 功能無回歸
chrome 重構後 MUST 通過真實資料夾、多分頁、檔案操作、Clipboard、OLE drag-and-drop、context menu 與 search 的既有 gates，且真實 `D:\` 不得作為破壞性測試目標。

#### Scenario: Explorer 到 application 的 copy drag
- **WHEN** 使用 `20260727-drag-v26-explorer-to-app` 證據所對應的隔離 fixture 執行 left-copy drop
- **THEN** drop effect、磁碟 oracle、selection/watch convergence 與資源 cleanup 必須成功，且原始證據檔不得被修改
## ADDED Requirements

### Requirement: As-invoker startup, on-demand elevation and readable background diagnostics
The Windows executable SHALL keep its UI process at the invoking user's integrity level, SHALL request UAC only for native file operations that require elevated rights, and background tracing output SHALL be plain UTF-8 text without ANSI terminal escape sequences.

#### Scenario: Windows starts the executable
- **WHEN** the user launches the packaged or Cargo-built Explorer executable
- **THEN** diagnostics, Shell/OLE and GPUI SHALL start without a UAC prompt at the invoking user's integrity level
- **AND** the application SHALL NOT bind or consume an otherwise unhandled F1 key

#### Scenario: A file operation needs elevated rights
- **WHEN** `IFileOperation` reaches a protected destination or item that requires administrator rights
- **THEN** Windows Shell SHALL display its UAC elevation prompt despite general error UI being disabled
- **AND** cancelling the prompt SHALL return through the typed operation outcome without elevating the UI process

#### Scenario: A background host captures process output
- **WHEN** stdout or stderr is captured by a log viewer or redirected to a file
- **THEN** timestamp, level, message and structured fields SHALL remain readable
- **AND** the output SHALL contain no ANSI escape byte
### Requirement: Address input text is optically centered
The editable address and search inputs SHALL use Explorer-scale typography and derive equal vertical padding from the field height and line height.

#### Scenario: Address text is displayed or selected
- **WHEN** the address input renders at any supported Windows DPI
- **THEN** its 14 logical px text SHALL use a 22 logical px line box centered inside the 32 logical px field
- **AND** text, selection and caret SHALL share the same vertical origin

### Requirement: Command actions update the visible presentation without pointer side effects
The command bar, sort/view menus and Details header SHALL dispatch against the active tab's presentation order and SHALL NOT cause unrelated file-view scrolling or selection.

#### Scenario: User clicks a command after sorting the folder
- **WHEN** the visible order differs from the underlying directory snapshot order
- **THEN** selection, rename, open, context menu, drag/drop and scroll-to-visible SHALL resolve the same stable item at the clicked presentation index
- **AND** the command or menu mouse-down SHALL NOT begin a file-background marquee or scrollbar page action

#### Scenario: A Shell mutation reaches a successful terminal event
- **WHEN** Create, Rename, Paste, Delete or an invoked Shell command finishes or partially succeeds for the active generation
- **THEN** the application SHALL request one correlated refresh so the right pane converges without waiting indefinitely for a watcher
- **AND** a stale operation generation SHALL NOT schedule a duplicate refresh

#### Scenario: Rename or Share is invoked from the command bar
- **WHEN** Rename has exactly one writable selected item
- **THEN** it SHALL enter the existing inline rename editor for that stable item
- **WHEN** Share has one or more selected items
- **THEN** it SHALL invoke the Windows Shell canonical Share verb or report a recoverable unavailable error

### Requirement: Details header remains pinned while rows scroll
The Details column header SHALL remain fixed at the top of the file viewport while its rows scroll.

#### Scenario: The Details view scroll position changes
- **WHEN** the user scrolls vertically by mouse wheel, scrollbar, keyboard, or marquee auto-scroll
- **THEN** the Name, Date modified, Type, and Size header SHALL keep the same viewport top coordinate
- **AND** horizontal scrolling SHALL keep every header column aligned with its corresponding row column

### Requirement: Search field preserves Explorer navigation proportions
The navigation row SHALL size the search field to the current Explorer reference proportion and SHALL give the remaining flexible width to the breadcrumb address field.

#### Scenario: The navigation row renders at the reference window width
- **WHEN** the application and Explorer render at approximately 1867 physical px and 175% DPI
- **THEN** the application search field SHALL use approximately 23.5% of the logical window width (about 435–450 physical px at this size)
- **AND** its width ratio and horizontal coordinates SHALL remain within the 10% named-region geometry tolerance
- **AND** compact and very wide windows SHALL clamp to the existing 120 and 384 logical px limits

### Requirement: Search hint names the current folder in Traditional Chinese

The idle search field SHALL display `搜尋 ` followed by the active tab's current resolved folder display name.

#### Scenario: Navigate between folders

- **WHEN** the active tab resolves a different folder or returns through history
- **THEN** the idle search hint SHALL immediately become `搜尋 {current folder}`
- **AND** an address-editor draft SHALL NOT replace the resolved folder name in that hint

### Requirement: Inline rename editor is compact and vertically centered
The F2 inline rename editor SHALL use Explorer-scale file-row typography inside a compact field centered within the active row.

#### Scenario: A Details item enters F2 rename mode
- **WHEN** the inline editor appears in a 32 logical px Details row
- **THEN** its visible field SHALL be 24 logical px high and vertically centered in the row
- **AND** the 16 logical px filename line box SHALL receive equal 4 logical px top and bottom padding
- **AND** text, selection, caret and focus border SHALL remain aligned at every supported DPI

### Requirement: Details column resizing follows the pointer one-to-one
The Details header column resize interaction SHALL use one logical client-coordinate space from pointer-down through every captured move.

#### Scenario: A separator is dragged at a supported Windows DPI
- **WHEN** the pointer moves a given logical distance left or right at 100%, 125%, 150%, 175%, or 200% DPI
- **THEN** the active column width SHALL change by the same logical distance, subject only to the documented minimum and maximum width clamps
- **AND** a Win32 physical client coordinate SHALL be divided by the window scale factor exactly once before it reaches the resize reducer
- **AND** native pointer capture SHALL continue the same one-to-one drag after the pointer leaves the separator or client area

### Requirement: Sort and View popups are anchored to their invoking controls
The command bar SHALL position the Sort and View popup menus from the actual rendered bounds of the button that invoked them, and SHALL paint the popup above the file and navigation surfaces.

#### Scenario: Pointer opens Sort or View
- **WHEN** the user clicks the Sort or View command-bar button
- **THEN** the first popup item SHALL appear directly below the invoking button and SHALL horizontally overlap that button
- **AND** the popup SHALL NOT use the window origin or command-bar origin as a fallback anchor
- **AND** the popup SHALL remain inside the current window viewport, shifting left or above only when required by an edge

#### Scenario: UITest verifies popup geometry
- **WHEN** the headful UITest opens each menu at the reference DPI and window size
- **THEN** it SHALL record the window, button and first menu-item physical rectangles
- **AND** it SHALL fail if the item is near `(0,0)`, is above the button without an edge constraint, does not overlap the button horizontally, or lies outside the window

### Requirement: More commands match Windows Explorer
The command bar SHALL expose the Windows 11 More menu with the same command order, typed availability, keyboard behavior, and button-relative placement.

#### Scenario: More menu opens from the command bar
- **WHEN** the user invokes the three-dot button
- **THEN** the menu SHALL list 復原、壓縮成 ZIP 檔案、加到我的最愛、複製路徑、全選、全部不選、反向選擇、內容、選項 in that order with the Explorer separator groups
- **AND** its top and right edges SHALL be derived from the invoking button rather than the window origin
- **AND** disabled operations SHALL remain visible but SHALL NOT dispatch

#### Scenario: More menu executes a command
- **WHEN** the user invokes a selection command, copy-path command, Shell canonical command, or folder options
- **THEN** it SHALL use the existing stable selection, clipboard, Shell context-command, or view-settings boundary respectively
- **AND** terminal failure SHALL be recoverable and observable instead of reporting false success

### Requirement: Folder Options excludes Search for this change
The application SHALL provide an in-app Folder Options modal containing General and View pages, while search options remain explicitly out of scope.

#### Scenario: Options is opened and applied
- **WHEN** the user opens 選項 from the More menu
- **THEN** the modal SHALL expose only 一般 and 檢視 tabs
- **AND** Cancel SHALL discard the draft
- **AND** Apply or OK SHALL update the active tab ViewSettings and refresh the right-hand presentation where required
- **AND** no Search tab or search preference SHALL be rendered or persisted by this change
### Requirement: Icon views use Explorer spatial flow
The file surface SHALL use Explorer-compatible fixed item cells for Small, Medium, Large, and Extra Large icon modes instead of stretching each item across the viewport.

#### Scenario: Small Icons renders a real folder
- **WHEN** Small Icons is active in a viewport wide enough for multiple cells
- **THEN** each item SHALL place a 20 logical px icon to the left of a single-line filename
- **AND** items SHALL advance left-to-right and wrap to the next row
- **AND** hover and selection SHALL cover only that item's fixed cell

#### Scenario: Medium or Large Icons renders a real folder
- **WHEN** Medium Icons or Large Icons is active
- **THEN** each item SHALL center the requested 48 or 64 logical px Shell icon above a centered filename of two lines normally and at most three lines while selected
- **AND** tiles SHALL advance left-to-right and wrap by viewport width
- **AND** no selected tile SHALL span the full viewport unless the viewport itself is narrower than one cell

#### Scenario: Icon grid interaction follows rendered geometry
- **WHEN** the user resizes, uses arrow keys, Shift selection, marquee selection, drag/drop, rename, or context menu in an icon mode
- **THEN** every interaction SHALL use the same row-major cell geometry as rendering
- **AND** stable item identity, focus and selection SHALL survive a column-count change

### Requirement: Navigation pane icons and optional roots are truthful
The navigation pane SHALL render the Windows Shell icon for every visible static or dynamically expanded location, and SHALL distinguish an unavailable optional root from an empty but usable location.

#### Scenario: This PC and a drive are expanded
- **WHEN** the navigation pane shows C:, D:, or a child folder discovered after startup
- **THEN** each row SHALL use the Shell icon belonging to that exact drive or folder location
- **AND** the asynchronous icon snapshot SHALL include dynamic tree children instead of reverting them to a generic fallback

#### Scenario: An optional root has no provider or content source
- **WHEN** Quick Access has no pins or OneDrive is not configured on the computer
- **THEN** the row SHALL remain visible with a prohibition marker and unavailable accessibility name
- **AND** pointer or keyboard activation SHALL NOT dispatch navigation

#### Scenario: Gallery is available on Windows 11
- **WHEN** the Gallery row is activated
- **THEN** it SHALL use the registered Gallery Shell namespace CLSID rather than the unresolved `shell:Gallery` alias

### Requirement: Command menu hover follows the pointer
The Sort, View, More, and Extensions command-bar menus SHALL use the same neutral-gray pointer hover and keyboard focus treatment as the breadcrumb address menu.

#### Scenario: Pointer moves between command menu rows
- **WHEN** the pointer moves from one enabled menu row to another
- **THEN** the neutral-gray highlight SHALL move to the row under the pointer
- **AND** the previously hovered row SHALL return to the normal menu fill
- **AND** no file or folder row beneath the popup SHALL receive hover or activation

#### Scenario: Keyboard focus follows pointer hover
- **WHEN** a command menu is opened by keyboard and the pointer subsequently enters another enabled row
- **THEN** that row SHALL become the focused menu item for Enter or Space activation
- **AND** Sort, View, More, and Extensions SHALL retain their existing Escape and outside-click dismissal behavior

### Requirement: Explorer tab strip pointer and focus parity
The tab strip SHALL close the tab under a physical middle click, SHALL keep focused active tabs free of an extra top focus line, and SHALL render the new-tab control as a plain Fluent Add glyph on the same idle surface as the strip.

#### Scenario: Middle-click a tab
- **WHEN** the user presses and releases the middle mouse button over a tab
- **THEN** exactly that tab SHALL close through the existing typed close-tab lifecycle
- **AND** the gesture SHALL NOT activate another tab or begin a native window drag

#### Scenario: A newly created tab owns tab-strip focus
- **WHEN** Ctrl+T creates and focuses the active tab
- **THEN** the active tab SHALL retain its normal content-matching surface without a blue line along its top edge
- **AND** accessibility selection and keyboard traversal SHALL remain available

#### Scenario: New-tab button is idle
- **WHEN** the pointer is not hovering or pressing the new-tab button
- **THEN** its background SHALL equal the tab-strip background
- **AND** it SHALL show the official Fluent Add glyph without an enclosing circle while preserving its full hit target and accessible name

### Requirement: Content view matches Explorer row contract
Ordinary filesystem Content view SHALL use full-width, equal-height rows with Explorer-like metadata placement and visible horizontal separators.

#### Scenario: A folder is shown in Content view
- **WHEN** a container row is rendered in Content view
- **THEN** its left column SHALL show its Shell icon and display name
- **AND** its right column SHALL show the localized modified-date label and value
- **AND** it SHALL NOT repeat a generic folder type line
- **AND** its bottom divider and vertical spacing SHALL match every adjacent Content row

#### Scenario: A file is shown in Content view
- **WHEN** a non-container row is rendered in Content view
- **THEN** its left column SHALL show the display name and localized type label
- **AND** its right column SHALL show localized modified-date and size labels

### Requirement: Continuous wheel zoom exposes every Explorer notch
Ctrl+wheel SHALL move through Content, Tiles, Details, List, and all twelve requested icon-size notches without becoming stuck at a named view boundary.

#### Scenario: The user wheels down from Details
- **WHEN** Details is active and the user sends consecutive Ctrl+wheel-down notches
- **THEN** the first notch SHALL activate Tiles
- **AND** the second notch SHALL activate Content
- **AND** further downward input SHALL remain bounded at Content

#### Scenario: The user wheels up through icon categories
- **WHEN** the user advances upward through Small, Medium, Large, and Extra Large icons
- **THEN** the rendered and Shell-requested logical sizes SHALL be 24/32/48, 64/72/84, 96/108/128, and 256/384/512 respectively
- **AND** cell geometry SHALL contain each exact icon size without clipping

### Requirement: Thumbnail aspect ratio never overlaps labels

The application SHALL preserve the source aspect ratio of thumbnails and SHALL confine thumbnail painting to a bounded image region that is separate from the filename region in stacked icon views.

#### Scenario: Portrait, landscape, and square images are contained

- **WHEN** a folder contains portrait, landscape, and square images
- **AND** the user selects Medium, Large, or Extra large icons
- **THEN** each thumbnail SHALL be uniformly scaled to fit within its square image host
- **AND** no thumbnail edge SHALL cross into the filename region

#### Scenario: Filename region remains stable while thumbnails arrive

- **WHEN** a Shell thumbnail asynchronously replaces a generic file icon
- **THEN** the item cell height, filename position, and selection hit target SHALL remain unchanged
- **AND** the image host SHALL clip any malformed or oversized renderer output

### Requirement: Stacked icon filenames never enter adjacent cells

Medium, Large, and Extra large icon filenames SHALL wrap only inside the owning fixed-width cell. Normal items SHALL show no more than two lines, selected items SHALL show no more than three lines, and remaining text SHALL end with an ellipsis without changing the stored filename.

#### Scenario: Adjacent items have long names

- **WHEN** adjacent icon cells contain a spaced Latin name, an unbroken Latin name, or a long CJK name
- **THEN** every visual line SHALL remain inside its owning cell's horizontal bounds
- **AND** unselected names SHALL truncate with an ellipsis after two lines

#### Scenario: A long-name item becomes selected

- **WHEN** a truncated icon item becomes selected
- **THEN** it MAY reveal a third line and SHALL ellipsize any remaining text
- **AND** its cell height, neighboring cell bounds, row-major positions, stable identity, and complete underlying filename SHALL remain unchanged

### Requirement: Wrapped icon columns fit the usable viewport

Small, Medium, Large, Extra large, and Tiles views SHALL derive one shared row-major grid from the usable file viewport after scrollbar space is reserved. A complete row SHOULD adjust each preferred cell width by no more than ten percent and SHALL place the final item edge at or before the usable viewport edge.

#### Scenario: Two nearby window widths retain the same column count

- **WHEN** two viewport widths can both fit five cells by adjusting the preferred cell width within ten percent
- **THEN** both viewports SHALL render five columns
- **AND** every cell in each complete row SHALL have the same fitted width
- **AND** the five cells SHALL consume the usable row width without leaving an accumulating remainder

#### Scenario: The rightmost selected item is next to a vertical scrollbar

- **WHEN** enough items require a vertical scrollbar and the user selects the rightmost item in a complete icon row
- **THEN** its complete selection or focus border SHALL remain at or before the scrollbar track
- **AND** painting, hit testing, marquee intersection, keyboard navigation, and virtualization SHALL agree on the same cell bounds

#### Scenario: A row contains fewer items than its capacity

- **WHEN** the folder contains fewer items than the fitted full-row column count
- **THEN** those cells SHALL retain their preferred profile width
- **AND** sparse items SHALL NOT stretch across the entire viewport

### Requirement: Enlarged file icons use native-resolution Shell pixels

The application SHALL request filesystem icon pixels for the actual DPI-adjusted file-view size instead of enlarging the legacy 16px or 32px `SHGFI_ICON` bitmap. The request and texture caches SHALL distinguish the complete physical size, theme, association generation, and overlay generation.

#### Scenario: Normal zoom uses an equal-or-larger Shell image list

- **WHEN** a filesystem icon is rendered between 16 and 256 physical pixels
- **THEN** the Shell small, large, extra-large, or jumbo image list supplies an equal-or-larger source raster
- **AND** GPUI only contains or downsamples that raster instead of magnifying a 32px bitmap

#### Scenario: Very large overlay-free icons use the image factory

- **WHEN** DPI-adjusted file-view demand exceeds 256 physical pixels and the item has no Shell overlay
- **THEN** `IShellItemImageFactory` is asked for the exact physical size up to 1024px
- **AND** the resulting cache key and owned payload retain that size

#### Scenario: Shell overlays survive the resolution upgrade

- **WHEN** TortoiseGit, OneDrive, or another Shell overlay is present
- **THEN** the visible icon comes from the live high-resolution system image list with the reported overlay mask
- **AND** improving edge quality does not remove or replace the overlay artwork
