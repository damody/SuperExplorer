# Rust + GPUI Windows Explorer 狀態

## 2026-07-29 Shell context menu／鎖定刪除完整化

`complete-shell-context-and-locked-delete-parity` 已完成原生 file／folder／multi-selection／background、一般／Shift 選單差異驗證，以及 Restart Manager 鎖定擁有者、安全關閉與原操作重試。真實 headful 鎖定刪除與十輪資源 soak 均通過；安全界線、限制與 rollback 見 `docs/SHELL_CONTEXT_LOCKED_DELETE_HANDOFF.md`。

## 2026-07-29 post-parity umbrella roadmap

`complete-explorer-post-parity-roadmap` 已完成 session/settings persistence、bounded async thumbnails、完整 Shell namespace、cross-process extension broker 與 Preview Handler host 的整合實作。五個正式 UITEST capability cases、10 輪 combined soak、installed-path broker/7-Zip/upgrade/uninstall 與 preview visual/accessibility gates 均通過。此機器只有單一 175% 顯示器；其他 DPI raster 與 mixed-monitor 項目維持 truthful hardware-dependent 狀態。詳見 `docs/POST_PARITY_ROADMAP_HANDOFF.md` 與 `docs/POST_PARITY_ROADMAP_REVIEW.md`。

## 統一 UITEST 狀態（2026-07-28）

`explorer-uitest` 現為主要自動驗證入口：manifest 會掃描全部作用中 OpenSpec requirement，提供 quick/full/interop/visual/soak suites，並產生 JSON、coverage、JUnit、Markdown、逐案 log、artifact glob 與 process census。執行方式與 truthful SKIP 規則見 `docs/UITEST.md`。

## 2026-07-27 `match-explorer-visual-address-parity` 收斂狀態

- production UI 已移除直接 Win32 pointer-capture 依賴；`explorer-ui` 只定義 platform-neutral factory/session，`explorer-app` 注入 audited `SetCapture/GetCapture/ReleaseCapture` RAII adapter。
- Explorer command bar 的 compact breakpoint 改讀 layout token，1120 logical px 不再誤隱藏 Cut/Copy/Delete。breadcrumb 繁中 UIA label、fresh-provider click navigation、dark baseline capture 尺寸與 comparator region inputs 均已修正。
- `cargo fmt --all --check`、workspace all-target check、Clippy `-D warnings -W clippy::pedantic`、workspace all-target tests、doc tests、release build、architecture/token gates、OpenSpec strict validation與 Python comparator tests 全部通過。
- 最終 light 實機 gate：`target/explorer-reference-evidence/real-d-light-all-gates-parity-final/report.json`；dark：`dark-parity-final-v4`；high contrast：`target/high-contrast-evidence/20260727-parity-final`。
- 已知未驗證邊界只有需要目前硬體／input driver 無法提供的 actual matrix：100/125/150/200% raster、mixed-DPI 跨螢幕移動、caption 全 DPI click-grid、Explorer↔app physical OLE Drop。typed contracts與單一 175% 實機結果不能替代這些 case。

更新日期：2026-07-26
OpenSpec change：`build-rust-gpui-windows-explorer`

## 目前狀態

工作區已完成 Windows-only Rust/GPUI-CE application、真實多分頁資料夾、`IFileOperation`、Clipboard/OLE drag-and-drop、`IContextMenu3` 與 search 主流程。詳細自動／實機證據見 `docs/CHECKPOINT_EVIDENCE.md` 及各 capability evidence 文件；未具備指定硬體／OS 狀態的 DPI、high contrast、Narrator、IME 與第三方 extension case 仍保持未驗證。

