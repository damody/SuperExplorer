# Rust + GPUI Windows 11 檔案總管設計規格

日期：2026-07-26  
狀態：已通過互動式設計審查  
目標平台：Windows 11 25H2 x64（本機 Build 26200）

## 1. 目標

建立 Windows-only、Rust-first 的檔案管理器，以本機 Windows 11 25H2 檔案總管作為視覺與行為基準。上層 UI 使用固定 revision 的 GPUI-CE 與專案內 semantic control helpers，Windows Shell 整合使用 `windows` crate 與公開 Win32、COM、OLE、Property System、Search 和 Shell API。

專案採 Milestone 0–9 漸進交付。每個 Milestone 必須可編譯、可執行、可獨立測試與驗收，不建立大量未使用介面或無行為的空模組。

## 2. 已確認的產品範圍

核心計畫必須交付：

- Windows 11 風格自訂視窗、標題列、分頁、命令列、地址列、搜尋列、導航窗格、檔案檢視與狀態列。
- 本機檔案、Windows Shell Namespace 與無 filesystem path 的 Shell Item。
- Details、List、Small／Medium／Large／Extra Large Icon 等檢視模式。
- 大型資料夾虛擬化、增量列舉、非同步 metadata／thumbnail 與取消。
- 穩定多選、框選、鍵盤導航、inline rename、排序、分組與欄位管理。
- 原生開啟、建立、重新命名、複製、移動、回收刪除、永久刪除、衝突處理、進度、取消與可用的 undo／redo。
- Clipboard、OLE drag-and-drop、`IContextMenu3` 與第三方 Shell extension 相容。
- Home、Gallery、最近使用、釘選、快速存取、ZIP、Libraries 與 Shell Namespace Extension。
- 資料夾範本、自訂欄位、分組、Windows Search 與搜尋語法。
- 分頁／工作階段還原、完整快捷鍵矩陣與 Windows accessibility 行為。
- 第三方 Shell extension 與 Preview Handler 的 timeout、掛起與崩潰隔離。

`docs/PARITY_MATRIX.md` 是正式完成條件的一部分。每項能力必須記錄所屬 Milestone、狀態、自動或手動驗收方法、已知差異與 Windows API 限制；未通過的項目不得標記完成。

## 3. 未來展望

下列能力不阻擋 Milestone 0–9 完成，保留在 post-parity roadmap：

- OneDrive Files On-Demand 與其他雲端 provider 的深度整合。
- 進階網路認證、離線快取與企業網路管理。
- 資源回收筒管理細節、Security／Sharing 專用 UI。
- FTP、SFTP 與 ADB providers。

架構會保留 provider 擴充點，但核心 Milestone 不建立這些功能的空 stub。

## 4. 參考來源與依賴政策

### 4.1 UI 參考

- GPUI-CE 提供 window、element、layout、focus、action、key binding、async executor、raw-window-handle 與 platform backend；Explorer controls 由專案內窄版 semantic helpers 組合，不建立通用 widget toolkit。
- 設計基準鎖定本機 Windows 11 25H2，而不是任意網路截圖。
- Shell 圖示、縮圖與檔案型別圖示優先來自公開 Windows API。Lucide 只用於系統沒有對應資源的應用程式命令。

implementation plan 以 `gpui-ce/gpui-ce` submodule commit `6c799b8e994266233014cea66d7769675ec1967c` 作為唯一 GPUI 依賴基線。

Cargo manifest 使用 submodule path並提交 `Cargo.lock`。`gpui-component bc174a7...` 對此 revision 實測有 11 個 API 編譯衝突，因此不納入 dependency graph，也不維護 fresh clone 無法取得的私有 patch。升級只能在明確更新計畫與 revision 後進行，必須通過完整 build、test 與視覺 smoke test。

### 4.2 舊後端參考

`damody/file_explorer` commit `2c7cb15d5c5c83e7a781121f60d788b96fb8ea74` 是選擇性移植來源。可重用候選包含：

- Win32 directory enumeration 與 UTF-16 buffer parsing。
- `IFileOperation`、Shell 開啟與已知資料夾呼叫。
- `ReadDirectoryChangesW` overlapped watcher。
- provider domain 的部分錯誤與傳輸概念。

不直接沿用：

- C ABI／FFI 與 C++／WinUI ViewModel 邊界。
- path string 作為唯一 identity 的資料模型。
- 同步 `FileSystemProvider` 介面。
- 全域 watcher handle registry。
- 每次 Shell 操作個別初始化 COM 的生命週期。

移植順序是先建立測試，再搬移可驗證的低階邏輯，最後接入新的 typed command/event 介面。

## 5. Workspace 架構

```text
crates/
├─ explorer-app/          Windows-only binary、GPUI application、視窗生命週期
├─ explorer-ui/           Explorer chrome、panes、FileView、theme、actions
├─ explorer-model/        FileEntry、Selection、Tab、History、Sort、Group、View settings
├─ explorer-shell-win/    Shell STA、PIDL、Namespace、operations、watcher、interop
├─ explorer-jobs/         純檔案 I/O、metadata、影像解碼、排序與 cache 工作池
├─ explorer-search/       Windows Search、query、fallback、增量結果
├─ explorer-common/       IDs、command/event、errors、cancellation、instrumentation
└─ explorer-test-support/ Fake Shell、測試資料集與 model/UI 測試工具
```

