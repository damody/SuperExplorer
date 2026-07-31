# Explorer 原生圖示快取、欄位互動與最上層選單設計

## 背景與目標

本設計延續 `match-explorer-visual-address-parity`。目前程式已能從 Windows Shell 取得檔案、資料夾、磁碟與 overlay 圖示，但只有 process 內記憶體快取；breadcrumb `>` 選單仍在普通 render tree；Details 欄位只有固定寬度且沒有真實排序；caption 三按鈕的繪製矩形、GPUI pointer bounds 與 native `WindowControlArea` 尚未以同一矩形驗證。

目標如下：

- 顯示 Windows Shell 實際回傳、未重新著色的 alpha-correct RGBA 圖示，包含檔案關聯及 TortoiseGit／OneDrive overlay。
- 使用 bounded memory LRU 與 `%LOCALAPPDATA%\RustGpuiExplorer\icon-cache\v1` 跨啟動硬碟快取。
- 讓 breadcrumb child menu 永遠在主 scene 上層，並正確處理 anchor、視窗邊界及關閉行為。
- 讓 caption 最小化、最大化／還原、關閉的可見矩形、hover、pointer、UI Automation 與 native hit-test 完全一致。
- 讓 Details 的名稱、修改日期、類型、大小可排序、可拖曳調整寬度，且為 per-tab 狀態。

## 決策

### 1. 圖示來源與色彩

檔案與資料夾圖示只接受 Windows Shell 公開 API 的輸出：filesystem item 優先用 `SHGetFileInfoW(SHGFI_ICON | SHGFI_ADDOVERLAYS)` 取得 Explorer 同源的合成 `HICON`，其他 namespace／大尺寸需求使用 Shell image list 或 `IShellItemImageFactory`。程式不讀 Explorer 私有 binary asset、不猜第三方狀態、不自行套 tint，也不以 app theme 改寫像素。`HICON`／`HBITMAP`／HDC 全部由 RAII 在 Shell STA 釋放，跨執行緒只傳 owned RGBA。

memory/disk cache 與 model 一律保存標準 RGBA；若 GPUI CE 的 Windows texture backend 使用 BGRA byte order，只允許在建立 `RenderImage` 的最後邊界交換 R/B，且必須以黃色 Shell folder 的實機截圖與通道單元測試驗證，不能把 backend 適配結果寫回快取。

### 2. 兩層快取

Shell STA 先查 bounded memory LRU，再查版本化 disk cache，最後才呼叫 Shell。disk key 的 canonical bytes 包含 cache schema、Windows build、Shell identity 或 path fingerprint、size bucket、DPI、theme、association generation 與 overlay generation；digest 作為檔名。value header 保存 magic、schema、key digest、width、height、stride、pixel format、payload length 與 checksum，後接未改色 RGBA。

disk entry 採同目錄 temporary file、flush、atomic rename。啟動不掃描／解碼所有 bitmap；按 request lazy read。格式錯誤、checksum 不符、key 不符或過期時刪除該 entry 並重新取得。總容量與 entry 數有上限，以最近存取時間淘汰；cache 失敗只降低效能，不得使圖示載入失敗。

### 3. Breadcrumb overlay

menu 以 `deferred(anchored(...))` 繪製。`deferred` 保證在 main scene 之後 paint/hit-test，`anchored` 依 chevron bounds 定位並在視窗邊界切換 anchor。overlay 背板負責 click-outside，Esc、window deactivate、切 tab、導覽及 stale generation 都走同一 close transition。menu item 保留既有 typed navigation pipeline。

### 4. Caption 單一矩形

三個 caption button 由同一個 element 同時持有尺寸、背景、glyph、pointer handler、accessibility role 與 `WindowControlArea`；不得在 child 上另建不同 hit target。diagnostics 與 UI Automation 讀取 layout 後的真實 bounds，再與 Windows `WM_NCHITTEST`／實際 click grid 比對。最大化按鈕保留 native Max area以支援 Snap Layout。

### 5. 排序模型

`ViewSettings` 新增 `SortDescriptor { column, direction }`。欄位為 Name、DateModified、Type、Size；點新欄設為 Explorer 預設方向，再點相同欄反轉。command bar Sort 與 header 共用 typed actions/reducer。排序只改 presentation order，不改 `DirectorySnapshot` identity 或 service enumeration order；selection、rename、open、drag 與 context menu 仍以 stable item identity／原始 entry 定位。

比較規則為 containers 優先、欄位 typed value、缺值固定放後、Windows-compatible case-insensitive name fallback、stable identity tie-break。所有 view mode 讀取同一 per-tab sort descriptor。

### 6. Details 欄寬

`ViewSettings` 新增四欄 logical width。header 與 row 共用同一份 widths。separator 具有 Explorer 尺寸的可見線與較寬透明 hit target；drag session 保存 column、起始 pointer、起始 width，支援 pointer capture、release-outside、clamp 與非有限值拒絕。double-click 依 header 文字與目前 snapshot 可見內容估算 auto-size。總欄寬超過 viewport 時使用水平 overflow，不壓縮至不可操作。

## 錯誤與恢復

- disk cache unavailable、唯讀、滿載或 corruption：記錄非敏感診斷，刪除／略過單一 entry，回到 Shell load。
- Shell load 失敗：維持同尺寸 fallback，不寫入成功 cache。
- overlay menu anchor 不可用：關閉 menu 並保留可重試狀態，不把 item 畫在被遮蔽層。
- metadata 缺失：排序缺值固定在具決定性的尾端，畫面保持空白，不偽造零值。
- resize 中 tab／view 改變：結束 drag session，已提交寬度仍 clamp；背景 tab 不被 active tab 覆寫。

## 驗證

- 真實 `D:\`：資料夾、ZIP、RAR、一般檔案、Unicode、TortoiseGit／OneDrive overlay，與 Explorer 相同條件截圖比較。
- cache：cold miss、warm hit、corrupt entry、schema／DPI／theme／association／overlay generation invalidation、容量淘汰及 concurrent request。
- overlay：UI Automation invoke 後 menu items 的 bounds 必須位於 command/body 上層且可點，並測 window edge 翻轉與 click-outside。
- caption：100/125/150/175/200% DPI click grid、hover screenshot、UIA bounds、min/max/restore/close 與 Snap hover。
- sort：四欄升降冪、資料夾／檔案、缺值、Unicode、stable tie、selection identity 與兩 tab 隔離。
- columns：四 separator drag、min/max clamp、release-outside、double-click auto-size、resize/DPI、header-row 對齊與水平 overflow。

## 非目標

- 不複製或散佈 Explorer 私有 icon 檔案。
- 不保證第三方 overlay handler 未向 Windows 註冊時仍能顯示該 overlay。
- 不以磁碟快取取代 Windows association／overlay generation invalidation。
- 不在真實 `D:\` 建立、刪除或修改驗收資料。
