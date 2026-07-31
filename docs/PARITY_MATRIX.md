# Windows Explorer parity matrix

## Complete Shell menu and locked-delete recovery (2026-07-29)

| Capability | Status | Evidence / boundary |
|---|---|---|
| File/folder/multi/background native context menus | Pass | All ordinary/Shift target combinations match the same disposable worker exactly through the broker; installed-provider inventory includes 7-Zip, WinRAR, TortoiseGit, editors, Defender and Send To. |
| Escape, outside click, right-click retarget, focus | Pass | Native popup lifecycle and headful interaction cases. |
| Locked owner discovery and recovery dialog | Pass | Real owned lock helpers plus UIA, keyboard, pointer, multi-owner and Close-and-retry evidence. |
| Safety | Pass | Graceful Restart Manager only; no force termination/elevation; PID creation time and eligibility are revalidated. |
| Third-party host-specific menu variation | Truthful limitation | Providers may vary commands by executable identity. Same-worker direct/broker result is exact; independent-host delta remains diagnostic. |

Full evidence, limitations, safety and rollback: `docs/SHELL_CONTEXT_LOCKED_DELETE_HANDOFF.md`.

## Post-parity roadmap comparison (2026-07-29)

Session restore、thumbnail scheduling/cache、Home/Quick Access/This PC/Libraries/ZIP/Recycle Bin/Network、brokered Shell extensions 與 Preview Pane 已逐項比對 Windows File Explorer 的 command、menu、mouse/keyboard、focus、fallback 與錯誤行為。可由 public API 達成的差異已關閉；provider-owned preview theme/rendering、網路/雲端可用性、第三方 handler 與缺少對應實體螢幕的 DPI raster 是明確記錄的環境限制。完整結果見 `docs/POST_PARITY_ROADMAP_HANDOFF.md`。

## 自動回歸入口

矩陣中的自動證據由 `explorer-uitest` manifest 統一管理。quick suite 負責 workspace 與契約守門；full/interop/visual suites 負責真實視窗、Shell 與 raster 證據。缺少互動桌面、TortoiseGit、指定磁碟或多螢幕時必須記為帶原因的 SKIP，不得視為 parity pass。

## 2026-07-27 Explorer parity 最終收斂

