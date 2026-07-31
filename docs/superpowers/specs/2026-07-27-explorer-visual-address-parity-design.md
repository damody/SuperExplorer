# Windows 檔案總管視覺與網址列同等性設計

日期：2026-07-27  
狀態：已由使用者核准方向，待文件審閱  
參考環境：Windows 11 專業版 build 26200、Explorer `10.0.26100.8875`、繁體中文、淺色主題、175% DPI、`D:\` 根目錄

OpenSpec 追蹤：[`proposal.md`](../../../openspec/changes/match-explorer-visual-address-parity/proposal.md)、[`design.md`](../../../openspec/changes/match-explorer-visual-address-parity/design.md)、[`explorer-visual-parity`](../../../openspec/changes/match-explorer-visual-address-parity/specs/explorer-visual-parity/spec.md)、[`interactive-breadcrumb-address`](../../../openspec/changes/match-explorer-visual-address-parity/specs/interactive-breadcrumb-address/spec.md)、[`tasks.md`](../../../openspec/changes/match-explorer-visual-address-parity/tasks.md)。實作前基準與盤點記錄在 [`EXPLORER_VISUAL_ADDRESS_BASELINE.md`](../../EXPLORER_VISUAL_ADDRESS_BASELINE.md)。

## 1. 目標

在相同視窗大小、資料夾、排序、檢視模式、Windows 主題與 DPI 下，使 GPUI 檔案總管的可見結構與 Windows 檔案總管一致。控制項名稱、順序、大小與位置必須對齊；最愛／釘選內容可依本機狀態不同。

本次同時把網址列從單一文字輸入欄位改為 Explorer 式雙模式控制項：

- 瀏覽模式顯示可點擊的路徑麵包屑。
- 點擊麵包屑名稱直接進入對應位置。
- 點擊各階層右側的 `>` 列出該位置的直接子資料夾，選取後導覽。
- 點擊網址列右側未被麵包屑占用的空白區，或按 `Ctrl+L`／`Alt+D`，切換成完整路徑編輯模式。
- Enter 提交路徑；Esc 放棄編輯並回到麵包屑；失敗時保留輸入與可理解的錯誤狀態。

## 2. 可量測的驗收契約

### 2.1 幾何

以視窗 client area 左上角為原點，對參考 Explorer 與 application 擷取同尺寸影像。針對標題／分頁列、導覽列、命令列、側欄、欄位標題、內容列、狀態列、搜尋框與 caption controls 建立具名矩形。

- 每個具名矩形的左、上、右、下邊界與中心座標，相對誤差不得超過參考尺寸的 10%。
- 高度、寬度與間距的相對誤差不得超過 10%；小於 10 logical px 的項目允許最多 1 logical px 的 rounding 差異。
- 參考環境為主要 release gate；100%、125%、150%、200% DPI 需驗證無重複縮放、裁切或重疊。
- 最愛／釘選列的內容文字及數量不比較，但其區域起點、縮排、列高與分隔線仍比較。

### 2.2 Icon、色彩與字型

- 檔案、資料夾、磁碟與 Shell namespace icon 優先由 Windows Shell 動態取得，不維護平行的自繪圖示集合。
- 導覽與命令列 icon 使用 Windows Fluent glyph 或經量測的向量路徑；比較可見邊界、中心、尺寸與線寬。
- icon 可見邊界與中心座標遵守 10% 幾何門檻；不得用文字箭頭或 Unicode 幾何符號冒充最終圖示。
- 色彩由 Windows theme/system color 建立 semantic tokens；參考環境中不受反鋸齒影響的平坦色塊，每個 sRGB channel 絕對差不得超過 12。
- 繁中介面採 Windows UI 字型回退鏈，以 Explorer 實測的字級、字重與行高為基準；字級允許最多 1 logical px 差異。
- 文字邊緣與斜線的 GPU／ClearType 反鋸齒不做逐像素相等判定，但其 baseline、可見高度與控制項內對齊仍需通過幾何 gate。

### 2.3 功能與語意

- 按鈕顯示名稱、tooltip／accessibility label、順序、enabled/disabled 狀態與 Explorer 對應。
- 網址列瀏覽與編輯模式必須可由滑鼠、鍵盤、IME 與 accessibility action 操作。
- `>` 選單只能列出直接子資料夾／可導覽 Shell container；不得因慢速 Shell provider 阻塞 GPUI 執行緒。
- 切換分頁、背景導覽完成或 watcher 更新不得覆寫使用者正在編輯的網址列文字。

## 3. 採用方案與取捨

採用「參考環境鎖定＋Windows 原生資源＋區域幾何量測」方案。

未採用的替代方案：

1. 全部自行重畫：容易在 Windows build、DPI、主題與檔案關聯變更後漂移。
2. 嵌入或自動控制原生 Explorer：無法保持 GPUI application 的狀態模型、測試能力與檔案操作架構。

所謂「一模一樣」定義為：可由公開 Windows/Shell API 取得的資源使用相同來源；其餘控制項依實際 Explorer 截圖量測，並通過本文件的幾何、色彩、字型與互動門檻。Explorer 私有二進位資產與 GPU rasterization 不列為 byte-for-byte 相等要求。

## 4. UI 架構

### 4.1 Explorer chrome

把現有單一 `LayoutTokens::WINDOWS_11` 拆成可追溯的區域 tokens：

- title/tab strip：分頁高度、active tab 曲線、new-tab、caption controls。
- navigation row：Back、Forward、Up、Refresh、breadcrumb address、search。
- command row：New、Cut、Copy、Paste、Rename、Share、Delete、Sort、View、More、Details。

### 2026-07-27 command update and pointer-index addendum

Command bar、Sort/View menu 與 Details header 的 mouse-down 必須在控制項邊界停止傳播，不得落到 file background 的 marquee 或 scrollbar paging。畫面輸入一律使用排序／篩選後的 presentation index，並由單一 resolver 對應 stable item；`scroll_to_item` 不接受 snapshot index。

Create、Rename、Paste、Delete 與可能變更檔案的 Shell command 在成功 terminal 後，若 active tab/generation 未被 watcher 推進，提交一次 correlated refresh。Rename 直接使用 inline editor；Share 透過 Shell canonical `Windows.Share` verb，未提供該 verb 時顯示可恢復錯誤。
- body：navigation pane、divider、details header、file rows、scrollbar。
- status row：item count、selection summary、view controls。

每個 token 附參考來源與 logical px 值。視窗寬度變化時，固定控制項維持參考尺寸，網址列與搜尋框按 Explorer 的優先序分配剩餘空間；窄視窗才進入明確的 compact/overflow 規則。

### 4.2 網址列狀態機

新增獨立 `AddressBarState`，不再用 focus surface 隱含模式：

```text
Browsing(segments)
  ├─ click blank / Ctrl+L / Alt+D → Editing(original, selected_all)
  ├─ click segment               → Navigating(target)
  └─ click chevron               → EnumeratingMenu(parent, request)