每個 crate 只承擔一項主要責任。Win32、COM、PIDL 與 `windows` crate 型別不得滲透到 UI。跨層只傳 domain command、event、owned descriptor 與純資料值。

### 5.1 Crate 公開邊界

| Crate | 對外提供 | 可依賴 | 禁止事項 |
|---|---|---|---|
| `explorer-common` | IDs、request context、command/event envelope、error、metrics | Rust std 與小型基礎 crate | GPUI、Win32、filesystem 實作 |
| `explorer-model` | tabs、history、directory snapshot、selection、sort/group/view settings | `explorer-common` | COM、阻塞 I/O、視覺繪製 |
| `explorer-shell-win` | `ShellService` command endpoint、Shell descriptors、operation progress | `explorer-common`、`windows` | GPUI entity/window、跨執行緒裸 COM pointer |
| `explorer-jobs` | 有界 CPU/I/O job scheduler、cache maintenance | `explorer-common` | Shell apartment-affine 工作 |
| `explorer-search` | query parser、search session、incremental result stream | common、model、shell abstraction | 直接修改 UI state |
| `explorer-ui` | application views、elements、semantic control helpers、theme、actions | common、model、GPUI-CE | 直接呼叫 Win32 Shell 或同步 I/O |
| `explorer-app` | composition root、process lifecycle、dependency wiring | 所有 production crates | 放置 domain 或 Shell 細節 |
| `explorer-test-support` | fake services、deterministic scheduler、large datasets | common、model、公開 service traits | production code 反向依賴 |

`explorer-app` 建立 service、queue 與 GPUI entities，再將 typed handles 注入 UI。UI 只能提交 command 並消費 event；測試可用 fake service 取代 Shell service，而不改 UI/model 呼叫方式。

### 5.2 依賴方向

```text
explorer-app
  ├─ explorer-ui ────────┐
  ├─ explorer-search ────┼─> explorer-model ─> explorer-common
  ├─ explorer-shell-win ─┤
  └─ explorer-jobs ──────┘

explorer-test-support ──> 各 crate 的公開 trait／model
```

不得建立 `explorer-ui -> explorer-shell-win` 的直接依賴。Shell-specific action 仍透過 `ExplorerCommand` 送至 composition root，以便測試、取消與記錄。

## 6. 執行緒與非同步模型

### 6.1 GPUI 主執行緒

只處理 GPUI state、layout、paint、focus、input、animation 與短時間 model merge。同步目錄列舉、網路探測、thumbnail decode、搜尋、大量 metadata 與可能阻塞的 COM 呼叫不得執行於 UI thread。一般 callback 目標少於 4 ms。

### 6.2 Shell STA

`explorer-shell-win` 建立專用 STA 執行緒，呼叫 `CoInitializeEx(..., COINIT_APARTMENTTHREADED)` 並維持可靠 message pump。Shell enumeration、context menu、Preview Handler、OLE 與 `IFileOperation` 由此邊界協調。

Apartment-affine COM interface 不直接跨執行緒傳遞。跨執行緒使用 cloned absolute PIDL、parsing name、可重建 descriptor、序列化 property、command、result 與 event。需要跨 apartment 的 interface 必須正式 marshaling。

### 6.3 背景工作池

純檔案 I/O、sorting、metadata、thumbnail decode、cache maintenance 與 fallback watcher 在有界工作池執行。工作池暴露 queue depth、outstanding count 與取消狀態，避免無界排隊。

### 6.4 導覽一致性

每次導覽建立 `request_id`、遞增 `generation` 與 cancellation token。Shell STA 增量發送 entry batches；UI 合併前驗證 generation。新導覽會取消舊請求，任何舊 batch、property 或 thumbnail result 都不能污染目前頁面。

導覽流程固定如下：

1. UI 將輸入解析成 `LocationDescriptor`，更新 tab 為 loading，但保留舊畫面直到第一批新資料可顯示。
2. Model 配發 `RequestContext { request_id, tab_id, generation, cancellation }`，立即取消該 tab 的舊 request。
3. Shell STA 解析 location、取得 display identity，先回傳 location metadata，再以有界 batch 列舉 children。
4. UI 每幀合併有限數量 batch；只有 request context 與目前 tab generation 一致時才套用。
5. 首個 viewport 可用後結束 blocking loading state；慢速 properties 與 thumbnails 保持增量更新。
6. 完成、取消或錯誤都發出 terminal event；model 清除 outstanding request，保留可重試資訊。

### 6.5 Queue、backpressure 與優先序

- UI → service command queue 有界；同一 tab 的重複 navigation、sort、search command 可被較新 command 取代。
- Directory batch 以項目數與估算 byte size 雙重限制，避免單批過大卡住 UI。
- 工作優先序依序為：目前 viewport、目前目錄非可見項目、鄰近預取、背景 cache maintenance。
- Thumbnail/property request 以 `(ShellItemId, property-or-size)` 去重；所有 consumer 離開後可取消尚未開始的工作。
- Queue 滿時不阻塞 GPUI thread；低優先工作被丟棄或延後，必要 command 回傳明確 overload error。
- Event channel 關閉視為 service failure，composition root 觸發受控 shutdown 或 service restart，不靜默忽略。

