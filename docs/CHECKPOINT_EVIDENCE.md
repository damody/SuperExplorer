# 2026-07-26 checkpoint evidence

## 2026-07-27 真實 D: 排序與欄寬

- `scripts/smoke_sort_columns.ps1` 直接啟動 production app 至只讀 `D:\`，以 UIA InvokePattern 點四個 details headers 與 command Sort menu，再用實體滑鼠拖曳四個 separator 並在視窗外釋放、雙擊 auto-size。
- 最終證據 `target/sort-column-evidence/20260727-final5/report.json`：15 個真實 Shell items 在名稱、修改日期、類型、大小的兩方向都保持相同 identity set；選取的 `AI_Pic Folder` stable identity 未丟失；command menu 最終套用名稱遞減。
- 四欄 physical width 分別由 490→612、261→373、201→311、157→268，均於 window bounds 外 mouse-up 後生效；auto-size 後為 384、279、266、154。model tests另覆蓋 MINIMUM/MAXIMUM clamp、非作用中 tab 隔離、view/window terminal 與 1900 logical px 的水平 overflow。

## Search chrome 與分頁隔離（2026-07-27）

- 搜尋框使用 Explorer 尺寸 token、左側 search glyph 與右側可操作/可存取的清除按鈕；清除 Loading search 會取消 cancellation token、恢復原資料夾 snapshot，且不清掉該分頁的查詢歷史。
- `clear_search_cancels_work_restores_directory_and_keeps_history_per_tab` 驗證兩分頁歷史互不污染。
- 175% DPI headful capture：`target/visual-evidence/20260727-search-clear/screenshot.png`、`metadata.json`、`diagnostics.json`。
- `real_d_unicode_and_parsing_name_two_tab_state_isolation` 逐一以真實 `D:\`、位於 D 槽的巢狀 Unicode fixture、`shell:MyComputerFolder` 建立同 location 的兩分頁，驗證 history、selection、search、address draft 與 view anchor 隔離；temporary fixture 由 RAII 清除。
- `end_to_end_two_tab_search_replacement_navigation_cancel_and_partial_fallback` 於 2026-07-27 重跑通過；輸出在 `target/search-evidence/20260727-regression/two-tab-search.log`，涵蓋快速 replacement、navigation cancellation、Windows Search unavailable/fallback、partial terminal 與兩分頁隔離。

## Windows high contrast、dark D: baseline 與完整滑鼠控制（2026-07-27）

- Production startup 現在以 `SPI_GETHIGHCONTRAST` 讀取 Windows 狀態，啟用時以 `GetSysColor` 將 14 個 semantic slots 映射至 `COLOR_WINDOW`、`WINDOWTEXT`、`BTNFACE`、`GRAYTEXT`、`HIGHLIGHT`、`HIGHLIGHTTEXT`、`HOTLIGHT`，而非用 alpha 或固定 RGB 模擬。
- `scripts/smoke_high_contrast.ps1` 快照 HIGHCONTRAST flags，實機由 126 暫時切至 127，擷取後在 `finally` 還原為 126。`target/high-contrast-evidence/20260727-final-v2` 證明 app diagnostics 的 surface/text/selection 與當下 system colors 完全一致，disabled 與 selected 使用不同且 alpha=255 的顏色；人工檢視黃框、青色 focus/selection、白色 divider 均可辨識。
- `scripts/capture_dark_explorer_baseline.ps1` 快照並暫設 `AppsUseLightTheme=0`、廣播 theme change、恢復最小化的真實 D: Explorer，再擷取 Explorer 與 app，最後還原原登錄值。正式證據 `target/explorer-reference-evidence/dark-final-v3`：Explorer 2685×1621、app 1984×1272、兩者 DPI 168（175%）；cross-app common-region diff 為 31.974665%，只供逐區 review，不當作 self-regression gate。真實 D: dark screenshot 已人工確認為有效內容而非最小化/空白 frame。
- `scripts/smoke_mouse_controls.ps1` 對 11 個 enabled GPUI controls 逐一擷取 hover/pressed 並要求 frame hash 不同；對 Back、Forward、Paste、More 四個 disabled controls 要求 hover 前後 hash 相同。divider 由真實 `WM_LBUTTONDOWN/MOUSEMOVE/LBUTTONUP` 產生 Begin/EndNavigationPaneResize trace。
- Caption 不以 client button 假設驗證：腳本先由 UI Automation 取得實體 bounds、送 client move 同步 GPUI hit-test，再確認 `HTMINBUTTON=8`、`HTMAXBUTTON=9`、`HTCLOSE=20`，以 non-client mouse down/up 驗證 IsIconic、IsZoomed、restore 與 clean close。正式證據為 `target/mouse-evidence/20260727-all-v10`。

## 原生文字輸入、Windows IME 與 UI Automation（2026-07-27）

- Address 與 Search 已改用 `gpui_elements::editable_text`，共享 GPUI-CE 的文字輸入 bindings、selection、caret、marked composition 與 IME handler；Explorer 自有的 Tab、Shift+Tab、Enter、Escape 維持在文字輸入 context 中明確路由。
- `scripts/smoke_ime.ps1` 透過真實 Win+Space TSF 切換至 Microsoft Pinyin，確認目標 UI thread HKL 為 `0x8040804`、LANGID 為 `0x0804`，在 Search 輸入 `ceshi` 並以 Space 提交「测试」。隱私安全 trace 只記錄 byte/char 數與 `contains_cjk=true`，不記錄輸入內容；composition 結束後 `Ctrl+Shift+D` 仍由 Explorer dispatcher 處理，程式 exit 0。
- IME 證據：`target/ime-evidence/20260727-pinyin-final`，包含 composition、commit、後續快捷鍵三張 `PrintWindow` 截圖、各自 SHA-256、實際 HKL、diagnostics 與 stdout/stderr。175% DPI 實機驗證曾發現搜尋框被固定邏輯寬度推離畫面，已縮減 address/search 邏輯寬度並以最終截圖確認兩欄完整可見。
- `scripts/smoke_accessibility.ps1` 以 Windows `.NET UIAutomationClient` RawViewWalker 檢查 production AccessKit tree。最終證據 `target/accessibility-evidence/20260727-uia-final` 有 30 個 live nodes、29 個具名 nodes，涵蓋 Window、TabItem、Button、Document、Edit、Pane、Separator、ListItem、StatusBar；address/search 均為可鍵盤聚焦的 `ControlType.Edit`，active tab 與 selected row 的 selected state 正確。
- UI Automation 的 global focused element 為相同 process 的 application Window；目前 Windows AccessKit bridge 未在 RawViewWalker 的子節點回報 `HasKeyboardFocus=true`。焦點實際路由另由 `target/keyboard-evidence/20260727-editable-final` 的真實前景鍵盤事件與逐 surface `handled_surface` trace 交叉證明。啟用 AccessKit 後關窗會留下上游 bridge 的 invalid-window-handle diagnostic，但 lifecycle、exit code 與 UIA report 都乾淨完成。
- 引入 editable text 後，Windows debug UI thread 在預設 1 MiB PE stack 會於初次排版溢位；`explorer-app/build.rs` 對 MSVC binary 明確設定 `/STACK:8388608`，最終 IME、UIA、keyboard smoke 與 51 個 UI unit tests 均在該 production binary 通過。

## Windows Explorer D: light reference and cross-app diff

- 使用者提供的 D: 參考圖保存於 `target/explorer-reference-evidence/explorer-d-drive-light.png`，
  2667×1603、SHA-256 `8244D6C0DE21B6194C86151C9F6DF6187CD01FEC4D4B0FF36CCC608D42527A71`。
- `scripts/capture_explorer_reference.ps1` 透過 `Shell.Application` 以 LocationURL
  `file:///D:/` 找到真正 Explorer HWND，於 DWM present 後使用
  `PrintWindow(PW_RENDERFULLCONTENT)` 擷取；證據：
  `target/explorer-reference-evidence/d-drive-light-175`。
