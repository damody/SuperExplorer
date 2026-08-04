## Context

SuperExplorer 是 Windows x64 MSVC 的 Rust／GPUI 檔案總管。目前詳細資料欄位、檢視模式、資料夾選項與 session state 多為封閉的內部 enum／struct；`explorer-jobs`、`explorer-automation`、`explorer-model` 與 `explorer-shell-win` 已有可重用基礎，但尚無第三方可依賴的程序內 Rust Plugin API、動態 registrar、`.sepack` manager 或可重現 UI Plugin SDK。

本 change 橫跨套件管理、ABI、GPUI、背景工作、Lua、檔案操作、Windows Restart Manager、導覽、虛擬容器與設定 UI。Rust DLL 會在 SuperExplorer 程序內執行，因此相容性、生命週期與 Safe Mode 必須在第一個 callback 前後形成完整防線。八個完整官方範例同時是作者教材、整合 fixture 與 stable SDK release gate。

既有 `explorer-extension-protocol`／broker 繼續處理 Windows Shell／COM 等程序外整合；新 Extension Host 不取代其 IPC 協定，也不共享 ABI struct。

## Goals / Non-Goals

**Goals:**

- 交付 Rust、Lua 與 data-only Skin 可共用的 `.sepack`、manifest、feature/capability、套件狀態與管理 UI。
- 讓單一 Rust DLL 經一個 root module 註冊多個資料與 GPUI contribution。
- 以 Rust `1.97.1`、`abi_stable 0.11.3` 及 `damody/gpui-ce-explorer` 精確 snapshot 提供可重現 UI Plugin SDK，並以本機 offline Rust、PowerShell contract 與 `explorer-uitest` 驗證及已簽署 evidence bundle 證明完成。
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
- 本 change 永不使用 CI、GitHub Actions、remote artifact service、`ci://` locator 或 hosted release gate 作為實作、完成、evidence 或 release prerequisite；external automation 沒有 completion authority，且不得取代、弱化或成為本機流程的權威。

## Decisions

### 0. 0→1 階段先完成單一可見 vertical slice

目前只驗證 canonical `rust-folder-size-visual-column`（由早期 `p0-consumer` prototype 直接遷移，不保留第二份重複實作）。App 接受一個明確的 `--plugin-dll <absolute-path>` 開發參數，Extension Host 重用既有 Windows DLL loader、`abi_stable` root layout validation、SDK-owned registrar factory 與 registration，再把 plugin ID、contribution ID、kind 複製成最小 owned summary，交給既有 Extensions menu 顯示。沒有參數時不掃描或載入 unsigned local DLL。

此階段不建立多 Plugin abstraction、package-source framework、dynamic-column provider/renderer、scheduler、evidence ledger、snapshot/release framework、contract/integration/mock/fake framework。驗證只用最小 unit、smoke 與人工 demo；UITEST 只可能在完整 example 可見後作單一既有-runner smoke。其餘 decisions 是 validation GO 後的 roadmap，不得成為本 milestone 的前置 gate。

`build_install.bat` 的同一條 release 流程必須使用 fixture manifest 以 `--release --target x86_64-pc-windows-msvc --locked --offline` 建置唯一的 `rust_folder_size_visual_column.dll`，驗證明確產物後交給 NSIS。NSIS 固定安裝為 `$INSTDIR\plugins\rust_folder_size_visual_column.dll`，並讓桌面捷徑、開始選單捷徑及完成頁傳入 `--plugin-dll "$INSTDIR\plugins\rust_folder_size_visual_column.dll"`。不採用 app 目錄掃描或額外 launcher；uninstaller 只刪除該已知 DLL 與空目錄，不遞迴刪除未知檔案。

folder-size slice 完成後，下一個 active consumer 是獨立 `rust-folder-size-map-view`。它仍單獨透過 `--plugin-dll` 載入，不改變 installer 的唯一 bundled Plugin。SDK-owned `abi_stable` renderer 只接收完整 host-minted revision 的 owned node snapshot、viewport/theme/selection/settings並回傳data-only treemap rectangles；同步 ABI callback 只在有界 host worker 內執行並以每次呼叫的 durable marker 保護。實際 GPUI element只畫已返回且revision相符的 plan，並保有正式 selection/navigation/F5 action；GPUI thread、entity、handle與I/O均不跨 ABI。P0先呈現目前位置第一層節點，使用有界且generation-aware的背景計量逐步補值；通用100,000-node scan framework保留在deferred roadmap，不阻擋這個產品驗證slice。

### 1. Extension Host 是唯一擴充入口

