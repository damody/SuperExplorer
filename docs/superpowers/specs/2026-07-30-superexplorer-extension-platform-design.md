# SuperExplorer 擴充平台設計規格

## 一、文件摘要

SuperExplorer 將建立一套統一的擴充套件系統，用來承載 Rust 原生外掛、Lua 自動化腳本，以及完全由資料構成的視覺 Skin。第一階段的最高優先級是八個含完整原始碼的官方範例外掛，以及讓這些範例可由外部作者實際建置、安裝、執行、除錯與打包所需的正式介面和 SDK 環境；開發採範例驅動的垂直切片，不先堆出未被真實外掛使用的空介面。Steam Workshop、付費項目、作者分潤與 DLC 所有權只規畫介面及未來接點，不列入第一階段實作。

產品預計以輕量、快速、可擴充、可高度客製化的 Windows 檔案總管為定位，基礎版預定售價為 2.99 美元。未來可另行推出 9.99 美元的 Pro Toolkit DLC，提供官方製作的專業解析與預覽套件，例如 Perfetto、trace-cmd、Word、XLSX 與本機 OCR。完整的第三方擴充能力、Rust SDK、Lua 與 Skin 不會被鎖在 Pro 方案後方；Pro 的定位是官方專業內容合集，而不是擴充平台的付費門檻。

## 二、核心設計決策

本設計採用下列已確認的決策：

1. Rust 只有一種外掛格式，不區分「資料外掛」與「UI 外掛」。同一個 DLL 可以註冊多個選擇性介面與功能。
2. Rust 外掛可以直接使用固定版本的 GPUI 繪製任意 UI，包括欄位儲存格、預覽、面板、按鈕、圖表、動畫與設定頁。
3. Rust 外掛只在 SuperExplorer 啟動時載入，不支援熱載入、熱更新或熱卸載；已載入功能可用 runtime gate 停止，但安裝、更新及啟用本次啟動時未載入的 DLL 必須重新啟動。
4. `abi_stable` 用於根模組、能力協商、資料介面與載入時型別檢查，但不宣稱能讓 GPUI 型別取得穩定 ABI。
5. 只要外掛註冊任何 GPUI 介面，就必須與官方 UI Plugin SDK toolchain bundle 的 ABI 指紋完全相符。
6. 未使用 GPUI 的穩定資料介面，在遵守演進規則的前提下，保證於同一 SDK 大版本內相容，例如 SDK 1.x。
7. 開啟資料夾後，外掛可以在背景處理全部項目；主要驗收規模為 1,000 個項目。
8. Lua 採能力權限模型，不能直接取得任意系統權限；檔案異動必須經過宿主的操作計畫與復原機制。
9. Skin 能替換圖片、按鈕、圖示、字型、不規則視覺外框與透明背景，但仍保留矩形 Windows 視窗，以維持 Snap、最大化與縮放行為。
10. AI 提示詞只提供給 Rust 外掛作者，不為 Lua 或 Skin 提供生成提示詞。
11. 免費 Lua 與 Skin 未來可使用 Ready-To-Use Workshop；所有 Rust 外掛無論免費或付費，未來都必須經過審核、重建與簽署。
12. 第一階段不實作 Steam，只保留 Package Source 與 Entitlement 等抽象介面。
13. 自訂容器格式可透過虛擬資料夾介面加入導覽樹；第一個範例是可瀏覽及修改的 7z 壓縮檔。
14. 八個完整原始碼官方範例、其公開接口及 SDK 執行／建置環境列為 P0；在 P0 驗收完成前，不開始 Steam、付費商城或 Pro 專業解析套件的 production implementation。
15. Rust SDK 的 canonical `Cargo.lock`、Rust `1.97.1`、`damody/gpui-ce-explorer` 精確 snapshot、`abi_stable 0.11.3`、離線依賴與 fingerprint fixture 列為 P0-0；開發期可將 GPUI 更新到 `main` 的新 commit，但每次更新都產生新的不可變 snapshot bundle，Release 時凍結最終 commit。

## 三、目標

### 3.1 功能目標

- 讓單一 Rust 外掛註冊多個選擇性能力，包括資料提供、檔案解碼、命令、GPUI 欄位渲染、預覽、工具列、面板與設定頁。
- 發布可重現建置的 UI Plugin SDK toolchain bundle，讓作者與 AI 使用固定工具鏈建立相容 DLL。
- 使用 `abi_stable` 建立可檢查、可演進的非 GPUI ABI 邊界。
- 讓外掛能在背景解析 Perfetto、trace-cmd、Office 文件、圖片或其他自訂格式，再把結果寫入欄位或預覽。
- 讓 Lua 巨集安全地完成批量改名、建立資料夾與其他檔案工作流程。
- 讓 Skin 作者自由替換視覺資產、按鈕狀態、背景透明度與點穿遮罩。
- 讓 Rust 外掛把 7z 或其他容器格式映射成可導覽、可預覽並可修改的虛擬資料夾。
- 讓基本檔案列表先顯示，再於背景處理最多約 1,000 個項目的擴充資料，避免阻塞 GPUI 執行緒。
- 提供 Rust 專用 AI 提示詞，使程式助手能依實際 SDK、固定版本與驗證指令產生可建置的外掛。
- 保留未來 Steam Ready-To-Use Workshop、Curated Workshop 與 Pro DLC 的明確整合接點。

### 3.2 品質目標

- 擴充介面不得直接洩漏 SuperExplorer 私有 model 或 GPUI entity。
- 外掛錯誤必須能清楚歸因到套件、介面、項目與工作。
- 不相容的 GPUI 外掛必須在執行任何註冊 callback 前遭到拒絕。
- 背景結果更新必須批次合併，不能因 1,000 筆結果造成 1,000 次同步重繪。
- 原生外掛崩潰後，下次啟動必須能提供 Safe Mode 及疑似外掛資訊。
- SDK 發布必須有歷史相容測試、GPUI 指紋測試與範例外掛測試。

## 四、第一階段不處理的範圍

下列項目不屬於第一階段實作：

- Steamworks SDK 整合。
- Workshop 上傳、下載、訂閱、取消訂閱與更新。
- Steam Inventory Service、Microtransactions、作者 payment rules 與分潤。
- Pro DLC 所有權檢查與 Steam 離線授權同步。
- Rust DLL 熱載入、熱更新或卸載。
- Rust 外掛的程序隔離或 sandbox。
- 跨平台原生外掛；第一個正式 target 為 Windows x64 MSVC。
- 讓 Skin 執行 Rust、Lua、JavaScript 或任意原生 shader 程式碼。
- Lua 與 Skin 的 AI 生成提示詞。
- 將 SuperExplorer 私有 workspace crate、內部 model、視窗狀態或 GPUI entity 公開給外掛。

目前專案中的 `explorer-extension-protocol` 與 extension broker 仍用於 Windows Shell handler 或其他需要程序隔離的整合，不會在本階段被新的程序內 Rust SDK 取代。

## 五、總體架構

```text
SuperExplorer UI 與內部 Model
        │
        ▼
Extension Host
        ├─ Package Manager
        ├─ Capability / Permission Manager
        ├─ Job Scheduler
        ├─ Result Cache
        ├─ Rust Plugin Loader / Registrar
        ├─ Lua Runtime Adapter
        ├─ Skin Loader
        └─ GPUI Extension Adapter
                │
                ▼
Public Extension SDK
        ├─ abi_stable 基礎介面
        ├─ 精確工具鏈 GPUI 介面
        ├─ Lua Host API
        └─ Skin Schema
```

### 5.1 Extension Host

Extension Host 是擴充功能進入主程式的唯一入口。它負責：

- 發現、驗證與選擇套件版本。
- 驗證 ABI、SDK 大版本與 GPUI 指紋。
- 建立 Rust registrar 並收集外掛註冊的介面。
- 建立 Lua 執行環境及核發能力。
- 載入 Skin 資產並套用視覺回退。
- 將背景工作排入正確的 CPU 或 I/O 佇列。
- 保存結果快照、排序值、快取與錯誤資訊。
- 將公開的擴充狀態轉接到 SuperExplorer 內部 model。

外掛不會直接取得 `explorer-ui`、`explorer-model` 或其他私有 crate 的型別。即使是 GPUI 外掛，也只會拿到公開 SDK context、固定版本的 GPUI `Window`／`App`，以及不可變的資料快照。

### 5.2 與既有 Extension Broker 的關係

既有 broker 繼續負責 Windows Shell extension、COM handler、預覽 handler 或其他需要程序隔離的操作。新的 Rust Plugin SDK 是已明確選擇的程序內擴充機制。

兩者可以共享「預覽」、「欄位」、「錯誤狀態」等領域概念，但不能直接共享 ABI struct 或 IPC frame。這樣可以避免把 broker 的序列化限制帶入 GPUI 外掛，也避免讓程序內指標意外跨越 broker 邊界。

## 六、統一 `.sepack` 套件格式

每個擴充套件使用 `.sepack` 邏輯格式。在執行階段，它是經過驗證的資料夾；未來用於下載時，可以使用 ZIP 相容的封裝。單一套件可以同時包含 Rust DLL、Lua 腳本與 Skin。

```text
package/
├─ manifest.json
├─ native/
│  └─ plugin.dll
├─ tools/
│  └─ windows-x64/
│     └─ <tool-id>/
│        ├─ tool.exe
│        ├─ LICENSE.txt
│        └─ NOTICE.txt
├─ lua/
│  └─ commands.lua
├─ skin/
│  ├─ skin.json
│  ├─ images/
│  ├─ icons/
│  └─ fonts/
├─ locales/
│  ├─ zh-TW.json
│  └─ en-US.json
└─ signature.json
```

### 6.1 Manifest 必要資料

`manifest.json` 至少包含：

- 全球唯一的 package ID，建議使用反向網域或作者命名空間。
- 作者／發行者身分與必要聯絡資訊。
- 套件版本、SDK 大版本與最低宿主版本。
- Rust、Lua、Skin 各自的入口點。
- 宣告的能力與權限。
- 支援的檔案副檔名、MIME 或檔案偵測規則。
- 套件相依關係與版本需求。
- 套件內外部工具清單，包括工具 ID、target、相對路徑、精確版本、檔案大小、SHA-256、預期輸出格式、來源與授權檔案。
- 本地化字串 key。
- 內容檔案清單與 hash。
- UI ABI fingerprint；未註冊 GPUI 能力時可以不存在。
- 資料快取版本，用於決定更新後是否保留舊結果。
- 可開關功能清單；每個功能包含穩定 feature ID、名稱、說明、分類、預設狀態、入口點、能力、相依功能與重新啟動規則。

作者資料不得只是一段無法驗證或解析的自由文字。Manifest 使用結構化的 `publisher` 物件：

```text
publisher:
  id: 穩定且不可隨意變更的發行者 ID
  display_name: UI 顯示名稱
  contacts: 至少一筆公開聯絡方式
  homepage_url: 選填官方網站
  source_url: 選填原始碼儲存庫
```

`contacts` 是結構化陣列，每筆資料包含聯絡類型、顯示標籤、實際值或網址，以及用途：

```text
contacts:
  - kind: email
    label: 安全與技術支援
    value: support@example.com
    purposes: [security, support]
  - kind: discord_server
    label: Discord 討論區
    value: Discord 邀請網址或永久伺服器識別資訊
    purposes: [community, support]
  - kind: qq_group
    label: QQ 討論群
    value: QQ 群號或可加入網址
    purposes: [community]
```

第一版已知的 `kind` 包括 `email`、`website`、`support_forum`、`github_issues`、`discord_server`、`discord_user`、`qq_group` 與 `other`。Manifest 至少要有一筆聯絡方式，而且至少一筆必須標示為 `support` 或 `security`；只有 `community` 群組而沒有任何支援／安全通報管道是不足的。Email 是建議但不是唯一合法選項，作者可以只提供 Discord、QQ 群或公開論壇，只要其中至少一個管道明確接受支援或安全問題。

Package Manager 依聯絡類型驗證格式、長度與 URI scheme，但不會主動加入群組、開啟連結或傳送訊息。對於可能失效的 Discord 邀請連結，SDK 建議同時提供穩定的伺服器名稱或官方網站入口。QQ 群號以字串保存，避免數字長度或前導零處理問題。

所有 manifest 聯絡資料都視為公開資訊。作者不應填入不希望公開的私人 email、私人 Discord 帳號或其他敏感聯絡資料；建議使用專用支援信箱、公開討論區或官方社群帳號。

正式簽署套件的 `publisher.id` 必須與簽章中的發行者身分一致，避免第三方只修改顯示名稱或聯絡方式冒充作者。未簽署的本機開發套件仍須提供聯絡欄位，但 UI 必須清楚標示其身分未經驗證。未來接入 Steam Workshop 時，可額外保存 Steam 作者 ID；Steam ID 不取代公開支援管道，也不直接作為跨平台 package ID。

凡是外掛執行時需要另一個 executable，該 executable 就是套件內容的一部分，作者必須自行封裝。Manifest 的 `tools` 使用結構化資料，例如：

```text
tools:
  - id: tokei
    target: windows-x64
    path: tools/windows-x64/tokei/tokei.exe
    version: 精確版本
    size_bytes: 精確檔案大小
    sha256: 小寫十六進位 SHA-256
    protocol: json-stdout
    source_url: 官方來源
    license_files:
      - tools/windows-x64/tokei/LICENSE.txt
      - tools/windows-x64/tokei/NOTICE.txt
```

外掛不能只宣告工具名稱，然後要求使用者自行下載、安裝或設定路徑。每個宣告支援的 target 都必須有對應的工具 payload；目前產品只支援 `windows-x64`，因此第一階段至少提供這個 target。套件缺少工具、target 不符、hash 不符或授權檔缺少時，Package Manager 將依賴該工具的 feature 標記為 `blocked`，且不執行任何 Lua callback。

### 6.2 套件來源與選版

Package Manager 未來會合併以下來源：

```text
內建套件
→ Pro Toolkit 套件
→ Steam Workshop 已安裝套件
→ 本機開發者套件
```

第一階段只啟用內建套件與本機開發者套件。Pro 與 Steam 來源只有抽象介面，不連接 Steamworks。

相同 package ID 同一時間只能啟用一個版本。若相依條件無法滿足、hash 不符、簽章無效或 ABI 不相容，整個套件都不得載入。不能只載入其中一部分，否則資料 provider 與 UI renderer 可能落入不一致狀態。

### 6.3「資料夾選項／擴充功能」分頁

現有「資料夾選項」對話框在「一般」與「檢視」旁新增第三個「擴充功能」分頁。這是所有擴充功能的主要管理入口，設定預設套用到目前 Windows 使用者的所有 SuperExplorer 視窗與資料夾，不是只套用到開啟選項時所在的單一資料夾。

分頁頂端提供總開關「啟用擴充功能」。關閉後，所有非核心 Rust、Lua、Skin、官方範例與未來 Pro 擴充都停止提供功能，但套件、設定與快取不會被刪除。SuperExplorer 的核心檔案導覽、檔案操作及進入 Safe Mode 的能力不受此開關控制。

總開關下方提供搜尋、類型篩選與狀態篩選：

