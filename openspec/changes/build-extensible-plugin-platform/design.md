## Context

SuperExplorer 是 Windows x64 MSVC 的 Rust／GPUI 檔案總管。目前詳細資料欄位、檢視模式、資料夾選項與 session state 多為封閉的內部 enum／struct；`explorer-jobs`、`explorer-automation`、`explorer-model` 與 `explorer-shell-win` 已有可重用基礎，但尚無第三方可依賴的程序內 Rust Plugin API、動態 registrar、`.sepack` manager 或可重現 UI Plugin SDK。

本 change 橫跨套件管理、ABI、GPUI、背景工作、Lua、檔案操作、Windows Restart Manager、導覽、虛擬容器與設定 UI。Rust DLL 會在 SuperExplorer 程序內執行，因此相容性、生命週期與 Safe Mode 必須在第一個 callback 前後形成完整防線。八個完整官方範例同時是作者教材、整合 fixture 與 stable SDK release gate。

既有 `explorer-extension-protocol`／broker 繼續處理 Windows Shell／COM 等程序外整合；新 Extension Host 不取代其 IPC 協定，也不共享 ABI struct。

## Goals / Non-Goals

**Goals:**

- 交付 Rust、Lua 與 data-only Skin 可共用的 `.sepack`、manifest、feature/capability、套件狀態與管理 UI。
- 讓單一 Rust DLL 經一個 root module 註冊多個資料與 GPUI contribution。
- 以 Rust `1.97.1`、`abi_stable 0.11.3` 及 `damody/gpui-ce-explorer` 精確 snapshot 提供可重現 UI Plugin SDK。
- 讓開發期 GPUI 能持續更新，同時使每次 host/plugin build 仍可由 immutable bundle ID 重現；Release 時永久凍結最終 commit。
- 讓外掛在有界背景工作中計算 typed data，再由 GPUI thread 只負責布局與繪製。
- 讓 Lua 經 capability 與 operation plan 安全完成批次工作，並只執行套件內驗證過的 bundled tools。
- 將欄位、檢視模式、命令、表單、虛擬資料夾與選項頁改造成正式公開擴充點。
- 以八個獨立 consumer 範例證明公開接口足以實作實際功能。

**Non-Goals:**

- 本 change 不整合 Steamworks、Workshop 上傳下載、DLC ownership、付費或分潤。
- 不支援 Rust DLL 熱載入、熱更新、熱卸載或程序 sandbox。
- 不支援 Windows x64 MSVC 以外的第一版原生 target。
- 不把 `explorer-ui`、`explorer-model`、內部 GPUI entity、HWND wrapper 或私有 action 暴露給外掛。
- 不讓 Lua 回傳任意 GPUI element；任意 UI 繪製僅屬固定 toolchain 的 Rust 外掛。
- 不在本 change 實作 Pro Toolkit 的 Perfetto、trace-cmd、Word、XLSX 與 OCR 商業內容。

## Decisions

### 1. Extension Host 是唯一擴充入口

新增 `explorer-extension-api`、`explorer-extension-ui-api` 與 `explorer-extension-host`。Host 擁有 package discovery、manifest validation、resolver、registries、feature gates、DLL loader、Lua adapter、job dispatch、cache、diagnostics 與 Safe Mode；既有 model/UI 只透過 adapter 接收公開 owned snapshot。

替代方案是讓各 UI component 自行載入外掛。此方案會重複權限與生命週期邏輯，並讓 feature 停用產生半啟用狀態，因此不採用。

### 2. `.sepack` 是統一套件與信任邊界

Rust DLL、Lua、Skin、locales、bundled tools、授權與簽章都由同一 manifest 描述。Package Manager 只啟用同一 package ID 的一個完整版本；相依、hash、簽章、ABI 或 capability validation 任一失敗時拒絕整包，避免 provider 與 renderer 分裂。

Manifest 的每個 contribution 必須對應穩定 `feature_id`，並宣告實際 capability。Publisher 至少提供一個公開聯絡方式，且至少一個用途為 support 或 security。

### 3. Rust 只有一種 DLL 模型，但使用兩層相容契約

每個 DLL 匯出單一 `abi_stable` prefix root module。`abi_stable 0.11.3` 會拒絕新版 root 載入欄位較少的舊 DLL，因此 SDK 1.x 凍結 root 本體，只在 registrar 尾端增加 optional function 或固定寬度 data field，並一律透過 optional accessor 讀取。FFI-safe metadata、typed value、provider 與 host service 使用固定寬度 primitive 和 `abi_stable` collection/result。

GPUI 型別不宣稱具有 stable Rust ABI。只要 DLL 註冊任何 GPUI contribution，整個 DLL 必須與 host 的 UI fingerprint 完全一致；只註冊 stable data interface 的 DLL 才使用 SDK 同一大版本與 layout compatibility。

替代方案是只鎖 GPUI semver。Rust/GPUI callback 還受 compiler、features、dependency graph、panic/profile 影響，單一版本號不足，因此不採用。

### 4. P0-0 使用「浮動 update channel、不可變 build snapshot」

