## 1. Roadmap Baseline and Shared Contracts

- [x] 1.1 建立五個 capability 的現況對照表，逐項標出可重用 contract、缺口、預定 crate、測試層級與 phase owner。
- [x] 1.2 盤點 `explorer-model` 現有 `ShellItemId`、location、tab、history、`ViewSettings` 與 generation invariants，記錄任何 path-only 假設。
- [x] 1.3 盤點 `explorer-shell-win` 的 STA、navigation、icon cache、watcher、context menu 與 search ownership，標出不得跨 apartment 傳遞的型別。
- [x] 1.4 盤點 `explorer-ui` 的 virtualized rows、view modes、panes、focus、actions、UIA 與 pointer capture，為新增 UI 找出 typed reducer 接點。
- [x] 1.5 盤點 release finalization、NSIS、UITEST manifest、headful scripts、diagnostics 與 evidence 文件，列出 broker binary 與新 suite 所需改動。
- [x] 1.6 在 `explorer-common` 定義共用 `RequestContext` 擴充，統一 request/correlation、tab、generation、deadline 與 cancellation identity。
- [x] 1.7 定義 exactly-one-terminal gate 與 late/duplicate event classification，補 success/error/cancel/timeout/disconnect race 單元測試。
- [x] 1.8 定義中央化 roadmap limits 結構，涵蓋 session、history、thumbnail、IPC、worker、deadline、quarantine 與 preview debounce 預設值。
- [x] 1.9 為 roadmap limits 加入非零、上下界、交叉欄位與版本驗證，拒絕會形成無界 queue/cache 的設定。
- [x] 1.10 擴充 reconstructible location descriptor，明確區分 filesystem path、Known Folder、absolute PIDL、parsing name 與 synthetic root。
- [x] 1.11 為 location descriptor 加入大小限制、canonical equality/hash、redacted diagnostics 與 invalid-byte/unknown-kind round-trip tests。
- [x] 1.12 新增 architecture gate，禁止 GPUI callback 同步執行 filesystem/COM/IPC、禁止 Preview Handler 在主程序 activate、禁止以 display name/path 取代 Shell identity。
- [x] 1.13 將 44 個 requirements 與 51 個 scenarios 登錄 UITEST manifest，指定 quick/full/interop/visual/soak suite、prerequisite 與 artifact glob。
- [x] 1.14 執行 pre-roadmap fmt/check/Clippy/tests/architecture/manifest/OpenSpec strict baseline，將命令、commit、硬體及既有失敗保存到 roadmap evidence。

## 2. Session Persistence Data Model and Store

- [x] 2.1 在 `explorer-model` 定義 current-version session envelope，含 schema、checksum、write generation、app/Windows provenance 與 payload。
- [x] 2.2 定義 persisted main-window placement，只保存 normal bounds、maximized state、來源 monitor work area 與來源 DPI，不保存 live HWND 狀態。
- [x] 2.3 定義 persisted tab record，含 stable tab key、current location、bounded back/forward history、tab-local durable view settings 與 active marker。
- [x] 2.4 定義 persisted `ViewSettings` 映射，涵蓋 view mode、sort/group、Details columns/order/width、pane、compact、hidden items 與 extensions。
- [x] 2.5 定義 persisted Quick Access pin record 與 ordering key，拒絕 duplicate identity、non-reconstructible item 與 privacy-excluded entry。
- [x] 2.6 建立 runtime model → persisted snapshot 的 pure projection，明確排除 selection、rename editor、clipboard、preview、search results、credentials 與 operations。
- [x] 2.7 建立 persisted snapshot → validated restore plan 的 pure parser，所有失敗回傳具名 field/path/version/invariant 錯誤。
- [x] 2.8 加入 tab count、history depth、descriptor bytes、column count/width、window dimensions 與 total payload size bounds。
- [x] 2.9 為 current schema 建立 golden JSON fixtures、deterministic serialization、checksum、unknown-field 與 enum-version tests。
- [x] 2.10 定義 `SessionStore` platform-neutral trait 與 load/save/reset outcome，讓 model/UI tests 不依賴 Windows filesystem。
- [x] 2.11 在 app/Windows adapter 實作 `%LOCALAPPDATA%\RustGpuiExplorer\state\v1` 路徑解析與 owned directory 建立。
- [x] 2.12 實作同目錄 temporary snapshot write、flush、close 與 replace 前驗證，任何錯誤不得破壞 current snapshot。
- [x] 2.13 實作 Windows atomic replacement、last-known-good backup rotation 與 replacement 後 directory/file durability 處理。
- [x] 2.14 實作 load precedence：current valid → backup valid → defaults，並保存 corrupt/unsupported artifact 的 privacy-safe diagnostics。
- [x] 2.15 實作精確 reset scopes：session-only、view-settings-only、Quick Access-only 與全部 roadmap state，禁止刪除其他 app data。
- [x] 2.16 注入 create/open/write/short-write/flush/close/replace/backup/access-denied/disk-full 錯誤，逐一驗證 recovery 與既有 snapshot 不變。
- [x] 2.17 建立 supported prior-version migration registry 與 v0→v1 fixture，驗證 recognized data 保留、未知版本安全拒絕、下一次 save 升級 current schema。