| 階段 | 狀態 | 備註 |
|---|---|---|
| M0 — Bootstrap 與 parity audit | 完成 | locked Cargo gates、release lifecycle、panic、10-run process/resource snapshot 與文件證據齊全 |
| M1 — Explorer shell UI | 完成（設備限制明載） | chrome/theme/layout/actions/focus、light/dark/high-contrast、IME/UIA、headful bundle 與 DPI contract 完成；唯一螢幕 175%，四種正式 raster baseline 不偽造 |
| 多分頁與真實資料夾 | 完成 | per-tab history/generation、real Shell enumeration、watcher、100k explicit soak 與 E2E |
| 原生檔案操作 | 完成 | create/rename/copy/move/delete/cancel/conflict/journal 與 owned destructive fixtures |
| Clipboard/OLE/context menu | 完成（desktop limitation 明載） | native adapters、真實 Explorer Clipboard、OLE matrix harness、resource soak、installed 7-Zip submenu/安全 invoke 完成；本 runner 跨程序 physical Drop 回 None，不列 parity pass |
| Search | 完成 | typed parser、Windows Index probe、bounded fallback、partial/cancel/stale isolation 與真實資料夾 E2E |

## 可重現建置基線

擷取日期：2026-07-26；shell：PowerShell；target：`x86_64-pc-windows-msvc`。

| 項目 | 實測值 |
|---|---|
| `rustc -Vv` | `rustc 1.95.0 (59807616e 2026-04-14)`；LLVM 22.1.2 |
| `cargo -V` | `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` |
| Rust toolchain | `stable-x86_64-pc-windows-msvc` |
| Windows | Windows 11 Professional x64，Version `10.0.26200`，Build `26200` |
| Process architecture | `X64` |
| Visual Studio Build Tools | VS 2022 Build Tools `17.14.32`（installation version `17.14.37301.10`） |
| 其他可用 MSVC | VS 2026 Community `18.6.3`、VS 2019 Build Tools `16.11.58` |
| Windows SDK | `10.0.26100.0`（另有 `10.0.19041.0`） |

基線命令：

```powershell
rustc -Vv
cargo -V
rustup show active-toolchain
Get-CimInstance Win32_OperatingSystem
```

## 固定依賴查核

- `gpui-ce/gpui-ce` 已固定 checkout `f9740c88e5f799cef36c14662e3bccff9e0ca363`；此專案內 commit 在上游基線上加入 native external-drop negotiation，`vendor/gpui-ce` gitlink 與指定 SHA 完全一致。
- GPUI-CE 的 `crates/gpui` package 名稱仍是 `gpui`、版本 `0.2.2`；Windows backend 使用同一 submodule 的 `gpui_windows 0.1.0`。
- `gpui-component bc174a7...` 與此 GPUI-CE revision 實測有 11 個編譯錯誤：缺少 `container_query`、`flex_grow_1`、`flex_shrink_1`，以及 `flex_grow(f32)` signature 改變。上游 component `main` 仍是同一 commit，沒有可升級 revision。
- 為維持單一 GPUI type graph 與 fresh-clone 可重現性，本專案移除 `gpui-component`，改由 GPUI-CE 原生 elements 與窄版 semantic helpers實作 Explorer controls；不建立未發布的本地 fork。
- GPUI API capability spike 記錄於 `docs/GPUI_CAPABILITY_SPIKE.md`。

### 依賴解析修正

採用的配置是將 GPUI-CE 加為 `vendor/gpui-ce` Git submodule：

- workspace 的直接 `gpui` 與 `gpui_windows` 使用 `vendor/gpui-ce/crates/...` path，app 直接建立 `WindowsPlatform`。GPUI-CE backend 會啟用其內建 PerMonitorV2／Common Controls／SegmentHeap manifest；app 自有 resource 只嵌 VERSIONINFO，避免兩個 manifest 同為 ID 1 造成 `CVT1100`。
- `.gitmodules` 保存 `https://github.com/gpui-ce/gpui-ce.git`，clone 後以 `git submodule update --init --recursive` 重現精確 gitlink。
- `Cargo.lock` SHA-256 為 `6466888933319B163D9B06C5347A93090A2045384A61E42A0BFE98004D005995`；gates 前後無 diff，所有 locked commands 通過。
- `Cargo.lock` 只有一個 `gpui 0.2.2` path package；`cargo tree -i gpui` 的 consumers 只有 app、UI 與同 submodule 的 `gpui_windows`。`gpui-component`、`gpui_platform` 與 Zed Git source 數量皆為 0。

