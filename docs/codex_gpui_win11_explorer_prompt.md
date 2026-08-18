# Codex 主提示詞：使用 Rust + GPUI 重製 Windows 11 檔案總管

## 你的角色

你是一名資深 Windows Shell、Rust、GPU UI 與桌面應用架構工程師。

請在目前 Repository 中，從零開始實作一個 **Windows 11 專用檔案管理器**。目標不是做一個簡化版檔案瀏覽器，而是逐步達成與 Windows 11 檔案總管高度一致的：

- 功能
- 操作邏輯
- 鍵盤與滑鼠行為
- Shell 相容性
- 視覺層級
- 效能
- 穩定性

上層 UI 必須使用 **GPUI**，主要程式語言必須使用 **Rust**。

不可使用 Electron、Tauri、WebView、C#、WinUI、WPF、Avalonia、Qt 或 Slint 作為主要 UI。

允許直接使用 `windows` crate 呼叫 Win32、COM、WinRT、Windows Shell 與 OLE API。

---

# 一、核心目標

建立一個 Windows-only 的 Explorer 替代程式，具備：

1. Windows 11 風格的自訂視窗與標題列。
2. 多分頁檔案瀏覽。
3. 完整地址列、Breadcrumb、搜尋列與導覽歷史。
4. 左側 Navigation Pane。
5. 中央檔案檢視區。
6. 詳細資料、清單、小圖示、中圖示、大圖示、超大圖示等檢視模式。
7. 與 Windows Shell 相容的檔案、資料夾與虛擬項目模型。
8. 原生右鍵選單與 Shell Extension。
9. 原生複製、移動、刪除、重新命名、建立資料夾與復原操作。
10. 系統圖示、縮圖、檔案屬性與預覽。
11. 拖放、剪貼簿、多選、框選與完整鍵盤操作。
12. 大型資料夾虛擬化與非同步載入。
13. 快速存取、已知資料夾、磁碟機、網路位置、資源回收筒與雲端 Placeholder。
14. 與 Windows 11 檔案總管接近的錯誤處理、進度顯示與操作體驗。

「一模一樣」是指公開可觀察的功能與操作行為高度一致，不得反編譯、複製或散布 Microsoft 私有程式碼、二進位資源或受限制素材。

圖示應優先由 Windows Shell、系統字型、Stock Icon 或公開 API 取得，不要從 Explorer.exe 抽取或直接複製資源。

---

# 二、不可妥協的技術約束

## 2.1 UI

必須使用 GPUI：

- GPUI 負責視窗、Layout、繪製、輸入、焦點、動畫與狀態更新。
- 不依賴 GPUI 內建完整控制項。
- 可以自行實作所有控制項。
- 把 GPUI 視為 GPU UI 引擎，而不是完整桌面控制項庫。
- 優先使用官方 Zed Repository 中的 GPUI。
- 將 GPUI 版本或 Git commit 固定，避免未預期 breaking change。
- 不使用來源不明的非官方 GPUI 發行版，除非 Repository 已經明確採用且有理由。

## 2.2 Windows 整合

必須使用 `windows` crate：

- Win32
- COM
- Windows Shell
- OLE Drag-and-Drop
- Clipboard
- DirectWrite 或 GPUI 現有文字系統
- Windows Accessibility
- DPI 與視窗訊息

不得只用 `std::fs` 或 `tokio::fs` 模擬完整 Windows Shell。

一般本機檔案 I/O 可以用 Rust 標準庫，但 Shell Namespace、右鍵選單、資源回收筒、Known Folder、雲端項目與虛擬項目必須以 Shell Item 為中心。

## 2.3 支援範圍

第一版：

- Windows 11 x64
- Rust stable
- MSVC toolchain
- Unicode
- 高 DPI
- 多螢幕
- 深色與淺色模式

暫時不要求 Linux、macOS、ARM64，但架構不得故意阻止未來加入 ARM64 Windows。

---

# 三、開工規則

開始修改程式碼前，先完成：

1. 檢查 Repository 現況。
2. 列出現有 crate、依賴、編譯方式與可重用程式碼。
3. 建立或更新 `docs/IMPLEMENTATION_PLAN.md`。
4. 建立或更新 `docs/STATUS.md`。
5. 把任務拆成可獨立編譯、測試與驗收的 Milestone。
6. 先完成最小垂直切片，再擴大功能。
7. 不要一次建立大量未使用介面、空模組或假實作。
8. 每個 Milestone 完成後執行：

