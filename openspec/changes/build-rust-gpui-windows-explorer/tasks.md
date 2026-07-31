## 1. 建置前基線與決策紀錄

- [x] 1.1 在 `docs/STATUS.md` 記錄目前工作區只有設計／OpenSpec artifacts、尚無 Cargo 專案，並將 M0、M1 標為進行前狀態。
- [x] 1.2 在 `docs/IMPLEMENTATION_PLAN.md` 摘錄本 change 的完整範圍：M0/M1 foundation、多分頁真實資料夾、檔案操作、Clipboard/OLE/context menu 與 search，並連結 proposal、design 與全部 capability specs。
- [x] 1.3 記錄本機 `rustc -Vv`、`cargo -V`、Windows edition/build、architecture 與 Visual Studio Build Tools/Windows SDK 版本，保存為可重現建置基線。
- [x] 1.4 驗證 `gpui-ce/gpui-ce` commit `6c799b8e994266233014cea66d7769675ec1967c` 可取得，並將 source、revision 與 `gpui-component` 不相容查核證據寫入 `docs/STATUS.md`。
- [x] 1.5 建立一筆依賴 capability spike 記錄，列出該 GPUI revision 在 Windows 可用的 window options、custom titlebar、caption/drag region、focus、key binding 與 HWND 存取 API，未知項目明確標成待驗證。
- [x] 1.6 建立 `docs/PARITY_MATRIX.md` 初始欄位：Capability、Milestone、Status、Automated Evidence、Manual Evidence、Known Difference、Windows/API Limitation，並為本 change 全部 requirements 建立未完成列。
- [x] 1.7 建立 `docs/MANUAL_TESTS.md` 的環境、前置條件、實際結果、證據路徑與未驗證原因模板，禁止以預期結果填充實際結果。

## 2. Cargo workspace 與 Windows-only 政策

- [x] 2.1 建立根 `Cargo.toml` workspace，納入 `explorer-app`、`explorer-ui`、`explorer-common`、`explorer-model`、`explorer-shell-win`、`explorer-jobs`、`explorer-search` 與 `explorer-test-support`，並確認每個 member 在後續 production flow 或 tests 有明確責任。
- [x] 2.2 在根 manifest 設定共用 Rust edition、最低支援 Rust version、license、repository metadata、resolver 與一致的 workspace lint policy。
- [x] 2.3 在 workspace dependencies 以 submodule path 固定 GPUI-CE revision，移除不相容的 `gpui-component` 與 Zed source patch以保持單一 GPUI type graph；加入本 slice 實際使用的 `windows` features、logging 與 error dependencies，避免寬泛 `features = ["all"]`。
- [x] 2.4 建立 `crates/explorer-common/Cargo.toml` 與最小 `src/lib.rs`，先暴露 diagnostics/lifecycle，後續依 specs 加入 IDs、request context、typed command/event、errors 與 cancellation。
- [x] 2.5 建立 `crates/explorer-shell-win/Cargo.toml` 與最小 `src/lib.rs`，以 target-specific dependency 精確限制 `windows` crate 的 COM、Shell、OLE、message、file-operation、watcher 與 Search features。
- [x] 2.6 建立 `crates/explorer-ui/Cargo.toml` 與最小 `src/lib.rs`，只依賴 common、model 與 GPUI-CE，確認 manifest 不含 `explorer-shell-win`、`gpui-component` 或 Windows Shell features。
- [x] 2.7 建立 `crates/explorer-app/Cargo.toml` 與 `src/main.rs`，由 binary 作為所有 production crates 的 composition root，設定 Windows subsystem 策略且保留可診斷的 debug 啟動方式。
- [x] 2.8 在 app 與 shell-win 加入 compile-time Windows target guard，讓非 Windows target 以專案自訂訊息快速失敗。
- [x] 2.9 產生並提交 `Cargo.lock`，連續執行兩次 `cargo metadata --locked --format-version 1`，確認第二次不修改 lockfile。
- [x] 2.10 以 `cargo tree -d` 與 `cargo metadata` 檢查 GPUI 相關 revision 沒有意外分叉，將必要的同版重複與任何無法消除項目記錄到狀態文件。
- [x] 2.11 新增架構檢查測試或 script，驗證 `explorer-ui` 不依賴 `explorer-shell-win`，並在 CI 中執行。
- [x] 2.12 建立 Windows CI workflow，使用 lockfile 執行 fmt、check、clippy、test，並將 GUI-only case 留給有明確標記的手動／專用 runner 流程。
- [x] 2.13 將 GPUI source submodule 從 Zed 改為 `vendor/gpui-ce`，`.gitmodules` 指向 `https://github.com/gpui-ce/gpui-ce.git`，gitlink 固定 `6c799b8e994266233014cea66d7769675ec1967c`，並確認 fresh clone 可用 `git submodule update --init --recursive` 重現。
- [x] 2.14 重新產生 lockfile，驗證 workspace 與 `gpui_windows` 只解析到一個 `gpui 0.2.2` path package；最終 exe 的 manifest 由 GPUI-CE `windows-manifest` 唯一提供，app resource 只提供 VERSIONINFO 以避免 resource ID 1 衝突；確認沒有 `gpui-component`、`gpui_platform`、殘留 `vendor/zed`、舊 Zed gitlink 或第二套 GPUI types，最後記錄 lockfile hash。
- [x] 2.15 針對固定 GPUI-CE revision 重新查核 `WindowOptions`、custom titlebar/caption/drag region、focus、key binding、HWND 與 Windows platform API；更新 capability spike，舊 revision 才有的 API 不得保留為已確認。
- [x] 2.16 在 GPUI-CE dependency graph 上重新執行 architecture check、fmt、workspace check、clippy、test 與 headful startup smoke；將失敗視為相容性問題，不以取消 patch 或混用兩套 GPUI 規避。

## 3. Diagnostics 與錯誤基礎