## 3. Session Lifecycle, Restore, and User Experience

- [x] 3.1 建立 persistence coordinator，訂閱 accepted durable model transitions，而非 UI gesture 或暫態 render state。
- [x] 3.2 實作 debounce/coalescing state machine，涵蓋 idle write、dirty-during-write、write failure retry、shutdown flush 與 cancellation。
- [x] 3.3 將 serialization 與 filesystem write 排程到 background job，加入 callback duration test 證明 GPUI input/paint/layout 不執行 I/O。
- [x] 3.4 在 app startup load/validate restore plan，無 state、disabled restore、corrupt state 與 load error 都必須繼續啟動。
- [x] 3.5 依 saved order 建立 tab shells，再非同步 resolve locations，避免一個慢或壞 location 阻塞其他 tabs 顯示。
- [x] 3.6 實作 invalid current location fallback：nearest valid saved ancestor → configured start location，並留下 tab-local unavailable reason。
- [x] 3.7 還原 bounded Back/Forward history，逐 entry 延遲驗證；stale entry 被略過時不得破壞 remaining order。
- [x] 3.8 還原 saved active tab 與其 file-view/address/navigation focus；active tab 無效時選擇第一個成功 tab。
- [x] 3.9 透過既有 typed reducer 還原每個 tab 的 view/sort/group/columns/panes/visibility settings，禁止 active-tab values 洩漏到其他 tab。
- [x] 3.10 使用 current monitor work areas 與 DPI 轉換 window bounds，涵蓋 monitor 拔除、解析度/taskbar 改變、negative coordinates 與極端 corrupt bounds。
- [x] 3.11 還原 maximized state 時先套用可達 normal bounds，再 maximize，驗證 caption controls 始終位於 active work area。
- [x] 3.12 在 Folder Options/General 加入 restore toggle、startup choice 與 reset actions，所有命令走 typed actions 並具 UIA name/state。
- [x] 3.13 為 reset session/view/all 加入確認、成功/error notification 與 retry，任何失敗不得留下半刪除 state。
- [x] 3.14 整合 orderly shutdown final flush、重複 shutdown、window-close/quit race 與 diagnostics flush ordering。
- [x] 3.15 建立 round-trip、partial stale、disabled restore、multi-tab independent settings、rapid changes、reset scopes 與 crash-between-replace tests。
- [x] 3.16 建立 headful two-process restart harness，保存 before-state、forced/clean exit、after-state、window bounds、tabs、focus、UIA 與 screenshots。
- [x] 3.17 執行 session capability gate：quality commands、quick/full/visual、10 次 clean/crash restart soak（各 5 次）、每輪完整 before/after oracle、resource snapshot、spec coverage、rollback 與文件更新。

## 4. Thumbnail Contracts, Scheduling, and Shell Retrieval

