# Windows 手動驗收

## 先執行自動 UITEST

手動驗收前先執行 `cargo run -p explorer-uitest -- --suite quick`，再依範圍執行 full、interop 或 visual。報告位於 `target/uitest-runs/<run-id>`；手動紀錄應引用該次 `report.json` 與 evidence 路徑。只有 prerequisite 不符時才接受 SKIP，release gate 請加上 `--fail-on-skip`。

## 2026-07-27 parity closure 實際結果

| Case | 結果 | 證據／限制 |
|---|---|---|
| Explorer `D:\` light 175% geometry/icon/color/type | Pass | `target/explorer-reference-evidence/real-d-light-all-gates-parity-final/report.json`；15/15 regions、4/4 icons、4/4 colors、8/8 typography |
| Dark / high contrast | Pass | `target/explorer-reference-evidence/dark-parity-final-v4`；`target/high-contrast-evidence/20260727-parity-final` |
| Breadcrumb / `>` menu / address | Pass | `target/breadcrumb-uia-evidence/20260727-parity-final-v5`；真實 `D:\test` 子資料夾、topmost hit、click navigation |
| Sort / Details columns | Pass | `target/sort-column-evidence/20260727-parity-final/report.json`；四欄雙向排序、四 separator drag/double-click/outside release |
| View / panes / show flags | Pass | `target/view-pane-evidence/20260727-parity-final/report.json` |
| Scrollbars / capture | Pass | `target/scrollbar-capture-evidence/20260727-parity-final/report.json`；pointer 離開 scrollbar、client、HWND 後仍更新並 exactly-once release |
| Keyboard / UIA / mouse / lifecycle | Pass | `target/headful-evidence/20260727-parity-final-v4/report.json`；`target/accessibility-evidence/20260727-parity-final` |
| Windows IME | Pass | `target/ime-evidence/20260727-parity-final-v2/report.json` |
| 100/125/150/200% raster / mixed-DPI monitor | 未驗證 | 本機只有一個 175% 顯示器；DPI report 與 monitor topology 明確保留 mismatch |
| Explorer↔app physical OLE Drop | 未驗證 | 合成 input driver 無法完成跨程序 Drop；production OLE/data/effect/cancel/resource tests 通過，但不得升格為 physical parity pass |

本文件只記錄實際執行結果。尚未執行的案例填「未執行」並說明原因；不得把 expected result 複製成 actual result。

## 執行環境

| 欄位 | 實際值 |
|---|---|
| 日期／時間 | 2026-07-26 23:51–24:01（Asia/Taipei） |
| Tester | Codex |
| App commit/build | release binary；source checkpoint `eaa302320682531034f84373f968921b05faf1e1`（早期 artifacts 依 metadata 記錄其 dirty revision） |
| Windows edition/build | Windows 11 Professional x64 / 10.0.26200 Build 26200 |
| Explorer version | `10.0.26100.8875` |
| Rust/toolchain | `rustc 1.95.0` / `stable-x86_64-pc-windows-msvc` |
| Monitor/DPI | 實際 168 DPI（175%）；100/125/150/200% 未執行 |
| Theme/high contrast | light/dark app capture；high contrast 未執行 |
| IME/accessibility tool | 未執行 |

## 單一案例紀錄模板

### Case ID：`<capability>-<number>`

| 欄位 | 內容 |
|---|---|
| Capability／Milestone | 未填 |
| 前置條件 | 未填 |
| 測試資料 ownership | 未填；破壞性案例必須是測試唯一擁有的 temporary root |
| 操作步驟 | 未填 |
| Expected | 未填 |
| Actual | 未執行 |
| 結果 | 未執行（Pass／Fail／Blocked） |
| 證據路徑 | 未產生 |
| 已知差異／Issue | 無資料 |
| 未驗證原因 | 尚未執行 |

## 必測矩陣

| Case 群組 | 主要情境 | Actual | Evidence | 未驗證原因 |
|---|---|---|---|---|
| M0 lifecycle | 啟動、resize、關閉、panic、重複啟停 | Pass：release resize/close、panic 101 後再啟動、10/10 啟停均符合 expected | `target/smoke-evidence/20260726T155330464Z-7818dfd2fa654062a0db7be4efcc5757`；`target/panic-evidence/20260726T155330224Z-20cc00c9693949f9801b9b44d43d1bfb`；`target/smoke-repeat-evidence/20260726T155100288Z-ca78545b2b64483297621adddb87322f` | 無；已知 shutdown tracing error 另記但不影響結果 |
| M1 visual/input | light/dark、focus、100/125/150/200% DPI、Snap/caption | Partial：light/dark 與 175% 七狀態 capture 可讀；指定 DPI、high contrast、keyboard/Narrator/IME、Snap 未執行 | `target/visual-actual/20260726T154742729Z-8fd0b9ab261f46198056a930f8ceb5f8`；`target/visual-actual/20260726T154744546Z-feb048e06a2b45d58b1df5e09608c58f`；`target/visual-state-evidence/<state>` | 本 session 螢幕為 175%，不能冒充指定 DPI；無 accessibility/IME 操作者 evidence |

## 可重現 headful 自動 smoke

下列命令會建置並 finalization 成品、等待 `window_ready`、實際調整 HWND 尺寸、送出 `WM_CLOSE`，再檢查 exit code 與 `application_stopped`／`clean_shutdown` 順序。它是 M0 自動 evidence，不取代本文件的人工 resize／caption／DPI 驗收。

```powershell
./scripts/smoke_windows_lifecycle.ps1 -Profile debug
```

2026-07-26 本機實測：通過；1134×757 → 1254×837；exit code 0。evidence 由腳本寫入 `target/smoke-evidence/<UTC>-<GUID>/`，屬本機產物不提交。GPUI-CE 關閉期間 tracing 曾輸出 invalid-window-handle error，後續人工 caption/close case 必須確認是否可重現及是否有可見影響。
| Tabs/navigation | 多分頁隔離、Back/Forward/Up、快速導覽取消 | Pass（自動 real-folder E2E） | `docs/REAL_FOLDER_EVIDENCE.md` | 頭戴式滑鼠流程未另錄影 |
| Real folders | Unicode、100k、permission、reparse、watcher overflow | Pass（real temporary roots；100k explicit benchmark） | `docs/REAL_FOLDER_EVIDENCE.md` | 真實受權限保護的使用者資料夾未破壞性測試 |
| File operations | create/rename/copy/move/delete/cancel/conflict/undo | Pass（owned destructive fixtures） | `docs/FILE_OPERATIONS_EVIDENCE.md` | Recycle Bin UI 最終呈現未人工比對 |
| Clipboard | Explorer ↔ app copy/cut/paste | Pass：實際 Explorer single/multi copy/cut → app paste matrix | `docs/CLIPBOARD_INTEROP_EVIDENCE.md` | app → Explorer paste 尚未完成完整人工矩陣 |
| OLE drag/drop | Explorer ↔ app left/right drag、effects、auto-scroll | Partial：native DoDragDrop/drop negotiation/cancel/resource tests通過 | `docs/DRAG_DROP_EVIDENCE.md` | 實際 Explorer 雙向手勢矩陣未執行 |
| Context menu | background/single/multi、owner draw、extension failure | Partial：真實 Shell menus與 controlled IContextMenu3 fixture通過 | `docs/CONTEXT_MENU_EVIDENCE.md` | 已安裝第三方 extension 未執行 |
| Search | Windows Search/fallback、cancel、stale results、100k | Pass（real folders；index availability truthful） | `docs/SEARCH_EVIDENCE.md` | OS index 對 temporary root 不可用時正確使用 fallback |
| Accessibility | keyboard-only、Narrator smoke、high contrast、IME | 未執行 | 未產生 | 尚無 UI |

## M0 STA 自動快照的解讀限制

自動測試的 `StaResourceSnapshot` 僅計算本實作擁有的 STA thread、control channel endpoint 與 `JoinHandle`。它刻意不把整個測試程序的 handle count 當成洩漏判準，因為 test harness 與其他 runtime 可能同時建立無關 handles。手動 M0 lifecycle 驗收仍需在獨立 app process 記錄啟停前後的程序 handle/thread snapshot、log 路徑與實際差異。

## 2026-07-26 actual cases

### Case ID: `ContextMenu-installed-7Zip-01`

| 欄位 | 實際結果 |
|---|---|
| Capability | Installed third-party Shell context menu extension |
| Handler | 7-Zip `{23170F69-40C1-278A-1000-000100020000}` |
| Fixture ownership | owned temporary root；`third-party.txt` 與輸出 archive 均隨 fixture cleanup |
| Actual | Pass：真實 menu 出現 top-level 7-Zip、11 個 depth-1 commands、CRC SHA depth-2 submenu；keyboard query 成功；invoke `加入 "third-party.7z"` 建立非空 archive |
| Owner-draw | 7-Zip 非 owner-draw；相同 host path 的 controlled IContextMenu3 fixture 已驗證 measure/draw/menu-char/init-popup/reentrancy |
| Evidence | explicit ignored test stdout；`docs/CONTEXT_MENU_EVIDENCE.md` |

### Case ID: `Explorer-D-light-reference-175dpi-01`

| 欄位 | 實際結果 |
|---|---|
| Capability / Milestone | Windows Explorer light reference / app light comparison |
| 環境 | Windows build 26200、Explorer 10.0.26100.8875、DPI 168（175%）、light、Microsoft JhengHei UI/system UI |
| Explorer 步驟 | 保持 `file:///D:/` Explorer 視窗開啟；Shell.Application 找 HWND；DwmFlush；PrintWindow(PW_RENDERFULLCONTENT) |
| App 步驟 | 同機、同 theme/font/DPI，以 1520×919 logical fixture 擷取 2684×1620 physical image |
| Actual | Explorer 2685×1621；app 2684×1620（1 px rounding）；cross-app changed ratio 23.281977% |
| 已修正 | chrome 順序改為 tab→address/search→command→content；selected row 與 active/inactive fill 接上 semantic token |
| Known differences | 完整 Explorer navigation namespace、details columns/metadata、system icons、command set、typography 尚有差異 |
| Evidence | `target/explorer-reference-evidence/{d-drive-light-175,app-light-175,light-diff-175}` |