```text
類型：全部 / Rust / Lua / Skin / 官方 / 第三方
狀態：全部 / 已啟用 / 已停用 / 需要重新啟動 / 錯誤 / 不相容
```

每個已安裝套件顯示一張可展開項目，至少包含：

- 套件名稱、圖示、版本與 package ID；
- 作者／發行者名稱及 manifest 中的公開聯絡方式；
- 官方、已簽署、未簽署、本機開發或未來 Workshop 來源標記；
- Rust、Lua、Skin 類型標籤；
- 套件總開關；
- 已要求能力與敏感權限摘要；
- 隨套件封裝的外部工具、版本、target、來源與授權入口；
- SDK／GPUI fingerprint 相容狀態；
- 錯誤、慢外掛與 Safe Mode 診斷入口；
- 「重新啟動後生效」狀態標記。

展開套件後，列出 manifest 宣告的每個可獨立開關功能。例如同一個 Rust DLL 可以分別開關「資料夾大小欄位」、「重新計算命令」與「設定頁」，但它們仍屬於同一個 Rust 套件。每個功能列顯示：

```text
feature_id
本地化名稱與說明
功能分類
開關
相依功能
所需能力
立即生效或重新啟動標記
```

所有 registrar contribution 都必須關聯一個 manifest `feature_id`。未關聯、重複、未宣告或實際能力超出 manifest 的 contribution 會使套件驗證失敗。如此 Package Manager 才能精確停止欄位、按鈕、預覽、虛擬資料夾或其他功能，而不是只能粗略停用整個 DLL。

#### 設定狀態模型

設定同時保存 `desired_state` 與 `effective_state`，並可呈現：

- `enabled`：已啟用且正在生效；
- `disabled`：已停用；
- `pending_restart`：使用者設定已保存，但需重啟；
- `disabling`：正在取消工作、移除 UI contribution 或等待 callback 結束；
- `blocked`：相依功能停用、權限不足或套件不相容；
- `faulted`：載入、執行或驗證失敗。

關閉父套件或總開關時，子功能保留各自的 desired state；重新打開父層後恢復原本個別選擇，不會把所有子功能強制改成啟用。

#### 套用、確定、取消與關閉

- 「套用」保存本頁所有變更並嘗試讓可即時切換的功能生效，對話框保持開啟。
- 「確定」執行與套用相同的驗證及保存，成功後關閉對話框。
- 「取消」放棄自上次套用後尚未保存的變更，不回復先前已按下「套用」的設定。
- 右上角「關閉」若沒有未保存變更就直接關閉；若有變更，顯示「套用／捨棄／返回」選擇，不能靜默遺失設定。

若一個開關會連帶停用其他功能、關閉面板、移除欄位或離開目前虛擬資料夾，套用前必須顯示具體影響。相依套件不允許在沒有確認的情況下形成半啟用狀態。

#### 不同擴充類型的切換語意

- Lua：停止新 callback、取消現有工作並移除命令／欄位後即可停用；重新啟用可以立即重新註冊。
- Skin：停用目前使用中的 Skin 時立即回到預設 Skin；啟用 Skin 代表使其可選，不自動取代使用者目前 Skin，除非使用者另外選取。
- 已載入的 Rust：停用時先關閉其 UI contribution、停止新 dispatch、取消工作並等待有界時間讓 callback 結束；DLL 保持 resident，不進行卸載。若 drain 成功，功能可在本次執行階段停止或重新啟用。
- 啟動時未載入的 Rust：由停用改成啟用時需要重新啟動，因為本設計不做執行期 DLL 熱載入。
- Rust 更新、替換 DLL 或 fingerprint 改變：一律需要重新啟動。
- Virtual Folder：若使用者正在 7z 或其他虛擬位置內，停用 provider 前先提示並把受影響的分頁導回容器所在的一般資料夾；使用者取消導覽時，不套用該停用動作。

若 Rust callback 在有界時間內未結束，狀態改為 `pending_restart`，不強制卸載 DLL。所有切換都要寫入診斷紀錄，但不記錄密碼、私密路徑內容或其他 secret。

## 七、單一 Rust 外掛模型

### 7.1 一個 Root Module，多個選擇性介面

每個 Rust DLL 只匯出一個 `abi_stable` root module。Root module 提供 metadata、相容資訊與註冊入口。啟動時，宿主將 `PluginRegistrar` 傳給外掛；外掛可依需求註冊任意數量的介面。

可註冊的第一版介面包括：

- `ColumnProvider`：計算欄位資料。
- `BatchColumnProvider`：以有界批次計算多個項目的欄位資料，適合呼叫一次外部分析工具後回填多列。
- `ColumnAggregator`：對目前資料夾的多筆欄位資料做聚合。
- `ColumnRenderer`：直接使用 GPUI 繪製欄位 cell。
- `PreviewProvider`：解析預覽資料。
- `PreviewRenderer`：直接使用 GPUI 繪製預覽面板。
- `FileDecoder`：解析自訂檔案格式，提供欄位或預覽共用資料。
- `VirtualFolderProvider`：把檔案或其他容器映射成可導覽的虛擬資料夾樹。
- `VirtualFileStreamProvider`：為虛擬項目提供有界唯讀 stream，供預覽、複製與其他 provider 使用。
- `VirtualMutationProvider`：以交易方式加入、刪除、改名或建立虛擬資料夾項目。
- `CommandProvider`：註冊命令與快捷鍵。
- `ContextMenuProvider`：加入檔案或背景選單項目。
- `ToolbarProvider`：加入工具列按鈕或元件。
- `PanelProvider`：加入可停駐或可切換的自訂面板。
- `SettingsProvider`：建立外掛設定頁。

Rust 在 manifest 中只有一種內容類型，例如 `kind: "rust"`。是否需要嚴格 GPUI 指紋，不由套件類型決定，而是由外掛實際註冊的能力決定。

### 7.2 Plugin Registrar 演進

`PluginRegistrar` 使用 `abi_stable` prefix type。SDK 1.x 若要新增介面，只能在尾端加入新的 optional function。外掛必須先檢查宿主是否提供該註冊函式，再決定是否註冊。

宿主不得推測未知 capability 的意思。遇到未知、已移除或宣告與實際註冊不一致的能力時，必須回傳明確錯誤並拒絕整個 DLL。

## 八、`abi_stable` 的使用範圍

### 8.1 適合使用的部分

`abi_stable` 負責：

- `RootModule` 與 DLL 載入入口。
- 載入時 layout 檢查。
- prefix type 與選擇性欄位演進。
- metadata 與 capability negotiation。
- 非 GPUI provider 介面。
- FFI-safe 字串、陣列、選擇值與結果值。
- 可加入新 variant 的 non-exhaustive 資料型別。

穩定介面只使用固定寬度 primitive，以及 `RString`、`RVec`、`ROption`、`RResult` 等 FFI-safe 型別。

### 8.2 禁止跨穩定 ABI 傳遞的資料

下列資料不能直接跨越穩定 ABI：

- `std::String`、`Vec<T>` 與一般 Rust collection。
- 普通 Rust trait object。
- Rust `Future`、Tokio runtime 或 async task handle。
- 捕捉環境的 closure。
- GPUI entity、SuperExplorer model 或 HWND 包裝型別。
- 生命週期不明、由另一側保存的借用 reference。
- 由一側 allocator 建立、卻由另一側用不同 allocator 釋放的裸記憶體。

### 8.3 SDK 1.x 相容規則

同一大版本內允許：

- 在 prefix type 尾端增加 optional 欄位。
- 增加新的 optional capability。
- 在 non-exhaustive enum 增加 variant。
- 加入不改變既有語意的新錯誤資訊。

同一大版本內禁止：

- 改變既有欄位順序、大小或意義。
- 移除已發布的必要函式。
- 改變 allocator 或所有權責任。
- 將同步 callback 改成不同 calling convention。
- 重新使用既有數值 ID 表示不同狀態。

需要上述破壞性變更時，必須發布 SDK 2.0。

## 九、GPUI 原生繪製能力

### 9.1 設計原則

任何 Rust 外掛都可以選擇註冊 GPUI renderer。作者可以自由建立：

- 自訂欄位 cell。
- 文字、圖片、表格、樹狀、時間軸或十六進位預覽。
- 迷你圖表、波形、熱度圖、進度條與動畫。
- 工具列按鈕、互動控件與狀態指示。
- 自訂面板與設定頁。

GPUI callback 可接收公開 SDK context、固定版 GPUI `Window`／`App` 與資料快照，並回傳固定 bundle 所定義的 GPUI element。它不能接收 SuperExplorer 私有 UI state，也不能把 `Entity<PrivateModel>` 等內部型別保存起來。

### 9.2 UI ABI Fingerprint

Rust 官方不保證 `extern "Rust"` ABI 穩定，因此固定 GPUI crate 版本仍不足以單獨保證相容。正式 SDK bundle 需要固定並雜湊下列輸入：

```text
ui_abi_fingerprint =
    exact Rust toolchain
  + target triple
  + GPUI revision
  + SDK versions
  + complete locked dependency graph
  + enabled features
  + panic strategy
  + supported build profile
```

只要外掛註冊任一 GPUI 介面，載入器就要求 fingerprint 完全一致。如果外掛只註冊 `abi_stable` 資料介面，則使用 SDK 大版本與 layout compatibility 規則。

若同一 DLL 同時註冊資料 provider 與 GPUI renderer，整個 DLL 採較嚴格的 fingerprint 規則。不得只啟用 provider、跳過 renderer，避免作者假設的內部狀態不成立。

### 9.3 執行緒與效能規則

- GPUI renderer 只在 GPUI thread 呼叫。
- renderer 只能讀取不可變快照與公開 context。
- renderer 不能做檔案 I/O、網路請求或長時間解析。
- 背景結果更新後，外掛透過宿主提供的 invalidation handle 要求重繪。
- 宿主量測每個 renderer 的執行時間，並在診斷頁標示慢外掛。
- 宿主可以降低慢 renderer 的更新頻率，但無法安全強制中斷正在執行的程序內 Rust callback。

## 十、UI Plugin SDK Toolchain Bundle

官方發布的 bundle 結構如下：

```text
superexplorer-ui-plugin-sdk/
├─ rust-toolchain.toml
├─ sdk-lock.json
├─ Cargo.lock
├─ Cargo.toml
├─ .cargo/
│  └─ config.toml
├─ crates/
│  ├─ superexplorer-plugin-api/
│  └─ superexplorer-ui-plugin-api/
├─ vendor/
│  └─ cargo-sources/
│     ├─ gpui-explorer-<commit>/
│     └─ .../
├─ compatibility/
│  ├─ host-fixture/
│  ├─ plugin-fixture/
│  └─ expected-fingerprint.json
├─ templates/
│  └─ rust-plugin/
├─ examples/
│  ├─ rust-folder-size-visual-column/
│  ├─ rust-tokei-code-lines-column/
│  ├─ lua-tokei-code-lines-column/
│  ├─ lua-bulk-folder-generator/
│  ├─ rust-exif-rename-command/
│  ├─ rust-lock-owner-column/
│  ├─ rust-7z-virtual-folder/
│  └─ rust-folder-size-map-view/
├─ AI_RUST_PLUGIN_PROMPT.md
├─ build-plugin.ps1
├─ validate-plugin.ps1
├─ package-plugin.ps1
└─ bundle-manifest.json
```

### 10.1 P0-0：SDK 相容基線 Bootstrap Gate

Rust 外掛所需的版本規格檔與建置環境是所有官方 Rust 範例的前置條件，列為 `P0-0`。在此 gate 通過前，不得開始撰寫依賴 GPUI callback 的正式範例程式碼，也不得向第三方發布任何「暫定 SDK」。

截至 2026-07-31，本規格已選定下列 P0-0 baseline；「當前最新穩定版」只用於這次選版，提交後全部轉為不可移動的精確值：