### 6.6 Process 與執行緒生命週期

啟動順序為 logging/panic hook、DPI/COM process prerequisites、Shell STA、job scheduler、GPUI application、首個 window。關閉順序相反：停止接收 command、取消 sessions、卸載 preview/context menu、停止 watcher、排空必要 operation notification、結束 STA message pump、關閉 GPUI。

STA thread 擁有其 COM object；RAII wrapper 明確記錄 allocator 與 release function。`PIDLIST_ABSOLUTE`、`HBITMAP`、`HICON`、`HMENU`、registry notification、event/file handles 都要有單一 ownership 與 leak tests。

## 7. 核心資料模型

### 7.1 ShellItemId

`ShellItemId` 是 stable identity，可包含 owned absolute PIDL、parsing name 與必要的 volume/file identity。一般路徑只是可選 descriptor。模型必須表示 This PC、Recycle Bin、Libraries、ZIP、Network 與第三方 Namespace Extension。

### 7.2 FileEntry

`FileEntry` 保存 identity、parent identity、display name、可選 parsing name／path、kind、attributes 與快速 metadata。Type、properties 與 thumbnail 使用 lazy cache 補齊；初始 viewport 不等待慢速 metadata。

### 7.3 SelectionModel

selected set、focused、anchor、hover、pending rename、drag candidate 與 context target 全部保存 `ShellItemId`。排序、插入、刪除與 watcher diff 後，選取仍指向原項目。

### 7.4 Watcher merge

變更事件先 coalesce，再轉為 stable-ID diff。Rename old/new 盡量配對；buffer overflow、無法配對或通知不完整時重新列舉，再做 diff。更新不能重建整個 UI tree 或任意清除選取。

### 7.5 Location、Tab 與 Directory 狀態

```rust
pub struct TabState {
    pub id: TabId,
    pub location: LocationDescriptor,
    pub history: NavigationHistory,
    pub directory: DirectoryState,
    pub selection: SelectionModel,
    pub view: ViewSettings,
    pub generation: u64,
}

pub enum DirectoryState {
    Idle,
    Loading { request_id: RequestId, retained_items: bool },
    Ready { snapshot: DirectorySnapshot, partial: bool },
    Error { error: ExplorerError, retained_items: bool },
}
```

`NavigationHistoryEntry` 保存 location、display title、scroll anchor、selection IDs、view mode、sort/group 與 column state。Back/forward 只在新 location 成功解析後提交 history；失敗不破壞目前 history。Refresh 不新增 history entry。

`DirectorySnapshot` 使用穩定 entry store 加上可重建的 presentation index。Sort/group 改變 presentation index，不複製大型 `FileEntry`。Watcher diff 先更新 store，再增量修補 index；需要全排序時移至工作池並以 generation 防止舊排序覆蓋新狀態。

### 7.6 Command 與 Event 契約

主要 command 類別：

```rust
pub enum ExplorerCommand {
    Navigate { tab: TabId, target: LocationDescriptor, disposition: OpenDisposition },
    Refresh { tab: TabId },
    LoadProperties { request: RequestContext, items: Vec<ShellItemId>, keys: Vec<PropertyKey> },
    LoadThumbnails { request: RequestContext, items: Vec<ThumbnailRequest> },
    ExecuteFileOperation(FileOperationRequest),
    ShowContextMenu(ContextMenuRequest),
    StartSearch(SearchRequest),
    Cancel { request_id: RequestId },
}
```

主要 event 類別：

```rust
pub enum ExplorerEvent {
    LocationResolved { request: RequestContext, location: ResolvedLocation },
    DirectoryBatch { request: RequestContext, entries: Vec<FileEntry>, is_last: bool },
    PropertiesReady { request: RequestContext, values: Vec<PropertyUpdate> },
    ThumbnailsReady { request: RequestContext, images: Vec<ThumbnailUpdate> },
    DirectoryChanged { subscription: WatchId, changes: Vec<EntryChange> },
    OperationProgress(OperationProgress),
    OperationFinished(OperationOutcome),
    Failed { request: Option<RequestContext>, error: ExplorerError },
}
```

實際欄位可在 implementation plan 中細分，但必須保持三項不變：所有非同步結果可追溯 request、所有 item reference 使用 stable ID、所有 terminal path 都可觀測。

### 7.7 設定與工作階段持久化

設定檔採 versioned schema 與 atomic replace：先寫同目錄 temporary file、flush、rename，再保留最後一份可讀備份。內容分為：

- `AppSettings`：theme、hidden/system visibility、default view、privacy options。
- `WindowSession`：normal bounds、maximized state、active tab、tab ordering。
- `TabSession`：location descriptor、history 截斷、scroll anchor、selection、view settings。
- `FolderViewProfile`：folder identity/template、mode、columns、widths、sort、group。
- `PinnedLocation`：stable descriptor、display override、order、availability。

啟動時逐項驗證 location；無效或離線 tab 顯示可恢復錯誤，不讓整個 session restore 失敗。不同 DPI／monitor 下先將 window bounds clamp 到可見工作區。