新增 `explorer-extension-api`、`explorer-extension-ui-api` 與 `explorer-extension-host`。Host 擁有 package discovery、manifest validation、resolver、registries、feature gates、DLL loader、Lua adapter、job dispatch、cache、diagnostics 與 Safe Mode；既有 model/UI 只透過 adapter 接收公開 owned snapshot。

替代方案是讓各 UI component 自行載入外掛。此方案會重複權限與生命週期邏輯，並讓 feature 停用產生半啟用狀態，因此不採用。

### 2. `.sepack` 是統一套件與信任邊界

Rust DLL、Lua、Skin、locales、bundled tools、授權與簽章都由同一 manifest 描述。Package Manager 只啟用同一 package ID 的一個完整版本；相依、hash、簽章、ABI 或 capability validation 任一失敗時拒絕整包，避免 provider 與 renderer 分裂。

Manifest 的每個 contribution 必須對應穩定 `feature_id`，並宣告實際 capability。Publisher 至少提供一個公開聯絡方式，且至少一個用途為 support 或 security。

### 3. Rust 只有一種 DLL 模型，但使用兩層相容契約

本 change 在第一個公開 V1 前執行一次性 ABI reset：先前的 handwritten raw-callback/custom-root 是未發布 experimental layout，不構成 SDK 1.x。首次發布 baseline 為目前 Rust-first `ExtensionRootModuleV1`：required root 欄位順序與語意、root fingerprint、直接位於 checked root prefix 的 required SDK-owned `create_registrar` factory、stateful ABI-safe registrar object及 descriptor output 由此凍結。外掛作者只實作 ordinary Rust traits；SDK adapter 負責 `#[sabi_trait]` type erasure、ABI-safe factory與 panic translation，作者不得手寫 `extern "C"` callback、layout 或 trampoline。legacy raw root 必須在任何 accessor/factory/callback 前被 layout reject。

首次發布後，完整 root layout、required factory與 numeric semantics 不得在 SDK 1.x 重定義或追加。`abi_stable 0.11.3` 的 prefix 檢查不支援 newer host 以較長 layout 載入 shorter older plugin，因此 1.x 演進只使用 baseline 已具備的 descriptor/capability data contract與已核准的 non-exhaustive values；任何結構性 factory/trait/root 變更必須升 SDK major。FFI-safe metadata、typed value、provider 與 host service 使用固定寬度 primitive 和 `abi_stable` collection/result。

GPUI 型別不宣稱具有 stable Rust ABI。只要 DLL 註冊任何 GPUI contribution，整個 DLL 必須與 host 的 UI fingerprint 完全一致；只註冊 stable data interface 的 DLL 才使用 SDK 同一大版本與 layout compatibility。

替代方案是只鎖 GPUI semver。Rust/GPUI callback 還受 compiler、features、dependency graph、panic/profile 影響，單一版本號不足，因此不採用。

### 4. P0-0 使用「明確 upstream update、不可變 build snapshot 與本機 offline gate」

Rust 固定為 `1.97.1`，`abi_stable` 固定為 `0.11.3`。GPUI 的唯一 source authority 是 `https://github.com/damody/gpui-ce-explorer.git`；只有 primary agent 明確執行的 upstream update operation 可使用網路從 `main` 尋找候選。該 operation 將 HEAD 解析為完整 commit，產生 canonical Cargo.lock、sdk-lock、fingerprint、host/plugin fixtures 與新 development bundle ID；其後 candidate generation、host/plugin build、fixture、promotion、rollback、release freeze 與 evidence verification 都是本機操作。第三方來源不提交或追蹤 vendor；實際建置要求預先填好的本機 Cargo registry cache，並固定使用 `--locked --offline`，缺 cache 時明確阻擋並要求 bootstrap。

每個 gate matrix entry 只宣告一個在 release integrator Windows workstation 執行的 command 或 manual review procedure、working directory/environment、expected exit status 與 required artifacts。Rust unit/integration/ABI gate 使用精確的 `cargo test --locked --offline` command；PowerShell contract gate 使用 `powershell -NoProfile -ExecutionPolicy Bypass -File <script>` 並輸出 deterministic report；schema 與 architecture gate 也在本機執行。Phases 1–5 只執行這些 local checks，零 UITEST；Task 6 在除 final gate 外的全部 leaves 完成前也零 UITEST。headful/UI gate 使用 repository 的 `cargo run -p explorer-uitest --bin explorer-uitest --locked --offline -- --case <case-id>` 與 `uitest/manifest.json`，但 6.4.7 是第一個 case，且只有 Task 6 的 framework、consumer contract、implementation、production wiring、public SDK、fixture、docs、package 與 inventory/composition 全部完成才可執行。此後每個範例也必須先完成相同 prerequisites 才可執行其 UITEST。UITEST 不得取代上述任何 check，且不得驗證 incomplete、mock、trait-only 或 source-shape-only example。