````bash
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
````

若某些 Windows-only 測試無法在目前環境執行，應：

- 保留可編譯測試。
- 把手動測試步驟寫入 `docs/MANUAL_TESTS.md`。
- 明確記錄未驗證項目。
- 不得聲稱未執行的測試已通過。

每次實作前先閱讀相關既有程式碼，不要猜測 API。

---

# 四、建議 Workspace 架構

可以依現有 Repository 調整，但職責必須清楚：

````text
crates/
├─ explorer-app/
│  ├─ 啟動
│  ├─ GPUI Application
│  ├─ 視窗生命週期
│  └─ 全域命令
│
├─ explorer-ui/
│  ├─ MainWindow
│  ├─ TitleBar
│  ├─ TabStrip
│  ├─ CommandBar
│  ├─ AddressBar
│  ├─ NavigationPane
│  ├─ FileView
│  ├─ DetailsPane
│  ├─ PreviewPane
│  ├─ StatusBar
│  ├─ ContextPopup
│  └─ Theme
│
├─ explorer-model/
│  ├─ DirectoryModel
│  ├─ FileEntry
│  ├─ SelectionModel
│  ├─ SortModel
│  ├─ GroupModel
│  ├─ NavigationHistory
│  ├─ TabModel
│  └─ ViewSettings
│
├─ explorer-shell-win/
│  ├─ COM 初始化
│  ├─ ShellItem
│  ├─ PIDL
│  ├─ ShellFolder
│  ├─ KnownFolder
│  ├─ Properties
│  ├─ Thumbnail
│  ├─ ContextMenu
│  ├─ FileOperation
│  ├─ ChangeNotification
│  ├─ DragDrop
│  ├─ Clipboard
│  └─ PreviewHandler
│
├─ explorer-search/
│  ├─ 搜尋查詢
│  ├─ Windows Search 整合
│  ├─ fallback 搜尋
│  └─ 取消與增量結果
│
├─ explorer-common/
│  ├─ ID 型別
│  ├─ command/event
│  ├─ error
│  ├─ cancellation
│  └─ logging
│
└─ explorer-test-support/
   ├─ fake shell provider
   ├─ 測試資料夾產生器
   └─ UI 行為測試工具
````

不要讓 Win32、COM、PIDL 或 `windows` crate 型別滲透到所有 UI 模組。

UI 與 Shell 之間透過清楚的 domain model、command 與 event 溝通。

---

# 五、執行緒與非同步模型

至少分成三類執行上下文。

## 5.1 GPUI 主執行緒

只負責：

- GPUI state
- Layout
- Paint
- 輸入事件
- Focus
- Animation
- 短時間 model 更新

禁止在 GPUI 主執行緒執行：

- 大型目錄同步列舉
- 縮圖解碼
- 網路路徑探測
- Hash
- 搜尋
- 大量 metadata 讀取
- 阻塞式 COM 呼叫
- 長時間檔案操作

單次 UI callback 應盡量少於 4 ms，任何可能阻塞的工作都必須移出 UI thread。

## 5.2 Shell STA Thread

建立專用 STA 執行緒：

- `CoInitializeEx(..., COINIT_APARTMENTTHREADED)`
- 執行 Shell COM 物件操作
- 處理需要 message pump 的 COM/OLE 行為
- 管理 Shell context menu
- 管理 Preview Handler
- 管理部分 Drag-and-Drop
- 管理 `IFileOperation`

必須有可靠的 message pump。

不可任意把 apartment-affine COM object 直接送到其他執行緒。

跨執行緒傳遞：

- 自己定義的穩定 ID
- cloned absolute PIDL
- parsing name
- 可重新建立 Shell Item 的 descriptor
- 序列化的 property value
- command/result/event

如必須跨 Apartment 傳遞 COM interface，使用正式 COM marshaling，不得用裸指標硬傳。

## 5.3 背景工作池

負責：

- 純檔案 I/O
- 搜尋
- Hash
- 資料排序
- metadata
- 影像解碼
- 快取整理
- fallback watcher
- 長時間工作

每次導覽都要有：

- `request_id`
- `generation`
- cancellation token

舊導覽或舊搜尋結果不得覆蓋目前頁面。

---

# 六、核心資料模型

## 6.1 ShellItemId

不得把路徑字串當成唯一 ID。

建立穩定的 `ShellItemId`，可包含：

- Absolute PIDL 的 owned representation
- Shell parsing name
- Volume/file ID
- 其他可重建 Shell Item 的資訊