## 已執行但受環境限制的項目

- 100/125/150/200% typed DPI geometry 與四次 capture flow 已執行；唯一螢幕實際 175%，report 保留 requested/actual mismatch，不把圖片升級成四種正式 raster baseline。mixed-DPI 因只有一台 active monitor 無法執行。
- high contrast、UIA accessibility、Microsoft IME composition、light/dark Explorer 對照均已實機執行。完整 Narrator 朗讀體驗未取代 UIA role/name/state/focus 證據。
- 本程式↔Explorer 的 left/right、single/multi、copy/move/none strict harness 已執行；本 Codex desktop 的合成 input sequence 得到 `DROPEFFECT_NONE`，DragEnter/Over、data object、effect/terminal 與磁碟管線以分層證據驗證，physical Drop 需專用 GUI/input-driver runner。

## 已完成的自動證據

- GPUI-CE gates：architecture check、`cargo fmt --all --check`、`cargo check --workspace --locked`、workspace all-targets Clippy `-D warnings`、`cargo test --workspace --locked` 全部通過。
- M0 lifecycle/resource 變更後於 2026-07-26 再次執行 architecture check 與四個 Cargo gates，全部 exit code 0；workspace tests 包含 app 5、panic subprocess 1、common 5、Shell STA 5、test-support 1，其餘 crates 目前無測試。
- M1 theme/layout foundation：同機 Explorer light capture 完成初始 surface/control/divider/accent 量測；typed light/dark palettes、high-contrast system-role mapping、logical layout tokens、pane/hit-target invariants、100/125/150/200% scaling、root token injection 及 CI token source lint均已實作。`explorer-ui` 7 個 tests 與 targeted Clippy 通過；dark/high-contrast 尚未實機視覺驗收，因此只視為 foundation。
- M1 state/action/focus foundation：`AppViewState`、typed actions、8 組 default bindings、same-scope conflict check、single-dispatch outcome、focus coordinator/restore、shared interaction state 與 privacy-safe tracing 已實作。`explorer-ui` 累計 14 個 tests 通過；Back/Forward/Up 在真實 per-tab history 接入前保持 disabled，address/search 只有 focus/restore，不建立虛構 submit。
- M1 Explorer chrome：`ExplorerWindow` 已組合自繪 title/tab chrome、command/navigation bar、address/search placeholders、navigation pane、五態 `FileViewHost` 與中性 status bar；所有區域有穩定 ID。新分頁、Back/Forward/Up、檔案命令與 submit 在真實 model/service 接入前保持 disabled。GPUI click 與 7 組 Windows 快捷鍵共用 typed dispatcher，theme action 由 root 一次更新整棵 component tree。`explorer-ui` 累計 18 個 tests、targeted Clippy 與 UI token lint 通過。
- M1 caption capability：Drag／Min／Max／Close 使用 GPUI-CE `WindowControlArea` 原生 non-client hit-test，Windows backend 映射至 `HTCAPTION`／`HTMINBUTTON`／`HTMAXBUTTON`／`HTCLOSE`；2026-07-26 完整 chrome headful resize／WM_CLOSE smoke exit 0。Snap flyout 視覺仍保留人工驗收，未宣稱完成。
- M1 geometry/divider：新增 deterministic `WindowGeometry`，覆蓋 baseline、極窄、maximize/restore 與 pane/content split；所有有限尺寸均不產生負 geometry，非有限輸入明確拒絕。最小 GPUI-CE divider 使用內建 hitbox capture、move/up/up-out、雙擊 reset，並提供 Ctrl+Alt+Left/Right/Home 鍵盤調整；terminal model 覆蓋 pointer-up、拖出、取消與 window blur。固定 GPUI-CE revision 與 `gpui-component` API 不相容，因此維持小型專案內 element。M1 pane width 只存在記憶體，關閉程序後不持久化。
- M1 theme/interaction/accessibility：theme toggle 以單一 root transition 同步整套 semantic provider，behavior test逐槽驗證 dark tokens；button、tab、placeholder、divider、caption 已套用 hover／pressed／disabled／selected／focus tokens。AccessKit roles/names、tab selected、splitter numeric range、caption invoke 與 splitter increment/decrement 已接入；GPUI-CE `StatefulInteractiveElement` 未公開 disabled state setter，故 disabled 控制只有不可 invoke 與 unavailable name，具體差異保留在 parity matrix。非必要動畫統一由 duration token 控制，reduced-motion contract 回傳 0 ms。
- M1 input/IME boundary：address/search 仍是明確的 `FocusOnly` placeholder，不註冊 GPUI text input handler，因此不接收、攔截或改寫 IME composition；只有後續切換為 `Editable` 的真實輸入元件才允許 composition。contract test 同時鎖定兩個 M1 placeholder。
- M1 UI regression harness：新增完全不需要 Window/Shell service 的 `UiTestHarness`，共用 production dispatcher 並收集 typed traces。測試覆蓋 mouse/keyboard disabled Back/Forward/Up、address/search focus restore、整套 light/dark semantic snapshot、divider terminal paths與多尺寸 geometry。architecture gate 也新增 source scan，阻止 `explorer-shell-win`／Win32 dependency、Shell coupling與同步 filesystem I/O 進入 `explorer-ui`；目前 UI 累計 29 個 tests。
- M1 callback instrumentation：production root 對 resize、theme 與 focus 等所有 typed UI dispatch 收集 callback duration；4 ms 內寫 trace，超過 4 ms 寫包含 action 與微秒值的 warning。release explicit benchmark 各跑 20,000 samples：resize median/p95 100/200 ns、theme 100/100 ns、focus traversal 100/100 ns，三組 `over_4ms=0`；headful fixture 另以 GPUI `on_next_frame`、`DwmFlush` 與 `PrintWindow` 驗證完整 frame present。
- M1 repeated headful smoke：`scripts/smoke_windows_repeated.ps1` 會重用已 finalization 的 exe，逐次驗證 window ready、resize、WM_CLOSE、exit 0、ordered lifecycle/diagnostics flush及無殘留 PID，並保存每次 thread/process handle/GDI/User/working-set 樣本。2026-07-26 debug 實測 10/10 通過：crash 0、殘留 process 0、threads 86–87、process handles 648–650、GDI 46、User 20；報告位於 ignored evidence `target/smoke-repeat-evidence/20260726T102441508Z-e88a38c2dff540b8b2609c854385009e`。這是跨新程序的 smoke range，不等同長時間同程序 leak proof。
- GPUI-CE headful smoke：`EXPLORER_AUTO_CLOSE_MS=750 cargo run -p explorer-app --locked` 啟動 D3D 11.1 window、寫入 `window_ready`，程序 exit code 0。
- 成品 exe resource 已由 `scripts/finalize_windows_artifact.ps1` 驗證：Cargo link 階段採 GPUI-CE 內建 manifest 的唯一 ID 1；finalization 以保留相同 PerMonitorV2／Common Controls 6／SegmentHeap 設定且補上 definition `assemblyIdentity` 的專案 manifest 原位更新 ID 1，避免 duplicate resource，並通過 Windows `mt -validate_manifest`。同一腳本確認 PE machine 為 x64 (`0x8664`)，VERSIONINFO 顯示 `Rust GPUI Windows Explorer`／`0.1.0`。CI 會取得 recursive submodule 後執行此驗證；GPUI-CE submodule不需修改。
- Diagnostics targeted test：5 個 common unit tests與 1 個 subprocess panic integration test通過。
- Diagnostics targeted clippy：`explorer-common`／`explorer-app` all-targets、`-D warnings` 通過。
- Panic report 已驗證包含 version/thread/location/backtrace availability、回傳失敗 exit code，且 configured sensitive root 不出現在 log。
- Shell STA targeted test：5 個測試通過；涵蓋全部合法／非法狀態轉移、真實 `COINIT_APARTMENTTHREADED`、message-pump cycle、正常與重複 shutdown/join、啟動失敗注入、bounded startup timeout 及最終資源回收。
- Shell STA targeted clippy：`explorer-shell-win --all-targets --locked -- -D warnings` 通過；必要 Win32 unsafe calls 均逐處記錄 pointer、thread/apartment、return-code 與 cleanup invariant。
- App startup coordinator：3 個 unit tests 通過；驗證固定五階段啟動／反向關閉、每個啟動失敗點只清理已完成階段一次，以及 Shell HRESULT error 可被 composition boundary 觀察並觸發先前資源 unwind。
- Windows DPI prerequisite：2 個 unit tests 通過；程序在建立 GPUI platform／HWND 前呼叫 `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`。呼叫成功記為 `Applied`；manifest 或 host 已先固定 process DPI 時的 `ERROR_ACCESS_DENIED` 記為可接受的 `AlreadyConfigured`；其他 HRESULT 保留並中止啟動。GPUI-CE manifest 同時宣告 `PerMonitorV2`。
- Production composition root 集中持有 diagnostics、DPI prerequisite、Shell STA 與 GPUI event loop；最小視窗使用 1120×720 logical px、640×480 minimum、允許 resize。GPUI-CE Windows backend 會在 event loop 結束後直接 `ExitProcess(0)`，因此 cleanup 已註冊在 `on_app_quit` 並與 explicit shutdown／Drop 共用 idempotent state；headful auto-close 實測 exit code 0，且在 process exit 前依序留下 `window_ready`、`application_stopped`、`clean_shutdown`。
- `explorer-common/build.rs` 將目前 Git short revision 編入 `AppBuildInfo`；可用 `EXPLORER_GIT_REVISION` 明確覆寫，工作樹有 tracked 變更時加上 `-dirty`。實測 startup diagnostics 記錄 `revision="793fbe0e7e94-dirty"`，視覺／效能 evidence 可回溯到基準 commit 與 dirty 狀態。
- `scripts/smoke_windows_lifecycle.ps1` 提供本機／專用 GUI runner 的 headful harness：等待 diagnostics ready marker 與 HWND、用 Win32 `MoveWindow` 實際 resize、送 `WM_CLOSE`、等待程序退出並驗證 cleanup event 順序。2026-07-26 實測由 1134×757 調整為 1254×837、exit code 0；GPUI-CE 在視窗銷毀期間另輸出兩筆 invalid-window-handle tracing error，未影響 ordered cleanup，但保留為後續 caption/window-close parity 觀察項目。