- Explorer：Windows build 26200、Explorer `10.0.26100.8875`、light、Microsoft
  JhengHei UI/system UI、DPI 168（175%）、2685×1621。
- app：`target/explorer-reference-evidence/app-light-175`，同機、light、同 font、DPI
  168，2684×1620。1×1 px 尺寸差是 1.75 scaling rounding tolerance。
- cross-app diff：`target/explorer-reference-evidence/light-diff-175`；4,348,080 pixels
  比較後 1,012,319 pixels 超過 12/channel（23.281977%），mean max-channel delta
  12.4103。這不是 self-regression pass/fail gate，而是區域差異證據。
- 已依 reference 修正結構順序為 tab → address/search → command → content，並補上
  selected row、active/inactive selected semantic fill。仍可見差異：Explorer 的完整
  namespace navigation、details columns/metadata、系統 icons、較完整 command set 與精確
  typography 尚未達 parity；這些差異沒有被 baseline approval 隱藏。

## Interaction fixture evidence

- 路徑：`target/interaction-evidence/{hover,pressed,focused-light,focused-dark,disabled,selected}`。
- hover/pressed 由 Win32 mouse messages 實際命中 `+ New`；相同 1984×1272 capture
  的像素差異為 7,080（0.280546%），唯一 bounding box `(33,103)-(164,158)`。