- [x] 4.1 定義 `ThumbnailRequestKey`，包含 Shell identity、physical size、DPI scale、thumbnail/icon mode、source generation 與 relevant theme/association generation。
- [x] 4.2 定義 source/status/fallback enums，區分 memory hit、disk hit、Windows cache、provider extract、icon fallback、offline、unsupported、timeout 與 corrupt。
- [x] 4.3 定義 owned pixel payload invariants：width/height/stride/format/alpha/byte length/maximum decoded bytes，拒絕 overflow 與 inconsistent buffers。
- [x] 4.4 定義 thumbnail request/progress/terminal events，沿用共用 request context 與 exactly-one-terminal gate。
- [x] 4.5 在 UI/model 發布 active viewport item range 與 bounded before/after prefetch range，不暴露 GPUI entities 給 scheduler。
- [x] 4.6 為 Details/List/Content/Small/Medium/Large/Extra Large modes 定義是否取 thumbnail、目標 logical size 與 physical rounding contract。
- [x] 4.7 在 `explorer-jobs` 實作 bounded priority queue，active visible > active prefetch > background visible，穩定處理同 priority ordering。
- [x] 4.8 實作相同 key 的 cross-tab/cross-consumer deduplication、consumer refcount、priority promotion 與 shared terminal fan-out。
- [x] 4.9 實作 consumer 離開、tab navigation、view-size change、file generation change 時 cancellation；不可中止的 late result 必須被 suppress。
- [x] 4.10 實作 concurrency、queue length、decoded-in-flight bytes 與 per-provider limits，超限時保留 icon 而非阻塞 render。
- [x] 4.11 在 Shell STA 實作 Windows thumbnail cache/query adapter，所有 COM pointers 與 native bitmap handles 以 RAII 留在 owning apartment。
- [x] 4.12 實作 provider extraction adapter，套用 request size、cache-only/flags、deadline 與 typed HRESULT mapping。
- [x] 4.13 將 HBITMAP/WIC/Shell payload 複製成 owned pixels，處理 stride、premultiplied alpha、BGRA→render boundary、EXIF orientation 與 zero/huge dimensions。
- [x] 4.14 盤點 offline/cloud placeholder attributes，建立 cache-only branch；viewport visibility 不得開啟 content stream 或觸發 hydration。
- [x] 4.15 實作 authentic Shell icon/overlay fallback，保留 association/overlay generation 並禁止任意 placeholder glyph 冒充 thumbnail。
- [x] 4.16 建立 fake provider tests：success、cache hit、unsupported、offline、slow、cancel、duplicate、out-of-order、malformed buffer、huge dimensions 與 HRESULT failure。
- [x] 4.17 建立 real-Shell retrieval test matrix，涵蓋 JPG/PNG/GIF/rotated image/PDF/document/media/archive/folder/unknown/overlay 與 placeholder availability。

## 5. Thumbnail Rendering, Cache, Invalidation, and UX