- [x] 3.1 先為 diagnostics configuration 撰寫 unit tests，涵蓋 app version、log location、敏感路徑 redaction 與重複初始化的確定行為。
- [x] 3.2 在 `explorer-common` 實作 diagnostics 初始化，輸出結構化啟動階段、app version、process/thread context 與乾淨關閉事件。
- [x] 3.3 定義可擴充的 `ExplorerError`／startup stage error，保留 operation、location、Win32/HRESULT、可恢復性、安全 user message 與 technical source；各功能階段只加入實際使用的錯誤變體。
- [x] 3.4 先為 panic report formatting 撰寫測試，確認包含 thread、location、version、backtrace availability，且測試用敏感路徑會被遮蔽。
- [x] 3.5 安裝 panic hook，將 panic report 寫入診斷目的地並串接既有 hook；避免在 hook 中再次 panic 或執行無界阻塞工作。
- [x] 3.6 加入 opt-in test entrypoint 或 integration harness 觸發受控 panic，驗證報告存在、內容安全且程序回傳非零狀態。
- [x] 3.7 為 log flush 與 diagnostics shutdown 建立 idempotent 路徑，並以重複呼叫測試確認不 panic、不遺失最後事件。

## 4. Windows 程序與 Shell STA 生命週期

- [x] 4.1 先定義並測試 `ShellStaState` 狀態轉移：Created → Starting → Ready → Stopping → Stopped，以及每個非法轉移的錯誤。
- [x] 4.2 建立最小 `ShellStaHandle` 生命週期介面，先允許 `start`、readiness observation 與 idempotent `shutdown/join`；後續 command endpoint 必須由 common typed contract 擴充而非另建平行通道。
- [x] 4.3 在專用執行緒呼叫 `CoInitializeEx(COINIT_APARTMENTTHREADED)`，將 HRESULT 映射為 startup stage error，並確保 `CoUninitialize` 僅在成功初始化的同一執行緒執行。
- [x] 4.4 建立 STA message pump 與明確 shutdown signal，確保等待訊息時仍能回應關閉，不使用 busy loop。
- [x] 4.5 為 ready handshake 加入 bounded startup timeout 與 correlation logging，失敗時回收 thread/handle 並回傳原始 HRESULT 或 timeout context。
- [x] 4.6 為 shutdown/join 加入 bounded diagnostic threshold；超過門檻時記錄 thread/state，不靜默卡住 application 關閉。
- [x] 4.7 撰寫正常啟停 integration test，驗證 ready、message pump、shutdown、join 與最終 Stopped 狀態。
- [x] 4.8 撰寫重複 shutdown／drop 順序測試，驗證不 double-uninitialize、不 double-close、不 panic。
- [x] 4.9 撰寫初始化失敗注入測試，驗證 composition root 能觀察失敗且所有先前取得資源都被釋放。
- [x] 4.10 在 source 的每個 unsafe block 旁記錄 pointer validity、ownership、thread/apartment、return-code 與 cleanup invariant。
- [x] 4.11 建立 debug handle/thread snapshot helper，量測 STA 啟停前後的 application-owned thread 與可辨識 handles，將限制寫入測試說明。

## 5. Composition root 與 M0 最小視窗

- [x] 5.1 先為 startup coordinator 撰寫使用 fake stages 的 unit tests，驗證 diagnostics → Windows prerequisites → Shell STA → GPUI → window 的順序。
- [x] 5.2 為每個 startup stage 的失敗撰寫 unwind 測試，確認已完成階段依反向順序且只清理一次。
- [x] 5.3 實作 Windows DPI awareness 初始化，記錄所採 API、process manifest 配置及已由 host 設定時的可接受結果。
- [x] 5.4 建立 Windows manifest/resource build 設定：採 GPUI-CE 內建 PerMonitorV2／Common Controls／SegmentHeap manifest 作為唯一 ID 1，app resource 提供正確 architecture 的 VERSIONINFO 與可執行檔 metadata，並以成品 exe 驗證。
- [x] 5.5 實作 `explorer-app` composition root，將 diagnostics、DPI、Shell STA 與 GPUI application 的 ownership 集中在單一生命週期物件。
- [x] 5.6 建立最小 GPUI window options 與空內容 root，設定基準初始尺寸、合理最小尺寸及可 resize 行為。
- [x] 5.7 將 window close、最後視窗離開與程序退出事件統一導向 idempotent shutdown，不從 UI 元件直接管理 STA。
- [x] 5.8 加入 application version/build commit 顯示或 diagnostics 欄位，確保視覺與效能 artifacts 能關聯到實際 commit。
- [x] 5.9 建立 headful smoke harness：啟動 app、等待 ready marker、調整視窗、要求關閉並檢查 exit code；若 CI 無法 headful 執行，保留可在本機執行的命令。
- [x] 5.10 執行 M0 的四個 Cargo gates，修正所有 warning，不以 allow lint 隱藏可修正問題。
- [x] 5.11 執行 M0 啟停與 panic manual test，記錄實際日期、環境、結果、log/report 路徑與 handle/thread snapshot。
- [x] 5.12 更新 parity matrix 與 status；只有在 M0 自動 gates、啟停、panic 與資源證據齊全後才將 M0 標記完成。

## 6. Theme 與 layout token 系統

- [x] 6.1 先建立 theme contract tests，列舉 surface、subtle surface、control fill、hover、pressed、selected active/inactive、divider、primary/secondary/disabled text、focus、danger 與 accent 必要鍵。
- [x] 6.2 定義型別化 semantic color tokens，讓缺少 token 在編譯或 contract test 時失敗，避免由字串 key 靜默 fallback。
- [x] 6.3 建立 light theme token set，初始值來自同機 Windows 11 25H2 Explorer 實測，將量測日期與環境寫入視覺文件。
- [x] 6.4 建立 dark theme token set，逐一對應相同 semantic keys，不以單純反相產生色彩。
- [x] 6.5 建立 high-contrast-ready mapping contract，優先引用系統 semantic colors；M1 未實機驗證的項目在 parity matrix 保持部分完成。
- [x] 6.6 定義型別化 layout tokens：title/tab、command、address、status 高度，navigation pane min/default/max width，content spacing、padding、radius、focus stroke 與 animation duration。
- [x] 6.7 為 layout token invariants 撰寫測試，驗證 min ≤ default ≤ max、所有高度/spacing 為有限非負值、hit target 不小於可見 glyph bounds。
- [x] 6.8 建立 logical-to-physical scale 測試資料，涵蓋 100%、125%、150%、200%，確認 token 只套用一次 DPI scaling。
- [x] 6.9 將 theme/layout tokens 注入 GPUI root/context，禁止 feature component 自行建立第二份 theme source。
- [x] 6.10 加入 source lint、review checklist 或集中式 helper，追蹤 feature UI 中新增的固定 RGB／重複主要尺寸，例外必須有註解理由。

## 7. UI state、typed actions 與 focus coordinator