| 元件 | 鎖定值 | 來源與完整性 |
| --- | --- | --- |
| Rust | `rustc 1.97.1 (8bab26f4f 2026-07-14)` | 完整 commit `8bab26f4f68e0e26f0bb7960be334d5b520ea452`，host `x86_64-pc-windows-msvc`，LLVM `22.1.6` |
| Cargo | `cargo 1.97.1 (c980f4866 2026-06-30)` | 完整 commit `c980f4866141969fab6254a680546a277789d6f0`，隨上述 toolchain 固定，不能混用其他 Cargo 解析 canonical lock |
| GPUI | Git source `https://github.com/damody/gpui-ce-explorer.git`，追蹤分支 `main`，crate package `gpui` | 2026-07-31 查詢到的開發 snapshot HEAD 為 `33ed975bf2dff2735eaa21366aa7fa19015c891c`，package metadata version `0.2.2`；每次 snapshot 另記錄實際 commit 與 vendor tree hash。[repository](https://github.com/damody/gpui-ce-explorer) |
| ABI library | `abi_stable = 0.11.3` | SHA-256 `69d6512d3eb05ffe5004c59c206de7f99c34951504056ce23fc953842f12c445`；crate publish VCS commit `9966b8f0084fc768e3fb557bf81affea0b5868d8`；非 yanked stable release。[crates.io](https://crates.io/crates/abi_stable/0.11.3) |

目前主 repository 已提交的 vendor GPUI submodule pointer 仍是 `0cd06bd8cc469e606e2bbf0d82679c88cfe8a951`；P0-0 必須把它的 remote/source authority 改為 `damody/gpui-ce-explorer`，並讓宿主、公開 SDK、模板與所有 Rust fixtures 使用同一個已發布 snapshot commit。若新 snapshot 公開 API 造成主程式遷移工作，這仍屬 P0-0 blocker，不能讓宿主與外掛使用不同 commit 形成雙軌。

Rust 與 `abi_stable` 固定後不得在 Cargo 設定中以 `stable`、`latest`、範圍版本或未鎖定 path 代替。GPUI 的 `main` 只作為「尋找下一個開發 snapshot」的 update channel；任何實際 host、plugin、CI 或 bundle build 都必須使用解析後的完整 commit，不能直接依賴浮動 branch。既有 snapshot 與 Release bundle 內容永不原地改寫。

正式規格檔至少包含：

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.97.1"
profile = "minimal"
targets = ["x86_64-pc-windows-msvc"]
components = ["rustfmt", "clippy"]
```

```toml
# SDK consumer Cargo.toml 的 protected dependencies
[package]
rust-version = "1.97.1"

[dependencies]
gpui = { git = "https://github.com/damody/gpui-ce-explorer.git", rev = "33ed975bf2dff2735eaa21366aa7fa19015c891c", package = "gpui", default-features = false }
abi_stable = { version = "=0.11.3", default-features = false }
```

`sdk-lock.json` 至少包含下列不可省略的 canonical fragment；`bundle_id` 與 vendor tree hashes 在 P0-0 產生器實際封裝後填入：

```json
{
  "schema_version": 1,
  "bundle_id": "sha256:<generated-from-canonical-inputs>",
  "rust": {
    "channel": "1.97.1",
    "release": "1.97.1",
    "commit": "8bab26f4f68e0e26f0bb7960be334d5b520ea452",
    "llvm": "22.1.6",
    "target": "x86_64-pc-windows-msvc"
  },
  "cargo": {
    "release": "1.97.1",
    "commit": "c980f4866141969fab6254a680546a277789d6f0"
  },
  "protected_dependencies": {
    "gpui": {
      "package": "gpui",
      "package_version": "0.2.2",
      "git": "https://github.com/damody/gpui-ce-explorer.git",
      "update_branch": "main",
      "rev": "33ed975bf2dff2735eaa21366aa7fa19015c891c",
      "vendor_tree_sha256": "<generated-for-this-snapshot>",
      "release_frozen": false,
      "default_features": false,
      "features": []
    },
    "abi_stable": {
      "package": "abi_stable",
      "version": "0.11.3",
      "checksum_sha256": "69d6512d3eb05ffe5004c59c206de7f99c34951504056ce23fc953842f12c445",
      "publish_vcs_commit": "9966b8f0084fc768e3fb557bf81affea0b5868d8",
      "default_features": false,
      "features": []
    }
  }
}
```

GPUI production fingerprint 的 feature set 為空集合，明確關閉預設 `wayland`、`x11`、`font-kit` 與 `windows-manifest`。SuperExplorer executable 已自行嵌入 Windows manifest，外掛 DLL 不重複加入 GPUI manifest。若測試工具需要 GPUI `test-support`，必須放在獨立 test fixture graph 與 fingerprint 中，不能讓該 feature 混入 production DLL。

P0-0 的必要輸出為：

1. 上述精確 `rust-toolchain.toml`，並驗證 rustc／Cargo commit 與 host triple；不能只驗證顯示版本字串。
2. SDK consumer workspace 的 `Cargo.toml` 與唯一權威 `Cargo.lock`；`abi_stable` 與其他 registry dependency 使用精確 `=` version，GPUI 使用核准 Git URL 加完整 `rev`，或使用 bundle 內與該 rev tree hash 相符的已驗證 path source。
3. 精確 GPUI Git URL、package metadata version、完整 commit、vendor tree hash、空 production feature set、snapshot channel／release freeze 狀態與來源 provenance。
4. 精確 `abi_stable` version、crates.io checksum、空 top-level feature set、衍生巨集版本及其 transitive dependency lock；必須通過 root-module fixture、layout check 與 panic boundary 測試。
5. `.cargo/config.toml`，固定 target、source replacement、offline vendor、linker／rustflags 及允許的 environment input。
6. `sdk-lock.json` 與 `bundle-manifest.json`，以 machine-readable 形式列出上述所有值、檔案 SHA-256、SDK crate versions、build profile、panic strategy、allocator／CRT policy、feature set 與 fingerprint algorithm version。
7. 可離線解析的 `vendor/cargo-sources/`、GPUI vendor tree，以及所有第三方 LICENSE／NOTICE／provenance。
8. `host-fixture` 與 `plugin-fixture` 分開執行 `cargo build --locked --offline` 的相容測試；plugin DLL 必須能由 host 在 callback 前驗證並載入最小 root module 與 GPUI renderer。
9. `build-plugin.ps1`、`validate-plugin.ps1`、`package-plugin.ps1` 與診斷命令；所有官方 Rust 範例及 AI 生成 fixture 只能經這些入口建置。

初始版本選定後，P0-0 產物必須先獨立提交並在 CI 發布為不可變的 development snapshot bundle ID。後續每次 GPUI 更新都建立另一個不可變 snapshot；任何 Rust 官方範例 task 都顯式依賴同一個當前核准 bundle ID，不能各自挑選 Rust、GPUI commit 或 `abi_stable` 版本。

### 10.2 Bundle 固定內容

Bundle 固定：

- 精確 Rust toolchain，而不只是最低 Rust 版本。
- `x86_64-pc-windows-msvc` target。
- 精確 `damody/gpui-ce-explorer` commit；branch 只作 update channel，不作 build identity。
- 精確 `abi_stable` 與相關 features。
- 完整 `Cargo.lock`。
- SDK crate 精確版本。
- 所有影響 public type 的 features。
- panic strategy。
- 正式支援的 build profile 與相關 compiler flags。

`sdk-lock.json` 是作者工具與 CI 使用的依賴／編譯規格來源，`bundle-manifest.json` 保存 bundle 檔案清單、hash、簽章與計算後的 fingerprint；兩者的重疊欄位必須由同一產生器輸出並在驗證時比對，不能手工維護成不同內容。SuperExplorer 的其他內部功能若沒有改變這些輸入，不得任意改變 fingerprint；如此一般應用程式更新不會無故讓 UI 外掛失效。

### 10.3 Cargo.lock 與版本使用規則

- SDK bundle 根目錄的 `Cargo.lock` 是 protected dependency closure 的唯一權威鎖檔，不使用主程式 workspace 的 `Cargo.lock` 代替。受保護集合包含 GPUI、`abi_stable`、公開 SDK crates，以及 `sdk-lock.json` 標記為會影響 ABI、public type 或 renderer layout 的 transitive packages。
- 官方範例及第三方外掛可以有自己的完整 lockfile，以加入 DLL 內部使用的 parser、演算法或其他私有相依；validator 必須用 `cargo metadata --locked` 沿實際 dependency edge 驗證受保護集合的 package ID、source、checksum、features 與 canonical lock 完全一致，不能只搜尋同名 package 是否存在。
- 作者新增的非受保護 dependency 必須精確鎖定、保存 provenance／授權，並提供可重現的 vendor source；它不能讓另一份不同版本 GPUI 或 SDK 型別進入 callback 邊界。
- `cargo update` 若改動受保護集合、自行變更 GPUI Git source／rev、修改 `abi_stable` 版本或移除 `--locked`，官方 validator 必須失敗；只更新外掛私有 dependency 時仍須重新 lock、vendor、測試與發布新的外掛版本。
- 正式 build 使用 `cargo build --locked --offline`；SDK vendor 加上外掛自身 vendor 若缺檔或 checksum 不符應直接失敗，不得回退網路取得另一版本。
- `sdk-lock.json` 的 fingerprint input 至少包含 rustc commit hash、Cargo version、target triple、GPUI tree hash、`abi_stable` 精確 package ID、SDK public crate hash、features、profile、panic、LTO、codegen units、rustflags 與 ABI schema version。
- Loader 比對的是 bundle fingerprint，不綁 SuperExplorer build ID。主程式可在不改變此契約時更新；一旦任一 fingerprint input 改變，就發布新的 bundle ID，舊外掛依既定相容政策載入或被明確拒絕。

### 10.4 GPUI 開發更新與 Release 凍結政策

`damody/gpui-ce-explorer` 是 SuperExplorer 唯一核准的 GPUI source authority。開發期間仍會修改 GPUI，因此採用「浮動 update channel、不可變 build snapshot」模型：

```text
damody/gpui-ce-explorer main 更新
→ update job 解析完整 HEAD commit
→ 產生候選 Cargo.lock、vendor tree、sdk-lock 與 fingerprint
→ 分別建置 host、SDK fixture、八個官方範例
→ 執行 ABI、GPUI callback、UI、效能與封裝測試
→ 全部通過才發布新的 development snapshot bundle ID
→ 宿主與官方範例原子切換到同一 snapshot
```

開發通道規則：

- `main` 可以持續更新；`update-gpui-snapshot.ps1` 或 CI update job 是唯一可更新 protected GPUI rev 的入口。
- Update job 每次先把遠端 `main` 解析成完整 40-character commit，再寫入 `Cargo.toml`、`Cargo.lock` 與 `sdk-lock.json`。一般 `cargo build` 不得自行追蹤 HEAD。
- 每個通過測試的 snapshot 都有新的 bundle ID，例如 `dev-20260731.1+g33ed975b`；已發布 snapshot 永不改寫。
- 更新失敗時保留上一個通過測試的 snapshot，不能留下宿主已升級但 SDK／範例仍使用舊 rev 的半更新狀態。
- 開發中的外掛作者依賴明確 snapshot bundle ID，不直接依賴 `main`。當專案切換 snapshot 時，所有使用 GPUI callback 的外掛必須以新 bundle 重建並重新驗證 fingerprint。
- Snapshot manifest 保存 resolved commit、commit time、parent、repository URL、package metadata version、Cargo git source ID、vendor tree hash、完整 protected dependency graph 與測試結果。
- 即使 `main` 被 force-push 或 commit 日後不可達，已發布 snapshot 仍可由自身 vendor source 離線重建；update job 必須警告非 fast-forward，並在未經明確核准時拒絕切換。

Release／RC 通道規則：

1. 建立 Release Candidate 時停止自動 GPUI snapshot 更新，從最後一個通過完整 CI 的 development snapshot 選定 freeze candidate。
2. 在 `damody/gpui-ce-explorer` 建立對應的受保護 tag，例如 `superexplorer-sdk-v1.0.0-gpui`，並記錄 tag object／commit；實際建置仍以 commit hash 為準。
3. 將 `sdk-lock.json` 的 `release_frozen` 改為 `true`，產生 release bundle ID、簽章、canonical `Cargo.lock`、vendor tree 與最終 compatibility report。
4. 使用乾淨、離線環境重建宿主、SDK fixtures 及八個官方範例，確認產物與 release manifest 一致。
5. Release 發布後，即使 GPUI `main` 繼續更新，該 SuperExplorer 版本仍永久使用凍結 commit。需要 GPUI 修正時發布新的 SuperExplorer／SDK patch bundle，不修改原 release bundle。
6. 若 RC freeze 後仍必須更換 GPUI commit，原 RC bundle 作廢，遞增 RC／bundle ID 並重新執行全部 gate，不能只替換 vendor 內容而保留舊 fingerprint。

Development snapshot 可以有較短支援週期；正式 Release bundle 必須依產品的同一大版本相容政策保存、提供下載與診斷。Loader 錯誤要同時顯示 host bundle ID、plugin bundle ID 及各自 GPUI commit，方便作者判斷是否只是開發 snapshot 不一致。

### 10.5 載入與更新生命週期

- Rust DLL 只在應用程式啟動階段載入。
- 完成註冊後，DLL 保持載入直到程序結束。
- 安裝、更新、替換、移除或啟用本次啟動時未載入的 Rust 外掛後，UI 明確提示需要重新啟動。
- 已載入 Rust 外掛的個別功能透過 runtime gate 停止 dispatch、取消工作並移除 UI contribution；DLL 仍保持載入，不視為熱卸載。
- 不嘗試卸載已有 callback、thread、GPUI element 或 allocator state 的 DLL。
- Fingerprint 不符時，在執行外掛程式碼前拒絕載入，並顯示外掛需要的 bundle 版本。

## 十一、Rust 專用 AI 開發提示詞

`AI_RUST_PLUGIN_PROMPT.md` 用於協助作者將需求交給 Codex 或其他程式助手。Lua 與 Skin 不提供這份提示詞的變體。

提示詞必須要求 AI：

1. 先讀取 machine-readable SDK 描述、manifest schema 與相關範例。
2. 判斷需求需要哪些 registrar 介面，不註冊無關功能。
3. 使用 bundle 內固定的 toolchain、lockfile、GPUI、features 與 build profile。
4. 不自行升級 `abi_stable`、GPUI 或任何 locked dependency；即使開發通道已有較新的 GPUI `main`，仍只使用目前 SDK snapshot 的完整 rev，等待官方 update job 發布新 bundle。
5. 將檔案 I/O、解析與 OCR 等工作放入宿主背景 job，而不是 renderer。
6. 正確處理取消、部分結果、權限錯誤與資料失效。
7. 產生 `manifest.json`、測試、套件結構與本地化資料。
8. 在 manifest 填入作者確認過的 `publisher.id`、顯示名稱與至少一個公開聯絡管道；不得由 AI 虛構 email、Discord、QQ 群、論壇或其他聯絡資料。
9. 若需求使用外部 executable，將精確 target 的工具、hash、版本、來源與授權一起放入 `.sepack/tools/`，使用 `tools.execute_bundled`；不得要求使用者安裝或設定 PATH。
10. 若 Rust 功能使用 parser library，預設將它連結進 `plugin.dll` 並記錄 provenance；不得無故改成需要使用者安裝的外部工具。EXIF 範例必須自包含 parser。
11. 執行官方 build、validate 與 package 指令。
12. 在輸出程式碼前先列出相容性檢查結果。
13. 不引用 SuperExplorer 私有 workspace crate。

提示詞應引用 SDK 內的正式定義與範例，不複製大量容易過期的 API 內容。SDK CI 會使用維護過的 prompt fixture 建立最小欄位 provider 與 GPUI renderer，以驗證提示詞、模板與實際 SDK 沒有脫節。

## 十二、背景工作與資料流

### 12.1 開啟資料夾

開啟資料夾時的順序為：

```text
列舉原生檔案資訊
→ 立即顯示基本列表
→ 配對已啟用的外掛介面
→ 建立背景工作批次
→ 優先處理可見列
→ 繼續處理其餘項目
→ 增量提交結果
→ 更新快取與排序值
→ 批次通知 GPUI renderer
```

主驗收規模為 1,000 個項目。基本檔案列表不得等待外掛完成後才顯示。

### 12.2 Job Scheduler

Job Scheduler 由宿主擁有，提供：

- CPU-bound 與 I/O-bound 兩種佇列。
- 全域並行上限。
- 每個外掛的並行上限，避免單一外掛壟斷資源。
- 目前可見列優先級。
- 資料夾切換、項目刪除、設定變更與程式結束時取消工作。
- 每 16 至 50 毫秒合併一次 UI 更新，避免更新風暴。
- 外掛與項目級的計時、錯誤與診斷資訊。

外掛看到的 provider call 是同步函式，但它由宿主背景 worker 呼叫。ABI 不跨界傳遞 Rust `Future` 或 async runtime。`JobContext` 提供取消旗標、受控檔案讀取、進度回報與增量結果提交。昂貴工作必須定期檢查取消狀態。

### 12.3 結果與排序

`PluginValue` 支援：

- 布林、整數、浮點數。
- Bytes、時間、期間等帶語意數值。
- 文字與本地化顯示資料。
- 圖片、表格、樹狀與時間序列等結構化資料。
- 由外掛擁有的 opaque payload。

宿主使用帶型別的 stable value 進行排序與篩選，不能使用格式化後的字串。例如 `342 GB` 的排序鍵是精確 bytes，不是文字。Opaque payload 只能傳回建立它的同一外掛 renderer，宿主不嘗試解析。

未知值排序時放在固定區域；結果逐步到達時可增量重排。若頻繁重排會造成視覺跳動，宿主可以批次套用排序更新。

## 十三、資料夾大小欄位參考外掛

資料夾大小外掛是整套 SDK 的端到端驗收案例。它使用同一個 Rust DLL 註冊：

- `ColumnProvider`：在背景遞迴計算資料夾大小。
- `ColumnAggregator`：找出目前資料夾內最大的已完成兄弟資料夾。
- `ColumnRenderer`：作者直接用 GPUI 繪製欄位 cell。
- `CommandProvider`：提供「重新計算大小」命令。
- `SettingsProvider`：設定忽略規則、symlink 行為與顯示單位。

### 13.1 計算流程

1. 列表先顯示「計算中」狀態。
2. Scheduler 在背景掃描各子資料夾。
3. Provider 逐筆提交精確 bytes 與掃描狀態。
4. 宿主立即保存數值，使已完成項目可排序。
5. 當兄弟資料夾掃描達到穩定終態後，Aggregator 找出最大值。
6. 宿主通知相關 cell 失效並重繪。
7. Renderer 以 `folder_size / largest_sibling_size` 計算相對比例。

為避免每發現一個更大的資料夾就讓所有長條反覆縮短，第一階段可以先顯示數值，等本輪兄弟掃描穩定後再一次顯示相對長條。

### 13.2 GPUI 創作自由

SDK 不規定資料夾大小一定要畫成藍色進度條。作者可以用同一份資料繪製：

- 水平比例條。
- 圓環或儀表。
- 熱度色塊。
- 迷你長條圖。
- 動畫或 hover 詳細資訊。
- 可互動的重新掃描按鈕。

這正是允許外掛直接使用 GPUI 的主要目的。

### 13.3 檔案系統邊界案例

Provider 必須處理：

- Junction 與 symbolic link 循環。
- 權限不足的子目錄。
- 掃描期間檔案被建立、刪除或移動。
- 使用者切換資料夾後的取消。
- 部分結果與完整結果的區別。
- 極深目錄與長路徑。

宿主區分 `Unsupported`、`Unavailable`、`Cancelled` 與 `PluginError`，避免把正常取消誤報為外掛故障。

## 十四、含完整原始碼的官方範例外掛

SDK 必須隨附八個可直接建置、執行及修改的完整範例。這些範例是 P0 正式交付物，不是文件附錄，也不是只有 README 中的程式片段；每個範例都是正式 `.sepack` 專案，包含來源碼、manifest、本地化、測試、fixture、授權、預覽圖與打包指令。

共同目錄要求如下：

```text
example-name/
├─ README.zh-TW.md
├─ README.md
├─ LICENSE
├─ manifest.json
├─ src/ 或 lua/
├─ locales/
├─ fixtures/
├─ tests/
├─ screenshots/
└─ package.ps1
```

每個 README 必須說明用途、必要權限、使用方法、預期結果、已知限制、相依工具、建置方法、除錯方法，以及使用者如何把範例改成自己的外掛。Rust 範例必須使用正式 toolchain bundle 建置；Lua 範例必須通過 Lua API 與 capability validator。所有範例都納入 CI。

八個範例安裝後都會出現在「資料夾選項／擴充功能」分頁，並至少提供下列功能開關：

| 範例套件 | 可獨立開關功能 |
| --- | --- |
| Rust 資料夾視覺化 | 資料夾大小欄位、重新計算命令、設定頁 |
| Rust tokei | 程式碼行數欄位、欄位渲染、設定頁 |
| Lua tokei | 程式碼行數欄位、設定表單 |
| Lua 大量建立資料夾 | 擴充功能按鈕、命令 |
| Rust EXIF 改名 | 擴充功能按鈕、解碼器、改名命令 |
| Rust 鎖定程序 | 鎖定程序欄位、重新查詢命令 |
| Rust 7z 虛擬資料夾 | 7z 導覽、預覽 stream、修改命令、設定頁 |
| Rust 資料夾 Size Map | Size Map 檢視模式、遞迴掃描、設定頁 |

關閉欄位功能後，該欄位從可選欄位與目前詳細資料檢視移除，但欄寬、順序與其他使用者設定保留，重新啟用後可恢復。關閉命令或按鈕後，對應入口立即從擴充功能區與選單移除。關閉資料 provider 時，相關 renderer 必須同時 blocked；不能留下永遠沒有資料的空 UI。

### 14.1 Rust：資料夾大小視覺化欄位

路徑：`examples/rust-folder-size-visual-column/`

此範例實作前章定義的資料夾大小欄位。它在詳細資料檢視加入「資料夾大小」欄位，以背景工作計算每個子資料夾的遞迴大小，並以目前兄弟資料夾中的最大值作為比例基準。

註冊介面：

- `ColumnProvider`；
- `ColumnAggregator`；
- `ColumnRenderer`；
- `CommandProvider`；
- `SettingsProvider`。

必要能力：

```text
filesystem.read
jobs.background
columns.data
columns.aggregate
columns.gpui_render
commands.register
settings.gpui_render
```

作者直接使用 GPUI 繪製 cell。預設範例外觀參考「上方比例長條、下方容量文字」的形式，但來源碼要清楚分離資料計算與繪圖，讓作者能替換成圓環、熱度圖、動畫或其他設計。

排序鍵使用精確 bytes。掃描中的列顯示 loading；權限不足時顯示 partial／unavailable；取消不顯示為錯誤。測試涵蓋 symlink cycle、junction、權限不足、最大兄弟聚合、快取失效與 1,000 個項目背景處理。

### 14.2 Rust：使用 tokei 計算程式碼行數欄位

路徑：`examples/rust-tokei-code-lines-column/`

此範例在詳細資料檢視加入「程式碼行數」欄位。Rust 外掛使用 toolchain bundle 鎖定的 `tokei` Rust library API 分析檔案，不為每一列啟動外部程序。

註冊介面：

- `BatchColumnProvider`；
- `ColumnRenderer`，用於示範簡單數字與語言標籤；
- `SettingsProvider`，用於選擇顯示 code、comments、blanks 或 total。

必要能力：

```text
filesystem.read
jobs.background
columns.data
columns.gpui_render
settings.gpui_render
```

Provider 以有界批次接收項目，對支援的程式碼檔案回傳語言、code lines、comment lines、blank lines 與總行數。預設欄位的 numeric sort value 是 code lines。二進位檔、未知語言與過大檔案回傳 `Unsupported` 或明確狀態，不以零冒充有效結果。

測試 fixture 至少包含 Rust、C/C++、Python、Lua、JavaScript、空檔案、混合換行、UTF-8、無法解碼內容與未知副檔名。測試必須證明處理 1,000 個檔案時沒有為每個檔案建立 OS process。

### 14.3 Lua：呼叫 tokei 計算程式碼行數欄位

路徑：`examples/lua-tokei-code-lines-column/`

此範例提供與 Rust tokei 範例相同的「程式碼行數」欄位，但使用 Lua `BatchColumnProvider` 與受控的 `tools.execute_bundled` 呼叫套件內 `tokei` CLI，藉此示範 Lua 如何安全整合自行封裝的外部工具。

註冊介面：

- Lua `BatchColumnProvider`；
- Lua `SettingsForm`。

必要能力：

```text
filesystem.read
tools.execute_bundled
columns.data
settings.form
```

Manifest 宣告套件內的 `tokei` 工具、精確版本、`windows-x64` target、相對路徑、SHA-256、JSON 輸出要求、來源與授權。`tokei.exe` 必須實際放在 `.sepack/tools/windows-x64/tokei/`；外掛不得要求使用者另外安裝，也不能使用 PATH、Registry、常見安裝目錄或使用者指定路徑作為替代來源。

Lua 不得取得 `tokei.exe` 的任意路徑，也不得組合 shell command string。它以 manifest `tool_id` 與 argument array 向宿主要求執行，宿主解析成驗證過的 tool handle，要求 JSON 輸出，再將每個檔案的 code lines 映射回穩定 item handle。為避免 1,000 次 process spawn，項目要依命令列長度與數量切成有界批次；單批預設最多 128 個檔案，並同時遵守 Windows command-line 長度、timeout 與輸出大小上限。

若 `tokei` 未安裝、版本不符、JSON 無效、逾時或取消，欄位顯示可診斷狀態。測試使用假的 tokei executable fixture，驗證參數不經 shell、特殊檔名不會注入命令、批次映射正確，並驗證外部程序可在取消時終止及回收。

### 14.4 Lua：大量建立資料夾按鈕

路徑：`examples/lua-bulk-folder-generator/`

此範例在「擴充功能」區域註冊一個「大量建立資料夾」按鈕。按下後由宿主顯示宣告式參數表單，欄位包括：

- 目標父資料夾；
- 名稱前綴；
- 起始編號；
- 建立數量；
- 數字補零寬度；
- 可選後綴；
- 遇到既有名稱時略過、停止或自動改名。

註冊介面：

- Lua `CommandProvider`；
- Lua `ExtensionButton`；
- Lua `ParameterForm`；
- Lua `OperationPlanProvider`。

必要能力：

```text
selection.read
filesystem.create_directory
commands.register
extension_ui.form
```

腳本不直接建立資料夾，而是產生一份 create-directory operation plan。表單送出後，宿主顯示總數、前幾筆與最後幾筆名稱、衝突、估計工作量及目標路徑。數量合法範圍為 1 至 100,000；超過 1,000 時必須進行第二次明確確認。

執行由宿主工作系統分批完成，支援進度、取消與錯誤摘要。Undo 只能移除本次建立且仍為空的資料夾；若使用者已在其中加入內容，復原必須保留該資料夾並列入未復原清單。

測試涵蓋補零、特殊字元、Windows 保留名稱、尾端空白／句點、路徑過長、重複名稱、取消、部分成功與安全 undo。

### 14.5 Rust：依 EXIF 規則批量改名

路徑：`examples/rust-exif-rename-command/`

此範例在「擴充功能」區域加入「依 EXIF 改名」按鈕。使用者輸入文字規則，例如：

```text
{rawname}_{XResolution}x{YResolution}
```

註冊介面：

- `CommandProvider`；
- `ToolbarProvider` 或擴充功能按鈕；
- `SettingsProvider`／參數表單；
- `FileDecoder`，用於讀取 EXIF；
- operation plan 產生器。

必要能力：

```text
filesystem.read
filesystem.rename
jobs.background
commands.register
extension_ui.gpui_render
```

EXIF 解析功能必須編譯進同一個 `plugin.dll`。範例使用由 `Cargo.lock` 固定版本、授權相容且經測試的 Rust EXIF parser library；Cargo 將 parser 及其必要 Rust 相依程式碼連結進 DLL。`.sepack` 不得包含 `exiftool.exe`、`exif.dll` 或第二個專用解析程序，也不能要求使用者安裝 EXIF 軟體、搜尋 PATH、呼叫網路服務或在執行階段下載 parser。

`FileDecoder` 透過宿主提供的 capability-authorized `InputStreamV1` 讀取圖片，該 stream 提供有界 read、seek、length、cancel 與 deadline。外掛在自身 DLL 內完成 TIFF／EXIF 結構解析，再回傳 typed metadata；它不能為了使用 parser 而直接取得未授權路徑或繞過 host job。

EXIF parser 雖然編譯進 DLL，仍須在範例的第三方 provenance、SBOM、README 與 NOTICE 中記錄 crate 名稱、精確版本、來源與授權。它是 `static_rust_library` 類型的 build dependency，不列入 manifest `tools`，也不使用 `tools.execute_bundled`。

範例至少支援 `{rawname}`、`{extension}`、`{XResolution}`、`{YResolution}`、`{PixelXDimension}`、`{PixelYDimension}` 與 `{DateTimeOriginal}`。`XResolution`／`YResolution` 是 EXIF 解析度密度標籤，不一定等於圖片像素寬高；若使用者需要像素尺寸，應使用 `PixelXDimension`／`PixelYDimension`。README 與表單提示必須清楚說明此差異。

改名前先顯示原名稱、新名稱、缺少 tag、非法字元清理、大小寫衝突與重名結果。檔名規則只產生 basename，不能藉由 `..`、斜線或絕對路徑把檔案移出目前資料夾。所有改名交給宿主 operation plan、衝突處理與 undo journal。

測試 fixture 包含有／無 EXIF、rational resolution、Unicode 名稱、相同輸出名稱、缺少 tag、損壞圖片、大小寫衝突與 Windows 保留名稱。整合測試在沒有 `exiftool`、沒有第三方 EXIF DLL、清空測試 PATH 且禁止網路的乾淨環境執行，證明只靠 `plugin.dll` 即可讀取 EXIF。SDK validator 另檢查 PE imports；除 SDK 明確允許的 Windows／runtime 相依外，不得出現未宣告的非系統 DLL。

### 14.6 Rust：顯示鎖定檔案的程序名稱欄位

路徑：`examples/rust-lock-owner-column/`

此範例在詳細資料檢視加入「鎖定程序」欄位。Provider 在背景使用 Windows Restart Manager API 查詢目前使用或鎖定檔案的 process，將 process name 顯示於欄位中。

註冊介面：

- `BatchColumnProvider`；
- `ColumnRenderer`；
- `CommandProvider`，提供手動重新查詢。

必要能力：

```text
filesystem.read_metadata
windows.restart_manager.query
jobs.background
columns.data
columns.gpui_render
commands.register
```

結果資料包含零個或多個 process 的顯示名稱、PID 與可取得的應用程式型別。預設 cell 顯示 process name；多個程序時使用逗號摘要並在 tooltip 或展開 UI 顯示完整清單。此範例只有查詢權限，不得終止 process、關閉 handle 或要求 Restart Manager 關閉應用程式。

Restart Manager 查詢可能受權限、程序退出競態或系統保護限制。無鎖定程序回傳空值而不是錯誤；存取遭拒或查詢失敗則回傳 `Unavailable` 或 `PluginError`。結果採短 TTL，因為鎖定狀態變化快速。

使用者在檔案總管按下 `F5` 重新整理目前資料夾時，宿主必須遞增該位置的 refresh generation、使目前資料夾內所有「鎖定程序」欄位快取失效，並為目前資料夾快照中的項目重新排程背景查詢。畫面可以先顯示 loading／空白狀態，再以增量結果更新可見列，不得阻塞 UI thread。若檔案已解除鎖定，新結果必須清除原有 process name；若出現新的鎖定程序，欄位必須顯示新結果。

連續按下 `F5` 時，前一個 generation 尚未完成的查詢應被取消或允許自然結束，但其結果必須因 generation 不符而被丟棄，不能覆寫較新的資料。切換資料夾、關閉分頁或停用該 feature 也必須取消或忽略尚未完成的查詢。外掛另註冊的「重新查詢鎖定程序」命令與 `F5` 共用同一套 invalidation 與排程路徑，不得形成第二套快取。

測試使用可控 helper process 開啟測試檔案並保持 handle，驗證單一鎖定、多程序鎖定、程序在查詢中退出、存取限制、取消、TTL 失效與 handle／Restart Manager session 正確釋放。整合測試還必須依序驗證「原本未鎖定 → helper 建立鎖定 → 按 F5 顯示程序名稱 → helper 解除鎖定 → 再按 F5 清除程序名稱」，以及快速連按 F5 時舊 generation 不得回填畫面。

### 14.7 Rust：將 7z 壓縮檔當作虛擬資料夾

路徑：`examples/rust-7z-virtual-folder/`

此範例讓使用者像在 Windows 檔案總管開啟 ZIP 一樣，直接雙擊 `.7z` 進入壓縮檔。壓縮檔內的目錄會出現在麵包屑、返回／前進歷史與詳細資料檢視中；其中的檔案可以使用既有欄位、圖示、預覽與複製流程。

範例使用 toolchain bundle 鎖定的純 Rust 7z backend。Backend 必須提供 archive entry 讀取、解壓 stream 與重新寫入 archive 的能力；實際 crate 版本由範例 `Cargo.lock` 固定，不在執行階段下載。官方文件顯示 `sevenz-rust2` 類型的 backend 提供 `ArchiveReader`、`ArchiveWriter`、壓縮、解壓與 AES 功能，因此適合作為第一個 adapter 實作。[sevenz-rust2 文件](https://docs.rs/sevenz-rust2/latest/sevenz_rust2/)

註冊介面：

- `VirtualFolderProvider`；
- `VirtualFileStreamProvider`；
- `VirtualMutationProvider`；
- `FileDecoder`；
- `CommandProvider`；
- `SettingsProvider`。

必要能力：

```text
filesystem.read
filesystem.write
filesystem.replace
filesystem.temp_staging
jobs.background
virtual_folder.enumerate
virtual_folder.open_stream
virtual_folder.mutate
commands.register
```

#### 虛擬位置與導覽

宿主使用結構化 `VirtualLocation`，而不是讓外掛拼接可逃逸的字串路徑：

```text
VirtualLocation
  provider_id
  container_file_id
  container_generation
  entry_id
  normalized_components
```

`.7z` 檔案被視為容器根目錄。雙擊後建立新的導航位置；返回、前進與麵包屑操作使用穩定 `entry_id`。Archive entry 名稱必須正規化，拒絕絕對路徑、磁碟機前綴、`..` 逃逸、NUL 與其他無效元件。兩個正規化後相同的 entry 不得無聲覆蓋。

列舉結果至少包含名稱、虛擬路徑、檔案／資料夾類型、未壓縮大小、壓縮大小、CRC、修改時間、加密狀態與可用操作。詳細資料檢視可用這些欄位排序；未解壓前不需要把全部內容寫入磁碟。

#### 預覽、複製與拖曳

預覽或其他欄位 provider 需要內容時，宿主向 `VirtualFileStreamProvider` 要求有界唯讀 stream。小型檔案可以使用記憶體或 pipe；需要實體路徑的 Windows handler 則解壓到宿主管理的暫存區，並在 preview session 結束後清理。

從 7z 複製或拖曳到一般資料夾時，宿主建立 extract operation plan，先檢查路徑逃逸、目的地衝突、可用空間與宣告的未壓縮大小。解壓工作支援進度與取消，不允許 archive entry 把檔案寫到目的資料夾之外。

#### 加入、刪除、改名與建立資料夾

本範例依使用者選擇支援修改模式，包括：

- 從一般檔案系統加入檔案或資料夾；
- 在壓縮檔內建立虛擬資料夾；
- 刪除 archive entry；
- 重新命名檔案或資料夾；
- 在壓縮檔內移動 entry。

7z mutation 不直接覆寫原檔。每次修改都建立 `ArchiveMutationPlan`，列出新增、保留、刪除、改名、移動、壓縮方法、加密狀態、估計暫存空間與衝突。使用者確認後，外掛在與原壓縮檔相同磁碟區的宿主管理 staging 目錄重建完整新 archive。

交易提交順序為：

1. 取得原 archive 的 file identity、大小與修改時間。
2. 建立 staging archive，逐項複製未變更內容並套用 mutation。
3. 完成 writer、flush，重新開啟並驗證 archive header、entry 數量及可取得的 CRC。
4. 再次確認原 archive identity 與時間沒有被其他程序修改。
5. 建立 undo 所需的原始 archive 備份或可恢復替代資料。
6. 使用同磁碟區原子替換把 staging archive 換成正式 archive。
7. 更新 container generation，使舊 `VirtualLocation` 與快取全部失效。

若取消、空間不足、驗證失敗、原檔已變更或替換失敗，原 archive 必須保持不變；staging 檔案由宿主回收。Undo 以受配額管理的原 archive 備份復原整個容器，而不是嘗試反向重播個別壓縮操作。若 archive 太大而超過 undo 配額，執行前必須清楚告知此次修改不可由應用程式復原，並要求額外確認。

#### 密碼、加密與資源限制

遇到 header 或內容加密時，宿主使用安全密碼提示 UI。密碼以短生命週期 secret handle 傳給外掛，不寫入 manifest、log、錯誤文字或一般設定；第一版預設不永久保存。修改加密 archive 時，必須保留原有加密政策，除非使用者在明確的進階選項中變更。

為防止壓縮炸彈與資源耗盡，讀取及解壓需限制單項未壓縮大小、總輸出、壓縮比、entry 數量、路徑深度、CPU 時間與暫存空間。超出限制時顯示可診斷錯誤，不能只讓程序持續配置記憶體或磁碟。

#### 測試要求

Fixture 至少涵蓋一般 7z、巢狀資料夾、空 archive、空資料夾、Unicode、重複／衝突名稱、solid archive、AES 加密、損壞 header、錯誤 CRC、極深路徑、`..` 路徑逃逸、超大宣告大小與壓縮炸彈模擬。

整合測試必須驗證雙擊進入、返回／前進、麵包屑、排序、預覽、複製解壓、拖入新增、建立資料夾、刪除、改名、移動、取消、原檔競態、低磁碟空間、交易替換、undo、密碼不進入 log，以及失敗時原 archive 位元內容保持不變。

### 14.8 Rust：目前資料夾 Size Map 檢視模式

路徑：`examples/rust-folder-size-map-view/`

此範例在檔案總管既有的「圖示／清單／詳細資料」檢視切換器中註冊新的「Size Map」模式。啟用後，它以階層式 treemap 顯示目前資料夾的完整遞迴內容：矩形面積代表 logical file size，巢狀矩形代表資料夾層級，預設顏色依檔案類型／正規化副檔名分組。資料夾大小為可計入子孫檔案大小的聚合值；空資料夾仍保留可互動的最小視覺標記，但不偽造容量。

註冊介面：

- `ViewModeProvider`；
- `ViewModeRenderer`；
- `SettingsProvider`。

必要能力：

```text
views.register
views.selection
navigation.request
filesystem.enumerate_recursive
filesystem.metadata
jobs.background
settings.register
```

#### 顯示與操作

外掛自行使用固定版本 GPUI 實作 treemap layout、矩形、邊框、文字、hover、tooltip、選取效果、鍵盤焦點與動畫；宿主不提供只能畫固定 treemap 的專用 widget。第一版參考實作使用 squarified treemap，但公開接口不能限制第三方只能使用此演算法。

- 單擊矩形會選取對應檔案或資料夾，並透過 selection bridge 與同一分頁的其他檢視模式共享選取狀態。
- `Ctrl`／`Shift`、方向鍵、Space、Enter、內容選單與 UI Automation 必須有明確且可測試的選取／啟動語意。
- 雙擊資料夾透過宿主 navigation request 進入該資料夾，更新網址列、麵包屑、返回／前進歷史與目前位置；不能只在外掛私有狀態內縮放。
- 雙擊檔案使用宿主既有的安全開啟流程。
- hover 顯示完整名稱、相對路徑、精確 bytes、百分比、類型及掃描狀態。只有矩形足夠大時才直接繪製標籤；極小項目可視覺聚合成「其他」，但資料模型、tooltip、鍵盤導覽與搜尋不能遺失原項目。
- 預設依副檔名／檔案類型使用穩定色盤；無副檔名、資料夾與未知類型有獨立 fallback。設定頁可調整色盤、最小可見面積、標籤密度、動畫、忽略規則、symlink 行為與最大掃描資源，但不能把預設完整遞迴悄悄改成固定淺層掃描。

當使用者停用 Size Map feature，而任一分頁正在使用此模式時，宿主先保存一般 view state，再把受影響分頁切回「詳細資料」或使用者上次使用的內建模式。重新啟用後 Size Map 回到檢視切換器，但不強迫改變目前模式。

#### 掃描與資料流

磁碟列舉與 metadata I/O 不得在 GPUI renderer 或外掛自行建立的無限制執行緒中進行。外掛透過宿主 `DirectoryTreeScanServiceV1` 要求目前位置的完整遞迴快照；宿主執行權限檢查、長路徑處理、取消、deadline、並行與資源配額，並以有界 `DirectoryTreeDeltaV1` 批次回傳節點新增、大小更新、完成、不可存取與移除事件。

每個節點至少包含 opaque item ID、parent ID、顯示名稱、item kind、logical bytes、可選 allocated bytes、修改 generation 與掃描狀態。外掛只能以 owned snapshot／delta 建立自己的 treemap model，不得保存宿主 `ExplorerState`、原生 handle 或內部 GPUI entity。

掃描先回傳直接子項目與已知檔案大小，再逐步深入並向祖先累加；畫面必須在掃描完成前可互動。增量結果由宿主批次節流，外掛可以有界頻率重新計算 layout，不能每發現一個檔案就同步重排整張圖。權限不足、競態刪除或部分失敗顯示為 partial，不得讓整個檢視消失。

預設不跟隨 directory symlink／junction；使用者明確啟用跟隨時，仍須以 file identity 偵測 cycle。Hard link 的預設統計語意是每個目錄項目的 logical size，設定頁可切換為同一 volume file identity 只計一次；目前使用的語意必須出現在 tooltip／圖例。所有規則與資料夾大小欄位共用宿主掃描政策型別，但兩個範例不能藉此引用彼此的私有程式碼。

#### F5、位置與過期結果

按下 `F5` 時，宿主遞增目前 location 的 refresh generation、取消或作廢舊掃描、清除不再可信的聚合結果並開始新掃描。每個 request、delta 與 layout commit 都攜帶 location ID 與 refresh generation；舊 generation 即使較晚完成也不得覆寫新畫面。切換資料夾、切換離開 Size Map、關閉分頁或停用 feature 時，同樣取消或忽略尚未完成的工作。

檔案系統 watcher 事件可以使受影響子樹失效並觸發有界增量更新，但 watcher 不取代 F5 的完整重新整理語意。掃描快取可以由宿主在資料夾大小欄位與 Size Map 間重用，但快取 key 必須包含掃描政策、location identity 與 generation，且不能讓其中一個外掛取得另一個外掛的 opaque state。

#### 測試要求

Fixture 至少包含空資料夾、單一巨大檔案、許多微小檔案、深層樹、寬樹、Unicode、長路徑、無副檔名、大小寫不同副檔名、權限不足、symlink／junction cycle、hard link、掃描中建立／刪除／改名與超過可視節點上限的目錄。

單元測試驗證 bytes 聚合、百分比、穩定類型色盤、squarified layout 不重疊且不越界、小項目聚合仍可存取，以及 selection hit testing。整合與 UITEST 驗證檢視切換器註冊、完整遞迴增量顯示、UI thread 不做 I/O、單擊共享選取、雙擊資料夾更新正式導覽歷史、雙擊檔案、鍵盤與 UI Automation、F5 新 generation、快速連續刷新、取消、partial error、停用時 fallback，以及從乾淨 consumer workspace 建置與封裝。

### 14.9 範例的第三方依賴與授權

使用 `tokei`、EXIF parser 或其他第三方 crate／工具時，範例必須：

- 由 bundle 或範例 lockfile 固定版本；
- 保存來源、版本與授權 provenance；
- 在 README 說明是 library dependency 還是外部 executable；
- 若為外部 executable，必須把每個支援 target 的實際執行檔封裝進 `.sepack/tools/<target>/<tool-id>/`；
- 不允許安裝時、啟用時或執行時從網路下載工具；
- 不允許以系統 PATH、Registry、常見安裝路徑或使用者選檔取代套件內工具；
- Package Manager 必須在安裝與每次套件內容改變後驗證大小與 SHA-256，執行前由 Tool Resolver 驗證 handle 仍指向同一個已核准檔案；
- 將必要的 NOTICE／LICENSE 隨範例或套件提供。

Rust library dependency 與外部 executable 必須清楚區分：

- `static_rust_library`：來源碼在建置時連結進 `plugin.dll`；執行時不需要額外 executable 或專用 DLL。EXIF parser 屬於此類。
- `bundled_executable`：獨立程序，必須放在 `.sepack/tools/<target>/<tool-id>/`，並透過 Tool Resolver 執行。Lua tokei 屬於此類。

作者不能把本來應編譯進 Rust DLL 的必要解析功能漏掉，再把責任轉嫁給使用者電腦上的工具。反之，若選擇獨立 executable 架構，就必須完整遵守 bundled tool 規則，不能假裝是 Rust library dependency。

工具執行檔是套件簽章與內容 hash 的一部分，不能獨立更新。作者升級 `tokei` 或另一個外部工具時，必須發布新的套件版本，重新提供 provenance、授權、測試與簽章。若防毒軟體隔離或刪除工具，功能狀態變為 `Unavailable` 並提供修復／重新安裝說明，不得靜默改用另一份系統程式。

Rust tokei 範例示範 library 整合；Lua tokei 範例刻意示範受控外部 CLI 整合。兩者的功能重疊是有意安排，用來比較 Rust 與 Lua 擴充能力、效能與權限差異。

## 十五、結果快取與失效

快取鍵至少包含：

```text
package_id
+ interface_id
+ plugin_data_version
+ file_identity
+ file_size
+ modified_time
+ query_or_options_hash
```

一般檔案欄位可以依檔案身分、大小與修改時間判斷失效。遞迴資料夾大小不能只依賴頂層資料夾的修改時間，因為子孫變動不一定可靠更新頂層時間。

第一階段採：

- filesystem watcher 事件使相關結果失效；
- 較短的 TTL 避免長期保存過期資料；
- 使用者手動「重新計算」命令；
- 外掛資料版本變更時，只清除該外掛不相容的結果。

未來可在相同 invalidation 介面後加入 NTFS USN Journal，不需要改變外掛 API。

## 十六、Lua 自動化

Lua runtime 預設沒有任意檔案系統、網路或程序 API。腳本只能使用宿主核發的 capability。

第一版能力包括：

```text
selection.read
filesystem.read
filesystem.rename
filesystem.create_directory
filesystem.delete
tools.execute_bundled
network.request
```

Lua 不只可以執行一次性命令，也可以經由受限的 Lua Registrar 註冊：

- 純資料欄位與批次欄位 provider；
- 擴充功能頁面、工具列或命令選單按鈕；
- 由宿主渲染的參數表單；
- 命令與 operation plan 產生器。

Lua 欄位回傳數字、文字、時間、Bytes 或其他宿主已知的 stable value，並使用宿主內建 cell renderer。Lua 不能直接建立 GPUI element；任意 GPUI 繪製仍是 Rust 外掛能力。

`.sepack` 內的 Lua 外掛不能提交任意 executable path。它只能呼叫 `tools.execute_bundled(tool_id, arguments, options)`；宿主以 Tool Resolver 將 `tool_id` 解析為套件內已驗證的 opaque tool handle。呼叫仍使用 argument array、working directory policy、timeout、最大 stdout／stderr bytes 與取消 token，且不經過 `cmd.exe` 或 PowerShell。現有本機自動化腳本的低階 process API 屬於不同信任模式，不會暴露給可散布的 Lua 外掛套件。

套件安裝時顯示所需能力。刪除、執行外部程序與網路等敏感能力可以再次要求確認。

批量改名、批量建立資料夾、移動或刪除不由 Lua 直接呼叫作業系統。腳本先建立 operation plan，宿主顯示預覽、衝突與預期結果；使用者批准後，交給既有 file-operation、取消、衝突處理與 undo journal 執行。

Lua 不得取得 SuperExplorer 私有 model reference，也不能繞過宿主操作路徑。

## 十七、視覺 Skin

Skin 是純資料套件，可以修改：

- 圖片與背景材質。
- 檔案、資料夾與工具列圖示。
- 字型與字重。
- 按鈕 normal、hover、pressed、disabled 等狀態圖。
- nine-slice 與向量路徑。
- 色彩、間距、圓角、陰影與密度。
- 背景透明度、模糊或壓克力參數。
- 不規則視覺外框與透明點穿遮罩。

Windows 視窗幾何仍為可縮放矩形，以保留 Snap、最大化、多螢幕與 DPI 行為。Skin 的不規則外觀由透明區域與 hit-test mask 達成，而不是建立真正的不規則作業系統視窗。

下列行為由宿主保留：

- 鍵盤焦點與快捷鍵。
- 無障礙語意。
- resize handle。
- 標題列拖曳及 Windows 視窗命令。
- 核心操作按鈕的安全 fallback。

若 Skin 缺少或損壞某個資產，只針對該資產回退到預設 Skin；不能因一個錯誤就讓整個 UI 無法操作。Skin 不執行 Rust、Lua、JavaScript 或任意原生程式碼。

## 十八、錯誤處理與 Safe Mode

### 18.1 Typed Outcome

- `Unsupported`：此介面不適用於該項目，不顯示為錯誤。
- `Unavailable`：權限不足、檔案鎖定或缺少選擇性資料。
- `Cancelled`：正常取消，不計為故障。
- `PluginError`：外掛可恢復錯誤，記錄診斷並顯示簡短狀態。
- `Incompatible`：SDK、layout 或 fingerprint 不符，在載入前拒絕。

### 18.2 Panic 與程序級錯誤

可安全捕捉的 FFI 入口要攔截 Rust panic，轉換為 `PluginError`。但 access violation、記憶體破壞、死鎖或破壞 GPUI state 仍可能終止或卡住整個 SuperExplorer，因為 Rust DLL 是程序內執行。

宿主在每次原生外掛呼叫前寫入精簡的 plugin call marker，正常返回後清除。若上次程序結束時 marker 仍存在，下次啟動提供 Safe Mode，顯示疑似 package ID、interface ID 與操作，並先停用該外掛直到使用者確認。

簽章與人工審核只能降低風險，不能形成 sandbox。這項限制必須清楚寫入 SDK 與未來商店說明。

## 十九、產品分級

### 19.1 基礎版：2.99 美元

基礎版包含完整平台能力：

- 檔案管理核心。
- Rust 外掛載入與 GPUI 擴充介面。
- UI Plugin SDK toolchain bundle。
- Rust 專用 AI 提示詞。
- Lua 巨集。
- Skin。
- 未來的 Workshop 瀏覽與安裝能力。

第三方作者不需要購買 Pro 才能開發或使用套件。

### 19.2 Pro Toolkit：9.99 美元

Pro Toolkit 規畫為第一方專業解析器合集：

- Perfetto 欄位、事件、時間軸與預覽。
- trace-cmd 解碼與 CPU frequency 等欄位。
- Word metadata 與文件預覽。
- XLSX metadata、工作表與表格預覽。
- 本機 OCR、圖片文字選取與複製。

上述功能以公開 Rust SDK 製作，不能依賴只有主程式內部能使用的特殊 API。它們同時作為正式範例、效能基準與相容性 fixture。

第一階段可以開發這些 first-party 外掛，但不實作 Steam DLC entitlement。Entitlement Provider 先提供本機或測試實作，Steam 版本留到後續規格。

## 二十、未來 Steam 規畫

本章只定義未來方向，不授權第一階段加入 Steamworks 依賴或上架流程。

### 20.1 免費內容

- 免費 Lua 與 Skin 使用 Ready-To-Use Workshop 的 Community item。
- 作者上傳後，其他使用者可直接訂閱、下載與更新。
- 即使內容免費，仍須通過 manifest、schema、hash 與權限檢查。

### 20.2 Rust 內容

- 所有 Rust 外掛無論免費或付費，都不能讓作者上傳未審核 DLL 後直接執行。
- 作者提交原始碼、manifest、預覽與測試。
- 官方 CI 使用指定 bundle 重建、驗證 fingerprint、執行測試並簽署。
- 免費 Rust 外掛可以在審核後以免費官方簽署內容提供。
- 付費 Rust 外掛使用 Curated Workshop、Inventory 或 item service、payment rules 與應用程式內商店。

### 20.3 Pro DLC

- Pro Toolkit 未來使用 Steam DLC App ID。
- Steam Entitlement Provider 查詢所有權與安裝狀態。
- 短暫離線時使用最近一次有效的本機 entitlement 狀態，詳細期限與退款同步規則另行設計。

### 20.4 後續獨立規格

Steam 實作開始前，必須建立另一份經核准的規格，涵蓋：

- Workshop 實際 configuration 與 item type。
- 上傳器與審核後台。
- 作者合約、分潤、稅務與退款。
- Inventory 與 Microtransactions。
- DLC ownership、離線與撤銷。
- Steam Client 尚未下載完成時的啟動行為。
- Workshop 更新與 Rust 外掛需要重新啟動的 UX。

## 二十一、測試與驗收

### 21.1 套件與相容性

- P0-0 CI 必須驗證 `rust-toolchain.toml`、`.cargo/config.toml`、canonical `Cargo.lock`、`sdk-lock.json`、`bundle-manifest.json`、GPUI tree hash 與 `abi_stable` package ID 一致，任何未重新產生 bundle ID 的漂移都失敗。
- 將 host fixture 與 plugin fixture 放在隔離目錄分別執行 `cargo build --locked --offline`；使用全新的隔離空 `CARGO_HOME` 且禁止網路時仍須可建置，並確認 plugin 不會意外解析到主 workspace dependency。測試不得刪除或修改開發者既有 Cargo cache。
- 對 Rust、GPUI、`abi_stable`、SDK crate、features、profile、panic、rustflags 與 target 各做一項單因子變更，確認 fingerprint 變化且 loader 在第一個 callback 前給出可診斷的不相容錯誤。
- GPUI update-channel 測試以兩個可控 commit 模擬 `main` 前進，驗證 update job 產生不同 snapshot bundle ID、原子更新宿主／SDK／八個範例，且任一步驟失敗時仍保留上一個核准 snapshot。
- 模擬非 fast-forward／force-push，確認沒有明確核准時拒絕更新；既有 snapshot 使用自身 vendor source 仍可離線重建。
- Release freeze 測試在 `release_frozen = true` 後移動遠端 `main`，確認 release rebuild 仍只使用凍結 commit；更換 commit 必須產生新 RC／bundle ID 並重跑完整 gate。
- 測試 manifest schema、hash、相依關係、版本選擇與循環相依。
- 驗證 publisher ID、至少一筆聯絡方式、`support`／`security` 用途、各類型格式、URL scheme、欄位長度，以及簽章發行者與 manifest 發行者是否一致。
- 保存以舊版 SDK 1.x 編譯的非 GPUI fixture DLL，驗證相容載入。
- 驗證相同 fingerprint 的 GPUI 外掛可以載入。
- 分別改變 toolchain、target、GPUI、features、panic strategy、SDK 或 profile，確認載入器在 callback 前拒絕。
- 驗證宣告能力與實際註冊不一致時拒絕整個套件。
- 驗證 bundled tool 缺檔、錯誤 target、大小／SHA-256 不符、缺少授權、絕對路徑、`..`、symlink、junction 與 reparse-point 逃逸都會在 callback 前阻擋相關 feature。
- 驗證 Tool Resolver 不查詢 PATH、Registry、常見安裝目錄、網路或使用者選檔，且套件 generation 改變後舊 tool handle 失效。
- 驗證 Rust 外掛安裝、更新、替換、移除或啟用未載入 DLL 時提示重新啟動；已載入功能停用成功時不得錯誤要求重啟。
- 驗證「資料夾選項／擴充功能」第三分頁、總開關、搜尋／篩選、套件開關、個別 feature 開關、作者聯絡方式及狀態標記。
- 驗證套用、確定、取消與右上角關閉的 draft／saved state 語意。
- 驗證父層關閉後保留子功能 desired state，重新開啟父層時恢復個別選擇。
- 驗證 Lua 與 Skin 可即時切換、已載入 Rust 可經 runtime gate 停止、未載入 Rust 啟用時顯示 `pending_restart`。
- 驗證停用欄位時保存欄寬與順序、停用資料 provider 時 renderer 轉為 blocked、停用 Virtual Folder 時安全離開受影響分頁。
- 驗證相依功能、進行中工作、逾時 callback、Safe Mode 與不相容套件不會形成半啟用狀態。

### 21.2 效能與工作排程

- 使用 1,000 項目的資料夾 fixture。
- Size Map 另使用至少 100,000 個節點的合成樹，驗證增量 scan delta、layout 節流、取消延遲、記憶體上限與 UI 可互動性；測試不能真的建立 100,000 次同步 GPUI 重繪。
- 確認基本列表不等待擴充資料完成。
- 確認檔案 I/O 與解析不在 GPUI thread 執行。
- 記錄初始可互動時間、工作完成時間、取消延遲與 UI 批次更新次數。
- 驗證可見列優先，之後完成其餘項目。
- 建立慢 renderer fixture，確認診斷能定位 package 與 interface。

### 21.3 含原始碼範例外掛

- 八個範例都必須包含來源碼、雙語 README、manifest、授權、fixture、測試、預覽圖與打包指令。
- Rust 資料夾視覺化欄位驗證遞迴大小、cycle、部分結果、取消、bytes 排序、兄弟聚合、自訂 GPUI renderer 與快取失效。
- Rust tokei 欄位驗證多語言 fixture、code／comments／blanks、numeric sorting、批次處理，以及不建立每檔案 OS process。
- Lua tokei 欄位驗證套件確實包含 `tokei.exe` 與授權、Tool Resolver 不搜尋 PATH、hash／target 驗證、JSON 解析、argument array、防止 shell injection、命令列長度分批、timeout、取消與 child process 回收。
- Lua 大量建立資料夾驗證參數表單、1 至 100,000 數量、超過 1,000 的再次確認、命名清理、衝突、部分成功與安全 undo。
- Rust EXIF 改名驗證 parser 已連結進 `plugin.dll`、PE import allowlist、InputStreamV1、離線／空 PATH、token、rational tag、像素尺寸 tag、缺少 metadata、命名清理、衝突預覽與 undo journal。
- Rust 鎖定程序欄位使用 helper process 驗證單一／多重鎖定、程序退出競態、Restart Manager 資源釋放、短 TTL、F5 重新查詢、解除鎖定後清除舊值，以及連續刷新時拒絕 stale generation。
- Rust 7z 虛擬資料夾驗證導覽、stream 預覽、複製解壓、加入、建立資料夾、刪除、改名、移動、加密、限制、交易替換、競態、失敗不破壞原檔與整包 undo。
- Rust Size Map 驗證完整遞迴增量掃描、bytes 聚合、treemap layout、類型配色、共享選取、正式雙擊導覽、鍵盤／UIA、F5 generation、取消、partial error、檢視 fallback，以及 renderer 不執行檔案 I/O。
- 每個範例的 manifest capability 必須與執行期間實際使用的能力完全一致。

### 21.4 Lua

- 未授權 capability 必須失敗。
- Lua 可以註冊純資料欄位、批次欄位、擴充按鈕與宿主參數表單，但不能回傳任意 GPUI element。
- `tools.execute_bundled` 必須只接受 manifest `tool_id` 與 argument array，不接受 executable path，也不得經由 shell 字串執行。
- 批量改名與建立資料夾先顯示 operation plan。
- 衝突、取消與 undo 可正確工作。
- Lua 無法直接取得任意 filesystem、process 或 network API。

### 21.5 Skin

- 不同 DPI 與高對比模式。
- 透明背景與點穿遮罩。
- Snap、最大化、resize 與多螢幕。
- 鍵盤焦點與無障礙。
- 缺少、損壞或過大資產的逐項 fallback。

### 21.6 崩潰復原與 AI Prompt

- 使用合成 call marker 驗證下次啟動 Safe Mode。
- 停用疑似外掛後能正常啟動。
- SDK release gate 使用維護過的 AI prompt fixture 建立最小 provider 與 GPUI renderer。
- AI 產出的 fixture 必須不修改 locked dependency，並通過 build、validate 與 package。

Steam 測試不在本階段執行；未來 Steam 規格開始實作時才成為必要驗收。

## 二十二、範例驅動的實作介面規格

本章是第一階段的規範性實作清單。八個官方範例所依賴、但目前程式碼尚未提供的功能與介面，都必須先成為正式平台能力，不能只寫在範例內部、以私有 API 繞過，或用無法替換的臨時 stub 假裝完成。

### 22.1 目前程式碼可重用的基礎

截至 2026-07-31，現有專案具備下列可重用能力：

- `explorer-jobs` 已有 bounded endpoint、優先級與有界 priority queue，可作為外掛工作排程器的基礎。
- `explorer-automation` 已有受限 Lua 5.4 VM、記憶體／CPU 限制、事件、hotkey、schedule、watch 與 host adapter 架構。
- Lua `ProcessHost` 已使用 executable 與 argument array，拒絕直接以 shell host 執行一般命令，並具 timeout 與 stdout／stderr 大小限制。
- `explorer-model` 已有檔案操作 request、衝突處理與保守的 undo journal，可作為 Extension Operation Plan 的提交端。
- `explorer-shell-win::restart_manager` 已封裝 `RmStartSession`、`RmRegisterResources`、`RmGetList` 與 `RmEndSession`，可重用為鎖定程序查詢服務。
- `explorer-ui` 已有資料夾選項對話框、draft、Apply／OK／Cancel transition，以及「一般／檢視」兩個分頁。
- `explorer-extension-protocol` 與 broker 已提供程序外 Windows Shell extension 的有界協定，但它不等於新的程序內 Rust Plugin API。

### 22.2 目前確認缺少的基礎

下列能力目前不存在，必須列入實作：

- Workspace 尚未加入 `abi_stable`，也沒有公開 Rust root module、registrar 或 DLL loader。
- 主 workspace 仍是舊的 `rust-version = 1.85.0` 與 GPUI vendor commit `0cd06bd8cc469e606e2bbf0d82679c88cfe8a951`；必須遷移到 P0-0 已指定的 Rust `1.97.1`、`damody/gpui-ce-explorer` development snapshot、`abi_stable 0.11.3`，並新增作者用 canonical lock、resolved Git rev、vendor tree hash、offline Cargo source 與 host／plugin compatibility fixture。
- 尚無 `.sepack` manifest parser、publisher contact、hash、簽章、相依關係、Package Manager 或 feature state store。
- 詳細資料欄位目前由固定 `SortColumn` enum、固定 `DetailsColumnWidths` 欄位與 `u16` visibility bitmask 表示，不能容納動態外掛欄位。
- 尚無 `PluginValue`、批次欄位 provider、跨列 aggregator、外掛排序值或 GPUI cell renderer registry。
- `explorer-jobs` 只有基礎佇列，尚無 per-package quota、worker lifecycle、取消 token、增量結果、UI batching 或 slow callback diagnostics。
- Lua registration phase 目前只有事件、hotkey、schedule 與 watch，沒有欄位、按鈕、命令、參數表單或 operation plan registrar。
- 現有 Lua process host 雖具 timeout，但 Future 被丟棄時不能保證終止並回收 child process；也沒有 manifest 驗證過的 Tool Resolver。
- 現有 `FileHost` 只有 read、write、remove，沒有大量建立資料夾、批次改名、預覽、衝突策略與 undoable operation plan 公開介面。
- 資料夾選項只有 `General`／`View`，`FolderOptionsDraft` 目前是固定且可 `Copy` 的小型資料，不能直接承載動態套件清單、搜尋與 desired／effective state。
- 尚無 Virtual Folder、Virtual File Stream、Virtual Mutation、container generation、archive staging transaction 或 secure secret handle。
- 現有檢視模式與目前位置／選取／導覽狀態尚未提供外掛 registrar；也沒有可向外掛安全傳遞遞迴目錄樹、增量大小、refresh generation 與 partial scan state 的服務。
- 尚無 UI Plugin SDK toolchain bundle、fingerprint、外部 consumer build、AI prompt validator 或八個範例的 CI runner。

### 22.3 必須新增的 crate 與責任邊界

第一階段至少新增下列 workspace crate；名稱一旦進入公開 SDK，就依同一大版本相容規則維護：

| Crate | 責任 |
| --- | --- |
| `explorer-extension-api` | `abi_stable` root module、registrar、stable value、provider、command、form、job、錯誤與 host-service 介面 |
| `explorer-extension-ui-api` | 固定 toolchain 的 GPUI renderer context、公開 UI helper、fingerprint metadata；不得依賴 `explorer-ui` 私有型別 |
| `explorer-extension-host` | `.sepack`、manifest、Package Manager、DLL loader、feature gates、registries、job dispatch、cache、Safe Mode 與診斷 |

`explorer-model`、`explorer-jobs`、`explorer-automation`、`explorer-shell-win` 與 `explorer-ui` 依後續小節擴充，但不能反向依賴範例套件。八個範例放在 SDK bundle 的獨立 consumer workspace；它們不得成為主程式 workspace member，藉此證明外部作者只靠公開 SDK 與 bundle 就能建置。

### 22.4 套件、功能與載入介面

`explorer-extension-host` 必須實作：

- `PackageManifestV1`：package、publisher、contacts、版本、entry point、feature、capability、tool、dependency、hash、signature 與 data version。
- `PackageId`、`PublisherId`、`FeatureId` 與 `InterfaceId`：正規化、可序列化且有長度限制的穩定 ID。
- `PackageSource`：第一階段提供 built-in 與 local developer 實作；Steam 與 Pro 只保留 trait。
- `PackageResolver`：版本選擇、相依圖、循環偵測、hash、簽章與整包拒絕。
- `FeatureStateStore`：保存 global、package 與 feature 的 desired state。
- `EffectiveFeatureResolver`：結合相依關係、權限、相容性、Safe Mode 與 runtime 狀態計算 effective state。
- `ContributionGate`：每次 dispatch 前檢查 feature 是否仍有效；停用時阻止新 callback。
- `PluginLoader`：使用 `abi_stable` 驗證 root module；GPUI contribution 額外驗證完整 fingerprint。
- `PluginCallGuard`：建立／清除 call marker、捕捉可恢復 panic、量測 callback、禁止卸載仍 resident 的 DLL。

Manifest 中每個 registrar contribution 都必須攜帶 `feature_id`。Host 驗證該 feature 已宣告相對應 capability，否則拒絕整個套件。

### 22.5 工作排程與資料介面

`explorer-jobs` 與公開 API 必須新增：

- `ExtensionJobScheduler`：CPU／I/O queue、global limit、per-package limit 與 visible-row priority。
- `ExtensionJobId`、`JobGeneration` 與 `CancellationTokenV1`。
- `JobContextV1`：取消查詢、進度、受控檔案讀取、增量結果與安全診斷。
- `IncrementalResultSinkV1`：以有界批次提交結果，具 backpressure 與 generation 檢查。
- `ExtensionJobTerminal`：completed、partial、cancelled、unsupported、unavailable、failed。
- `UiInvalidationBatcher`：在 16 至 50 毫秒內合併同一視窗／欄位的 invalidation。
- `ExtensionTimingStats`：queue、provider、renderer、aggregation 與取消延遲。

資料介面必須包含 `PluginValueV1`、`StableSortValueV1`、`PluginErrorV1`、`ItemHandleV1` 與 `ItemSnapshotV1`。Handle 包含 generation，不能在導航或檔案身分改變後誤用。

檔案 decoder 另使用 `InputStreamV1`，由宿主依 `filesystem.read` capability 建立。它提供有界 read、可選 seek、length、deadline、cancel 與來源 generation，不把任意檔案路徑或 OS handle 暴露給外掛。EXIF parser、預覽 decoder 與自訂格式 parser 都共用此介面。

### 22.6 動態詳細資料欄位

為支援資料夾大小、兩個 tokei 與鎖定程序範例，固定欄位模型必須改造成可擴充模型：

- `ColumnId` 可表示 built-in ID 或 `(package_id, feature_id, local_column_id)`。
- `ColumnDescriptorV1` 包含名稱、值型別、預設寬度、對齊、支援項目、排序語意與 provider 成本。
- `ColumnProviderV1` 處理單項工作。
- `BatchColumnProviderV1` 處理有界項目批次並按 item handle 回填。
- `ColumnAggregatorV1` 接收同一資料夾的 typed snapshot，產生最大值或其他 group result。
- `ColumnRendererRegistrationV1` 關聯 GPUI renderer factory 與 feature gate。
- `ColumnRenderContextV1` 提供 value、aggregate、loading／error state、selected、hovered、geometry、DPI、theme facade 與 invalidation handle。
- `DynamicColumnLayout` 以 map／ordered list 保存寬度、順序與可見性，不再依賴 `u16` bitmask 表示全部欄位。
- Session persistence 必須遷移既有固定欄位資料，未知外掛欄位設定保留但不顯示，重新安裝後可恢復。
- Sort pipeline 必須使用 `StableSortValueV1`，對 pending、unavailable 與 error 提供固定順序。

詳細資料 header、row virtualization、欄位選單、resize、horizontal scroll、keyboard／UIA 與 session restore 都必須改用動態欄位 registry。不能只在畫面尾端臨時附加一個無法排序或保存寬度的文字欄。

### 22.7 GPUI contribution 介面

`explorer-extension-ui-api` 必須提供：

- `GpuiColumnRendererV1`、`GpuiViewModeRendererV1`、`GpuiPreviewRendererV1`、`GpuiPanelFactoryV1`、`GpuiSettingsFactoryV1` 與 `GpuiToolbarFactoryV1`。
- 只包含公開 snapshot 的 context；不得出現 `ExplorerState`、內部 `Entity<T>` 或私有 action enum。
- `ExtensionActionSink`：renderer 以公開 command／feature action 回報事件，不直接修改內部 model。
- `ExtensionThemeFacade`：提供顏色、字型、間距與高對比語意，而非暴露可被外掛永久保存的內部 theme reference。
- `ExtensionInvalidationHandle`：只允許請求自身 contribution 重繪。
- Renderer lifecycle：create、render、focus／blur、close 與 drop 順序。
- UI thread assertion、callback timing、panic boundary 與 Safe Mode marker。

資料夾大小 renderer 與 Rust tokei renderer 必須只使用這些公開介面完成，不能加入僅供官方範例使用的後門。

### 22.8 命令、擴充按鈕、表單與 Operation Plan

Lua 大量建立資料夾與 Rust EXIF 改名需要共用下列公開介面：

- `CommandDescriptorV1`：ID、名稱、圖示、位置、選取條件、快捷鍵與 feature ID。
- `ExtensionButtonDescriptorV1`：擴充功能區、工具列或 context menu placement。
- `FormSchemaV1`：text、integer、boolean、choice、path、template 與 validation message。
- `FormValueV1` 與 `FormSubmissionV1`：有界 typed value，不傳任意 GPUI state。
- `OperationPlanV1`：create directory、rename、copy、move、delete、extract、archive mutation 等 typed step。
- `OperationPreviewV1`：變更前後、衝突、不可復原原因、警告與估計工作量。
- `OperationPlanValidator`：路徑正規化、保留名稱、逃逸、重複目標、權限、數量與大小限制。
- `OperationPlanExecutor`：轉接既有 file-operation pipeline，提供 progress、cancel、partial terminal 與 undo journal。

大量建立資料夾要求新增批次 `CreateDirectoryStep` 與「只刪除仍為空的本次建立資料夾」undo 規則。EXIF 改名要求新增 `FileDecoderV1`、`InputStreamV1`、typed metadata map、template parser、metadata token value、basename sanitizer、case-insensitive collision graph 與 batch rename preview。

### 22.9 Lua Registrar 與受控工具執行

`explorer-automation` 必須在現有 registration phase 增加：

- `register_column` 與 `register_batch_column`；
- `register_command` 與 `register_extension_button`；
- `register_form`；
- `register_operation_plan`；
- 每筆 registration 的 feature ID 與 capability 驗證；
- 對應的 immutable descriptor 與 Lua registry callback。

Lua runtime 必須重用公開 `PluginValueV1`／`OperationPlanV1` 的 serde mirror，不能建立另一套不相容的欄位或操作語意。

現有 `ProcessHost` 必須擴充為：

- `BundledToolDescriptorV1`：tool ID、target、套件內相對路徑、精確版本、檔案大小、SHA-256、輸出協定、來源、授權與 NOTICE 路徑。
- `ToolPackageValidator`：確認工具檔案存在於 package root 內，拒絕絕對路徑、`..`、symlink、junction、reparse-point 逃逸、target 不符、大小不符、hash 不符與缺少授權。
- `ToolResolver`：只解析當前套件 manifest 宣告且已驗證的 bundled tool；不得查詢 PATH、Registry、常見安裝目錄、網路或使用者選擇的替代檔案。
- `ToolHandleV1`：opaque、package-scoped、帶 package generation 的執行權杖；Lua 看不到實體 executable path。
- `ProcessRequestV2`：使用 `ToolHandleV1` 而非任意 executable；另包含 arguments、受控 cwd、environment allowlist、stdin policy、timeout、output limits 與 cancellation token。
- `ProcessLease`：Future 被取消、feature 被停用或資料夾切換時，終止並回收 child process；Windows 上以 Job Object 管理子程序樹。
- `ProcessTerminal`：exit、timeout、cancelled、spawn failed 與 output truncated 分開表示。
- 工作目錄與 argument 中的路徑必須來自 capability-authorized handle 或經驗證路徑。

Lua tokei 範例只能使用上述 Tool Resolver、`ToolHandleV1` 與 `ProcessRequestV2`，不能直接呼叫現有低階 `NativeProcessHost`、取得 executable path、自行搜尋 PATH 或要求使用者安裝工具。Package 更新後，舊 tool handle 因 package generation 改變而失效。

### 22.10 鎖定程序查詢服務

Rust 鎖定程序欄位不得複製另一份 Restart Manager 實作。`explorer-shell-win` 現有 adapter 必須包裝成公開、唯讀、受限的 `LockOwnerQueryServiceV1`：

- 輸入為最多固定數量的 capability-authorized item handles。
- 輸出為 owned `LockOwnerRecordV1`，包含 PID、process／service display name、應用程式型別與可安全公開的狀態。
- 每次查詢有 deadline、取消、最大結果數與 session cleanup。
- 不公開 native handle。
- 查詢請求與回傳都攜帶目前 location／item snapshot 對應的 refresh generation；宿主只接受仍屬於目前 generation 的結果。
- 資料夾的 `F5` refresh event 必須接到動態欄位的通用 cache invalidation 與 reschedule 管線，`LockOwnerQueryServiceV1` 不自行攔截鍵盤事件。
- 不提供 shutdown、terminate 或 close-handle 方法。

範例以 `BatchColumnProviderV1` 呼叫此服務，證明 Windows-specific host service 可以透過公開 SDK 使用。

### 22.11 外掛檢視模式與遞迴目錄樹服務

Size Map 範例要求把檢視模式從內建封閉集合改造成可擴充 registry，並新增下列公開介面：

- `ViewModeRegistrationV1`：stable view ID、feature ID、本地化名稱、圖示、適用 location kind、priority、selection capability 與 renderer factory。
- `ViewModeContextV1`：目前 `LocationIdV1`、location generation、refresh generation、viewport、DPI、theme facade、focus、共享 selection snapshot 與 action sink。
- `GpuiViewModeRendererV1`：create、render、focus／blur、selection changed、location changed、refresh、suspend、resume 與 close lifecycle；不能取得 `ExplorerState` 或內部 GPUI entity。
- `CurrentLocationSubscriptionV1`：提供 owned location snapshot 與 generation，不把可變 tab model 借給 DLL。
- `ViewSelectionBridgeV1`：以 opaque item IDs 提交 replace／toggle／range／activate，並接收其他檢視模式造成的選取變更。
- `NavigationRequestV1`：open item、enter folder、open in new tab 與 reveal；宿主負責權限、history、breadcrumb、address bar 及實際 open policy。
- `ViewRefreshContextV1`：把 F5、watcher invalidation、設定變更與 location change 轉成帶原因及 generation 的統一刷新事件。
- `ViewModeSettingsV1`：保存 package-scoped、versioned 的檢視設定，並支援 schema migration 與 fallback。

宿主另提供 `DirectoryTreeScanServiceV1`，而不是讓每個 UI 外掛自行無限制掃描磁碟：

- 輸入 `DirectoryTreeScanRequestV1`：capability-authorized location、完整遞迴政策、symlink／hard-link 語意、忽略規則、metadata 欄位、deadline、資源上限、cancel token 與 refresh generation。
- 輸出 `DirectoryTreeDeltaV1`：有界批次的 add／update／remove／partial-error／subtree-complete／scan-complete 事件。
- `DirectoryTreeNodeV1`：opaque node／item ID、parent ID、名稱、kind、logical bytes、可選 allocated bytes、scan state 與 generation，不公開 native handle 或任意未授權絕對路徑。
- `DirectoryTreeScanTerminalV1`：complete、partial、cancelled、unavailable、resource-limited 與 failed。
- `DirectoryTreeScanCache`：以 location identity、scan policy、filesystem generation 與 refresh generation 分區；可以跨相容 consumer 重用純掃描資料，不共享外掛 opaque model。

檢視切換器、分頁 session persistence、focus、context menu、keyboard、UI Automation、drag/drop 及 status bar 必須接受動態 view ID。外掛檢視不存在、不相容、faulted 或被停用時，宿主回退到使用者上次的可用內建檢視，保留未知 view ID 供重新安裝後恢復。Renderer callback 只能布局與繪製已取得的 owned model；所有檔案 I/O 都經背景 scan service。

### 22.12 Virtual Folder 與 7z 修改交易

7z 範例要求下列正式平台介面：

- `VirtualProviderRegistrationV1`：副檔名／signature probe、provider priority 與 feature ID。
- `VirtualLocationV1`、`VirtualEntryIdV1`、`ContainerGeneration` 與 `VirtualEntrySnapshotV1`。
- `VirtualFolderProviderV1`：open container、enumerate children、resolve breadcrumb、parent 與 refresh。
- `VirtualFileStreamProviderV1`：bounded read stream、seek capability、length、CRC 與 cancellation。
- `VirtualMutationProviderV1`：create folder、add、delete、rename、move 與 transaction preview。
- `VirtualNavigationRouter`：把雙擊 `.7z`、地址列、返回／前進、tabs 與 session restore 接到虛擬位置。
- `VirtualPreviewMaterializer`：只在 Windows handler 需要實體路徑時建立配額化暫存檔。
- `ArchiveStagingService`：同磁碟 staging、space preflight、flush、verify、atomic replace、cleanup 與 original backup。
- `ContainerUndoRecord`：以原 archive 備份復原整包，並遵守 undo quota。
- `SecretHandleV1`：短生命週期密碼，禁止 Debug、Serialize、log 與一般設定保存。
- `ArchiveResourcePolicy`：entry count、path depth、single／total output、ratio、CPU、memory 與 temporary disk limits。

現有 `LocationDescriptor`、tab history、search、preview、drag/drop、file operation 與 session persistence 必須接受 virtual variant 或透過明確 router 適配。不能只在 7z 範例內建立一個與主導覽系統無關的檔案清單視窗。

### 22.13「擴充功能」選項頁介面

現有 `FolderOptionsPage` 必須新增 `Extensions`，但動態套件資料不可硬塞進目前可 `Copy` 的 `FolderOptionsDraft`。實作必須新增：

- `ExtensionOptionsDraft`：搜尋、篩選、global desired state、package states、feature states 與未保存變更。
- `ExtensionOptionsSnapshot`：由 Extension Host 提供的 immutable catalog、effective state、診斷、作者聯絡資料與 restart impact。
- `ExtensionOptionsAction`：toggle global、package、feature、search、filter、open diagnostics 與 contact link。
- `ExtensionSettingsTransaction`：validate、impact preview、apply、rollback draft 與 persisted commit。
- `FeatureDrainCoordinator`：取消 jobs、關閉 contribution、等待 callback、處理 virtual tabs 與決定是否 pending restart。

UI 必須支援鍵盤、UI Automation、長清單虛擬化、高 DPI、高對比與本地化。套件狀態變化不得阻塞 GPUI thread。

### 22.14 八個範例的介面覆蓋矩陣

| 官方範例 | 必須先完成的公開平台介面 | 可重用的現有能力 | 禁止的捷徑 |
| --- | --- | --- | --- |
| Rust 資料夾視覺化 | 動態欄位、單項 provider、aggregator、GPUI renderer、job、cache、feature gate | `explorer-jobs` priority queue、filesystem enumeration | 直接讀 `ExplorerState`、在 renderer 掃描磁碟 |
| Rust tokei | Batch column、typed numeric sort、GPUI renderer、settings、job | Rust 背景工作基礎 | 為每個檔案 spawn process、固定寫死欄位 enum |
| Lua tokei | Lua batch column registrar、bundled tool manifest／validator、Tool Resolver、ToolHandleV1、ProcessRequestV2、process cancellation、typed result | 現有 restricted Lua VM、direct argument process host | 缺少 `tokei.exe` 仍啟用、要求使用者安裝、自行搜尋 PATH、shell command string、直接呼叫低階 host |
| Lua 大量建立資料夾 | Lua button／form registrar、OperationPlan、create-directory executor、preview、cancel、undo | 現有 Lua VM、file operation／undo 基礎 | Lua 直接呼叫 OS 建立 100,000 個資料夾 |
| Rust EXIF 改名 | Command／button、form、InputStreamV1、內建 DLL 的 Rust EXIF decoder、typed metadata、template parser、batch rename plan、collision preview、undo | 現有 rename operation／journal | 依賴 exiftool／外部 EXIF DLL／網路服務、外掛直接 `rename`、略過預覽或檔名清理 |
| Rust 鎖定程序 | Batch column、LockOwnerQueryServiceV1、short TTL、F5 refresh generation、cache invalidation、refresh command | 現有 Restart Manager adapter、資料夾刷新入口 | 複製 Windows adapter、終止程序、公開 native handle、讓舊 generation 結果覆寫新狀態 |
| Rust 7z 虛擬資料夾 | Virtual provider／stream／mutation、navigation router、staging transaction、secret、resource policy、container undo | 現有 navigation／preview／operation 概念 | 全部解壓後冒充資料夾、原地覆寫 archive、私有旁路視窗 |
| Rust 資料夾 Size Map | View mode registry、GPUI view renderer、DirectoryTreeScanServiceV1、tree delta、selection bridge、navigation request、F5 generation、view settings | 現有 navigation、filesystem enumeration、GPUI view composition 概念 | 在 renderer 掃描磁碟、直接讀寫 tab／ExplorerState、只做私有視窗、讓舊 generation 覆寫新 layout |

### 22.15 OpenSpec 產物與任務完整性要求

後續 OpenSpec change 必須至少建立下列 capability specs；可以放在同一 change 中，但不得省略：

1. `extension-package-and-feature-lifecycle`
2. `rust-plugin-abi-and-ui-toolchain`
3. `extension-jobs-values-and-dynamic-columns`
4. `extension-commands-forms-and-operation-plans`
5. `lua-extension-registrar-and-tool-execution`
6. `lock-owner-host-service`
7. `virtual-folder-stream-and-mutation`
8. `extension-view-modes-and-directory-tree-scan`
9. `extension-options-management`
10. `source-example-plugin-suite`

每個 spec 的 tasks 必須包含 production implementation、unit tests、integration tests、UITEST manifest mapping、公開文件與範例使用。只新增 trait、只建立空 crate、只放 mock、只加入未被 composition root 使用的程式碼，均不算完成。

`rust-plugin-abi-and-ui-toolchain` 的第一個 blocking milestone 必須是第 10.1 節的 P0-0。其他所有 Rust 範例 task、`source-example-plugin-suite` 的 Rust 部分及 AI prompt fixture 都依賴已發布的 immutable bundle ID；OpenSpec task graph 不得把版本選定、canonical lockfile、GPUI vendor hash 或 `abi_stable` compatibility test 排到範例完成之後。

範例 task 必須依賴對應平台 task，且兩者同屬 P0 垂直切片；排程上不得把所有 host interface 做完後才開始範例，也不得用範例尚未完成作為先啟動 P1／P2／P3 的理由。CI 要從獨立 consumer workspace 以正式 bundle 建置範例；若範例需要依賴任何 SuperExplorer 私有 crate，驗收直接失敗。八個範例、所需正式接口及作者環境全數通過前，擴充平台不得標記完成，也不得發布 stable SDK。

## 二十三、實作順序

### 23.1 P0：官方範例、正式接口與作者環境

P0 是第一個 production milestone，也是目前唯一允許優先投入擴充平台人力的範圍。它必須交付以下三類不可拆開的成果：

1. **作者環境基線**：`.sepack`、manifest、feature／capability、built-in／local developer package source、安裝與驗證流程、SDK 目錄、獨立 consumer workspace、雙語文件、範本、除錯診斷與範例 CI。
2. **Rust UI Plugin SDK 環境**：鎖定的 Rust toolchain、target、GPUI、`abi_stable`、SDK crates、Cargo 設定、fingerprint、build／validate／package scripts、相容 fixture，以及 Rust-only AI prompt。外部作者必須能只安裝 bundle 後，在沒有 SuperExplorer 私有原始碼的環境建置 DLL。
3. **Lua 作者環境**：鎖定 Lua API、capability validator、`.sepack` 打包器、bundled tool validator／resolver、測試 harness、日誌與錯誤診斷。Lua 範例必須能在不引用 Rust workspace 的情況執行與測試。

在上述成果中，Rust 作者環境先執行 `P0-0 SDK 相容基線 Bootstrap Gate`。它必須先提交及發布 canonical `Cargo.lock`、Rust `1.97.1` toolchain、`damody/gpui-ce-explorer` resolved commit／vendor tree hash／snapshot metadata、`abi_stable 0.11.3` version／checksum／features、`.cargo/config.toml`、`sdk-lock.json`、離線 vendor、fingerprint fixture 與 build／validate／package scripts。任何 Rust 外掛垂直切片都不能與 P0-0 平行猜測版本；只有取得通過 CI 的 snapshot bundle ID 後才能開始。

P0 依下列垂直切片順序實作。每一項都同時包含 production host interface、SDK public surface、完整範例原始碼、單元／整合／UITEST、文件及打包驗證；不能把「接口完成」與「範例完成」拆成彼此相隔的里程碑：

1. 使用已發布的 P0-0 bundle 建立套件載入、Rust root module、Plugin Registrar、feature gate、options 管理、工作排程、增量結果、取消、快取與 GPUI column registrar；用 **Rust 資料夾大小視覺化欄位**完成第一個端到端 reference slice。
2. View mode registry、`GpuiViewModeRendererV1`、`DirectoryTreeScanServiceV1`、selection bridge、navigation request、F5 generation 與 view settings；完成 **Rust 目前資料夾 Size Map**。
3. Batch column、typed value／sorting 與 Rust library consumer 規則；完成 **Rust tokei 程式碼行數欄位**。
4. `LockOwnerQueryServiceV1`、F5 refresh generation 與 stale-result rejection；完成 **Rust 鎖定程序欄位**。
5. Lua batch column、bundled tool manifest、Tool Resolver、`ToolHandleV1`、`ProcessRequestV2`、取消及 child-process 回收；完成 **Lua tokei 程式碼行數欄位**。
6. Lua button／form registrar、operation preview、衝突處理、取消與 undo journal；完成 **Lua 大量建立資料夾**。
7. Rust command／form、`InputStreamV1`、typed decoder metadata、rename template、collision preview 與 undo；完成自含 EXIF parser DLL 的 **Rust EXIF 批量改名**。
8. Virtual Folder／File Stream／Mutation、navigation router、container generation、staging transaction、secret 與資源限制；完成可瀏覽及修改的 **Rust 7z 虛擬資料夾**。
9. 對八個範例執行乾淨機器、獨立 consumer、效能、安全、相容、封裝與文件重現測試，產生可發布的 SDK bundle 與八個 `.sepack`。只有這個 release gate 全數通過，P0 才完成。

任何 P0 公開接口若尚未被至少一個官方範例從獨立 consumer workspace 使用，不得宣告穩定或完成。任何官方範例若繞過公開接口、引用私有 crate、只提供 mock／片段，或需要手動複製未列入 bundle 的檔案，對應垂直切片即視為失敗。

### 23.2 P1：Skin 與平台完善

P0 完成後，加入資料型 Skin schema、資產載入、圖片與按鈕替換、不規則視覺外框、透明背景／點穿遮罩、fallback、高 DPI 與可存取性驗收。第一階段最終交付仍包含 Skin，但 Skin 不得延後 P0 官方外掛與 SDK 環境。

### 23.3 P2：官方專業擴充

使用已由 P0 驗證的公開 SDK 建立 Perfetto、trace-cmd、Word、XLSX 與 OCR first-party 外掛。這些外掛不得要求新增只能由官方使用的私有旁路接口；若發現 SDK 缺口，必須回到公開接口、文件與相容測試補齊。

### 23.4 P3：Steam 與商業化

Steamworks、Workshop、Curated Workshop、DLC entitlement、付費作者工具包及分潤最後處理。本階段只保留 Package Source／Entitlement Provider 抽象，不得讓 Steam 工作阻塞或取代 P0。

## 二十四、完成定義

第一階段只有在下列條件全部成立時才算完成：

- P0-0 已先於所有 Rust 官方範例發布不可變 development snapshot bundle ID；其中的 canonical `Cargo.lock`、Rust `1.97.1` toolchain、`damody/gpui-ce-explorer` resolved commit／tree hash、`abi_stable 0.11.3` checksum／features、離線 vendor 與 fingerprint fixture 均可由 CI 重現。Release gate 另驗證 `release_frozen = true`、受保護 tag 與最終 GPUI commit，且不存在浮動 `stable`、`latest`、branch-only dependency、範圍版本或未鎖定來源。
- P0 已產出可重現的 Rust UI Plugin SDK toolchain bundle、Lua 作者環境、公開接口文件、build／validate／package 工具，以及八個可安裝 `.sepack`；它們能在沒有私有 workspace 原始碼的乾淨環境使用。
- 第 22.15 節列出的十份 capability spec 都有 production code、測試、UITEST mapping、文件與 composition-root 整合，沒有只存在於 mock 或未使用 crate 的接口。
- 一個 Rust DLL 能透過單一 root module 註冊多個資料與 GPUI 介面。
- 不相容 GPUI 外掛能在 callback 前由 fingerprint 拒絕。
- 資料夾選項具有可用鍵盤與輔助技術操作的「擴充功能」分頁，可開關總體、套件與個別功能，並正確呈現立即生效、blocked、faulted 與 pending restart 狀態。
- 資料夾大小參考外掛能背景計算 1,000 個項目、排序、聚合並自行用 GPUI 繪製。
- 八個含原始碼範例都能從乾淨環境依 README 建置或執行，並通過各自測試與 `.sepack` 驗證。
- 八個範例只依賴正式公開 SDK、鎖定第三方依賴與宣告過的 host capability；不得引用 `explorer-ui`、`explorer-model`、`explorer-shell-win` 或其他私有 workspace crate。
- Rust 與 Lua 兩種 tokei 範例都能產生可數值排序的程式碼行數欄位，且 Lua 版本不經 shell 執行命令。
- Lua tokei `.sepack` 內含可離線執行、版本與 hash 固定且附授權的 `windows-x64` `tokei.exe`；移除或竄改工具後 feature 必須在執行 Lua callback 前被阻擋，不能回退到系統工具。
- Lua 大量建立資料夾範例能註冊擴充按鈕、顯示參數表單、預覽操作並安全取消／復原。
- Rust EXIF 範例只靠自身 `plugin.dll` 與宿主 InputStreamV1 就能讀取 metadata、預覽並套用 `{rawname}_{XResolution}x{YResolution}` 等規則；它正確區分密度與像素尺寸 tag，且不依賴 exiftool、專用外部 DLL、PATH 或網路。
- Rust 鎖定程序欄位能顯示目前鎖定檔案的 process name，按 `F5` 可重新查詢目前資料夾並反映新增或解除的鎖定；舊 generation 不會回填，且不提供終止程序能力。
- Rust 7z 範例能把 `.7z` 當作可導覽虛擬資料夾，支援預覽、複製解壓、加入、刪除、改名、移動與建立資料夾；任何失敗都不能破壞原 archive。
- Rust Size Map 範例能在目前分頁註冊正式檢視模式，完整遞迴並增量呈現目前資料夾；矩形面積代表 bytes、顏色預設代表檔案類型，單擊共享選取、雙擊資料夾走正式導覽、F5 拒絕舊 generation，且所有磁碟 I/O 都不在 GPUI renderer 執行。
- Lua 能在 capability 限制下完成可預覽、可取消、可復原的批次檔案操作。
- Skin 能替換圖片、按鈕、視覺外框與透明背景，同時保留 Windows 視窗行為。
- Rust AI prompt 能在固定 bundle 中產生並驗證最小可用外掛。
- Safe Mode 能根據上次 native call marker 協助停用疑似外掛。
- 核心程式不依賴 Steamworks，且未來 Steam 與 Pro entitlement 有清楚的抽象接點。