- focused-light/focused-dark 顯示 Search focus ring；disabled 顯示不可用 navigation 與
  file commands；selected 顯示一個 stable-identity selection 與 `1 selected` status。
- active/inactive 由 `WM_ACTIVATEAPP/WM_NCACTIVATE/WM_ACTIVATE` 驅動 GPUI production
  activation path；兩張 1599×992 capture 有 77,169 pixels 不同（4.864999%），差異
  bounds `(33,32)-(1586,311)`，涵蓋 non-client chrome、active tab 與 selected row。
- metadata 保存 OS/Explorer/app/GPUI revision、theme、logical/physical size、font、
  screenshot/diagnostics SHA-256。實際 DPI 168（175%）與預期 100% 不符，僅作
  interaction fixture 證據，不作 DPI baseline。

本文件記錄實際執行結果；`target/` 路徑是本機 ignored artifacts，不是提交後仍存在的固定連結。

## 成品啟停與 panic

- Tester：Codex；Windows 11 Professional x64 10.0.26200 Build 26200；Explorer `10.0.26100.8875`；實際 window DPI 168（175%）；NVIDIA GeForce RTX 5090。
- Release lifecycle：`scripts/smoke_windows_lifecycle.ps1 -Profile release -SkipBuild` 通過。視窗從 1134×727 resize 至 1254×807，`WM_CLOSE` 後 exit 0，依序寫入 `window_ready`、`application_stopped`、`clean_shutdown`。
- 完整 window-state evidence：`target/smoke-evidence/20260726T161136893Z-1c5da15d8eea4e68a592686a94984007`。流程為 ready → resize → minimize/restore → maximize/restore → close；`IsIconic`/`IsZoomed` 驗證成功，四張 `PrintWindow(PW_RENDERFULLCONTENT)` 圖片只包含目標 HWND，不會擷取後方使用者視窗。
- Ready snapshot：120 threads、860 process handles、46 GDI handles、21 User handles、70,430,720 bytes working set。完成視窗互動後：120 threads、864 process handles、46 GDI handles、22 User handles、69,574,656 bytes working set；短流程沒有 thread/GDI成長，handle/User差異分別 +4/+1。
- Controlled panic：release `explorer-app.exe` 在 `EXPLORER_TEST_PANIC=1` 下 exit 101；指定 log 具有 `panic` event、版本、thread、location 與受控訊息，測試 sensitive root 未出現在 log。evidence：`target/panic-evidence/20260726T155330224Z-20cc00c9693949f9801b9b44d43d1bfb`。
- Panic 後立即執行上一個 lifecycle case 仍通過，證明沒有留下會阻止下次啟動的半寫程序狀態。
- 已知差異：GPUI-CE 在 `WM_CLOSE` 後仍輸出 invalid-window-handle tracing error；使用者可見流程、ordered cleanup 與 exit code 不受影響，但此差異不能當作已修復。

## Release startup 與資源基線

### UI callback percentile benchmark