- [x] 5.1 在 file-view row/tile state 加入 generation-scoped thumbnail slot，初始立即顯示 icon，成功後只更新相同 identity/key。
- [x] 5.2 實作 owned pixels → GPUI texture adapter 與 texture RAII，decode/cache thread 不得持有 window/entity handle。
- [x] 5.3 實作 tile image fit/crop、aspect ratio、alpha、selection/hover/focus、overlay positioning 與 Explorer-like padding contracts。
- [x] 5.4 驗證 thumbnail progressive update 不改變 sort order、selection identity、focused item、scroll anchor、hit target 或 inline rename state。
- [x] 5.5 實作 Ctrl+wheel typed zoom action，逐級切換 icon sizes，保留 nearest visible anchor 並取消舊尺寸 requests。
- [x] 5.6 實作 decoded-byte-cost memory LRU，加入 insert/get/promote/evict/oversized-reject/clear 與 shared-consumer tests。
- [x] 5.7 將 memory budget、entry count、current bytes、evictions 與 pinned in-flight bytes 接入 diagnostics/performance snapshot。
- [x] 5.8 建立 cold/warm benchmark，比較無 disk cache、Windows cache only 與 project disk cache，先保存啟用判斷證據。
- [x] 5.9 實作 versioned/checksummed disk entry header、opaque hashed key、bounded dimensions/bytes 與 atomic write；禁止保存來源 path/content。
- [x] 5.10 實作 disk cache index/eviction/clear/corruption recovery，I/O failure 必須回退 Shell/icon 且不污染 memory cache。
- [x] 5.11 實作 watcher-based modify/replace/rename/delete invalidation與 association/overlay generation invalidation。
- [x] 5.12 實作 DPI、requested size、theme、Windows build、schema 與 explicit settings change invalidation，不可 reuse 尺寸不符 pixels。
- [x] 5.13 在 Folder Options/View 加入 always-show-icons/thumbnail toggle 與 clear thumbnail cache command，補 typed action、UIA 與 error notification。
- [x] 5.14 建立 1,000 次 fast-scroll/zoom/resize/navigation/replacement/cache-corruption/memory-pressure soak，保存 queue/cache/texture/GDI/handle/latency terminal metrics，UI 互動由既有 UTIT 執行。
- [x] 5.15 建立 light/dark/high-contrast、100/125/150/175/200% DPI、compact window 與 Explorer screenshot comparator evidence。
- [x] 5.16 執行 thumbnail capability gate：quality、quick/full/visual/soak、real provider matrix、cache reset/rollback、文件與 truthful limitations。

## 6. Shell Namespace Identity, Enumeration, and Metadata

- [x] 6.1 定義 namespace root enum 與 availability descriptor，涵蓋 Home、Quick Access、Known Folders、This PC、drives、Libraries、ZIP、Recycle Bin、Network 與 third-party。
- [x] 6.2 定義 item/container capability bitset，至少涵蓋 enumerate/open/rename/delete/restore/empty/pin/copy/paste/drop/search/properties/context/thumbnail/preview。
- [x] 6.3 定義 Shell display/parsing identity、absolute PIDL ownership、serialization policy 與 nonserializable reason；禁止 display name 作 identity。
- [x] 6.4 在 navigation adapter 實作 Known Folder ID → `IShellItem` resolve 與 reverse reconstructible descriptor tests。
- [x] 6.5 實作 absolute PIDL clone/serialize/deserialize/validate RAII，覆蓋 empty/truncated/oversized/misaligned bytes。
- [x] 6.6 實作 parsing-name resolve，限制最大長度並區分 user address input error、provider unavailable 與 unsupported scheme。
- [x] 6.7 擴充 Shell child enumeration command 以接受 path/non-path container，維持 bounded batches、generation、deadline、cancel 與 terminal event。
- [x] 6.8 在 Shell STA 實作 `IEnumShellItems`/folder enumeration ownership，批次間 pump messages 並在 cancellation/deadline 後停止發送。
- [x] 6.9 從 public Shell attributes 映射 capability bitset，針對未知/失敗採 deny-by-default 並附 unavailable reason。
- [x] 6.10 定義 typed property key/value/format 與 dynamic column descriptor，不允許 UI 直接處理 VARIANT/PROPVARIANT。
- [x] 6.11 實作 viewport-priority Shell property retrieval、formatting、sort/group identity 與 per-item failure fallback。
- [x] 6.12 實作 namespace icons、overlays 與 thumbnail eligibility，共用 Shell identity/cache 而非另建 root-specific icon table。
- [x] 6.13 實作 provider change notification 能力偵測；支援者註冊事件，不支援者提供 bounded refresh 並標示 limitation。
- [x] 6.14 建立 fake namespaces：duplicate names、non-path、nonserializable、slow batches、out-of-order、capability change、malformed metadata、failure/cancel。
- [x] 6.15 建立 real fixtures：Desktop/Known Folders/This PC/drives/Libraries/ZIP/Recycle/Network root 與安全可用 third-party namespace。
- [x] 6.16 為 namespace enumeration 加入 first-item/first-viewport/total latency、batch/queue/outstanding/COM handle telemetry。
- [x] 6.17 執行 model/Shell namespace contract gate，確認 filesystem navigation 舊測試與 100k/stale/watcher invariants 全數維持。