### Case ID: `M1-mixed-dpi-equipment-audit-01`

| 欄位 | 實際結果 |
|---|---|
| Capability / Milestone | Mixed-DPI cross-monitor window movement |
| 命令 | `./scripts/audit_monitor_topology.ps1` |
| Actual | 無法執行跨螢幕 DPI 測試：Windows Forms 與 WMI 都只偵測到一台 active MSI monitor |
| Evidence | `target/mixed-dpi-evidence/monitor-topology.json`；`screen_count=1`、`active_physical_monitor_count=1` |
| 已知本機 app DPI | interaction fixture 的 `GetDpiForWindow=168`（175%） |
| 未驗證原因 | 無第二台不同 DPI monitor；未更動使用者顯示設定，也不以單螢幕縮放模擬跨 monitor 行為 |

### Case ID: `M1-interaction-states-175dpi-01`

| 欄位 | 實際結果 |
|---|---|
| Capability / Milestone | M1 shared interaction states |
| 環境 | Windows 11 build 26200、Explorer 10.0.26100.8875、實際 DPI 168（175%） |
| 步驟 | headful 啟動 fixture；以 Win32 mouse messages hover/press `+ New`；擷取 light/dark focused、disabled、selected |
| Actual | Pass：hover/pressed 差異 7,080 pixels，bounding box `(33,103)-(164,158)`；focus ring 在 light/dark 都清楚；disabled 與 selected 狀態正確 |
| Evidence | `target/interaction-evidence/{hover,pressed,focused-light,focused-dark,disabled,selected}` |
| 未驗證 | 此 case 不是 100/125/150/200% baseline，亦不取代完整 keyboard-only traversal |