- 命令：`cargo test -p explorer-ui --release performance::tests::measures_production_dispatcher_callback_percentiles -- --ignored --nocapture --exact`。
- 每個 workload 20,000 samples，直接呼叫 production `dispatch_action`，release optimized、
  無 debugger；主機為 Ryzen 9 9950X3D、128 GiB、Windows build 26200。
- ResizeNavigationPane：median 100 ns、p95 200 ns、max 105,400 ns、`over_4ms=0`。
- ToggleTheme：median 100 ns、p95 100 ns、max 22,500 ns、`over_4ms=0`。
- FocusTraversal：median 100 ns、p95 100 ns、max 44,900 ns、`over_4ms=0`。
- 無超過 4 ms 樣本，因此沒有慢樣本原因。production root 仍逐 callback 記錄 duration，
  超過 budget 時發出 warning。headful interaction captures 均先收到 GPUI
  `on_next_frame` ready marker，再以 `DwmFlush` 同步 compositor 並用
  `PrintWindow(PW_RENDERFULLCONTENT)` 擷取；六個狀態皆產生完整 frame，未出現半繪製或
  timeout。這是 frame-present 診斷，不宣稱 GPU frame-time percentile。

- Harness：`scripts/smoke_windows_repeated.ps1 -Profile release -Runs 10 -SkipBuild`，從 process launch 量到 `window_ready`，每個樣本是全新 process。
- Hardware：AMD Ryzen 9 9950X3D（16C/32T）、128 GiB RAM、RTX 5090；debugger 未附加。量測時仍有互動式 Codex 工作負載，並非隔離 benchmark runner，這是數值限制。
- Evidence：`target/smoke-repeat-evidence/20260726T155100288Z-ca78545b2b64483297621adddb87322f`。
- 第一個 post-build process（cold）286.445 ms；其餘 9 個 warm processes median 262.962 ms、p95 313.247 ms；全部 10 個 median 263.384 ms、p95 313.247 ms。
- 10/10 exit 0、無殘留 PID、每輪 diagnostics flush 完整。threads 119–120、process handles 886–902、GDI handles 固定 48、User handles 25–26、peak working set 73,207,808–76,001,280 bytes；沒有逐輪單調成長。
- 目標 800 ms cold／400 ms warm 在此機器達成。此結果是短週期 process soak，不取代同一 process 的長時間 navigation/operation queue soak。
- `scripts/smoke_windows_repeated.ps1` 的每輪 lifecycle包含resize與close；window-state harness另驗證minimize/maximize。所有 service request在close前到達terminal，沒有殘留process。這能驗證navigation-free短soak，不宣稱長時間全能力queue soak。

## Cargo、架構與 OpenSpec