### 7.8 Windows Shell API 對照

| 能力 | 主要公開 API | 執行邊界 | 首次交付 |
|---|---|---|---|
| Location parse/resolve | `SHParseDisplayName`、`SHCreateItemFromParsingName`、Known Folder APIs | Shell STA | M2/M7 |
| Child enumeration | `IShellFolder`、`IEnumIDList`、`SHCreateItemWithParent` | Shell STA，batch events | M2/M7 |
| Identity/display/attributes | absolute PIDL helpers、`IShellItem`、`IShellItem2` | Shell STA → owned values | M2/M7 |
| Properties | `IPropertyStore`、`PROPERTYKEY`、`PROPVARIANT` | Shell STA／可取消 session | M2/M8 |
| Icons/thumbnails | `SHGetStockIconInfo`、`IShellItemImageFactory`、`IThumbnailCache` | Shell STA + decode jobs | M4 |
| File operations | `IFileOperation`、progress sink | Shell STA | M5 |
| Change notification | `SHChangeNotifyRegister`；filesystem fallback `ReadDirectoryChangesW` | window/message target + watcher worker | M2/M7 |
| Context menu | `IContextMenu`/2/3、native menu messages | broker/STA | M6/M9 |
| Clipboard/drag-drop | `IDataObject`、`IDropSource`、`IDropTarget`、`DoDragDrop`、Shell formats | OLE STA + UI coordination | M6 |
| Preview | `IPreviewHandler`、initialize-with-file/stream/item | preview broker | M8/M9 |
| Search | Windows Search query helper/store APIs | search service session | M8 |

所有 unsafe block 必須在 wrapper 旁寫明 pointer validity、buffer length、allocator、ownership transfer、thread/apartment requirement 與 return-code handling。公開安全介面不能讓 caller 製造 invalid PIDL、double-free handle 或 apartment 違規。

## 8. UI 與視覺策略

主要區域依序為：自訂 title/tab chrome、command bar、navigation/address/search row、navigation pane、virtualized file view、可選 details/preview pane 與 status bar。

優先重用 GPUI-CE 的 window、layout、element、focus、action、key binding、async executor 與 raw-window-handle primitives。專案自行實作：

- 只服務 Explorer 的 Button、Tooltip、Menu、Input、Scroll、Resizable divider、Status Bar semantic helpers；全部由集中 token、typed action、focus 與 accessibility contract 驅動。
- Explorer title/tab chrome 與精確的 breadcrumb 行為。
- Navigation tree 的 lazy expand、drag target 與狀態保存。
- Details／Icon FileView 虛擬化核心。
- Marquee、inline rename、drag indicators 與 Explorer 鍵盤空間導航。

FileView 不為每個檔案建立重量級 persistent view。Details mode 使用 row virtualization；Icon modes 使用依 viewport 與 column count 計算的 2D virtualization。需要獨立狀態的 input、popup 與 menu 才建立子 view。

視覺驗收固定同一台電腦、視窗尺寸、DPI 與 theme，比較 Explorer 與本專案的區域高度、間距、字級、色彩、focus、hover、pressed、disabled 與 selected 狀態。

### 8.1 Component tree

```text
ExplorerWindow
├─ WindowChrome
│  ├─ TabStrip ─ Tab* ─ NewTabButton
│  └─ CaptionButtons
├─ CommandBar
├─ NavigationBar
│  ├─ HistoryButtons
│  ├─ BreadcrumbAddressEditor
│  └─ SearchBox
├─ ContentSplit
│  ├─ NavigationPane
│  ├─ FileViewHost
│  │  ├─ DetailsView | IconGridView
│  │  ├─ InlineRenameEditor
│  │  └─ MarqueeOverlay
│  └─ DetailsPane | PreviewPane
├─ OperationCenter
└─ StatusBar
```

`ExplorerWindow` 協調區域，不包含 Shell 業務邏輯。`FileViewHost` 將同一份 selection 與 directory model 投影到不同 view mode。Popup、menu、rename editor 與 preview focus 進出時由 focus coordinator 保存/還原原始焦點。

### 8.2 Theme 與 layout tokens

Theme token 至少包含 surface、subtle surface、control fill、hover、pressed、selected active/inactive、divider、text primary/secondary/disabled、focus stroke、danger、accent，以及對應 high-contrast semantic token。禁止在 feature component 散落固定 RGB。

Layout token 至少包含 title/tab height、command/address/status height、navigation pane min/default/max width、details/preview pane width、row heights、icon cell sizes、corner radius、control padding、focus stroke 與 animation duration。Token 以 logical pixels 定義，經 GPUI/Windows DPI scale 轉換；命中區可以大於可見 glyph。

M1 先以本機 Explorer 實測值建立 baseline。後續 visual regression 允許明確 tolerance，不以逐像素相同掩蓋字型 rasterization 差異。

### 8.3 Details virtualization