## 7. Explorer Namespace Surfaces, Navigation, and Operations

- [x] 7.1 在 navigation pane model 建立 stable root tree 與 availability state，root 不可用時隱藏或顯示具名 unavailable，而非假資料。
- [x] 7.2 渲染 Home/Quick Access/This PC/Known Folders/Libraries/Network/Recycle roots與children，接入 disclosure、selection、focus、icon、UIA expanded/selected。
- [x] 7.3 實作 Home aggregation service，合併 pinned/recent reconstructible items，套用 privacy filter、dedupe、ordering、missing-item 與 empty/loading/error states。
- [x] 7.4 定義 recent-item ingestion policy，只接受成功使用且可重建的 location/item，加入 capacity、expiry、clear 與 sensitive-root exclusions。
- [x] 7.5 實作 Quick Access pin/unpin typed commands、duplicate prevention、stable reorder、persist failure rollback 與 UI/context menu feedback。
- [x] 7.6 擴充 breadcrumb model 支援 non-path ancestry、overflow chevrons、fresh child enumeration、root icons 與 stale menu cancellation。
- [x] 7.7 擴充 editable address parser 支援 Known Folder/parsing names，錯誤保持原 tab/location 並提供可採取的訊息。
- [x] 7.8 擴充 Back/Forward/Up/Refresh/New Tab/middle-click，使 path 與 namespace 使用同一 history/generation/focus pipeline。
- [x] 7.9 讓 command bar與keyboard enablement查詢 capability reducer；能力不足時 disabled/UIA unavailable，不 dispatch 假命令。
- [x] 7.10 讓 context menu query依 background/single/multi selection及namespace capability取得正確 Shell items與owner window。
- [x] 7.11 讓 Clipboard/OLE data object、paste/drop target 與 effect negotiation接受 path/non-path combinations，保留 Explorer interop。
- [x] 7.12 讓 file operations對 namespace items跳過 path preflight，改依 Shell capability與 per-item HRESULT 回報 progress/partial failure。
- [x] 7.13 實作 ZIP namespace browse/open/copy-out/copy-in（能力允許時）與 cancel/progress/stale tests，不用自行解析 archive 格式。
- [x] 7.14 實作 Library aggregate browse與 member navigation，history/breadcrumb/persistence維持 Library identity而非偷換第一個 path。
- [x] 7.15 實作 Network root discovery/navigation，使用 Windows-owned authentication UI，處理 offline/access-denied/cancel，不保存 enterprise credentials。
- [x] 7.16 實作 Recycle Bin browse/property/restore/permanent-delete/confirmed-empty，所有 destructive action 需確認與 per-item outcome。
- [x] 7.17 讓 sort/group/dynamic columns/view modes/thumbnails/selection/status counts在各 namespace依 capability運作，unsupported state明確可見。
- [x] 7.18 建立 keyboard/mouse/UIA matrix：root expand、activate、context、pin、history、address、breadcrumb、tab、focus restore與error recovery。
- [x] 7.19 執行 namespace capability gate：quick/full/interop/visual/soak、Explorer matrix、DPI/theme/high-contrast、resource telemetry、rollback與文件。

## 8. Broker Threat Model, Protocol, and Process Isolation