主要繁中 Windows 11 profile（build 26200、Explorer 10.0.26100.8875、175%、`D:\` Details）已由 `target/explorer-reference-evidence/real-d-light-all-gates-parity-final/report.json` 驗證：15/15 具名區域、4/4 icon、4/4 semantic color samples、8/8 typography 全部通過；最差幾何差是 file row height 8.56%，低於 10%。dark profile 位於 `target/explorer-reference-evidence/dark-parity-final-v4`，high contrast 位於 `target/high-contrast-evidence/20260727-parity-final`。

網址列與 chevron 的 UIA/headful 證據位於 `target/breadcrumb-uia-evidence/20260727-parity-final-v5`：`本機 >` 列出實際磁碟機，`D:\test >` 列出直接子資料夾；menu item 通過 topmost hit-test，並從重新取得的 UIA provider 點擊真實可見資料夾完成導覽。AccessKit 0.33 對 `Role::MenuItem` 不公開 `SelectionItemPattern`，因此 keyboard type-ahead 以實體按鍵與最終導覽驗證，不偽造 ListItem role。

功能矩陣補充：真實 `D:\` 四欄雙向排序與欄寬拖曳／auto-size／release-outside 在 `target/sort-column-evidence/20260727-parity-final` 通過；八種檢視與 panes 在 `target/view-pane-evidence/20260727-parity-final` 通過；雙捲軸及 HWND 外 pointer capture 在 `target/scrollbar-capture-evidence/20260727-parity-final` 通過。keyboard/accessibility/mouse/lifecycle 七步 headful 報告為 `target/headful-evidence/20260727-parity-final-v4/report.json`，IME 為 `target/ime-evidence/20260727-parity-final-v2/report.json`。

仍未宣稱通過：`target/dpi-evidence/20260727-parity-final/report.json` 與 `target/mixed-dpi-evidence/20260727-parity-final/monitor-topology.json` 證明本機只有一個 175% 螢幕；100/125/150/200% 僅通過 typed logical scaling。Explorer↔app 實體 OLE Drop 受此 runner 合成輸入限制，詳見 `docs/OLE_DRAG_DROP_EVIDENCE.md`。

狀態值：未開始／進行中／部分完成／完成／受阻／不適用。只有 Automated Evidence 或 Manual Evidence 指向已執行結果時才能標記完成。

Interaction fixture 補充證據：`target/interaction-evidence/{hover,pressed,focused-light,
focused-dark,disabled,selected}` 已涵蓋 deterministic hover、pressed、disabled、selected
與 focused；hover/pressed pixel diff 只落在 `+ New` control。此證據是 175% DPI，
不取代待完成的 100/125/150/200% 與 keyboard-only matrix。

Mixed-DPI equipment audit：`scripts/audit_monitor_topology.ps1` 實際只找到一台 active
MSI monitor，證據在 `target/mixed-dpi-evidence/monitor-topology.json`。依測試計畫記為
「無設備、未驗證」，不宣稱跨螢幕 DPI 已通過。

Explorer light cross-app evidence：`target/explorer-reference-evidence/{d-drive-light-175,
app-light-175,light-diff-175}`。兩邊同機 DPI 168、light/system font，physical size 只差
1×1 px（1.75 scaling rounding）；pixel changed ratio 23.281977%。chrome 區域順序已對齊，
完整 namespace navigation、details metadata/icons 與 typography 仍列為已知差異。

| Capability | Requirement | Milestone | Status | Automated Evidence | Manual Evidence | Known Difference | Windows/API Limitation |
|---|---|---|---|---|---|---|---|
| windows-app-foundation | Windows-only 可重現 workspace | M0 | 完成 | locked workspace gates；recursive GPUI-CE submodule `f9740c88e5`；成品 manifest/PE/VERSIONINFO validation；Cargo.lock hash | `docs/CHECKPOINT_EVIDENCE.md` | fresh-clone CI 由 workflow重現 | GPUI-CE manifest 缺 definition identity，成品 finalization 以相同設定補入並原位更新唯一 ID 1 |
| windows-app-foundation | 可啟動與可關閉的 GPUI application | M0 | 完成 | release resize/WM_CLOSE；10 次啟停均 exit 0、無殘留 PID、ordered cleanup；threads/handles/GDI/User/working-set snapshot | `M0-release-lifecycle-01` | close 後有無可見影響的 invalid-window-handle tracing error | GPUI-CE Windows backend 使用 `ExitProcess`；cleanup 已移至 `on_app_quit` |
| windows-app-foundation | 程序診斷 | M0 | 完成 | common tests；subprocess panic test；release controlled panic exit 101後正常再啟動 | `M0-controlled-panic-01` | 無敏感 root洩漏 | — |
| windows-app-foundation | Shell STA 生命週期 | M0 | 完成 | state/failure/timeout/idempotence tests；STA owned-resource snapshot；release process census與10-run ranges | `M0-release-lifecycle-01` | 程序 census含 GPU/GPUI resources，不作單一 STA歸因 | owned STA snapshot與process snapshot分開解讀 |
| windows-app-foundation | 明確啟停順序 | M0 | 完成 | startup failure unwind；DPI prerequisite；release ordered cleanup log | `M0-release-lifecycle-01` | close tracing error仍保留 | host／manifest 已固定 DPI 時可回傳 `ERROR_ACCESS_DENIED` |
| windows-app-foundation | 最小且單向的 crate 邊界 | M0 | 完成 | architecture script；metadata/tree；UI無Shell/Windows/I/O；test-support僅dev graph | `docs/CHECKPOINT_EVIDENCE.md` | — | — |
| windows-app-foundation | Cargo 品質閘門 | 全階段 | 完成 | 2026-07-26 architecture、fmt、workspace check、all-target/all-feature clippy `-D warnings`、workspace tests 全部 exit 0 | — | 後續每個 checkpoint 必須重新執行 | — |
| explorer-shell-ui | Explorer 視窗區域結構 | M1 | 部分完成 | 18 UI tests；stable region IDs；完整 chrome headful lifecycle smoke | 尚未人工視覺驗收 | 目前為 M1 disconnected shell | — |
| explorer-shell-ui | 多分頁 chrome | Tabs | 完成 | real multi-tab model/E2E與deterministic visual fixture | `M1-visual-states-175dpi-01` | — | Snap flyout視覺屬caption parity，另列未驗證 |
| explorer-shell-ui | 靜態命令與導航控制 | M1/Tabs | 部分完成 | typed click/7 GPUI key bindings 共用 dispatcher；disabled command tests | 尚未 keyboard-only headful 驗收 | 真實 history/file commands 接入前 disabled | — |
| explorer-shell-ui | Navigation pane 與 content split | M1 | 部分完成 | deterministic geometry／clamp tests；GPUI divider pointer terminal model；keyboard adjustment；unavailable semantics；disconnected file host | 尚未 divider resize 與多 DPI 實機驗收 | Quick Access/This PC 尚未連接；M1 pane width 不跨程序持久化 | 固定 GPUI-CE revision 與 `gpui-component` API 不相容，採小型原生 element |
| explorer-shell-ui | Status bar | M1/Tabs/Search | 部分完成 | 中性 M1 status 與 package version marker | 尚未人工視覺驗收 | 不顯示假 item count/progress | — |
| explorer-shell-ui | Semantic theme tokens | M1 | 部分完成 | UI 3 theme contract tests；同機 Explorer light sampling；CI raw-token lint | dark/high contrast 尚未實機驗收 | interaction colors 尚待 deterministic fixture 校正 | high contrast 目前為 system-role mapping，尚未解析／實機驗證 |
| explorer-shell-ui | 集中式 layout tokens 與 DPI 行為 | M1 | 部分完成 | UI 3 layout tests；root token injection；100/125/150/200% scale table | 尚未執行多 DPI capture | chrome 尚未消費全部 layout tokens | 實際 rounding tolerance 待 GPUI headful capture |
| explorer-shell-ui | Typed actions 與快捷鍵 | M1/全階段 | 部分完成 | action/binding/dispatch tests；same-scope conflict check；7 組 GPUI key event wiring；privacy-safe trace | 尚未 headful 鍵盤驗收 | Back/Forward/Up 在真實 history 前 disabled | — |
| explorer-shell-ui | Focus 與互動狀態 | M1/全階段 | 部分完成 | focus traversal/restore tests；shared interaction state test | 尚未 keyboard-only headful traversal | address/search 目前只有 focus/restore，無虛構 submit | GPUI focus handles 尚待 input components |
| explorer-shell-ui | AccessKit roles 與 actions | M1/全階段 | 部分完成 | stable IDs；button/tab/document/status/splitter roles；caption invoke；splitter increment/decrement；numeric range | 尚未 Narrator/Accessibility Insights 實機驗收 | disabled controls 不可 invoke且 name 標示 unavailable，但樹中無原生 disabled state | 此 GPUI-CE revision 的 `StatefulInteractiveElement` 未公開 AccessKit disabled setter |
| explorer-shell-ui | UI 與 Windows Shell 隔離 | 全階段 | 完成 | architecture gate與cargo tree | — | UI只收owned protocol/model types | — |
| tabbed-folder-navigation | 多分頁生命週期 | Tabs | 完成 | model tests；real two-tab E2E；deterministic multi-tab fixture | `docs/REAL_FOLDER_EVIDENCE.md` | — | — |
| tabbed-folder-navigation | Per-tab 導覽 history | Navigation | 完成 | transactional Back/Forward/Up E2E與failure preservation | `docs/REAL_FOLDER_EVIDENCE.md` | — | — |
| tabbed-folder-navigation | 真實 location 解析與增量列舉 | Navigation | 完成 | real Shell folder metadata/bounded batches/terminal tests | `docs/REAL_FOLDER_EVIDENCE.md` | — | — |
| tabbed-folder-navigation | Request generation 與取消 | Navigation | 完成 | A/B/C stale matrix；generation/cancel/terminal tests | `docs/REAL_FOLDER_EVIDENCE.md` | — | — |
| tabbed-folder-navigation | Stable item identity 與 selection | Navigation | 完成 | PIDL/filesystem identity、refresh/rename selection tests | `docs/REAL_FOLDER_EVIDENCE.md` | — | — |
| tabbed-folder-navigation | 真實檔案開啟與資料夾進入 | Navigation | 完成 | folder current/new-tab command與ShellExecute file command tests | `docs/REAL_FOLDER_EVIDENCE.md` | default app UI本身由Windows管理 | ShellExecute failure保留native error |
| tabbed-folder-navigation | Watcher merge 與 overflow recovery | Navigation | 完成 | ReadDirectoryChangesW rename storm、dedupe、overflow refresh E2E | `docs/REAL_FOLDER_EVIDENCE.md` | — | — |
| tabbed-folder-navigation | 真實資料夾整合測試矩陣 | Navigation | 完成 | Unicode/long/hidden/system/case/reparse/100k owned fixtures | `docs/REAL_FOLDER_EVIDENCE.md` | 真實受保護使用者資料不作destructive測試 | — |
| native-file-operations | Typed 原生檔案操作 | Operations | 完成 | production `IFileOperation` adapter與typed model E2E | `docs/FILE_OPERATIONS_EVIDENCE.md` | — | — |
| native-file-operations | Copy 與 move | Operations | 完成 | real disk oracle含cross-volume/reparse capability | 同上 | cross-volume依可用磁碟truthful分類 | — |
| native-file-operations | 回收刪除與永久刪除 | Operations | 完成 | explicit confirmation、安全fixture、terminal tests | 同上 | Recycle Bin UI未人工比對 | — |
| native-file-operations | Progress、取消與 terminal semantics | Operations | 完成 | large copy cancel、exactly-one terminal、late progress rejection | 同上 | — | — |
| native-file-operations | 衝突決策 | Operations | 完成 | Prompt/Skip/Replace/KeepBoth preflight/model tests | 同上 | — | — |
| native-file-operations | 安全 operation journal 與 undo/redo | Operations | 完成 | finished-only journal與preimage revalidation | 同上 | 不承諾任意Shell operation可undo | — |
| native-file-operations | Destructive integration test 安全性 | Operations | 完成 | owned root guard拒絕drive/workspace/profile/unresolved/reparse escape | 同上 | — | — |
| shell-data-transfer-and-menus | Explorer 相容 Clipboard copy/cut/paste | Clipboard | 部分完成 | real OLE IDataObject；實際 Explorer single/multi copy/cut→app paste disk matrix | `docs/CLIPBOARD_INTEROP_EVIDENCE.md` | app→Explorer完整人工矩陣未執行 | clipboard busy有bounded retry與recoverable error |
| shell-data-transfer-and-menus | Cut 狀態與完成語意 | Clipboard | 完成 | preferred drop effect、sequence ownership、move completion/clear tests | 同上 | — | — |
| shell-data-transfer-and-menus | OLE drag source | OLE | 部分完成 | real DoDragDrop cancel/resource soak、effect negotiation | `docs/DRAG_DROP_EVIDENCE.md` | 實際app→Explorer滑鼠手勢未執行 | OLE modal loop以message wake完成bounded cancel |
| shell-data-transfer-and-menus | OLE drop target | OLE | 部分完成 | GPUI-CE native external drop negotiation與typed file-operation routing | 同上 | 實際Explorer→app手勢未執行 | — |
| shell-data-transfer-and-menus | Right-drag 與 auto-scroll | OLE | 部分完成 | right-drag terminal/effect/changed-generation與auto-scroll model tests；visual drag cue | 同上 | 實際right-drag menu手勢未執行 | — |
| shell-data-transfer-and-menus | Shell context menu sessions | Context menu | 完成 | 真實 background/single/multi QueryContextMenu；10 次 TrackPopupMenuEx cancel/message-forward/resource soak；installed 7-Zip 階層 submenu/keyboard query/安全 archive invoke | 7-Zip CLSID、command tree 與 owned output；controlled owner-draw/reentrant fixture | 7-Zip 本身非 owner-draw；永久 hang 回收仍需 process broker | `docs/CONTEXT_MENU_EVIDENCE.md` |
| shell-data-transfer-and-menus | Extension 故障邊界 | Context menu | 部分完成 | independent OLE worker、correlation/deadline、controlled slow/hung/error handler、late terminal suppression、RAII cleanup | installed第三方handler未執行 | in-process worker永久hang無法安全強制回收thread | 完整可終止隔離需要process broker；見 `docs/CONTEXT_MENU_EVIDENCE.md` |
| shell-data-transfer-and-menus | Explorer 互通測試矩陣 | Interop | 未開始 | — | — | — | — |
| file-search | 可驗證的搜尋語法 AST | Search | 完成 | Unicode/phrase/escape/filter/boolean precedence/validation tests | `docs/SEARCH_EVIDENCE.md` | — | — |
| file-search | 地址列與搜尋列分離 | Search | 完成 | parser boundary與UI focus/action tests | 同上 | — | — |
| file-search | Per-tab 可取消搜尋 session | Search | 完成 | two-tab replacement/navigation cancel E2E | 同上 | — | — |
| file-search | Windows Search backend | Search | 完成 | real index availability probe、AQS escaping/binding | 同上 | temporary root通常未被index，truthful fallback | Windows Index scope由OS政策決定 |
| file-search | 有界 fallback search | Search | 完成 | bounded queue、batch、cancel與partial terminal tests | 同上 | — | — |
| file-search | 增量結果、dedupe 與 identity | Search | 完成 | stable identity dedupe、incremental batches、stale generation rejection | 同上 | — | — |
| file-search | 搜尋錯誤與取消不是假空結果 | Search | 完成 | Finished/Partial/Failed/Cancelled terminal與UI fixture | 同上 | — | — |
| file-search | 真實搜尋測試 | Search | 完成 | real folder Unicode/properties/replacement與4100-directory partial E2E；100k explicit test | 同上 | — | — |
| parity-verification | 正式 parity matrix | 全階段 | 進行中 | 本文件已建立；尚無程式 evidence | — | 所有產品能力仍未開始 | — |
| parity-verification | 實作與狀態文件 | 全階段 | 完成 | implementation/status/capability/checkpoint evidence文件 | — | 每次checkpoint需同步更新 | — |
| parity-verification | 受控視覺基準 | M1/全階段 | 部分完成 | deterministic七狀態、metadata/hash、DPI拒絕規則、light/dark 175% captures | `M1-visual-states-175dpi-01` | 正式100/125/150/200%與Explorer baseline未完成 | mismatch artifact禁止升級baseline |
| parity-verification | 可診斷的 visual regression | M1/全階段 | 部分完成 | capture/compare/update分離；DwmFlush；semantic/layout metadata | 同上 | 尚無同DPI核准baseline可執行正式diff | — |
| parity-verification | Windows 手動驗收矩陣 | 全階段 | 進行中 | reproducible commands與actual/expected分欄 | M0 lifecycle/panic與175% state cases完成 | DPI/high contrast/accessibility/IME/drag/third-party仍未執行 | — |
| parity-verification | Milestone exit evidence | 全階段 | 進行中 | M0 exit matrix完整；M1自動gate與175% evidence完整 | `docs/CHECKPOINT_EVIDENCE.md` | M1因指定DPI/high contrast/accessibility/Snap缺證據維持部分完成 | — |
| parity-verification | 效能與資源基線 | 全階段 | 部分完成 | release cold/warm 10-run median/p95與process resource ranges；resize/theme/focus各20k production callback percentile，全部0個超過4ms；headful next-frame/DwmFlush present diagnostics | `docs/CHECKPOINT_EVIDENCE.md` | 尚缺同process全能力長時間soak | 目前量測有互動式background load |