### Case ID：`M0-release-lifecycle-01`

| 欄位 | 內容 |
|---|---|
| Capability／Milestone | M0 process lifecycle |
| 前置條件 | finalization 完成的 release x64 binary；一般互動式 desktop |
| 測試資料 ownership | 不執行 destructive filesystem operation |
| 操作步驟 | 啟動並等 `window_ready`；取得 HWND；resize；minimize/restore；maximize/restore；每個可見狀態以PrintWindow擷取；送 `WM_CLOSE`；等待 exit |
| Expected | 各 window state可觀察且restore bounds有效；exit 0；cleanup events依序 flush；無殘留 PID |
| Actual | 符合；ready 286.543 ms；`IsIconic`/`IsZoomed`狀態正確；四張目標HWND圖片完整；ready→互動後 threads 120→120、GDI 46→46、handles 860→864、User 21→22、working set 70.4→69.6 MB |
| 結果 | Pass |
| 證據路徑 | `target/smoke-evidence/20260726T161136893Z-1c5da15d8eea4e68a592686a94984007` |
| 已知差異／Issue | close 後 GPUI-CE tracing 輸出 invalid window handle；沒有可見 crash，exit/cleanup 仍正確 |
| 未驗證原因 | minimize/maximize/Snap 不屬本 case，仍列在 M1 未驗證 |

### Case ID：`M0-controlled-panic-01`