- [x] 8.1 撰寫 broker threat model，涵蓋 malicious/hung/crash/reentrant/oversized/protocol confusion/stale/privilege/unload/path disclosure 與對應控制。
- [x] 8.2 定義 broker supervisor、disposable worker、app client 三方 trust boundary、責任、允許訊息與禁止依賴。
- [x] 8.3 新增 broker protocol library crate，只依賴 common owned types，不依賴 GPUI、model entity或 apartment-affine COM。
- [x] 8.4 定義 protocol magic/version/feature negotiation/session nonce/request header/frame length/checksum 與最大尺寸。
- [x] 8.5 定義 start/progress/cancel/terminal/heartbeat/shutdown messages，所有 union variant 具 explicit numeric identity 與 unknown rejection。
- [x] 8.6 實作 incremental frame encoder/decoder，處理 partial reads/writes、EOF、oversize、checksum、unknown version/type 與 malformed length。
- [x] 8.7 建立 decoder corpus與 property/fuzz tests，證明任意 bytes 不 panic、不配置超限 memory、不執行 operation。
- [x] 8.8 實作 inherited secret/handle-based local authentication與一次性 handshake，拒絕未認證 client 及 replayed session。
- [x] 8.9 實作 bounded named-pipe/local IPC transport、read/write deadlines、queue backpressure、disconnect與shutdown ownership。
- [x] 8.10 新增 broker supervisor binary，嵌入 x64 manifest/version info，啟動後回報 ready/protocol/build marker。
- [x] 8.11 新增 disposable worker binary，依 operation class 初始化必要 COM apartment，不建立 GPUI/model runtime。
- [x] 8.12 實作 restricted token policy，移除不必要 privileges，設定 integrity/desktop/handle inheritance並保存有效 policy evidence。
- [x] 8.13 實作 Windows Job Object limits：active process、memory、CPU/wall-time策略、kill-on-close與禁止未授權 child processes。
- [x] 8.14 實作 item descriptor與duplicated handle capability傳遞，每個 request明列 operation class與允許資源。
- [x] 8.15 實作 supervisor worker assignment、spawn failure、ready timeout、graceful cancel、forced Job termination與replacement。
- [x] 8.16 實作 app-side broker lifecycle：binary locate/version check、lazy start、health、restart、shutdown、orphan cleanup與typed unavailable。
- [x] 8.17 實作 request/worker/handler deadline與exactly-one terminal race，涵蓋 success/error/cancel/timeout/crash/disconnect同時發生。
- [x] 8.18 實作 handler failure counters、exponential backoff、quarantine expiry、manual retry/reset與最大記錄容量。
- [x] 8.19 實作 privacy-safe broker diagnostics/crash reports，保留handler/protocol/correlation而redact sensitive path/content/secret。
- [x] 8.20 建立 controlled workers：normal/slow/reentrant/oversized/hung/crash/privilege/child-process/late-terminal/unload-failure，驗證隔離與recovery。

## 9. Broker Migration, User Recovery, and Distribution

- [x] 9.1 定義 context-menu broker request/result payload，涵蓋 background/single/multi items、owner HWND contract、menu tree、verb與invoke outcome。
- [x] 9.2 將 controlled context-menu query/show/invoke移入 worker，保持 `IContextMenu2/3` message forwarding、submenu、keyboard與owner-draw semantics。
- [x] 9.3 建立既有 in-process與brokered context-menu differential tests，確認 command identity、resulting file effects、cancel與error一致。
- [x] 9.4 定義 thumbnail broker payload，傳遞 descriptor/handle、size/flags而只回owned bounded pixels/status，不傳 COM objects。
- [x] 9.5 將 untrusted thumbnail/provider extraction切換到 disposable worker，保留scheduler/cache key/deadline/fallback與terminal contract。
- [x] 9.6 定義 namespace broker batch/capability/property messages，允許 partial batches但限制item count/bytes與generation。
- [x] 9.7 將指定 third-party/slow namespace activation與enumeration切換到worker，failure後filesystem與其他roots保持可用。
- [x] 9.8 在 diagnostics UI顯示 broker unavailable/version mismatch/crash/timeout/quarantine與retry，正常時不暴露多餘內部控制。
- [x] 9.9 對 missing/wrong-version/tampered/access-denied/startup-crash/unexpected-exit broker建立fault injection與safe fallback tests。
- [x] 9.10 更新 Cargo workspace/release finalization，驗證所有broker binaries為x64、manifest/version/protocol/build相容且非空。
- [x] 9.11 更新 `build_install.bat`/Lua/NSIS，把broker files納入install/upgrade/uninstall與missing-file verification，不刪除user data。
- [x] 9.12 更新CI artifact與fresh-install smoke，從installed path啟動app、brokered request、upgrade、uninstall並檢查orphan process/file。
- [x] 9.13 執行 installed 7-Zip與其他安全可用extensions的context/thumbnail/namespace interop，truthfully SKIP不存在handler。
- [x] 9.14 執行 mixed broker soak，量測IPC bytes/queues/workers/restarts/crash/timeout/quarantine/handles/memory與terminal balance。
- [x] 9.15 執行 broker capability gate：security review、quality、quick/full/interop/soak、installer、rollback、threat/evidence與limitations文件。