Host、SDK 與八個範例只有在每個必要且在該 phase eligible 的本機 Rust、PowerShell contract、schema、architecture 及（僅在完整 Phase 6 後）UITEST gate 成功、並產生已簽署的本機 release evidence bundle 後才原子切換 snapshot。Bundle 必須為 deterministic、store-only、content-addressed，並包含 evidence manifest、exact command/manual procedure、結果、artifact hash、task/subcheck 與 source-revision binding、RC identity、retention metadata 與 timestamp；它只在明確的本機 retained-bundle root 解析，且由獨立於 plugin publisher key 的 release-integrator trust policy 驗證。驗證拒絕 unsigned/untrusted bundle、hash mismatch、path traversal、duplicate normalized path、reparse-point escape、oversized archive、invalid retention metadata 與不符當前 task/subcheck 或 source revision 的 bundle。失敗或未核准的 non-fast-forward 保留上一 snapshot。RC cut 後設定 `release_frozen = true`、建立受保護 tag、離線重建並簽署 bundle；日後修正使用新的 RC 與 bundle ID，不改寫舊 Release。

外掛可加入自己的私有 Rust dependencies，但不得改動 protected dependency closure；其 lock、provenance 與授權由外掛包負責。第三方來源不提交或追蹤 vendor，建置前必須由本機 registry cache 提供鎖定來源。

### 5. Rust DLL 啟動載入、功能執行期 gate、程序結束才卸載

DLL 只在啟動階段載入並保持 resident。已載入 feature 可停止新 dispatch、取消工作、移除 UI contribution 並 bounded drain；失敗則標示 pending restart。安裝、更新、替換、移除或啟用本次啟動未載入的 DLL 必須重啟。

熱卸載可能留下 callback、thread、GPUI element 或 allocator state，因此不採用。

### 6. 有界 Scheduler 與 owned typed snapshots 隔離背景工作

在 `explorer-jobs` 上建立 CPU/I/O queue、global/per-package limits、visible-row priority、generation、cancellation、incremental sink、backpressure、16–50 ms UI batching 與 timing diagnostics。包括 visual cell 與 Size Map render-plan callback 在內的 ABI callback 保持同步，但只由 host worker 呼叫；每次 native callback 都以 durable marker 保護。GPUI 只消費相符完整revision的 data plan，不跨 ABI 傳遞 Future、runtime handle、GPUI object 或 render context。

資料以 `PluginValueV1` 與 `StableSortValueV1` 表示。Opaque payload 只能回到同一外掛 renderer。Item、location、scan 與 cache 都攜帶 generation，拒絕導航或刷新後的 stale result。

### 7. 動態欄位與 GPUI contribution 不暴露私有狀態

固定 `SortColumn`／bitmask 遷移為 dynamic registry、ordered layout 與 stable `ColumnId`。Header、row virtualization、selection、sorting、width/order persistence、UIA 與 session restore 都使用 registry。

Data-only render-plan context 只提供 immutable public data、theme facade、settings 與 host-minted full-snapshot revision，不提供 action sink、invalidation handle 或任何 GPUI type。每次同步 ABI callback 只在有界 host worker 執行，並各自建立與清除 durable call marker；不得做 I/O、網路或長時間解析。GPUI thread 只布局並繪製 revision 仍相符的 returned plan。

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

P0-0 之後依序完成資料夾大小、Size Map、Rust tokei、Lock Owner、Lua tokei、Lua 大量建立資料夾、Rust EXIF 改名、Rust 7z。Phases 1–5 的每個 slice 只完成 local offline Rust unit/integration/ABI、PowerShell contract、schema 與 architecture checks，零 UITEST；Task 6 也在除 final gate 外的全部 leaves 完成前零 UITEST。當 Task 6 的 framework、consumer contract、implementation、production host wiring、public SDK、fixture、docs、`.sepack` 與 inventory/composition 全部完成時，6.4.7 才可執行第一個 UITEST gate。此後每個 slice 也只能在自身同等 prerequisites 完成後執行其 UITEST，且 UITEST 不得取代 earlier checks 或驗證 incomplete、mock、trait-only、source-shape-only example。完成的 slice 連同其適用的 local checks、UITEST 與已簽署 release evidence bundle 才可作為 Release Gate。