- [x] 7.1 先為最小 `AppViewState` 撰寫測試，涵蓋 current theme、navigation pane width、focused surface 與靜態 command availability。
- [x] 7.2 定義 M1 typed actions：Back、Forward、Up、FocusAddress、FocusSearch、ToggleTheme、CloseWindow，以及必要的 pane resize action。
- [x] 7.3 建立 key binding registry 與衝突測試，讓同一 scope 的重複 binding 在測試中失敗並記錄 focused-surface 優先序。
- [x] 7.4 實作 window action dispatcher，確保每個 key event 最多產生一次 handled action，disabled action 不修改 state。
- [x] 7.5 建立 focus surface enum 與 coordinator，定義 title/tab、command、address、search、navigation pane、file view、status 的 traversal 順序。
- [x] 7.6 為 Tab／Shift+Tab traversal 撰寫 model/behavior tests，disabled control 與非互動 placeholder 不得進入可操作焦點序列。
- [x] 7.7 實作 FocusAddress 與 FocusSearch，保存前一焦點並在 Esc／明確退出時恢復；M1 checkpoint 不接受虛構 submit，後續分別接到真實 address parser 與 search parser。
- [x] 7.8 為 hover、pressed、disabled、active/inactive 與 keyboard focus 定義共享 interaction state，不在各 component 重複 raw key 判斷。
- [x] 7.9 加入 action tracing，記錄 action name、source、handled surface 與結果，不記錄 address/search 文字內容。

## 8. ExplorerWindow 結構與靜態 chrome

- [x] 8.1 建立 `ExplorerWindow` root component，只組合子區域、注入 state/actions/tokens，不加入 Shell call 或 filesystem I/O。
- [x] 8.2 建立 `WindowChrome` component，分離 tab strip、window drag region 與 caption controls 的 layout/hit-test 責任。
- [x] 8.3 建立 M1 checkpoint 的單一 active `Tab` 呈現與可測 NewTabButton 視覺；在多分頁 model 接入前保持 disabled，並由後續 task 啟用真實行為。
- [x] 8.4 實作 caption minimize/maximize/restore/close 對應的 GPUI/Windows 行為，並對無法支援的 Snap/caption parity 建立 capability test 與限制紀錄。
- [x] 8.5 建立 `CommandBar`，以 GPUI-CE 原生 elements 與集中式 semantic button/tooltip/menu helpers 呈現 M1 controls，未支援命令必須 disabled；helpers 必須由 typed action、token 與 accessibility contract 驅動，不建立通用元件框架。
- [x] 8.6 建立 `NavigationBar` 與 Back/Forward/Up controls，將 click/shortcut 全部送入 typed action dispatcher。
- [x] 8.7 建立 `BreadcrumbAddressEditor` 的 M1 placeholder，提供 FocusAddress、focus ring 與 disabled submit；後續以真實 location parser 取代 placeholder terminal behavior。
- [x] 8.8 建立 `SearchBox` 的 M1 placeholder，提供 FocusSearch、focus ring 與 disabled submit；後續以真實 search session 取代 placeholder terminal behavior。
- [x] 8.9 建立 `NavigationPane` 靜態區域，只呈現視覺 fixture 所需節點與 unavailable semantics，不聲稱已連接 Quick Access/This PC。
- [x] 8.10 建立 `FileViewHost` loading/empty/error/ready state 容器；M1 checkpoint 只顯示未連接服務狀態且不使用假檔案，後續接入真實 directory snapshot。
- [x] 8.11 建立 `StatusBar`，顯示中性 M1 shell 狀態與版本/debug marker，不顯示虛構 item count 或 operation progress。
- [x] 8.12 為每個區域加入穩定的測試 identifier/semantic label，供 behavior、accessibility foundation 與 visual capture 定位。

## 9. Layout、split resize 與視窗狀態

- [x] 9.1 以 layout tokens 組合 vertical chrome 與 remaining content，撰寫固定尺寸 geometry test 驗證區域順序、高度與無重疊。
- [x] 9.2 設定視窗最小尺寸並測試窄視窗策略；必要控制採明確 overflow/compact 行為，不允許負尺寸或 panic。
- [x] 9.3 建立 navigation pane/content split，初始寬度只從 layout tokens 取得。
- [x] 9.4 建立最小 GPUI-CE divider element，支援 pointer capture、clamp、double-click reset 與 keyboard adjustment；記錄未採 `gpui-component` 是因固定 revision 的 API 編譯衝突。
- [x] 9.5 為 divider pointer down/move/up 與 pointer capture 撰寫 behavior tests，涵蓋拖出視窗、取消與視窗失焦。
- [x] 9.6 實作 pane width clamp，測試低於 min、高於 max、NaN/無限值防護與 resize 後 content 有效尺寸。
- [x] 9.7 在視窗 resize 時保持固定 chrome token 高度並只重新配置 content，加入多組 window dimensions 的 geometry test。
- [x] 9.8 驗證 maximize/restore 後 layout 與 pane width 仍一致；M1 不持久化跨程序 view settings，文件需明確說明。
- [x] 9.9 在 100/125/150/200% DPI 擷取 layout diagnostics，確認 logical geometry 未重複 scaling，記錄任何 GPUI rounding tolerance。

## 10. Theme 切換、互動狀態與可及性基礎

- [x] 10.1 實作 light/dark theme action，以單一 state transition 更新整個 component tree，不允許局部殘留舊 theme。
- [x] 10.2 為 theme toggle 撰寫 behavior test，驗證所有 semantic token provider 同步更新且 action trace 只出現一次。
- [x] 10.3 為 button、tab、input placeholder、divider 與 caption control 套用 hover、pressed、disabled、active/inactive、focus tokens。
- [x] 10.4 建立 interaction state screenshot fixture，能在 deterministic 狀態擷取 hover、pressed、disabled、selected 與 focused 樣本。
- [x] 10.5 為每個可互動 control 設定 role、name、state 與 invoke/toggle semantics；若 GPUI revision 缺少必要 API，將具體缺口寫入 parity matrix。
- [x] 10.6 驗證 keyboard-only traversal 可從 window chrome 走到 search、navigation pane 與 file view，再反向返回，且 focus ring 在 light/dark 都可辨識。
- [x] 10.7 驗證真正 text input 才接收 IME composition；靜態 container/key dispatcher 不攔截或改寫 composition event。
- [x] 10.8 驗證 high contrast mode 下 semantic mapping 不只依透明度表達 disabled/selected；未完成的實機差異保持明確未通過。
- [x] 10.9 將非必要動畫集中使用 animation duration token，為 reduced-motion-ready 設定零/短 duration 路徑與 contract test。