Editing(text)
  ├─ text input                  → Editing(updated)
  ├─ Enter                       → Navigating(parsed_target)
  └─ Esc / focus cancellation    → Browsing(resolved_segments)

EnumeratingMenu
  ├─ batch                       → menu incremental update
  ├─ choose child                → Navigating(child)
  ├─ close / Esc                 → Browsing
  └─ error                       → inline unavailable/error item
```

`AddressBarState` 屬於每個 tab。背景 tab event 只更新其 resolved location，不修改 active tab 的 editor entity。

### 4.3 麵包屑資料模型

新增 `BreadcrumbSegment`：stable id、display name、`LocationDescriptor`、icon hint、是否可列舉 children。segment 由已解析的 Shell location ancestry 產生，不以字串切割路徑作唯一來源，以支援「本機」、磁碟、ZIP、Libraries 與其他 Shell namespace。

若目前只有 filesystem descriptor，允許先建立 filesystem ancestry，再由 Shell metadata event 補齊 display name；補齊不得改變 segment target identity。

### 4.4 `>` 子資料夾選單

點擊 chevron 後發出帶 `RequestContext` 的 child-container enumeration command。Shell STA 以 batch 回傳可導覽 containers；UI 顯示 loading、empty、partial、error 與 cancelled 狀態。

- 每次開啟建立新 generation；關閉選單、切 tab、導覽或關窗即取消。
- late batch 必須由 tab id、request id 與 generation 擋下。
- 選單按 Explorer 順序排序，支援滑鼠、方向鍵、Enter、Esc 與螢幕閱讀器。
- 慢速或失敗 provider 不影響網址列編輯與主要導覽。

## 5. Windows 原生視覺資源

### 5.1 Shell icon

在 `explorer-shell-win` 建立 icon request/cache 邊界，輸入 stable Shell item descriptor、所需 logical size、DPI 與 theme，輸出 owned RGBA/texture payload。優先使用 `IShellItemImageFactory`／Shell image list；所有 `HICON`、`HBITMAP` 與 COM ownership 以 RAII 包裝並在非 UI thread 釋放。

cache key 必須包含 identity、size bucket、DPI、theme 與 association generation。檔案關聯或主題變化時失效，viewport 項目優先，不讓大量資料夾一次建立無上限請求。

### 5.2 Chrome icon

導覽與命令列使用集中式 `ExplorerIcon` 列舉和 renderer。每個 icon 指定來源、view box、stroke/fill mode、logical size 與 disabled color，不允許 component 直接散落 Unicode 箭頭或任意字元。

### 5.3 Theme 與 typography

theme service 讀取系統 light/dark/high-contrast 狀態並產生 semantic colors。Explorer 專用 typography tokens 至少包含 tab、command、address、search、navigation、details header、file row 與 status；字型 family 使用 Windows UI 回退鏈，避免把單一英文或中文字型硬編碼到所有語系。

## 6. 事件與資料流

1. app 啟動解析 active location，Shell service 回傳 location metadata、ancestry 與 directory batches。
2. model 以 stable identity 保存 ancestry；UI 在瀏覽模式渲染 segments。
3. 使用者點 segment 或 menu child，action 轉為既有 typed navigation command。
4. 使用者點空白區時，editor 以 parsing path 初始化並全選；提交沿用 address parser，不轉成 search。
5. active tab 成功導覽後才提交 history 並替換 breadcrumb；失敗則保留先前 location，editor 顯示失敗輸入與錯誤。
6. icon/theme/typography 由 service/token 層注入 UI；component 不直接呼叫 Shell 或建立另一份顏色來源。

## 7. 錯誤與邊界情況

- 無權限、不存在或格式錯誤的路徑：不提交 history，保留輸入並顯示錯誤。
- UNC、磁碟根目錄、Shell namespace、ZIP 與重新命名中的位置：以 descriptor/ancestry 為準，不假設每層都有 filesystem path。
- 路徑過長：全程使用 owned UTF-16／Windows path 語意，不用 `MAX_PATH` 固定 buffer。
- 子資料夾選單 enumeration 超時或取消：顯示可恢復狀態，不關閉整個 tab。
- icon 取得失敗：使用同尺寸的明確 fallback，維持幾何不跳動，並記錄診斷。
- 主題或 DPI 在視窗存活期間變更：重新建立 tokens 與 size-dependent icon cache，不重複 scaling。
- Explorer 私有 icon 無公開來源：使用 Fluent 等價 glyph，記錄例外與區域差異，不以模糊截圖資產取代。

## 8. 驗證策略

### 8.1 單元與狀態測試

- address state transitions、Enter/Esc、blank-area click、focus restoration。
- filesystem 與 Shell namespace ancestry。
- segment navigation identity、chevron enumeration cancellation、late event rejection。
- active/background tab isolation、編輯文字不被 event 覆寫。
- icon cache key、DPI/theme invalidation、RAII resource release。
- layout token invariants、窄視窗 overflow 與各 DPI rounding。

### 8.2 整合與真實資料夾測試

- 在真實 `D:\` 以與 Explorer 相同的排序與 Details view 開啟，核對資料夾、檔案與 Shell icon。
- 點擊「本機」、磁碟與中間資料夾 segment，驗證 history、Back/Forward/Up。
- 逐一點擊各 `>`，驗證列舉、選取、取消、慢速與無權限資料夾。
- 使用網址列空白區、`Ctrl+L`、`Alt+D`、IME、貼上與長路徑導覽。
- 重跑 clipboard、context menu、search 與 OLE drag-and-drop，確保 chrome 重構沒有回歸。
- 納入 `target/explorer-interop-evidence/20260727-drag-v26-explorer-to-app/fixture/explorer-source/explorer-left-copy.txt` 所屬的 Explorer→app copy-drop fixture；測試不得修改原始證據檔。

### 8.3 視覺量測

- 擷取 Explorer 與 app 的同尺寸 screenshot 和 geometry diagnostics。
- 以具名 region JSON 比較控制項矩形，而非只依全圖 changed-pixel ratio。
- 產出 reference、actual、overlay、diff、region report 與 metadata。
- 主 gate：175% DPI、淺色、繁中、`D:\`；補充 gate：light/dark/high-contrast 與 100/125/150/200% DPI。
- failure report 列出超標 region、實際與參考座標、比例、色差、字級與 icon bounds，讓修正可重現。

## 9. 實作邊界與完成條件

這項變更只調整既有 GPUI 檔案總管，不嵌入 Explorer，也不修改使用者真實資料。破壞性檔案操作只在既有隔離 fixture 中驗證。

完成必須同時符合：

1. 上述網址列所有滑鼠、鍵盤、IME 與 accessibility 行為完成。
2. 參考環境所有具名控制項通過 10% 幾何門檻。
3. icon、平坦色彩與 typography 通過本文件門檻。
4. 真實 `D:\`、多分頁、檔案操作、Clipboard、OLE drag-and-drop、context menu 與 search regression gates 全部通過。
5. `cargo fmt`、workspace tests、Clippy、release build、OpenSpec strict validation 與 headful evidence 完成。