- Presentation index 將 visible row 映射到 `ShellItemId`；row element 只存在於 viewport 加 overscan。
- Column layout 分離 pinned name column、可捲動 columns 與 header hit testing。
- Resize drag 在 UI thread 只更新 lightweight width state；持久化 debounce 到互動結束後。
- Slow property 顯示空值或 loading glyph；property 回來只 invalidate 受影響 row/cell。
- Range selection 依當前 presentation order 計算，但 selected set 保存 IDs。
- PageUp/PageDown 以 viewport height 與 row geometry 移動 focused item，並保證 focus row 可見。

### 8.4 Icon grid virtualization

```text
columns       = max(1, floor(content_width / cell_width))
first_row     = floor(scroll_y / cell_height)
last_row      = ceil((scroll_y + viewport_height) / cell_height)
visible_start = first_row * columns
visible_end   = min(item_count, (last_row + overscan_rows) * columns)
```

Resize 改變 column count 時，以最靠近 viewport top-left 的 stable item 作 scroll anchor，避免畫面跳動。Marquee intersection 使用虛擬 cell geometry，不要求 cell element 已建立。方向鍵依 2D index 移動；跨行與最後一行不完整時選擇最近有效 cell。

### 8.5 Pointer、selection 與 rename 規則

- 左鍵 item：無 modifier 單選；Ctrl toggle；Shift 從 anchor 建 range；Ctrl+Shift 合併 range。
- 點擊空白：無 modifier 清除；開始拖曳後進入 marquee 並使用 pointer capture。
- 右鍵已選項目保留多選；右鍵未選項目先單選它；右鍵空白開背景 menu。
- Drag 只有超過 system drag threshold 才開始；拖曳期間 selection 不被 hover 改寫。
- F2 或慢速第二次點擊進入 rename；Enter commit、Esc cancel、失焦依明確規則 commit，collision/error 時保留 editor 與文字。
- Type-to-select 使用 timeout 累積 Unicode grapheme，依 Explorer natural/case-insensitive 規則循環匹配。

### 8.6 Accessibility 與輸入

每個互動元件提供 role、name、state、value、focus、selection 與 invoke/toggle/expand 等 action。虛擬化 item 必須能在 accessibility request 時 materialize 語意節點並 scroll into view，不因未繪製而消失。

鍵盤 action 先映射成 domain action，再由 focused surface 處理；不在各 view 重複比對 raw key。IME composition 只由真正的 text input 接收。High Contrast 使用系統 semantic colors，關閉僅靠透明度表達狀態的效果；reduced motion 時縮短或停用非必要動畫。

## 9. 錯誤與隔離

錯誤型別保存 operation、location、Win32 error／HRESULT、可恢復性與安全的使用者訊息。可恢復問題顯示 inline error 或 toast；需要決策或進度的操作使用 dialog／progress surface；程式錯誤記錄 diagnostics，不以 `unwrap` 取代處理。

第三方 Shell extension 與 Preview Handler 最終由 broker process 隔離。主 UI process 設定 timeout、取消／卸載協定、crash detection 與資源回收。初期 in-process 功能只能在明確 Milestone 中出現，並記錄未隔離風險；M9 完成時不得留下會無限期卡住 UI 的直接呼叫路徑。

### 9.1 錯誤分類

| 類別 | 例子 | UI 行為 | Retry |
|---|---|---|---|
| Input | 無效地址、rename 字元不合法 | input inline validation | 修正後 |
| Availability | drive 拔除、network offline、placeholder unavailable | 保留畫面、banner/inline error | 可手動或自動退避 |
| Authorization | access denied、elevation required | 解釋目標與操作，提供安全選項 | 視 API 支援 |
| Conflict | name collision、destination changed | operation dialog 顯示逐項決策 | 使用者決策後 |
| Cancellation | navigation/search/operation cancel | 非錯誤 terminal state | 不自動 |
| Extension | context/preview timeout、broker crash | 卸載/重啟 broker，停用該 handler 本次呼叫 | 有限次 |
| Internal | invariant/channel/process failure | diagnostics、可恢復 UI 或受控退出 | 視錯誤 |

任何 user-visible error 都保留可複製的 technical detail，但預設顯示可採取的下一步。Log 不記錄檔案內容、credential 或不必要的完整敏感路徑；diagnostic export 需由使用者主動觸發。

### 9.2 Extension broker

Broker 是與 UI process 分離的 Windows executable，依用途建立短生命週期 session：context menu session 或 preview session。IPC message 使用 version、correlation ID、deadline 與 bounded payload；大型 preview data 優先使用 stream/handle 協定，不複製無界 buffer。

主程序負責：建立 broker、傳入最小必要 item descriptor/host window contract、轉發允許的 input/window messages、監控 heartbeat/deadline、在 crash/hang 後終止 session 並清理 UI host。Broker 負責 COM apartment、extension activation、message dispatch 與卸載。

Context menu 的 native owner-draw 與 `WM_INITMENUPOPUP`、`WM_DRAWITEM`、`WM_MEASUREITEM`、`WM_MENUCHAR` 轉發要有 integration fixture。Preview Handler 必須處理 initialize-with-file/stream/item、resize、focus forwarding、accelerators 與 unload。若 Windows API 無法安全跨 process 呈現特定 extension，parity matrix 記錄限制並採 fallback，而不是讓主 UI 無限阻塞。

## 10. 指定 parity 行為規格