## 11. UI 行為、架構與回歸測試

- [x] 11.1 建立不需要真實 Shell service 的 UI test harness，以 deterministic state 建立 `ExplorerWindow` 並觸發 actions。
- [x] 11.2 撰寫初始 render structure test，驗證所有 M1 區域、唯一 active tab、placeholder 與 status semantics 存在。
- [x] 11.3 撰寫 disabled Back/Forward/Up 點擊與快捷鍵測試，驗證 state、focus 與 diagnostics 不產生假 navigation。
- [x] 11.4 撰寫 FocusAddress／FocusSearch／Esc focus restore 測試，驗證每個 event 只由一個 surface 處理。
- [x] 11.5 撰寫 navigation pane divider min/max/cancel 測試，驗證 pointer capture 在所有 terminal path 釋放。
- [x] 11.6 撰寫小於最小、基準、寬、maximize 等視窗尺寸的 geometry regression tests。
- [x] 11.7 撰寫 light/dark token snapshot tests，snapshot 只包含 semantic values/layout diagnostics，不依賴不穩定的文字 rasterization。
- [x] 11.8 撰寫 source/dependency architecture test，阻止 UI 新增 Shell COM、同步 filesystem I/O 或 `explorer-shell-win` dependency。
- [x] 11.9 加入 callback duration instrumentation fixture，對 resize、theme toggle 與 focus traversal 收集時間，將高於 4 ms 的穩定 regression 顯示為可診斷測試結果。
- [x] 11.10 加入 application repeated start/close smoke loop，觀察 crash、thread、GDI/User handle 與 diagnostics flush 是否持續成長。

## 12. 視覺 baseline 工具與 Explorer 對照

- [x] 12.1 建立 visual fixture 啟動參數，固定 window size、theme、DPI expectation、font configuration、placeholder state 與 stable-ready marker。
- [x] 12.2 建立 baseline metadata schema，包含 Windows edition/build、Explorer version、app commit、GPUI revisions、DPI、theme、window size、font 與 capture timestamp。
- [x] 12.3 建立 capture script/harness，等待 stable-ready marker 後輸出 actual screenshot 與 token/layout diagnostics，不以固定 sleep 作唯一同步機制。
- [x] 12.4 建立 comparison 設定，對文字 antialiasing 與動態 OS 區域使用明確 mask/tolerance，對區域 bounds、spacing 與 semantic colors使用嚴格門檻。
- [x] 12.5 建立 visual regression output layout，失敗時同時保留 baseline、actual、diff 與 diagnostics，通過時也保留 metadata/report。
- [x] 12.6 確保 regression command 永遠不自動覆寫 baseline；提供獨立、需人工 review 的 baseline update 流程。
- [x] 12.7 在本機 Explorer 以相同 window size、DPI、theme 與 font configuration 擷取 light baseline，記錄 Explorer 版本與操作步驟。
- [x] 12.8 擷取 application light baseline，比較 title/tab、command、address/search、content split 與 status 的高度、間距、字級、色彩與狀態。
- [x] 12.9 重複建立 dark Explorer/application baseline 與 diff，將接受的字型/GPU tolerance 和不可接受的 layout 差異分開記錄。
- [x] 12.10 擷取 deterministic focus、hover、pressed、disabled、active/inactive fixtures，逐項更新 parity matrix 的 evidence link。
- [x] 12.11 對 100/125/150/200% DPI 執行固定尺寸 capture；無法自動改變 DPI 的 case 依手動步驟執行並記錄實際結果。
- [x] 12.12 審查 Snap、caption hit-test、maximize/restore 與 custom chrome 差異，能修正者回到對應 UI task，API 限制則附證據寫入 parity matrix。

## 13. 手動 Windows 驗收

- [x] 13.1 執行冷啟動 → resize → minimize/restore → maximize/restore → close 流程，記錄每一步實際結果、log 與 screenshot/video 證據。
- [x] 13.2 執行暖啟動與連續十次啟停，確認沒有殘留 process、可見 console、持續增加的 thread 或明顯 handle leak。
- [x] 13.3 以滑鼠檢查所有 enabled controls 的 hover/pressed、disabled navigation controls、divider capture 與 caption controls。
- [x] 13.4 只用鍵盤執行正向/反向 focus traversal、focus address/search、theme toggle 與 close，記錄焦點遺失或 action 衝突。
- [x] 13.5 在 light 與 dark theme 執行相同固定尺寸流程，確認所有區域同步切換、文字可讀且沒有硬編碼殘色。
- [x] 13.6 在 100/125/150/200% DPI 執行啟動、resize 與 focus 流程，記錄 clipping、rounding、hit target 與 screenshot 證據。
- [x] 13.7 在 high contrast 模式執行最小 smoke test，確認 semantic colors/focus 可辨識；所有未達 parity 項目保持未完成並指定 M9 closure。
- [x] 13.8 在可用的多螢幕不同 DPI 組合移動視窗並 maximize/restore，記錄 bounds、scale 與 caption 行為；沒有設備時記錄未驗證原因。
- [x] 13.9 使用至少一種 Windows IME 將 composition focus 放入 address/search placeholder，確認其他 shortcut dispatcher 不攔截 composition。
- [x] 13.10 以 Windows accessibility inspector 或 Narrator smoke test檢查主要 controls 的 role/name/state/focus；M1 只驗證 foundation，不提前宣告 M9 完成。
- [x] 13.11 測試受控 panic，確認使用者可找到診斷、內容不含測試敏感資料，且下次啟動不受半寫狀態影響。
- [x] 13.12 將每個手動 case 的日期、tester、環境、actual result、evidence 與 issue link 回填 `docs/MANUAL_TESTS.md`。

## 14. 效能與資源基線

- [x] 14.1 建立 release-mode benchmark harness，區分 process cold start 與 warm start，從 process launch 量到 stable-ready marker。
- [x] 14.2 在固定本機環境各收集足夠樣本，計算冷/暖啟動 median 與 p95，記錄 hardware、OS build、debugger 關閉與 background load 條件。
- [x] 14.3 對 resize、theme toggle、focus traversal 收集 UI callback duration 與 frame diagnostics，報告 median/p95 及任何超過 4 ms 的樣本原因。
- [x] 14.4 記錄 idle 與互動後的 memory、thread count、GDI handles、User handles，將工具、取樣時間與限制寫入報告。
- [x] 14.5 執行重複 navigation-free UI 操作與啟停 soak，確認 queue/outstanding work 不持續成長，若有成長先建立可重現 issue 再決定 milestone 狀態。
- [x] 14.6 將實測數字寫入 `docs/STATUS.md` 作為 regression baseline；未達 800 ms 冷啟動／400 ms 暖啟動目標時記錄原因與後續 action，不竄改樣本。