Rust 固定為 `1.97.1`，`abi_stable` 固定為 `0.11.3`。GPUI 的唯一 source authority 是 `https://github.com/damody/gpui-ce-explorer.git`；`main` 只供 update job 尋找候選。每次 job 將 HEAD 解析為完整 commit，產生 canonical Cargo.lock、offline vendor、sdk-lock、fingerprint、host/plugin fixtures 與新 development bundle ID。

Host、SDK 與八個範例只有在完整 CI 通過後才原子切換 snapshot。失敗或未核准的 non-fast-forward 保留上一 snapshot。RC cut 後設定 `release_frozen = true`、建立受保護 tag、離線重建並簽署；日後修正使用新的 patch bundle，不改寫舊 Release。

外掛可加入自己的私有 Rust dependencies，但不得改動 protected dependency closure；其 lock、vendor、provenance 與授權由外掛包負責。

### 5. Rust DLL 啟動載入、功能執行期 gate、程序結束才卸載

DLL 只在啟動階段載入並保持 resident。已載入 feature 可停止新 dispatch、取消工作、移除 UI contribution 並 bounded drain；失敗則標示 pending restart。安裝、更新、替換、移除或啟用本次啟動未載入的 DLL 必須重啟。

熱卸載可能留下 callback、thread、GPUI element 或 allocator state，因此不採用。

### 6. 有界 Scheduler 與 owned typed snapshots 隔離背景工作

在 `explorer-jobs` 上建立 CPU/I/O queue、global/per-package limits、visible-row priority、generation、cancellation、incremental sink、backpressure、16–50 ms UI batching 與 timing diagnostics。ABI callback 保持同步，但由 host worker 呼叫；不跨 ABI 傳遞 Future 或 runtime handle。

資料以 `PluginValueV1` 與 `StableSortValueV1` 表示。Opaque payload 只能回到同一外掛 renderer。Item、location、scan 與 cache 都攜帶 generation，拒絕導航或刷新後的 stale result。

### 7. 動態欄位與 GPUI contribution 不暴露私有狀態

固定 `SortColumn`／bitmask 遷移為 dynamic registry、ordered layout 與 stable `ColumnId`。Header、row virtualization、selection、sorting、width/order persistence、UIA 與 session restore 都使用 registry。

GPUI context 只提供 immutable public data、theme facade、action sink 與 scoped invalidation handle。Renderer 只在 GPUI thread 執行，不得做 I/O、網路或長時間解析。

### 8. 命令與檔案變更統一走 typed Operation Plan

Rust/Lua 共用 command、button、form、plan、preview、validator、executor、progress、cancel 與 undo。外掛不能直接執行大量 create/rename/delete。Validator 統一路徑正規化、保留名稱、逃逸、case-insensitive collision、數量與權限。

EXIF parser 是鎖定的 static Rust library，編入同一 plugin.dll，經 `InputStreamV1` 讀取；不得依賴 exiftool、外部 EXIF DLL、PATH 或網路。

### 9. Lua 只取得 capability 與 opaque ToolHandle

現有受限 Lua VM 增加 column/command/button/form/operation-plan registrar。Bundled executable 必須位於 `.sepack/tools/<target>/<id>/` 並具版本、大小、SHA-256、來源及授權；Tool Resolver 不查 PATH、Registry、網路或使用者替代檔。

Lua 只提交 tool ID 與 argument array。Windows ProcessLease 使用 Job Object 終止並回收子程序樹，區分 exit/timeout/cancel/output-truncated。這保留 Lua 的便利性，同時避免 shell injection 與未宣告系統依賴。

### 10. Windows 特定能力由受限 Host Service 提供

Lock Owner 外掛不重複 Windows adapter。`LockOwnerQueryServiceV1` 包裝既有 Restart Manager，僅接收 capability-authorized handles，回傳 owned PID/display-name/type，且沒有 terminate/close-handle 方法。F5 走共用 refresh generation 與 cache invalidation。

### 11. Virtual Folder 是正式導覽 variant，不是旁路清單

Virtual location、entry ID、container generation、stream 與 mutation 接入 tab history、breadcrumb、preview、copy/drag-drop、search 及 session restore。7z 修改先產生 transaction preview，在同磁碟 staging 重建、flush/verify、檢查原檔 identity，最後 atomic replace；失敗保持原 archive 位元不變。

密碼只用短生命週期 secret handle；archive limits 控制 entry、depth、ratio、CPU、memory 與暫存空間。

### 12. View Mode registry 與 Directory Tree Scan 分離資料及創作

外掛可註冊完整 GPUI view mode，但磁碟掃描由 `DirectoryTreeScanServiceV1` 管理。Service 以有界 delta 傳遞 owned tree nodes、partial errors 與 generation；renderer 自行選擇 treemap 或其他 layout。

Selection bridge 與 navigation request 將單擊／雙擊接回正式 tab state。外掛模式不存在、faulted 或停用時 fallback 到內建 view，並保存未知 ID 供重新安裝後恢復。

### 13. Extension Options 使用獨立 transaction model