## Shell STA 資源快照限制

`StaResourceSnapshot` 只量測本實作擁有且可精確歸因的 STA thread、control channel endpoint 與 Rust `JoinHandle`（其持有原生 Windows thread handle）。測試不使用全程序 handle count，因為 Cargo test harness、tracing 與其他平行 runtime 可能同時建立無關 handles；因此此證據可偵測本生命週期的遺漏，但不能宣稱程序內所有第三方 handles 都沒有變化。M0 headful manual test 仍須另存程序層級快照。

## 2026-07-26 release regression baseline

- Release 10-run evidence：`target/smoke-repeat-evidence/20260726T155100288Z-ca78545b2b64483297621adddb87322f`。cold 286.445 ms；warm median 262.962 ms／p95 313.247 ms，低於 800/400 ms 目標。
- 10/10 exit 0；threads 119–120、process handles 886–902、GDI 48、User 25–26、peak working set 73.2–76.0 MB，沒有逐輪單調增加。
- Release panic evidence：`target/panic-evidence/20260726T155330224Z-20cc00c9693949f9801b9b44d43d1bfb`；exit 101、panic event 可找到、sensitive root 未洩漏；隨後正常 lifecycle exit 0。
- 七種 deterministic visual states 位於 `target/visual-state-evidence/<state>`；實際 175% DPI，因此不是 100/125/150/200% baseline。
- 完整命令、硬體、限制、architecture/dependency 與 safety review 見 `docs/CHECKPOINT_EVIDENCE.md`。