## 15. M0/M1 品質 checkpoint 與階段文件

- [x] 15.1 執行 `cargo fmt --all --check`，修正格式後再次執行並保存成功結果。
- [x] 15.2 執行 `cargo check --workspace --locked`，修正 target/features/dependency 問題後保存成功結果。
- [x] 15.3 執行 `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`，逐項修正 warning，不以全域 allow 規避。
- [x] 15.4 執行 `cargo test --workspace --locked`，保存 unit、integration、architecture 與 behavior test 摘要。
- [x] 15.5 執行 headful app smoke、STA lifecycle、panic、repeated start/close 與 visual regression commands，保存命令、exit code 與 artifact 路徑。
- [x] 15.6 檢查 `Cargo.lock` 在所有 gates 後無變更，使用 `cargo tree` 再次核對 GPUI revisions 與 Windows feature 範圍。
- [x] 15.7 掃描 production source 的 `unwrap`、`expect`、unsafe、固定 RGB、固定主要 layout 尺寸與直接 Shell/UI 依賴，逐項修正或加入具體安全／設計理由。
- [x] 15.8 檢查沒有建立任何無 production consumer 或 contract test 的空 crate、provider、command、event 或 fake feature；刪除未使用預留程式碼並重跑 gates。
- [x] 15.9 更新 `docs/IMPLEMENTATION_PLAN.md` 的每個 M0/M1 checkpoint task 狀態、實際偏差與後續真實功能階段前置條件。
- [x] 15.10 更新 `docs/STATUS.md`，列出 dependency revisions、Cargo gates、manual matrix、visual reports、performance/resource baseline、已知限制與未驗證項目。
- [x] 15.11 更新 `docs/PARITY_MATRIX.md`，確認每個完成項目都有 evidence；所有缺證據、high-contrast/accessibility closure 或 Windows API 差異維持正確狀態。
- [x] 15.12 更新 `docs/MANUAL_TESTS.md`，確認步驟可由另一位開發者重現，expected 與 actual 分欄且沒有空白成功宣稱。
- [x] 15.13 將 M0 exit matrix 與 M1 exit matrix逐列審查；任一必要 gate、文件或證據缺失時不得標記該 Milestone 完成。
- [x] 15.14 執行 OpenSpec validation 與 artifact status 檢查，確認本 change 的 proposal、design、specs、tasks 一致且 apply-ready。
- [x] 15.15 建立 M0/M1 checkpoint 摘要，列出可執行 binary、啟動命令、測試命令、視覺 artifacts、已知限制及進入多分頁真實資料夾階段不可跳過的前置證據。

## 16. Domain contract、model 與測試支援

- [x] 16.1 建立 `explorer-model` crate，加入 `TabId`、`RequestId`、`Generation`、`ShellItemId`、`LocationDescriptor` 與不可混用的 typed IDs。
- [x] 16.2 為 typed IDs 撰寫 equality/hash/serialization contract tests，確認 display/log formatting 不洩漏不必要完整敏感路徑。
- [x] 16.3 定義 `RequestContext { request_id, tab_id, generation, cancellation }` 與驗證 helper，測試 tab/request/generation 任一不符都拒絕 event。
- [x] 16.4 定義實際需要的 `ExplorerCommand`：Navigate、Refresh、OpenItem、Cancel、ExecuteFileOperation、ShowContextMenu、StartSearch 與 data-transfer commands。
- [x] 16.5 定義實際需要的 `ExplorerEvent`：LocationResolved、DirectoryBatch、DirectoryChanged、OperationProgress/Finished、SearchBatch/Finished 與 Failed，確保所有 request 有恰好一個 terminal path。
- [x] 16.6 建立 `CancellationToken`／registration contract，測試 cancel-before-start、cancel-during-work、重複 cancel 與 drop consumer。
- [x] 16.7 建立有界 command/event channel policy，測試 queue full 時 GPUI caller 不阻塞且必要 command 收到 overload error。
- [x] 16.8 建立 `TabState`、`DirectoryState`、`NavigationHistory`、`DirectorySnapshot` 與最小 `SelectionModel`，每個 state transition 都有 pure model tests。
- [x] 16.9 為 Back/Forward/Refresh/failed navigation 撰寫 history tests，確認只有成功解析的新 location 提交 history。
- [x] 16.10 為 presentation store 建立 stable-ID insert/update/remove/rename diff tests，禁止用 row index 作 identity。
- [x] 16.11 建立 `explorer-test-support` crate 與 deterministic scheduler/fake Shell service，讓 fake command/event 使用與 production 完全相同的公開 contract。
- [x] 16.12 建立 temporary-folder fixture，使用 OS temporary directory、唯一 marker 與 resolved-root 驗證，提供安全 create/rename/copy/move/delete helper。
- [x] 16.13 為 fixture destructive guard 撰寫測試，拒絕 drive root、workspace root、user profile、未解析 path 與透過 reparse point 逃出 fixture root 的目標。
- [x] 16.14 將 model/common/test-support 加入 workspace gates，確認 production crates 不反向依賴 test-support。

## 17. 多分頁 UI 與 per-tab 導覽狀態

- [x] 17.1 將 window state 從單一 view state 改為 `Vec<TabState>`、`active_tab_id` 與 stable tab ordering，加入至少一個 tab invariant。
- [x] 17.2 撰寫建立新分頁測試，驗證 unique ID、預設 location policy、獨立 history/directory/selection/search state 與 active tab 切換。
- [x] 17.3 撰寫關閉 active/background/last tab 測試，明確實作最後分頁關閉視窗或建立預設分頁的產品規則。
- [x] 17.4 撰寫切換分頁測試，驗證 file view、status、address/search、selection 與 action availability 全部來自目標 tab。
- [x] 17.5 撰寫 tab reorder 測試，確認 active identity 與 per-tab state 不因 index 改變而交換。
- [x] 17.6 將 `TabStrip` 接上真實 create/switch/close/reorder actions，NewTabButton 不再是 placeholder。
- [x] 17.7 實作 tab overflow/scroll/menu 行為，確保大量 tabs 可由滑鼠、鍵盤與 accessibility action 存取。
- [x] 17.8 讓 Back/Forward/Up availability 由 active tab history/location capability 驅動，切換 tab 時立即更新 disabled state。
- [x] 17.9 讓 address/breadcrumb 顯示 active tab resolved location；編輯中的未提交文字不得被背景 tab event 覆寫。
- [x] 17.10 關閉 tab 時取消其 navigation/search/operation-view subscriptions，拒絕所有 late events 並清除 tab-scoped UI entities。
- [x] 17.11 降低背景 tab 的非必要工作優先序，但保留其進行中列舉完成能力；以 scheduler test 驗證 active viewport 優先。
- [x] 17.12 加入多分頁 keyboard shortcuts、focus restore 與 accessibility semantics，測試 Ctrl+T/Ctrl+W/Ctrl+Tab/Ctrl+Shift+Tab 不與 text input 衝突。