動態 catalog 不塞入既有可 Copy 的 `FolderOptionsDraft`。新增 snapshot、draft、actions、settings transaction 與 drain coordinator，顯示 global/package/feature desired/effective state、聯絡資料、能力、工具、診斷與 restart impact。

Apply、OK、Cancel 與關閉遵守既有對話框語意；會關閉虛擬分頁、面板或欄位的變更先顯示 impact。

### 14. 八個官方範例是垂直切片與 Release Gate

P0-0 之後依序完成資料夾大小、Size Map、Rust tokei、Lock Owner、Lua tokei、Lua 大量建立資料夾、Rust EXIF 改名、Rust 7z。每個 slice 同時完成 production host、public SDK、範例、單元／整合／UITEST、文件與 `.sepack`。

範例位於獨立 consumer workspace，不是主 workspace member，也不得引用私有 crate。只有 trait、mock、未接 composition root 或 README 片段不算完成。

### 15. Skin 保持純資料並保留 Windows 行為

Skin schema 可以替換圖片、圖示、字型、按鈕狀態、nine-slice/vector、色彩、間距、透明度及 hit-test mask，但不執行 Rust、Lua、JavaScript 或 shader。視窗的作業系統幾何仍是矩形，Snap、最大化、resize、DPI、多螢幕、鍵盤焦點、UIA 與核心操作 fallback 由宿主保留。單一資產損壞只回退該資產，不能讓整個 UI 不可操作。

## Risks / Trade-offs

- **[程序內 Rust 可使整個應用程式崩潰或死鎖]** → 載入前驗證、panic boundary、call marker、慢 callback 診斷、Safe Mode；文件明示其不是 sandbox。
- **[開發期 GPUI 更新造成大量外掛重建]** → immutable snapshot、原子 migration、精確 loader 診斷；Release freeze 後不再漂移。
- **[固定 toolchain 增加 SDK 發布成本]** → 自動產生 lock/vendor/fingerprint、離線 fixture 與 AI prompt CI，換取可重現性。
- **[動態欄位與 view registry 觸及既有 session/UI 路徑]** → 先遷移 built-in 行為到同一 registry，保留未知 ID 並提供 fallback/rollback fixture。
- **[1,000 項目欄位與 100,000 節點 Size Map 造成 UI 更新風暴]** → background delta、backpressure、visible priority、layout/invalidation batching 與 memory quota。
- **[Virtual archive 修改可能毀損資料]** → staging、space preflight、verify、identity recheck、atomic replace、backup/undo；任何失敗不改原檔。
- **[Lua bundled executable 增加供應鏈與防毒問題]** → package hash/signature、target/license validation、opaque handle；缺檔時 blocked，絕不回退系統工具。
- **[完整 P0 範圍較大]** → 以八個範例作垂直切片，不先建立未被 consumer 使用的大量空介面。
- **[透明／不規則 Skin 破壞視窗操作或可存取性]** → 矩形 OS 視窗、宿主 hit-test/resize/focus/UIA、逐項資產 fallback 與高對比測試。

## Migration Plan

1. 完成 P0-0：把 repository、host、SDK fixtures 統一到 Rust `1.97.1`、`abi_stable 0.11.3` 與核准 GPUI snapshot，發布第一個 immutable bundle ID。
2. 新增三個 extension crates、`.sepack` parser、manifest IDs、Package Manager、loader、feature state、call guard 與 composition-root wiring；預設無第三方套件時維持現有行為。
3. 將 built-in 詳細資料欄位與 view/session state 遷移到 dynamic ID／registry；提供舊設定轉換與 rollback fixture。
4. 完成 scheduler/value/GPUI contribution，再以資料夾大小範例驗證第一條端到端路徑。
5. 依 P0 順序加入 view scan、batch columns、Lock Owner、Lua tools、operation plans、EXIF stream 與 Virtual Folder；每步均需相應官方範例通過才進下一步。
6. 加入 Extension Options、runtime drain、Safe Mode 與全套 UITEST mapping。
7. 在空 `CARGO_HOME`、禁止網路的環境建置 SDK 與八個範例，執行相容、安全、效能與封裝 gate。
8. P0 通過後完成純資料 Skin schema、asset loader、透明/hit-test 與 accessibility/fallback gate。
9. RC cut 凍結 GPUI commit/tag、設 `release_frozen = true`，重建並簽署 Release bundle。若 gate 失敗，保留上一 host/SDK，或停用尚未完成的 feature；不遷移使用者資料到不可回復格式。

## Open Questions

- 第一個正式 Release 的 GPUI commit 與受保護 tag 在 RC cut 時由最後一個通過完整 gate 的 development snapshot 決定；目前開發 snapshot 不是 Release 承諾。
- Per-package CPU/I/O quota、Lock Owner TTL、Virtual Folder 資源上限與 Size Map memory budget 的預設數值由效能 fixture 校準，但其 bounded 行為與可診斷 terminal state 已由 specs 固定。
- Steam 簽署／審核、DLC entitlement 與付費作者流程留待獨立 change；本 change 只建立可替換的 Package Source／Entitlement Provider 接點。