必須支援：

- 沒有一般檔案系統路徑的 Shell 項目
- 資源回收筒
- This PC
- Network
- Known Folders
- Libraries
- ZIP namespace
- 雲端 Placeholder
- 第三方 Shell Namespace Extension

## 6.2 FileEntry

建議包含：

````rust
pub struct FileEntry {
    pub id: ShellItemId,
    pub parent_id: ShellItemId,
    pub display_name: Arc<str>,
    pub parsing_name: Option<Arc<str>>,
    pub file_system_path: Option<PathBuf>,
    pub kind: EntryKind,
    pub attributes: EntryAttributes,
    pub size: Option<u64>,
    pub modified_at: Option<SystemTime>,
    pub created_at: Option<SystemTime>,
    pub accessed_at: Option<SystemTime>,
    pub file_type_text: Arc<str>,
    pub extension: Option<Arc<str>>,
    pub thumbnail_key: ThumbnailKey,
    pub property_cache: PropertyCache,
}
````

這只是參考，可依實際 API 調整。

## 6.3 SelectionModel

選取必須以穩定 ID 保存，不能以目前 index 保存。

至少支援：

- selected set
- focused item
- anchor item
- hover item
- marquee rectangle
- pending rename item
- drag candidate
- context menu target

排序、插入、刪除或 watcher 更新後，選取不得跳到其他檔案。

---

# 七、GPUI UI 架構

## 7.1 主要區域

主視窗由以下區域組成：

````text
┌────────────────────────────────────────────────────┐
│ 自訂標題列、分頁列、視窗按鈕                        │
├────────────────────────────────────────────────────┤
│ Command Bar                                        │
├────────────────────────────────────────────────────┤
│ Back / Forward / Up | Breadcrumb Address | Search  │
├───────────────┬───────────────────────────┬────────┤
│ Navigation    │ File View                 │ Details│
│ Pane          │                           │ Preview│
├───────────────┴───────────────────────────┴────────┤
│ Status Bar                                         │
└────────────────────────────────────────────────────┘
````

## 7.2 自訂控制項

預期自行實作：

- Button
- IconButton
- ToggleButton
- Tooltip
- Popup
- Menu
- ContextMenu host
- TextInput
- SearchBox
- Breadcrumb
- Tab
- TabStrip
- Tree row
- Scrollbar
- ColumnHeader
- Splitter
- Progress UI
- Toast/inline error
- File row
- Icon cell
- Marquee selection
- Inline rename editor

所有控制項必須支援：

- hover
- pressed
- disabled
- keyboard focus
- focus-visible
- pointer capture
- DPI scaling
- light/dark/high contrast
- keyboard navigation
- screen reader 可識別資訊

不要把每個檔案都做成重量級獨立 View。

---

# 八、FileView 必須自行虛擬化

FileView 是專案最重要的 UI 元件。

## 8.1 共用能力

必須支援：

- 數十萬項目
- 增量載入
- 非同步縮圖
- 滾動位置保存
- 多選
- Shift range selection
- Ctrl toggle
- Ctrl+A
- 空白區取消選取
- 右鍵保留或切換選取
- 滑鼠框選
- 自動捲動
- F2 inline rename
- Enter 開啟
- Alt+Enter 屬性
- Delete / Shift+Delete
- Ctrl+C / Ctrl+X / Ctrl+V
- Ctrl+Z / Ctrl+Y
- Home / End
- PageUp / PageDown
- 方向鍵空間導航
- type-to-select
- 拖放
- 排序
- 分組
- 對齊格線
- 焦點框
- hover
- context menu

## 8.2 Details View

詳細資料模式：

- 固定或可調整 row height。
- 可變欄寬。
- 欄位拖曳調整。
- 欄位排序。
- 欄位顯示/隱藏。
- 可水平捲動。
- 第一欄支援圖示與 inline rename。
- header 點擊切換正序/倒序。
- 使用自然排序。
- 資料夾與檔案排序行為盡量接近 Explorer。
- 不因 metadata 慢而阻塞初始顯示。

可以使用 GPUI 的 list/uniform list，但若無法精確達成行為，應自己實作低階 `Element`。

## 8.3 Icon View

小、中、大、超大圖示模式：

- 2D 虛擬化。
- 只建立或繪製 viewport 附近項目。
- 根據可用寬度動態計算 column count。
- 滾動時保持穩定。
- 文字最多顯示對應行數。
- 選取區域與文字區域行為接近 Explorer。
- 支援橫跨多列的框選。
- 避免十萬個元素進入 UI tree。