### 10.1 Home、Gallery、Recent、Pin 與 Quick Access

- Home 將 pinned locations、frequent folders 與 recent files 分成可獨立載入/失敗的 section；某來源失敗不讓整頁不可用。
- Gallery 只在能透過公開 Windows capability 合理重現時啟用，內容依影像時間語意分組；不可用時 parity matrix 記錄原因並隱藏入口，不放假資料。
- Recent/Frequent 尊重 Windows 與應用程式 privacy setting，支援清除歷史與停止收集。刪除 recent entry 不刪除原檔。
- Pin/unpin 保存 stable descriptor 與顯示順序；不存在或離線目標保持可辨識 unavailable 狀態，使用者可取消釘選。
- Quick Access navigation、drag reorder、context menu 與 keyboard focus 必須與一般 navigation tree 使用同一 selection/focus 規則。

### 10.2 ZIP、Libraries 與 Shell Namespace Extension

- 入口、children enumeration、display name、icon、open、breadcrumb 與 context menu 都以 Shell Item abstraction 實作。
- ZIP 與其他 namespace item 不假設 `PathBuf`、可 seek stream 或一般 filesystem metadata；unsupported operation 由 capability query 決定是否顯示/啟用。
- Libraries 可列出 library locations 並以 library identity 導覽；跨 location 的 sort/group 行為記錄於 parity tests。
- 第三方 namespace extension 的慢速列舉受 request deadline、取消與 UI incremental rendering 控制；無法強制中止的 COM call 不佔用 UI thread。

### 10.3 Folder template、columns 與 group

- Folder template 至少區分 General、Documents、Pictures、Music、Videos，決定預設 view mode、columns、group 與 thumbnail priority。
- 使用者 override 優先於 template default，並依 stable folder identity 保存；Reset 恢復 template default。
- Column definition 包含 property key、label、format、alignment、width constraints、availability 與 sort capability。
- Group header 是 presentation model 的一部分，支援 collapse、keyboard navigation、item count 與 stable scroll；group key 的慢速 metadata 未載入時放入明確 pending group。

### 10.4 Search syntax

- Query parser 將純文字、quoted phrase、property filter、comparison、date/size shorthand 與 boolean operator 轉成 AST，輸入錯誤提供位置與修正訊息。
- Windows Search backend 只接受經過 escape/bind 的 AST，不拼接未驗證 query string。
- Search session 具有 generation、cancel、incremental results、dedupe 與 source status；fallback search 明確標示能力差異。
- Address navigation 與 search input 是不同 parser；兩者不以模糊 heuristic 靜默互換。

### 10.5 Session restore、operation undo、shortcuts 與 accessibility

- 正常關閉保存 window/tabs；crash recovery 使用最近一次完整 atomic snapshot，不載入半寫檔案。
- Restore 保持 tab order、active tab、location、history、view、scroll anchor 與可重建 selection；不可用位置以 per-tab error 顯示。
- Operation journal 只記錄已完成且有安全 inverse 的 operation；每筆保存 affected identity、原/新 parent/name、timestamp 與 capability。外部變更造成 inverse 不安全時停用並說明。
- `PARITY_MATRIX` 維護完整 action/shortcut 表，涵蓋 global、tab、navigation、file view、rename、search、menu 與 accessibility focus 情境，以及衝突優先序。
- Accessibility 驗收至少包含 keyboard-only、Narrator、high contrast、200% DPI、IME 與 reduced motion；自動語意樹測試不能取代實機 Narrator 流程。

## 11. Milestone

### M0 — Bootstrap 與 parity audit

建立 Cargo workspace、固定依賴、Windows-only binary、logging、panic hook、theme tokens、CI、空視窗與 `docs/PARITY_MATRIX.md`。驗收為可啟動、resize、關閉，且四個 Cargo gate 通過。

### M1 — Static Explorer Shell UI

交付 title/tab chrome、單一 tab、command bar、back/forward/up、breadcrumb/search placeholder、navigation pane、file view placeholder、status bar、light/dark theme、基本 actions 與 key bindings。以本機 Explorer 固定尺寸截圖驗收階層與 DPI。

### M2 — 本機資料夾最小垂直切片

交付非同步本機列舉、檔案 Shell open、資料夾導覽、back/forward/up、generation/cancellation 與 watcher。驗收從啟動到瀏覽與開啟檔案完整可用，UI thread 無同步大目錄掃描。

### M3 — Details、選取與資料夾檢視設定

交付 row virtualization、columns、natural sort、完整 selection、keyboard、marquee、inline rename、資料夾範本、自訂欄位、分組與 identity-based view setting persistence。以 100,000 項目、sort/watch update 後選取穩定為主要驗收。

### M4 — Icon Views 與 thumbnails

交付 small／medium／large／extra-large、2D virtualization、system icons、async thumbnails、LRU cache 與 Ctrl+wheel zoom。不可見項目不大量解碼；切換 view 保留選取與捲動語意。

### M5 — 原生檔案操作與復原

交付 `IFileOperation` create／rename／copy／move／recycle／permanent delete、progress、cancel、collision、partial failure 與能力受限時可理解的 undo／redo。操作 journal 不承諾 Windows API 無法安全復原的動作。