## 10. Preview Pane Model, Broker Host, and Interaction

- [x] 10.1 定義 preview eligibility與selection summary，區分none/single eligible/folder/multiple/unsupported/offline/quarantined/error。
- [x] 10.2 定義 handler identity、registration source、initialize-by-file/stream/item mode與privacy-safe diagnostic descriptor。
- [x] 10.3 定義 preview lifecycle state machine：closed/idle/debounce/loading/visible/fallback/unloading/failed，列出所有合法transition。
- [x] 10.4 定義 preview request/host bounds/DPI/theme/focus/accelerator/unload/terminal broker messages與最大payload。
- [x] 10.5 在 per-tab `ViewSettings` 加入 Preview Pane visible/width，接入session persistence migration與tab-independent tests。
- [x] 10.6 在 View menu/command bar加入 Preview Pane typed command與標準shortcut，command checked/disabled狀態來自同一reducer。
- [x] 10.7 實作 preview splitter layout、minimum/maximum width、compact-window collapse、pointer capture、keyboard resize與double-click reset。
- [x] 10.8 實作 selection debounce/generation coordinator，rapid selection只activate最後一個eligible item。
- [x] 10.9 實作 zero/multiple/folder/unsupported/offline fallback，禁止offline visibility自動hydration並提供provider-owned availability action（若有）。
- [x] 10.10 在broker worker解析 Preview Handler registration，驗證CLSID/bitness/interface與quarantine後再activate。
- [x] 10.11 實作 `IInitializeWithFile`、`IInitializeWithStream`、`IInitializeWithItem` capability negotiation，僅提供所需descriptor/handle。
- [x] 10.12 實作broker-owned host HWND與handler `SetWindow`/`DoPreview`，所有native handles具RAII與single owner。
- [x] 10.13 實作 app preview host boundary與cross-process HWND attach/position/clip，DPI awareness不相容時安全fallback。
- [x] 10.14 實作pane/window resize、DPI/theme、move/maximize/restore bounds更新，舊generation bounds不得套到新handler。
- [x] 10.15 實作focus query/set、Tab traversal、mouse activation與`TranslateAccelerator` forwarding，不攔截app保留快捷鍵。
- [x] 10.16 實作 selection/tab/pane/window change時idempotent unload；deadline後terminate worker並suppress late callbacks。
- [x] 10.17 實作 loading/icon-properties/error/quarantine fallback chrome、localized action、retry與UIA live status。
- [x] 10.18 為 lookup/initialize/load/resize/input/unload 分別設定deadline與typed error，驗證一次失敗後下一個safe handler可載入。

## 11. Preview Compatibility, Accessibility, and Soak Evidence