建議核心公式：

````text
first_visible_row = floor(scroll_y / cell_height)
last_visible_row  = ceil((scroll_y + viewport_height) / cell_height)
first_index       = first_visible_row * column_count
last_index        = min(item_count, (last_visible_row + overscan) * column_count)
````

## 8.4 繪製策略

優先批次繪製：

1. 背景
2. hover/selection
3. icon/thumbnail
4. primary text
5. secondary metadata
6. focus rectangle
7. drag indicators
8. marquee

只有 TextInput、Popup、Menu 等真正需要獨立狀態的元件才建立子 View。

---

# 九、Windows Shell 整合

優先使用公開 Windows API。

## 9.1 Shell Namespace 與列舉

研究並適當封裝：

- `IShellItem`
- `IShellItem2`
- `IShellFolder`
- `IEnumIDList`
- `SHCreateItemFromParsingName`
- `SHCreateItemWithParent`
- `SHGetKnownFolderItem`
- `SHParseDisplayName`
- PIDL clone/free helpers
- Property System

資料夾列舉應支援：

- 快速回傳名稱與基本屬性。
- 其餘 property 延遲載入。
- 增量推送到 UI。
- cancellation。
- generation validation。
- permission error。
- offline/network timeout。
- reparse point。
- placeholder 狀態。

## 9.2 檔案操作

使用 `IFileOperation` 實作：

- copy
- move
- delete
- recycle
- permanent delete
- rename
- create folder
- collision handling
- elevation
- confirmation
- progress
- cancel
- partial failure
- undo-related state

實作 `IFileOperationProgressSink` 或等效進度通知。

不要用簡單的 `std::fs::copy` 取代所有 Shell 操作。

## 9.3 原生右鍵選單

使用：

- `IContextMenu`
- `IContextMenu2`
- `IContextMenu3`

需要：

- 取得選取項目的 Shell context menu。
- 插入應用程式自己的 command。
- 正確轉發 owner-draw menu 訊息。
- 正確轉發 `WM_INITMENUPOPUP`、`WM_DRAWITEM`、`WM_MEASUREITEM`、`WM_MENUCHAR` 等必要訊息。
- 支援第三方 Shell Extension。
- 支援多選 context menu。
- 支援背景 context menu。
- 不讓有問題的 extension 永久卡死 UI。

先做 in-process 版本，再將不可信或可能掛死的 extension 隔離列入後續 Milestone。

## 9.4 圖示與縮圖

優先研究：

- `IThumbnailCache`
- `IShellItemImageFactory`
- `SHGetStockIconInfo`

需求：

- 先顯示 generic icon。
- 非同步載入 thumbnail。
- 根據 DPI 與 view mode 請求合適尺寸。
- 記憶體 LRU cache。
- 可選磁碟 cache。
- 避免重複請求。
- item 不可見時降低優先序或取消。
- watcher 更新時失效。
- 正確釋放 HBITMAP/HICON/GDI 資源。
- 不在 UI thread 解碼大型圖檔或影片。

## 9.5 Property System

使用：

- `IPropertyStore`
- `PROPERTYKEY`
- `PROPVARIANT`

至少支援：

- Name
- Date modified
- Type
- Size
- Date created
- Dimensions
- Duration
- Authors
- Tags
- Rating
- Availability
- Status

Property 載入需 lazy、cache、cancelable。

## 9.6 檔案變更通知

優先使用 Shell change notification，必要時搭配：

- `SHChangeNotifyRegister`
- `ReadDirectoryChangesW`

必須處理：

- create
- delete
- rename old/new pair
- modify
- attribute change
- directory change
- drive arrival/removal
- network reconnect
- overflow

事件需 coalesce，避免大量更新造成 UI 抖動。

Watcher overflow 時重新列舉並以 stable ID 做 diff。

## 9.7 Drag-and-Drop

使用 OLE：

- `IDataObject`
- `IDropSource`
- `IDropTarget`
- `DoDragDrop`
- Shell drag image helper
- CF_HDROP
- Shell IDList formats
- Preferred DropEffect
- Performed DropEffect

支援：

- 應用內拖放。
- 拖到其他 Explorer。
- 從其他 Explorer 拖入。
- Ctrl=copy。
- Shift=move。
- Alt 或建立捷徑行為。
- 右鍵拖放。
- 自動捲動。
- 導航樹 hover 展開。
- 網路與虛擬項目。