## 18. 真實本機資料夾解析與增量列舉

- [x] 18.1 在 Shell STA command loop 接入 typed Navigate/Refresh/Cancel，所有 dispatch 記錄 correlation、tab、generation 與 queue latency。
- [x] 18.2 建立 owned PIDL/CoTaskMem/PROPVARIANT/handle RAII wrappers，為每個 unsafe block記錄 allocator、ownership transfer、apartment 與 cleanup。
- [x] 18.3 實作 `LocationDescriptor` 到 Shell Item 的解析，支援本機 path、known folder descriptor 與可重建 parsing name，錯誤保留 HRESULT。
- [x] 18.4 建立 `ShellItemId` construction contract，優先 owned absolute PIDL 並保存必要可重建 descriptor；測試 clone/hash/equality 與 invalid identity rejection。
- [x] 18.5 實作 location metadata event，先回傳 display title、capability 與 normalized descriptor，再開始 child enumeration。
- [x] 18.6 以 `IShellFolder`/`IEnumIDList` 增量列舉 children，將 UTF-16 display name、attributes、可選 path 與快速 metadata 轉成 owned `FileEntry`。
- [x] 18.7 為 directory batches 加入 item-count 與 estimated-byte 雙重上限，測試超長名稱/大量 entries 不產生超大單批。
- [x] 18.8 在 UI/model 每幀只合併有限 batches，首個 viewport ready 後解除 blocking loading，慢速剩餘資料繼續增量加入。
- [x] 18.9 實作新 Navigate 取消舊 generation，測試 A→B→C out-of-order batches、errors 與 terminal events 全部只接受 C。
- [x] 18.10 實作 Refresh 不新增 history、保留 selection/scroll anchor 可重建資訊，並以 real folder mutation 測試 snapshot 收斂。
- [x] 18.11 實作 folder item 在目前/新分頁導覽與 file item Shell open，確保 Shell open error 不破壞 directory state。
- [x] 18.12 將 `FileViewHost` 接上真實 loading/ready/empty/error states，以 stable IDs render 最小可用 rows，禁止假項目。
- [x] 18.13 讓 status bar 顯示 active tab 的真實 partial/final item count、selected count 與 error，不接受背景 tab 更新。
- [x] 18.14 測量本機資料夾 first-item、first-viewport 與 terminal enumeration latency，將 request tracing 串起 UI action 到 terminal event。

## 19. Watcher、selection 與真實資料夾測試矩陣

- [x] 19.1 實作 `ReadDirectoryChangesW` overlapped watcher 的 RAII handle、cancel 與 shutdown，必要 Shell notification 只透過明確 adapter 補足。
- [x] 19.2 建立 watcher parser tests，涵蓋 UTF-16 buffer、added/removed/modified、rename old/new、截斷與 malformed record。
- [x] 19.3 實作短時間 coalescing 與 stable-ID diff，rename 可配對時更新同一 entry identity。
- [x] 19.4 實作 watcher overflow／通知不完整 recovery：建立新 generation 重新列舉並 diff，而不是清空 snapshot。
- [x] 19.5 測試 watcher insert/remove/rename 後 selected/focused item 依 stable ID 保留或以明確規則失效。
- [x] 19.6 建立小型真實資料夾 integration test，驗證初始列舉、進入子資料夾、Back/Forward/Up、Refresh 與 file open error。
- [x] 19.7 建立 Unicode、emoji、組合字元、長名稱、hidden/system 與大小寫差異 fixture，驗證 display/identity/count 正確。
- [x] 19.8 建立 permission-denied case；若目前權限無法可靠建立，將 setup/skip 原因與手動步驟明確記錄，不假造通過。
- [x] 19.9 建立 reparse point/junction fixture，驗證 identity、導覽、循環保護與 cleanup 不越過 fixture root。
- [x] 19.10 建立 rapid create/delete 與 rename storm，驗證 watcher/model 最終與磁碟 oracle 一致。
- [x] 19.11 建立 watcher overflow fault injection，驗證 diagnostics、重新列舉、terminal state 與 selection invariants。
- [x] 19.12 建立 100,000 真實項目 dataset generator 與 oracle，量測 first-item/viewport、互動可用時間、final count、memory 與 queue depth。
- [x] 19.13 對 fake 與 real Shell service 執行同一 navigation contract suite，確認 cancellation/terminal semantics 一致。
- [x] 19.14 在 docs 記錄每個 real-folder case 的 fixture ownership、命令、實際結果、耗時與 cleanup 結果。

## 20. 原生檔案操作與 operation center