- `cargo fmt --all --check`：exit 0。
- `cargo check --workspace --locked`：exit 0。
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`：exit 0。
- `cargo test --workspace --locked`：exit 0；包含 real-folder、destructive operation、OLE Clipboard、native drag cancel、Shell context menu 與 search E2E。三個昂貴／headful案例保持 explicit ignored，需以專用命令執行。
- `scripts/check_architecture.ps1`：UI 無 Shell/Windows dependency 或同步 filesystem I/O；`explorer-test-support` 只出現在 app/Shell 的 dev dependency graph。
- `Cargo.lock` SHA-256：`6466888933319B163D9B06C5347A93090A2045384A61E42A0BFE98004D005995`，gates 前後無 diff。
- GPUI-CE submodule：`gpui-ce/gpui-ce` commit `f9740c88e5f799cef36c14662e3bccff9e0ca363`；workspace 使用 path package `gpui 0.2.2` / `gpui_windows 0.1.0`。
- `openspec validate build-rust-gpui-windows-explorer --strict`：valid。`openspec status --change ...`：proposal/design/specs/tasks 4/4 complete。

## Production safety review

- UI graph 無 COM/Shell 型別；apartment-affine COM interfaces 只在 Shell STA 或獨立 OLE/menu worker 內存活，不裸跨 thread。
- 所有 destructive integration targets 由 owned temporary-root guard 驗證；drive root、workspace、user profile、unresolved target 與 reparse escape 都被拒絕。
- Clipboard/OLE 的 `IDataObject`、`STGMEDIUM`、`HGLOBAL`、PIDL、HMENU、HWND 與 kernel handle 均由明確 RAII/transfer contract 管理；unsafe call sites 有 `SAFETY` ownership/thread/cleanup 理由。
- terminal ledger、generation、cancellation 與 timeout tests 覆蓋 exactly-once terminal、stale rejection、late worker suppression、bounded cancel 與 timeout recovery。
- Review 移除沒有 production consumer 的預留 `OwnedPropVariant`/`into_raw`；COM progress mutex poison 改為記錄並恢復，避免 panic 穿越 FFI callback。
- 剩餘 production `expect` 僅位於 constructor/window/parser/PIDL/buffer bounds 的局部已證明 invariant；它們不處理使用者輸入或 native failure。固定 RGB 掃描為 0；semantic colors 由 theme provider 統一供應。

## 視覺 fixture

- Fixture 支援 `empty`、`populated`、`error`、`multi-tab`、`operation`、`drag-cue`、`search` 七種 deterministic model state；不依賴真實 C:\ 內容。
- 2026-07-26 在實際 175% DPI 擷取七種 light state，位於 `target/visual-state-evidence/<state>`。DPI metadata 明確為 expected 100%、actual 168 DPI、`matches_expectation=false`，所以只能證明 state/harness，不是正式 DPI baseline。
- `DwmFlush` 在 ready marker 後同步 compositor，避免 CopyFromScreen 擷取半完成 frame；multi-tab、drag cue、operation center、partial search 與 error 畫面均經實際檢視。
- 100/125/150/200%、high contrast、Explorer 同尺寸 light/dark diff、Narrator/IME 與第三方 extension 仍需對應實機環境；沒有把這些 case 宣稱為通過。
# Capability soak（2026-07-27）

- 命令：`scripts/run_capability_soak.ps1 -Runs 3 -OutputDirectory target/capability-soak-evidence/20260726-full-3run-v3`。
- 完整機器可讀報告：`target/capability-soak-evidence/20260726-full-3run-v3/report.json`；摘要：同目錄 `report.md`；每個 workload 另保存 stdout/stderr。
- 七個 workload 各三輪皆 exit 0，且無殘留 descendant process；個別測試內的 queue、resource 與 disk oracle 亦全部通過。
- nearest-rank median / p95（ms）：multi-tab 679.235 / 709.372、folder-100k 27455.837 / 27733.315、file-operations 3559.957 / 4108.492、clipboard-ole 7105.789 / 7171.620、ole-drag 1133.466 / 1153.978、context-menu 1689.678 / 1729.507、search-100k 31793.046 / 32652.389。
- 全輪峰值：clipboard-ole 78 threads、2064 handles、206,696,448 bytes working set；context-menu 43 threads、1028 handles；folder-100k 115,015,680 bytes working set。GDI/USER 與逐輪資料保存在 JSON。
# Headful keyboard-only traversal（2026-07-27）

- production root 現在持有並追蹤同一個 GPUI `FocusHandle`；視窗建立時即聚焦，Tab／Shift+Tab 透過正式 key binding 與 typed dispatcher 移動，不是測試專用 state mutation。
- `scripts/smoke_keyboard_navigation.ps1` 使用 foreground Win32 keyboard events，全程不送 pointer click；依序驗證 FileView → StatusBar → WindowChrome → TabStrip → CommandBar → AddressBar → Search → NavigationPane → FileView、反向 traversal、Ctrl+L、Ctrl+F、Ctrl+Shift+D 與 Alt+F4。
- 證據：`target/keyboard-evidence/20260727-headful-v7`，含 14 張逐步 `PrintWindow` 截圖、11 個相異 frame hash、`report.json`、fixture diagnostics 與 clean lifecycle log。light command-bar ring、dark address ring 已人工檢視清楚可辨。
- 執行時 production trace 對每步回報正確的 `handled_surface`，所有 action outcome 均為 `Handled`，Alt+F4 走 `CloseWindow` 並 exit 0；未觀察到焦點遺失或 binding 衝突。
- 此測試也找到並修正先前 root 無 GPUI focus owner、原生按鍵無法進入 dispatch tree 的真實缺陷；artifact finalizer 改用 staging PE 寫入 manifest，以避免新連結 binary 的短暫共享鎖。