## 9.8 Clipboard

支援：

- Copy
- Cut
- Paste
- Paste shortcut
- cut item 半透明狀態
- clipboard ownership change
- 與 Explorer 互通

## 9.9 Preview 與 Details Pane

後期加入：

- `IPreviewHandler`
- Preview Handler host
- `IInitializeWithFile`
- `IInitializeWithStream`
- `IInitializeWithItem`

Preview Handler 可能不可信，需做好：

- timeout
- crash isolation 設計
- focus forwarding
- resize
- unload
- COM apartment
- keyboard forwarding

---

# 十、Navigation Pane

左側導覽至少包含：

- Home
- Gallery，如作業系統支援且可合理取得
- OneDrive 或其他雲端根節點
- Desktop
- Downloads
- Documents
- Pictures
- Music
- Videos
- This PC
- 固定磁碟
- 可移除磁碟
- Network
- Recycle Bin
- 使用者釘選項目

Tree 行為：

- lazy expand
- loading indicator
- expand/collapse animation
- active item
- hover
- right-click
- rename
- drag target
- auto-expand
- keyboard navigation
- Home/End/Arrow
- preserve expansion state
- drive removal update

不要遞迴預載整棵檔案系統。

---

# 十一、地址列與導覽

## 11.1 Breadcrumb

支援：

- 每一層可點擊。
- 每一層下拉子資料夾。
- 路徑過長時壓縮中間節點。
- 點擊空白切換成文字輸入。
- Ctrl+L 聚焦文字地址。
- 可輸入環境變數、Shell parsing name、UNC、一般路徑。
- Enter 導覽。
- Esc 還原。
- 錯誤提示不破壞目前頁面。

## 11.2 導覽歷史

每個 Tab 獨立保存：

- Back stack
- Forward stack
- Current location
- Scroll position
- Selection
- View mode
- Sort
- Group
- Column settings

支援：

- Alt+Left
- Alt+Right
- Alt+Up
- Backspace 行為
- 滑鼠側鍵
- 回到歷史位置時恢復合理狀態

---

# 十二、分頁與視窗

支援：

- 新分頁
- 關閉分頁
- 中鍵關閉
- Ctrl+T
- Ctrl+W
- Ctrl+Shift+T
- Ctrl+Tab
- Ctrl+Shift+Tab
- 拖曳排序
- 分頁拖出成新視窗，列入後期 Milestone
- 重複分頁
- 每分頁獨立導覽歷史
- 關閉最後分頁時關閉視窗
- 恢復前次工作階段，可做為設定

自訂 title bar 必須支援：

- drag region
- double-click maximize
- system menu
- minimize
- maximize/restore
- close
- Snap Layout hover 行為
- Win+Arrow
- Alt+Space
- 多 DPI
- DWM dark mode
- Mica/Acrylic 可用時啟用，無法使用時合理降級

不得破壞 Windows 標準視窗管理行為。

---

# 十三、搜尋

分兩層實作。

## 13.1 第一階段

目前資料夾 fallback 搜尋：

- 非同步。
- 可取消。
- 增量顯示。
- 支援檔名。
- 支援副檔名。
- 支援基本 wildcard。
- 不阻塞 UI。

## 13.2 第二階段

整合 Windows Search：

- Indexed search
- AQS 或對應查詢語法
- property filters
- scope
- date
- type
- size
- result ranking

搜尋結果也是 FileView 的一種資料來源，不要另外複製整套 UI。

---

# 十四、Win11 視覺要求

不要硬編碼大量散落色碼。

建立 Theme Token：

````rust
pub struct ExplorerTheme {
    pub window_background: Hsla,
    pub surface_background: Hsla,
    pub elevated_surface: Hsla,
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub border_subtle: Hsla,
    pub accent: Hsla,
    pub hover: Hsla,
    pub selected_inactive: Hsla,
    pub selected_active: Hsla,
    pub focus_ring: Hsla,
    pub danger: Hsla,
    pub corner_radius_small: Pixels,
    pub corner_radius_medium: Pixels,
    pub row_height: Pixels,
    pub command_height: Pixels,
}
````

從 Windows 系統設定取得：

- light/dark
- accent color
- high contrast
- text scale
- DPI
- animation preference
- reduced motion

視覺要接近目前 Windows 11 Explorer，但不要把像素值耦合到單一 OS build。

需要建立 screenshot baseline 與人工視覺比較流程。

---

# 十五、鍵盤與滑鼠行為驗收