| 欄位 | 內容 |
|---|---|
| Capability／Milestone | M0 diagnostics/panic |
| 前置條件 | release binary；獨立 `EXPLORER_LOG_DIR`；受控 panic env |
| 測試資料 ownership | evidence directory由測試建立；sensitive marker 只用於 leak oracle |
| 操作步驟 | 觸發受控 panic；檢查 exit/log/sensitive marker；立即執行正常 lifecycle |
| Expected | 非零退出、panic report 可找到、敏感資料不洩漏、下次啟動成功 |
| Actual | exit 101；panic event與受控訊息存在；sensitive root 未出現在 log；後續正常啟動 exit 0 |
| 結果 | Pass |
| 證據路徑 | `target/panic-evidence/20260726T155330224Z-20cc00c9693949f9801b9b44d43d1bfb`；後續 lifecycle evidence 同上 |
| 已知差異／Issue | panic message本身不含 sensitive root，因此沒有 `%REDACTED_ROOT%` token；以「實際 root 不存在」作 leak判準 |
| 未驗證原因 | 無 |

### Case ID：`M1-visual-states-175dpi-01`

| 欄位 | 內容 |
|---|---|
| Capability／Milestone | M1/全範圍 deterministic visual states |
| 前置條件 | 實際 175% desktop；900×560 logical fixture；light theme |
| 測試資料 ownership | presentation-only deterministic model，不碰真實資料夾 |
| 操作步驟 | 逐一擷取 empty/populated/error/multi-tab/operation/drag-cue/search；等 ready marker與 DWM present；檢視 PNG |
| Expected | 每個 state 顯示對應且不互相污染的狀態；無 clipping或半完成 frame |
| Actual | 七個 state 均成功擷取；multi-tab、operation center、folder drop cue、partial search與error訊息可辨識；DwmFlush 修正早期半完成 frame競態 |
| 結果 | Pass（state/harness only） |
| 證據路徑 | `target/visual-state-evidence/<state>` |
| 已知差異／Issue | actual DPI 168；metadata `matches_expectation=false`，不可更新正式 baseline |
| 未驗證原因 | 100/125/150/200%、high contrast、Explorer baseline與accessibility需要不同實機環境 |

### Case ID：`M1-dpi-matrix-actual-result-01`

| 欄位 | 內容 |
|---|---|
| Capability／Milestone | 100/125/150/200% DPI layout、固定尺寸 capture、啟動／resize／focus matrix |
| 命令 | `./scripts/capture_dpi_matrix.ps1 -OutputDirectory target/dpi-evidence/20260727-final` |
| 自動 contract | typed 100/125/150/200% scale matrix 全數通過；1120×720 logical geometry 在四種比例都只 scale 一次，沒有 double scaling |
| 實際桌面 | 唯一 active monitor 為 DPI 168（175%）；四個 requested case 均實際啟動、focus、resize、hit target 與擷取，但 `matches_expectation=false` |
| Actual | 四次 logical width/height 均為 1120×720；PNG SHA-256 相同；沒有 clipping、負尺寸或重複 scaling；本機 175% 的 1 px physical rounding 已納入既有 Explorer/app 對照 |
| Baseline 判定 | 不把 175% 擷取冒充 100/125/150/200% 正式 baseline；report 明確保留 requested/actual mismatch |
| 設備限制 | 只有一台 monitor；未修改使用者唯一互動式 session 的系統縮放，亦無第二台／隔離 session 可切換四種 DPI |
| 證據路徑 | `target/dpi-evidence/20260727-final/{report.json,100,125,150,200}` |

### Case ID：`M9-headful-command-bundle-01`

| 欄位 | 內容 |
|---|---|
| Capability／Milestone | 完整 headful app／STA lifecycle／panic／repeated／visual command bundle |
| 命令 | `./scripts/run_headful_validation.ps1 -SkipBuild -OutputDirectory target/headful-evidence/20260727-final-v3` |
| Actual | lifecycle、連續 3 次 start/close、keyboard、accessibility、mouse、controlled panic、visual capture 共 7 步全部 exit code 0 |
| UIA 穩定性 | caption control 查詢加入最長 5 秒的 100 ms retry；修正一次 Windows UIA 暫時漏列 Minimize 的競態，沒有放寬 hit-test／state oracle |
| 資源／關閉 | repeated report：3/3 diagnostics flush、crash 0、residual process 0；完成後另行清除早期被 timeout 中止的兩個測試 PID |
| 證據路徑 | `target/headful-evidence/20260727-final-v3/report.json` 與其 keyboard/accessibility/mouse/visual 子目錄 |