範例位於獨立 consumer workspace，不是主 workspace member，也不得引用私有 crate。只有 trait、mock、未接 composition root 或 README 片段不算完成。

### 15. Skin 保持純資料並保留 Windows 行為

Skin schema 可以替換圖片、圖示、字型、按鈕狀態、nine-slice/vector、色彩、間距、透明度及 hit-test mask，但不執行 Rust、Lua、JavaScript 或 shader。視窗的作業系統幾何仍是矩形，Snap、最大化、resize、DPI、多螢幕、鍵盤焦點、UIA 與核心操作 fallback 由宿主保留。單一資產損壞只回退該資產，不能讓整個 UI 不可操作。

### 16. ABI ownership、drop 與 panic 邊界由 SDK 擁有

Rust-first 不表示跨 DLL 直接傳遞一般 Rust ownership。所有 ABI object、returned value 與 error 的 allocation origin、destructor、drop thread、library lifetime 都由 public SDK contract 明確指定；host 必須在 DLL resident 期間於允許的 thread 釋放 object。Factory、registrar、provider、renderer、service 與 destructor 邊界一律不得 unwind。SDK adapter 擁有 `#[sabi_trait]` erasure、factory 與 panic trampoline；外掛作者只實作 ordinary Rust traits。允許的 panic strategy 必須與 bundle fingerprint 一致；需要 typed panic translation 的 callback 不得以 `panic=abort` 冒充可恢復行為。

### 17. DLL trust 分為 pre-load、load-attempt 與 post-load 三道門

Package/hash/signature/target/PE policy 必須在 `LoadLibrary` 前完成，但 Windows DLL 的 `DllMain` 或 TLS initializer 可能在 host 取得 root export 前執行，因此設計不得宣稱 root validation 能阻止所有載入期 native code。Host 在呼叫 `LoadLibrary` 前寫入 durable load-attempt marker；若 child-process crash harness 或實際 startup 在載入期間異常終止，marker保持未完成，下次啟動以 Safe Mode 抑制嫌疑 package。成功載入且root/fingerprint驗證、registrar建立完成後，marker原子轉為registered/cleared；post-load validation或registration被typed拒絕時，DLL保持resident但不可dispatch，marker轉為`rejected-resident`診斷terminal而不假裝成crash。Root layout、SDK major、numeric semantics 與 GPUI fingerprint 必須在任何 SDK accessor、factory、registrar 或 callback 前驗證；registration 後的每次 native call 再使用 correlation-scoped marker。文件必須明示程序內 DLL 不是 sandbox。

### 18. Capability authority 與 generation envelope 統一在每次使用時驗證

Manifest registration validation 只建立 authority，不能取代 runtime authorization。所有 stream、tool、lock-owner、navigation、operation、virtual entry 與 renderer handle 都綁定 package、feature、interface、package incarnation、capability、authorized root 及相應的 location/item/refresh/container/job generation。Host 在 dispatch 與資源實際使用時重驗 envelope；feature disable、package update、folder/view change、F5 或 container mutation會 revoke 或使舊 handle stale。Path、tool payload 與 mutation target 在 use/commit 前以 identity 重驗，不能只在 preview 或 handle issuance 時按名稱驗證。

### 19. Disable/drain 是可線性化的 lifecycle transaction

Runtime disable 的順序固定為：原子關閉 new-dispatch gate、取消 jobs/streams/child processes、在 GPUI thread detach columns/views/panels、經 impact authority redirect/close virtual tabs、等待 correlation-scoped active call leases 到 deadline，最後進入 disabled 或 pending-restart。Late registration、sink output、cache publish 與 invalidation 必須因 gate/generation 被拒絕。Nested/concurrent callbacks各有獨立 call record；一個 return 不得清除另一個 marker。Deadline 對 native callback 是 cooperative boundary，host 不 unsafe-interrupt；stuck callback 只產 diagnostics/pending-restart，DLL仍 resident。

### 20. Implementation evidence、調整與 gate 不可靜默弱化

詳細 task plan 使用 append-only evidence index。每個 completed atomic task 必須有唯一 task ID（或 immutable shared record＋unique subcheck）、procedure/command、expected/actual、exit status或reviewer、artifact hashes、related gates、adjustment ID與timestamp。只有 passed、evidence-backed not-applicable、或帶 replacement 的 superseded 可結案；failed、blocked、stale、unexecuted與trait/mock-only不能完成。