至少實作並測試：

| 操作 | 預期行為 |
|---|---|
| 單擊 | 選取並聚焦 |
| Ctrl+單擊 | toggle 單一項目 |
| Shift+單擊 | 由 anchor range select |
| 右鍵未選項 | 選取該項並開選單 |
| 右鍵已選項 | 保持多選並開選單 |
| 點空白 | 清除選取 |
| 空白拖曳 | marquee selection |
| 雙擊資料夾 | 導覽 |
| 雙擊檔案 | Shell 預設開啟 |
| Enter | 開啟 |
| F2 | 重新命名 |
| Delete | 移到資源回收筒或依 Shell 行為 |
| Shift+Delete | 永久刪除確認 |
| Ctrl+C/X/V | 剪貼簿操作 |
| Ctrl+Z/Y | 復原/重做 |
| Ctrl+A | 全選 |
| Ctrl+L | 地址列 |
| Ctrl+F | 搜尋 |
| Alt+Enter | 屬性 |
| Alt+Left/Right/Up | 導覽 |
| 滑鼠側鍵 | 前進/返回 |
| 中鍵資料夾 | 新分頁開啟，可配置 |
| Ctrl+滾輪 | 改變 view/icon size |
| Shift+滾輪 | 水平捲動 |
| ESC | 取消 rename、drag、marquee 或 popup |

細節應以 Windows 11 Explorer 實際公開行為為準。

---

# 十六、錯誤處理

建立結構化錯誤：

````rust
pub enum ExplorerError {
    AccessDenied,
    NotFound,
    AlreadyExists,
    SharingViolation,
    PathTooLong,
    Offline,
    NetworkUnavailable,
    DeviceRemoved,
    InvalidShellItem,
    ComFailure(HRESULT),
    Io(std::io::Error),
    Cancelled,
    Unsupported,
}
````

UI 不得因單一檔案 metadata 讀取失敗而中止整個資料夾。

需要區分：

- 可忽略單項錯誤
- 可重試錯誤
- 需要使用者決策
- 全頁失敗
- 操作部分成功
- 使用者取消

禁止在 production path 使用不加說明的：

- `unwrap()`
- `expect()`
- `panic!()`
- `todo!()`
- `unimplemented!()`

可以在測試或明確不可達分支使用，但需有合理理由。

---

# 十七、資源管理與安全

所有 Win32 handle、COM pointer、PIDL、HBITMAP、HICON、HMENU、GDI object 必須用 RAII 包裝。

要求：

- 正確 `CoTaskMemFree`
- 正確 `DestroyIcon`
- 正確 `DeleteObject`
- 正確 `DestroyMenu`
- 正確 COM release
- 正確取消 watcher
- 視窗關閉時停止背景任務
- Shell thread 能正常 shutdown
- 不持有失效 HWND
- 不讓 callback 在 owner 銷毀後回寫

所有 `unsafe`：

1. 盡量集中在 `explorer-shell-win`。
2. 每個 unsafe block 前寫 Safety invariant。
3. 為 wrapper 寫測試。
4. 不把裸指標暴露到 UI。

---

# 十八、效能目標

Release build、一般 NVMe SSD、主流 Windows 11 電腦：

| 指標 | 目標 |
|---|---:|
| 冷啟動至可互動視窗 | ≤ 800 ms |
| 暖啟動至可互動視窗 | ≤ 400 ms |
| 本機一般資料夾首批內容 | ≤ 150 ms |
| 10 萬項目資料夾可開始互動 | ≤ 1 秒 |
| 10 萬項目捲動 | 目標 60 FPS |
| UI thread 單次一般 callback | 盡量 ≤ 4 ms |
| 導覽取消後舊結果污染 | 0 |
| 縮圖載入造成主執行緒卡頓 | 0 |
| 穩態 UI CPU 使用率 | 接近 idle |
| 長時間瀏覽 handle 持續成長 | 0 |

這些是目標，不得為了數字犧牲正確性。請加入可量測 instrumentation。

至少紀錄：

- directory enumeration latency
- first item latency
- first viewport ready latency
- thumbnail cache hit rate
- number of visible elements
- frame time
- UI task duration
- watcher queue depth
- background queue depth
- outstanding request count
- GDI/User handle count
- memory cache size

---

# 十九、測試策略

## 19.1 Unit Tests

至少覆蓋：

- natural sort
- extension/type sort
- stable selection
- range selection
- marquee intersection
- navigation history
- cancellation
- stale generation rejection
- column resize
- icon grid index calculation
- diff update
- rename validation
- path/address parsing
- cache eviction