- [x] 20.1 定義 `FileOperationRequest`、item descriptors、destination、flags、conflict decision、progress 與逐項 outcome domain types。
- [x] 20.2 建立 operation state machine tests，覆蓋 queued→running→finished/cancelled/partial/failed，保證恰好一個 terminal event。
- [x] 20.3 在 Shell STA 初始化並封裝 `IFileOperation`，實作 progress sink registration/unregistration 與 apartment-correct release。
- [x] 20.4 實作 Create Folder，連接 UI command、validation、operation events 與 watcher dedupe。
- [x] 20.5 實作 inline Rename，涵蓋 invalid name、collision、Enter/Esc/失焦規則、錯誤後保留 editor 與 selection。
- [x] 20.6 實作單一/多選 Copy，保存每個來源/目的地 descriptor 與逐項 outcome。
- [x] 20.7 實作單一/多選 Move，涵蓋同 volume、可用時跨 volume、reparse point 與 capability failure。
- [x] 20.8 實作 Recycle Delete，明確使用回收語意並記錄 Windows/API 無法保證的情況。
- [x] 20.9 實作 Permanent Delete 與明確確認流程，取消確認不得建立 Shell operation。
- [x] 20.10 建立 UI `OperationCenter`，呈現總體/逐項 progress、目前名稱、cancel、conflict prompt 與 terminal summary。
- [x] 20.11 確保 progress callback 不同步更新重量 UI；使用有界/coalesced events 並量測 progress-to-render latency。
- [x] 20.12 實作取消傳遞與 late-progress rejection，測試大型 copy cancel 後不再更新舊 operation UI。
- [x] 20.13 實作 name collision/destination changed/access denied 的 typed decision，沒有決策不得靜默覆寫。
- [x] 20.14 實作 partial failure summary，成功項目保留成功、失敗項目保留 HRESULT/下一步，整批不得誤報。
- [x] 20.15 建立 operation journal，只記已完成且 inverse 可安全重建的 rename/move 等操作。
- [x] 20.16 實作 Undo/Redo 前置重新驗證；外部變更、identity 不符或名稱衝突時停用並顯示原因。
- [x] 20.17 在安全 fixture 跑 create/rename/copy/move/recycle/permanent-delete/cancel/conflict/partial/undo/redo matrix，逐案比對磁碟 oracle。
- [x] 20.18 為 destructive tests 再次驗證 canonical target 位於 fixture root，將越界、drive root、workspace root 與 reparse escape 納入負向測試。

## 21. Clipboard copy/cut/paste 與 Explorer 互通

- [x] 21.1 盤點並記錄 Explorer interoperability 所需 Shell clipboard formats、preferred/performed drop effect 與 `IDataObject` ownership規則。
- [x] 21.2 定義 Clipboard domain state：none/copy/cut、stable item IDs、source descriptors、allowed effects 與 generation。
- [x] 21.3 實作將目前 selection 建立為 Shell-compatible `IDataObject` 的 Copy，確保 COM interface 只在允許 apartment 使用或正式 marshaling。
- [x] 21.4 實作 Cut pending state 與視覺，selection/watch updates 依 stable identity 維持，不用 row index。
- [x] 21.5 實作讀取外部 Clipboard data object、檢查 target capability 與啟用/停用 Paste action。
- [x] 21.6 將 Paste 轉成既有 file-operation pipeline，重用 progress、cancel、conflict、partial failure 與安全 destructive guard。
- [x] 21.7 只有成功 move/paste 的項目清除 cut state；cancel/partial failure 逐項保留或恢復，加入 contract tests。
- [x] 21.8 處理 Clipboard ownership change/clear，確保舊 cut visual、COM references 與 subscriptions 都釋放。
- [x] 21.9 建立同 app 跨分頁 copy/cut/paste tests，驗證 source tab 關閉、destination 切換與 stale clipboard state。
- [x] 21.10 手動/可自動時測試 Explorer→本程式單一/多選 copy/cut/paste，記錄 formats、effects、actual outcomes。
- [x] 21.11 手動/可自動時測試本程式→Explorer 單一/多選 copy/cut/paste，記錄 Windows/Explorer build 與任何公開 API 限制。
- [x] 21.12 對外部 data object malformed/unsupported/slow cases 做 fault tests，UI 必須顯示可恢復錯誤且不洩漏 interface/handle。

## 22. OLE drag-and-drop

- [x] 22.1 在適當 STA/OLE 執行緒初始化 `OleInitialize` 或相容 apartment prerequisites，記錄與既有 COM 初始化的關係及同執行緒 cleanup。
- [x] 22.2 建立 drag session state machine：candidate→dragging→dropped/cancelled/failed，並測試每個 pointer/COM terminal path 清理狀態。
- [x] 22.3 使用 Windows system drag threshold，未超過前只保留 selection/drag candidate，不提早建立 `DoDragDrop`。
- [x] 22.4 實作 `IDropSource` 與 selection `IDataObject`，傳遞 allowed copy/move/link effects 並處理 Esc/cancel。
- [x] 22.5 實作 file view background、folder item 與 navigation target 的 `IDropTarget` 註冊/撤銷，視窗關閉後不得留下 registration。
- [x] 22.6 實作 DragEnter/Over 的 target capability、modifier、source preferred effect negotiation，cursor/drop cue 必須一致。
- [x] 22.7 實作 DragLeave/Drop cleanup，任何錯誤或取消都清除 hover、indicator、pointer capture 與 auto-scroll。
- [x] 22.8 將成功 Drop 轉成既有 file-operation pipeline，drop UI 不重製 copy/move/conflict 邏輯。
- [x] 22.9 實作 right-drag terminal menu，提供可用的 copy/move/cancel；取消不得建立 operation。
- [x] 22.10 實作靠近 file view 邊緣的 bounded auto-scroll，離開 edge、DragLeave、Drop 或 cancel 立即停止。
- [x] 22.11 測試 drag reentrancy、source tab 關閉、target generation 改變、視窗失焦與 shutdown during drag。
- [x] 22.12 手動/可自動時測試本程式→Explorer 與 Explorer→本程式的單一/多選、copy/move/none、left/right drag。
- [x] 22.13 在 100/125/150/200% DPI 測試 drag threshold、drop cue geometry 與 edge auto-scroll hit zone。
- [x] 22.14 量測 drag loop 前後 thread/GDI/User/COM references，執行重複 drag soak 並確認資源不持續成長。

## 23. `IContextMenu3` 與原生選單 session