### M6 — Clipboard、drag-and-drop 與 context menu

交付 Explorer 雙向 Clipboard、OLE drag source/target、right-drag、auto-scroll、`IContextMenu3`、background/multi-select menu 與 extension broker 的最小可行邊界。

### M7 — Namespace、Home 與快速存取

交付 Home、Gallery、Recent、pin/unpin、Quick Access、This PC、drives、Network root、Recycle Bin 基本瀏覽、ZIP、Libraries 與第三方 Shell Namespace Extension。所有導覽使用 Shell Item，不假設 filesystem path。進階網路認證／離線與完整 Recycle Bin 管理 UI 仍屬未來展望。

### M8 — Search、properties 與 preview

交付 Windows Search、可取消增量搜尋、Explorer-compatible search syntax、lazy properties、metadata columns 與 Preview Handler host。Preview 不阻塞 UI，搜尋舊結果不污染目前 query。

### M9 — Parity、restore、accessibility 與隔離

交付 tab/session restore、完整快捷鍵矩陣、high contrast、IME、screen reader、crash recovery、第三方 extension／preview broker timeout 與 crash isolation、performance instrumentation、leak soak test 與 visual regression。完成時 `PARITY_MATRIX` 的核心範圍不得有未分類項目。

### 11.1 Milestone exit matrix

| Milestone | 必須可示範的 end-to-end flow | 特定測試／量測 | 必須更新的文件 |
|---|---|---|---|
| M0 | 啟動空視窗、resize、關閉、panic 產生日誌 | 四個 Cargo gates、啟停 handle snapshot | implementation plan、status、manual tests、parity matrix |
| M1 | 以滑鼠/鍵盤切換主要 chrome focus、theme、視窗狀態 | layout token、action routing、固定尺寸 visual baseline | status、manual/visual steps、parity UI section |
| M2 | 地址輸入 → 增量列舉 → 進資料夾 → back → 開檔 → watcher update | cancellation/stale generation、permission error、first-item latency | shell API decisions、manual local/UNC smoke results |
| M3 | 100k items 滾動、sort/group、Ctrl/Shift/marquee、rename | selection invariants、grid/row math、frame/visible element count | view behavior matrix、known sort/group differences |
| M4 | view modes/zoom、async icons/thumbnails、快速捲動 | cache hit/eviction、dedupe/cancel、GDI handle stability | thumbnail sources與fallback matrix |
| M5 | create/copy/move/delete/cancel/conflict/undo | progress sink、partial failure、journal invalidation | destructive operation manual matrix |
| M6 | 與 Explorer copy/cut/paste/drag、multi-select/background menu | clipboard formats、drop effects、owner-draw fixture | interoperability matrix、extension risks |
| M7 | Home pin/recent、Network/Recycle、ZIP、Library、第三方 namespace 導覽 | non-path identity、capability query、slow extension cancellation | namespace capability matrix |
| M8 | advanced search/cancel、lazy properties、preview focus/resize | parser AST、stale search、preview timeout/unload | search syntax與preview handler matrix |
| M9 | crash restore、Narrator/IME/high contrast、broker hang/crash recovery | soak/leak、visual regression、fault injection、shortcut coverage | parity closure、remaining API limitations |

每一列的文件更新與測試結果屬於 exit criteria，不是後補工作。若環境不能自動執行 Windows-only case，必須在同一 Milestone 記錄明確手動步驟、實際結果與未驗證原因。

## 12. 測試策略

每個 Milestone 執行：