## 19.2 Integration Tests

使用暫存資料夾建立：

- 10 個檔案
- 10 萬個檔案
- 深層目錄
- Unicode
- Emoji
- 超長名稱
- 隱藏檔
- system attribute
- symlink/reparse point
- junction
- readonly
- permission denied
- rapidly changing directory
- rename storm
- removable drive mock or手動流程
- UNC/manual network test

## 19.3 UI 行為測試

若 GPUI 暫時缺乏完整 automation，建立可測試的純 Selection/Layout Model，將視覺層保持薄。

至少測：

- Ctrl/Shift 多選
- sort 後選取保持
- watcher insert/remove 後選取保持
- details/grid navigation
- marquee
- inline rename
- keyboard focus
- scroll restoration

## 19.4 手動測試

`docs/MANUAL_TESTS.md` 至少包括：

- 深淺色
- 100%/125%/150%/200% DPI
- 多螢幕不同 DPI
- IME：注音、拼音、倉頡
- RDP
- High Contrast
- Screen Reader
- Shell Extension
- OneDrive Placeholder
- Network Share
- Recycle Bin
- USB 插拔
- Sleep/resume
- GPU driver reset
- Explorer 互相拖放
- Clipboard interoperability

---

# 二十、Milestone 執行順序

## Milestone 0：Bootstrap

交付：

- Cargo workspace
- GPUI 可開啟 Windows 視窗
- logging
- panic hook
- theme token
- CI 基礎
- docs
- 固定 GPUI revision

驗收：

- 可編譯。
- 可顯示空主視窗。
- 可正常關閉。
- 無明顯 handle leak。

## Milestone 1：靜態 Explorer Shell UI

交付：

- title bar
- tab strip
- command bar
- address bar
- navigation pane
- file view placeholder
- details pane
- status bar
- dark/light theme

驗收：

- 尺寸與階層接近 Win11 Explorer。
- resize 正常。
- DPI 正常。
- 標題列標準操作正常。

## Milestone 2：本機資料夾最小垂直切片

交付：

- 導覽至一般本機資料夾。
- 列出檔名、類型、大小、修改時間。
- 雙擊資料夾。
- 雙擊檔案以 Shell 開啟。
- back/forward/up。
- watcher 基礎。

驗收：

- 從啟動到瀏覽、開啟檔案完整可用。
- UI thread 不做同步大目錄掃描。

## Milestone 3：Details View 與完整選取模型

交付：

- 虛擬化 rows
- 欄位
- 排序
- Ctrl/Shift selection
- keyboard navigation
- marquee
- inline rename

驗收：

- 10 萬項目仍可操作。
- 排序後選取穩定。
- watcher 更新不破壞選取。

## Milestone 4：Icon Views 與縮圖

交付：

- small/medium/large/extra-large
- 2D virtualization
- system icon
- async thumbnail
- cache
- Ctrl+wheel zoom

驗收：

- 滾動流暢。
- 不可見項目不大量解碼。
- view mode 切換保留選取。

## Milestone 5：原生檔案操作

交付：

- create folder
- rename
- copy
- move
- recycle delete
- permanent delete
- progress
- cancel
- collision UI
- error UI

驗收：

- 使用 `IFileOperation`。
- 與 Explorer 行為接近。
- 部分失敗可理解。

## Milestone 6：Clipboard、Drag-and-Drop、Context Menu

交付：

- copy/cut/paste interoperability
- OLE drag source/target
- Explorer 互拖
- `IContextMenu3`
- background context menu
- shell extensions

驗收：

- 能與 Windows Explorer 雙向互動。
- owner-draw menu 正常。
- 多選 context menu 正常。

## Milestone 7：Shell Namespace

交付：

- Home
- Known Folders
- This PC
- drives
- Network
- Recycle Bin
- Libraries
- non-filesystem item

驗收：

- 資料模型不依賴一般路徑。
- 可瀏覽沒有 filesystem path 的項目。

## Milestone 8：搜尋、Preview、Details

交付：

- search
- Windows Search integration
- preview pane
- property details
- metadata columns

驗收：

- 搜尋可取消。
- preview 不阻塞主 UI。
- property lazy loading。

## Milestone 9：Parity 與硬化

交付：

- accessibility
- high contrast
- IME
- session restore
- crash recovery
- shell extension isolation 設計
- performance instrumentation
- leak soak tests
- visual regression