- [x] 23.1 定義 background、single-selection、multi-selection `ContextMenuRequest`，保存 parent/item descriptors、owner window contract、correlation 與 deadline。
- [x] 23.2 建立 context-menu session state machine，涵蓋 resolve→query→show→invoke/cancel→release 與恰好一次 cleanup。
- [x] 23.3 實作 selection/parent 到 `IContextMenu`/2/3 的取得，所有 PIDL/interface ownership 都由 STA RAII wrapper 管理。
- [x] 23.4 建立原生 `HMENU` ownership wrapper，確保 menu 關閉、錯誤、timeout 與 window shutdown 不 double-destroy/leak。
- [x] 23.5 實作 `QueryContextMenu` flags 與 command ID 範圍映射，背景/單選/多選 capability 分開測試。
- [x] 23.6 在 menu session 期間轉發 `WM_INITMENUPOPUP`、`WM_DRAWITEM`、`WM_MEASUREITEM`、`WM_MENUCHAR` 與必要後續 messages。
- [x] 23.7 實作 selected command invoke、working directory/point/keyboard invocation context 與 HRESULT/domain outcome。
- [x] 23.8 Context menu command 可能改變檔案時與 watcher/operation status 協調，不能假設 invoke return 即 snapshot 已更新。
- [x] 23.9 建立可控制 owner-draw fake handler fixture，測試 measure/draw/menu-char/reentrant messages 與 release 順序。
- [x] 23.10 建立 slow/hang/error handler fault fixture，驗證 GPUI callback 不直接無界阻塞、UI 能恢復且 session 有 correlation diagnostics。
- [x] 23.11 在完整 process broker 尚未交付前，將無法安全強制中止的 handler 限制、timeout 行為與 fallback 寫入 parity matrix。
- [x] 23.12 測試右鍵已選項目保留 multi-selection、右鍵未選項目先單選、右鍵空白開 background menu。
- [x] 23.13 手動測試 Windows 內建與至少一個可控第三方 context menu extension，記錄 owner-draw、submenus、keyboard navigation 與 invoke 結果。
- [x] 23.14 重複開關選單與 handler failure soak，量測 HMENU/GDI/User/COM refs 與 STA responsiveness。

## 24. 搜尋 parser、backend 與 per-tab 結果

- [x] 24.1 在 `explorer-search` 定義 token、source span、AST nodes、property keys、comparison、date/size values 與 boolean precedence。
- [x] 24.2 撰寫 parser tests：純文字、quoted phrase、escape、name/type/size/date filters、comparisons、AND/OR/NOT、括號與 Unicode。
- [x] 24.3 撰寫 invalid parser tests：未閉合引號、未知 property、無效值/日期、缺 operator/operand，驗證精確位置與可修正訊息。
- [x] 24.4 明確分離 address parser 與 search parser，測試 invalid address 不自動轉 search、query 不自動轉 location。
- [x] 24.5 定義 `SearchRequest`、`SearchBatch`、source status、partial/error/cancel terminal outcome 與 stable-ID dedupe store。
- [x] 24.6 在 active tab 提交 query 時建立新 generation；新 query、navigation、關閉 tab 取消舊 session並拒絕 late result。
- [x] 24.7 實作 AST 到 Windows Search query helper/store 的安全 escape/bind adapter，不允許 raw query string 直接拼接。
- [x] 24.8 實作 Windows Search backend 的增量 batch、bounded queue、cancel、source diagnostics 與 terminal event。
- [x] 24.9 實作 Windows Search availability/index-scope 檢查，UI 顯示 indexed、partial、unavailable 而非假空結果。
- [x] 24.10 實作有界 filesystem fallback，使用 cancellation、visited/reparse policy、batching 與 queue/backpressure，不在 UI thread traversal。
- [x] 24.11 合併多來源結果時以 stable identity 去重，保存 source attribution，不以 display name/path alias 判斷唯一性。
- [x] 24.12 將 SearchBox submit/error/cancel/history placeholder 接到真實 per-tab search state，address bar 保持 location 語意。
- [x] 24.13 將 FileViewHost/status 接到 search loading/partial/ready-empty/error/cancel states，離開搜尋恢復原 directory snapshot/history。
- [x] 24.14 建立真實 fixture oracle，測試 Unicode、phrase、name/type/size/date、boolean、zero-result、cancel 與快速 query replacement。
- [x] 24.15 在未索引 temporary folder 驗證 fallback，記錄 Windows Search 索引延遲與 backend/fallback 結果差異。
- [x] 24.16 以 100,000 項目測量 first-result、first-viewport、terminal latency、queue depth、cancel latency 與 memory。
- [x] 24.17 注入 backend error/partial source failure/channel close，驗證部分結果保留且 UI 不把錯誤顯示成「找不到項目」。

## 25. 全範圍整合、parity 與最終交付

- [x] 25.1 建立 end-to-end test：啟動→開多分頁→導覽不同真實資料夾→切換→Back/Forward→watcher 更新，驗證 tab state 隔離。
- [x] 25.2 建立 end-to-end test：真實選取→rename/copy/move/delete→progress/conflict/cancel/undo，逐步比對 model 與磁碟 oracle。
- [x] 25.3 建立 end-to-end manual flow：Explorer copy/cut/paste/drag 到本程式，再由本程式回到 Explorer，記錄逐項 effect/outcome。
- [x] 25.4 建立 end-to-end context menu flow：background/single/multi、owner-draw fixture、invoke、watcher 收斂與 handler failure recovery。
- [x] 25.5 建立 end-to-end search flow：兩分頁不同 query、快速 replacement、導覽取消、Windows Search/fallback 與 partial error。
- [x] 25.6 對所有新增 crates 執行 architecture audit，確認 UI 無 Shell/COM 型別、apartment-affine interface 不裸跨執行緒、test-support 不進 production graph。
- [x] 25.7 執行 fmt/check/clippy/test 四個 Cargo gates與所有 real-folder/destructive安全測試，保存 locked dependency evidence。
- [x] 25.8 執行多分頁、100k folder、operation、Clipboard/OLE/context menu、search 的 performance/resource soak，報告 median/p95、handles、threads、queues 與 leaks。
- [x] 25.9 更新 visual fixtures，涵蓋多分頁、真實 populated/empty/error folder、operation center、drag cues、context menu 入口與 search states。
- [x] 25.10 更新 `docs/MANUAL_TESTS.md`，加入所有真實資料夾、destructive fixture、Explorer interoperability、第三方 menu 與 search cases 的 actual results。
- [x] 25.11 更新 `docs/STATUS.md`，逐 capability 列出已完成、未驗證、已知差異、Windows/API 限制與後續 broker/namespace/preview 工作。
- [x] 25.12 更新 `docs/PARITY_MATRIX.md`，沒有真實證據的項目不得完成；fake-only、manual-only 與 OS-build-specific evidence 必須清楚分類。
- [x] 25.13 掃描所有 destructive path、COM/OLE ownership、unsafe blocks、terminal events、cancellation 與 timeout，完成安全/一致性 review。
- [x] 25.14 執行 OpenSpec strict validation，確認 proposal 的 7 個 capabilities 都有 spec、tasks 覆蓋全部 requirements，且沒有 M0/M1-only 排除條款殘留。
- [x] 25.15 建立最終 handoff，列出 binary、支援能力、啟動/測試命令、真實資料夾與 Explorer evidence、已知限制，以及後續 thumbnails/namespace/preview/broker hardening 邊界。