```powershell
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Unit tests 覆蓋 natural sort、stable/range selection、marquee、navigation history、generation rejection、cancellation、column resize、grid index、diff、rename validation、address/search parsing、cache eviction、folder-template resolution 與 operation journal。

Integration tests 使用暫存資料集驗證小型／100,000 項目資料夾、Unicode、emoji、長名稱、hidden/system、reparse point、permission denied、rapid changes、rename storm、watcher overflow 與 Shell namespace fake。

Windows 實機手動矩陣包含 100/125/150/200% DPI、多螢幕、light/dark/high contrast、注音／拼音／倉頡 IME、RDP、screen reader、ZIP、Libraries、第三方 context menu、Preview Handler、Explorer 雙向拖放、睡眠恢復與 extension broker crash/hang 注入。

### 12.1 測試分層與 seams

- Pure model tests 使用 deterministic scheduler 與 fixed IDs，不需要 GPUI 或 Windows Shell。
- Shell wrapper tests 將 unsafe FFI 限制在窄 adapter；buffer parser、flag mapping、error mapping 與 ownership wrapper 可獨立測。
- Contract tests 對 fake Shell service 與 real Shell service 跑相同 navigation/operation cases，驗證 command/event terminal semantics。
- GPUI behavior tests 驗證 action routing、focus、selection、geometry 與 virtual range；視覺層保持薄，避免只能靠 screenshot 找 model bug。
- Broker tests 使用可控制的 fake extension，模擬正常、慢速、reentrant message、hang、crash、oversized reply 與 unload failure。

### 12.2 Visual regression

Baseline key 由 OS build、Explorer version、app commit、DPI、theme、window size 與 font configuration 組成。比較前等待 animation 與 async content 進入指定 stable marker；對文字 anti-aliasing、thumbnail 動態內容使用 mask/tolerance，對 layout bounds、colors 與 control states使用較嚴格門檻。

Visual failure 必須同時輸出 baseline、actual、diff 與 token/layout diagnostics。更新 baseline 是明確 review 動作，不能由測試自動覆寫。

### 12.3 Fault injection 與 soak

可注入 directory enumeration delay/error、out-of-order batches、watcher overflow、thumbnail decode failure、operation partial failure、service channel close、broker hang/crash 與 settings corruption。Soak scenario 至少重複 navigation、tab open/close、view switching、thumbnail scrolling 與 preview loading，觀察 memory、GDI/User handles、threads、queues 與 outstanding requests 是否持續成長。

## 13. 效能與可觀測性

保留原提示的效能目標：冷啟動 800 ms、暖啟動 400 ms、本機資料夾首批 150 ms、100,000 項目 1 秒內開始互動、滾動目標 60 FPS、一般 UI callback 盡量小於 4 ms。

至少量測 enumeration latency、first item、first viewport、thumbnail hit rate、visible elements、frame/UI task duration、watcher/background queue depth、outstanding requests、GDI/User handles 與 cache size。效能數字是量測目標，不凌駕正確性。

### 13.1 Cache 分層

- Entry cache：以 parent identity + child identity 保存本次 session 的快速 metadata，watcher change 精確失效。
- Property cache：key 為 `(ShellItemId, PropertyKey, source version)`，值含 loaded/missing/error 與 freshness。
- Thumbnail memory cache：key 含 item identity、requested physical size、scale、thumbnail/icon mode 與 source version；以 decoded byte cost 做 LRU，而非只計 item 數。
- Thumbnail disk cache：M4 量測確認有收益後才啟用；採版本目錄與 content metadata，corruption 可丟棄重建。
- Negative cache：短期保存 unsupported property、無 thumbnail 與 transient failure；永久與暫時錯誤使用不同 TTL。

Cache budget 由設定集中管理，預設值在 M4 以 100k dataset、常見 RAM 等級與 handle telemetry 實測後寫入，不散落 magic number。Memory pressure event 會先清低優先 thumbnail，再清非目前 directory properties。

### 13.2 Tracing 與量測方法

每個 request 以 correlation fields 串起 UI action、command enqueue、Shell start、first batch、first viewport、terminal event。Release benchmark 關閉 debugger，在相同資料集上至少收集多次樣本並報告 median/p95；冷啟動與暖啟動分開量測。CI 只阻擋 deterministic regression，本機硬體相關目標由 benchmark report 驗收。

## 14. 首輪實作邊界

第一個 implementation plan 只處理 M0 + M1 的最小垂直切片。Shell crate 只建立真實、可編譯的生命週期邊界，不預先建立 M2–M9 的空 API。首輪交付必須包含：

- Cargo workspace 與可執行 Windows GPUI app。
- 固定 GPUI-CE submodule revision 與 `Cargo.lock`。
- 可 resize 的 Win11 25H2 shell UI、theme、actions/key bindings。
- `docs/IMPLEMENTATION_PLAN.md`、`docs/STATUS.md`、`docs/MANUAL_TESTS.md`、`docs/PARITY_MATRIX.md`。
- 四個 Cargo gate 的真實結果與本機 screenshot 驗收步驟。

## 15. 主要風險與控制

| 風險 | 影響 | 控制方式 |
|---|---|---|
| GPUI-CE API 快速變動 | build break、重工 | 固定 submodule commit/Cargo.lock；獨立 upgrade change |
| GPUI Windows chrome 與原生 HWND message 整合不足 | Snap、caption、context/preview host 差異 | M0/M1 先做 capability spike 與 HWND integration test，不延後到 M6 |
| 第三方 COM extension 無 cancellation | thread/process 掛起 | 不在 UI thread 呼叫；M6 broker 起點、M9 完整隔離與 fault test |
| Shell identity/serialization 不完整 | restore、selection、namespace 錯誤 | ShellItemId contract tests；無法持久化時保存可重建 descriptor 與明確失效狀態 |
| 100k items metadata/thumbnail fan-out | queue、RAM、frame time 惡化 | viewport priority、dedupe、bounded queues、cost-based cache、telemetry |
| Explorer 行為隨 Windows build 改變 | baseline 漂移 | parity baseline 包含 OS/Explorer version；升級後重新 audit，不靜默改預期 |
| Accessibility 事後補做成本過高 | component 重寫 | M1 起建立 semantics/action/focus contract，M9 做完整 closure |
| 舊後端 unsafe/ownership 假設 | leak、UB、COM 錯用 | 只移植窄邏輯、先加測試、RAII ownership、Miri 可測純邏輯與 Windows soak |

## 16. 完成定義

單一 Milestone 只有在程式可執行、指定自動測試通過、Windows-only 未自動化項目有手動結果、文件與 parity matrix 更新、已知差異明列時才算完成。

核心專案只有在 M0–M9 驗收完成、指定的五組擴充能力通過 parity matrix、沒有未分類的核心差異，且任何無法匹配 Explorer 的行為都有公開 API 限制與替代行為說明時才算完成。