驗收：

- 完成 parity checklist。
- 所有已知差異寫入 `docs/PARITY_GAPS.md`。

---

# 二十一、Codex 每次工作的輸出格式

每次開始實作前，先輸出：

1. 本次要完成的 Milestone。
2. 已檢查的相關檔案。
3. 設計決策。
4. 預計修改檔案。
5. 驗收方式。

完成後輸出：

1. 實際修改摘要。
2. 關鍵架構說明。
3. 執行過的命令。
4. 測試結果。
5. 未完成或未驗證項目。
6. 下一個最小步驟。

不要只回覆理論或範例。必須直接修改 Repository 並產生可編譯成果。

若任務太大，完成目前可驗收的最小垂直切片，不要留下大量無法編譯的半成品。

---

# 二十二、禁止事項

禁止：

- 以 Web UI 取代 GPUI。
- 以 `std::fs` 假裝完整 Shell 相容。
- 把每個檔案建立成重量級 persistent View。
- 在 UI thread 同步列舉大型資料夾。
- 直接跨執行緒傳 apartment-affine COM pointer。
- 使用 index 作為檔案永久 identity。
- 忽略 stale async result。
- 只做漂亮畫面而沒有真實 Shell 行為。
- 只做 Shell backend 而沒有可用 UI。
- 複製 Explorer.exe 私有資源。
- 未測試卻聲稱完成。
- 為了消除編譯錯誤而刪除必要功能。
- 大量使用 `unwrap`、`todo!` 或空 stub。
- 一次提交數千行彼此未整合的骨架。
- 修改與本次 Milestone 無關的程式碼。
- 使用未固定版本的 GPUI 依賴。

---

# 二十三、第一個立即任務

現在先執行 **Milestone 0 與 Milestone 1 的最小垂直切片**：

1. 檢查 Repository。
2. 建立 Cargo workspace。
3. 引入並固定可在 Windows 編譯的 GPUI revision。
4. 建立 Windows-only app。
5. 顯示一個可 resize 的自訂 Windows 11 風格主視窗。
6. 畫出：
   - title bar
   - 一個 tab
   - command bar
   - back/forward/up
   - breadcrumb placeholder
   - search placeholder
   - navigation pane
   - file view placeholder
   - status bar
7. 建立 theme tokens。
8. 建立基本 command/action/key binding。
9. 建立 `docs/IMPLEMENTATION_PLAN.md`、`docs/STATUS.md`、`docs/MANUAL_TESTS.md`。
10. 確保 `cargo fmt`、`cargo check`、`cargo clippy`、`cargo test` 可通過。
11. 提供執行方式與 screenshot 驗收步驟。

第一階段不要急著加入所有 Shell API；但架構必須保留 `explorer-shell-win` 邊界，下一個 Milestone 能直接加入真實資料夾導覽。

---

# 二十四、官方參考資料

開工時優先閱讀官方或主要來源：

- GPUI：
  - https://github.com/zed-industries/zed/tree/main/crates/gpui
  - https://www.gpui.rs/
- Zed Windows 實作：
  - https://github.com/zed-industries/zed
- Rust for Windows：
  - https://github.com/microsoft/windows-rs
  - https://microsoft.github.io/windows-docs-rs/
- Windows Shell：
  - https://learn.microsoft.com/windows/win32/api/_shell/
  - https://learn.microsoft.com/windows/win32/shell/interfaces
- `IFileOperation`：
  - https://learn.microsoft.com/windows/win32/api/shobjidl_core/nn-shobjidl_core-ifileoperation
- `IContextMenu3`：
  - https://learn.microsoft.com/windows/win32/api/shobjidl_core/nn-shobjidl_core-icontextmenu3
- `IThumbnailCache`：
  - https://learn.microsoft.com/windows/win32/api/thumbcache/nn-thumbcache-ithumbnailcache
- `IShellItemImageFactory`：
  - https://learn.microsoft.com/windows/win32/api/shobjidl_core/nn-shobjidl_core-ishellitemimagefactory
- OLE Drag-and-Drop：
  - https://learn.microsoft.com/windows/win32/com/drag-and-drop
- Property System：
  - https://learn.microsoft.com/windows/win32/properties/windows-properties-system
- Shell change notifications：
  - https://learn.microsoft.com/windows/win32/shell/change-notify

若 GPUI API 與本提示詞描述不同，以目前固定 revision 的實際 API 和 Zed 原始碼為準，並把差異記錄在文件中。