- **A — task refinement：** 可調整 task split/order/owner/command，但不得改 scope、requirement、public contract、gate或threshold；永久ID與舊evidence lineage保留。
- **B — design/spec correction：** 在已核准scope內修正不合理假設；affected branch暫停，design/spec/tasks/evidence一起更新，dependent evidence標stale後重新驗證。
- **C — material change：** scope、公開承諾、ABI major/layout/numeric semantics、blocking gate/threshold/required evidence、platform/framework、permission、external write、destructive operation、non-fast-forward approval、protected tag/signing等必須先取得使用者核准。

任何 blocking gate、resource/performance threshold 或 required evidence 不得為了讓 candidate 通過而降低。Contract改變時，contract owner先落地與審查，所有consumer evidence失效後才可更新；共享 locks/manifests、local orchestration、UITEST manifest、evidence ledger、trust policy 與 final signed bundle只由 primary release integrator 整合。任何 external automation 的成功都不能完成 task，亦不能取代、弱化或成為上述本機 evidence 的權威。

### Active Rust tokei vertical slice

`rust-tokei-code-lines-column` follows the same one-implementation-first rule as the first two examples. The public ABI adds one SDK-owned `abi_stable` batch provider object, while authors implement an ordinary Rust trait. One invocation receives at most 128 owned item records and host-attested generation-bound `InputStreamV1` objects; it never receives filesystem paths, native handles, futures, or runtime objects. The host limits every input to 8 MiB and rejects stale results after F5, navigation, or tab generation changes.

The consumer statically links one exact locked Rust `tokei` dependency closure and never launches `tokei.exe`. It returns typed language/code/comment/blank/total counts, uses exact unsigned code lines for sorting, and reports binary, unknown, oversized, or otherwise unsupported sources as `Unsupported` rather than zero. Production UI installs one `Code lines` integer/background-batch Details column and reuses the public data-only cell plan for host-owned GPUI rendering. One setting toggles comment/blank detail. No generic multi-plugin scheduler, Lua/tool path, installer bundle change, or release evidence framework is part of this slice. Its single local UITEST runs only after the consumer, loader/runtime, production UI, README/package path, fixture, unit checks, and real-window smoke are complete.

## Risks / Trade-offs

- **[程序內 Rust 可使整個應用程式崩潰或死鎖]** → 載入前驗證、panic boundary、call marker、慢 callback 診斷、Safe Mode；文件明示其不是 sandbox。
- **[開發期 GPUI 更新造成大量外掛重建]** → immutable snapshot、原子 migration、精確 loader 診斷；Release freeze 後不再漂移。
- **[固定 toolchain 增加 SDK 發布成本]** → 固定 lock/fingerprint、離線 fixture、本機預填 Cargo cache、local Rust／PowerShell／UITEST gate 與已簽署 evidence bundle，換取可重現性。
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
6. Task 6 完成除 final gate 外的全部 leaves，包括 framework、consumer contract、implementation、production wiring、public SDK、fixture、docs、`.sepack` 與 inventory/composition；此前不執行 UITEST。6.4.7 是第一個 UITEST mapping/gate；此後每個後續範例均遵守相同 complete-vertical-slice eligibility。
7. 在預先填好的本機 Cargo registry cache、禁止網路的環境建置 SDK 與八個範例，執行 local Rust、PowerShell contract、`explorer-uitest` 相容、安全、效能與封裝 gate，產生及驗證已簽署 release evidence bundle；缺少 cache 時先 bootstrap，不建立或追蹤 vendor。
8. P0 通過後完成純資料 Skin schema、asset loader、透明/hit-test 與 accessibility/fallback gate。
9. RC cut 凍結 GPUI commit/tag、設 `release_frozen = true`，離線重建、產生並驗證已簽署的 local release evidence bundle。若 gate 失敗、evidence 缺失或 bundle 無法驗證，保留上一 host/SDK，或停用尚未完成的 feature；不遷移使用者資料到不可回復格式。

## Open Questions

- 第一個正式 Release 的 GPUI commit 與受保護 tag 在 RC cut 時由最後一個通過完整 gate 的 development snapshot 決定；目前開發 snapshot 不是 Release 承諾。
- Per-package CPU/I/O quota、Lock Owner TTL、Virtual Folder 資源上限與 Size Map memory budget 的預設數值由效能 fixture 校準，但其 bounded 行為與可診斷 terminal state 已由 specs 固定。
- Steam 簽署／審核、DLC entitlement 與付費作者流程留待獨立 change；本 change 只建立可替換的 Package Source／Entitlement Provider 接點。