- [x] 11.1 建立 controlled Preview Handlers，分別覆蓋 file/stream/item initialization與可驗證render marker。
- [x] 11.2 擴充 fixtures覆蓋focus/accelerator/resize/reentrant/slow/malformed HWND/crash/hang/unload failure與oversized response。
- [x] 11.3 建立 deterministic lifecycle tests：rapid selection、tab switch/close、pane toggle、window deactivate、duplicate/late messages與shutdown。
- [x] 11.4 建立 real-handler inventory，記錄Windows build、CLSID、bitness、file types、initialization mode與availability，不以缺少handler當PASS。
- [x] 11.5 執行 image、text/code、PDF/document、media/property fallback、archive/unsupported與available third-party handler matrix。
- [x] 11.6 建立keyboard-only matrix，涵蓋View command、shortcut、file view→splitter→preview→chrome→file view focus與close/error return。
- [x] 11.7 建立UIA/screen-reader matrix，驗證pane/splitter/loading/error/fallback/handler boundary的name/role/state/action/live announcement。
- [x] 11.8 建立light/dark/high-contrast fallback chrome evidence，handler不支援theme時明確記錄public API limitation。
- [x] 11.9 建立100/125/150/175/200% DPI host/bounds/hit-target matrix，requested/actual DPI不符時truthful SKIP raster pass。
- [x] 11.10 建立mixed-monitor move/maximize/restore test，驗證cross-process HWND clipping、scale、focus與stale bounds suppression。
- [x] 11.11 執行supported/unsupported/large/malformed/slow/crashing preview循環soak，跨tabs與restart重複。
- [x] 11.12 在soak量測app/broker/worker process、threads、GDI/User handles、HWND、files、mapped buffers、IPC、working set與outstanding requests。
- [x] 11.13 驗證preview content、COM objects、surfaces、streams與temporary data不進session store、thumbnail cache或diagnostic export。
- [x] 11.14 執行 preview capability gate：quality、quick/full/interop/visual/soak、Explorer comparison、compatibility matrix、rollback與limitations文件。

## 12. Umbrella Integration, Parity Closure, and Handoff

- [x] 12.1 建立combined deterministic flow：load session→restore tabs/settings→namespace navigation→thumbnail→broker→preview→save session。
- [x] 12.2 建立combined failure flow：corrupt state、stale namespace、thumbnail corruption、broker crash/hang、preview failure後仍可filesystem navigation與clean shutdown。
- [x] 12.3 建立combined real-Windows E2E，涵蓋Home/Quick Access/This PC/Library/ZIP/Recycle/Network、fast-scroll thumbnails、brokered extension、preview與restart。
- [x] 12.4 驗證所有新typed commands的mouse/keyboard/context entry points共用同一reducer/outcome，disabled action不dispatch。
- [x] 12.5 驗證所有新增UI的focus restore、UIA、high contrast、reduced motion、compact window與light/dark theme一致性。
- [x] 12.6 在實際100/125/150/175/200% DPI與mixed-monitor設備執行combined matrix；設備不足保留prerequisite與未驗證狀態。
- [x] 12.7 與Windows File Explorer逐項比較commands、menus、mouse/keyboard、focus、namespace、thumbnail、preview與error behavior。
- [x] 12.8 修正所有可由public API達成的差異；無法匹配者必須記錄Windows build/provider/API、fallback與user impact。
- [x] 12.9 執行combined長時間soak：session churn、tab/navigation、thumbnail pressure、namespace failures、broker restart/quarantine與preview cycling。
- [x] 12.10 驗證combined soak的memory、GDI/User、threads、processes、workers、queues、caches、IPC、files、HWND與terminal events沒有無界成長。
- [x] 12.11 執行 `cargo fmt --all -- --check`、locked workspace/all-target check、Clippy warnings denied、tests/doc-tests與architecture/security gates。
- [x] 12.12 執行UITEST quick/full/interop/visual/soak，逐 requirement檢查PASS或具prerequisite的truthful SKIP、logs與artifacts。
- [x] 12.13 執行release finalization、broker binary validation、installer build/fresh install/upgrade/uninstall與installed-path E2E。
- [x] 12.14 執行privacy/threat/unsafe-COM-handle/IPC/cache/persistence migration/accessibility/dependency-license/destructive-operation reviews並關閉finding。
- [x] 12.15 更新STATUS、IMPLEMENTATION_PLAN、PARITY_MATRIX、READMEs、Folder Options/help、UITEST、installer、state/cache/broker/preview evidence文件。
- [x] 12.16 產出final handoff，列出binaries、capabilities、commands、settings/state/cache paths、reset/recovery、supported providers/handlers、tests、limitations、rollback與post-roadmap工作。
