## Context

工作區目前只有已通過互動式審查的完整產品設計，沒有 Cargo workspace 或 production code。本 change 由 M0/M1 建立基線，接著交付多分頁真實資料夾瀏覽、原生檔案操作、Clipboard／OLE drag-and-drop／context menu 與搜尋。目標環境是 Windows 11 25H2 x64；UI 採固定 revision 的 GPUI-CE 與專案內 semantic helpers，Windows 整合採 `windows` crate 與公開 Win32、COM、OLE、Property System、Search 和 Shell API。

主要限制如下：

- 每次提交後必須保持可編譯、可啟動、可獨立驗收。
- 不得建立沒有 production consumer 或 contract test 的 trait、command、provider 或 Shell API stub。
- Win32／COM 型別不得滲透到 UI；Shell/OLE apartment-affine 工作由 `explorer-shell-win` 隔離並以 owned domain values 交換。
- 依賴 revision 與 lockfile 必須固定，確保 GPUI 快速變動時仍可重現建置。
- 本機 Windows 11 Explorer 是視覺與互動基準；網路截圖不構成驗收依據。

## Goals / Non-Goals

**Goals:**

- 建立 Windows-only Rust workspace 與可執行 GPUI application。
- 驗證固定版本的 GPUI-CE 可在目標 Windows build 建置與啟動。
- 建立 logging、panic hook、DPI、啟停順序與 Shell STA 的真實生命週期。
- 交付可 resize 的單一視窗及完整 M1 靜態 Explorer chrome。
- 建立集中式 theme/layout tokens、typed actions、key bindings 與 focus routing。
- 建立可重複的 Cargo gates、behavior tests、visual baseline、manual tests 與 parity evidence。
- 交付真實可用的多分頁、資料夾導覽、檔案操作、Shell 互通與搜尋垂直切片，並維持 typed command/event 與可取消的非同步邊界。
- 使用真實暫存資料夾與 Explorer 雙向互通案例驗證，不能只依賴 fake service。

**Non-Goals:**

- 不在本 change 完成 thumbnails/icon views、Home/Gallery/ZIP/Libraries/第三方 namespace 的完整 parity。
- 不實作 Preview Handler 或第三方 extension 的完整跨 process broker；context menu 必須有 timeout/故障邊界，完整 process isolation 另行收斂。
- 不完成 session/crash restore、完整 accessibility closure、所有資料夾範本或自訂 metadata columns。
- 不宣稱逐像素複製 Explorer；字型 rasterization 差異使用明確 tolerance 處理。
- 不建立 Linux/macOS fallback，也不承諾其他 Windows 版本的 parity。

## Decisions

### 1. 以最小垂直切片建立 workspace

根 workspace 納入 `explorer-app`、`explorer-ui`、`explorer-common`、`explorer-model`、`explorer-shell-win`、`explorer-jobs`、`explorer-search` 與 `explorer-test-support`。每個 crate 必須被 production composition 或 contract/integration tests 使用；Shell、OLE 與 Search 實作仍不得反向滲透 UI。

選擇此方式是為了讓每個 crate 都有可執行責任，避免假介面在實際 Shell 行為出現前固化。替代方案是一次建立八個 crate 與完整 command/event schema；它能提早展示目錄結構，但會製造未驗證 API，違反既有設計的 YAGNI 邊界。

### 2. 固定 Git revisions 並提交 lockfile

GPUI 使用 `gpui-ce/gpui-ce` submodule，固定為 `6c799b8e994266233014cea66d7769675ec1967c`。根 manifest 統一宣告 workspace dependencies，production crates 不各自選版本。實測目前 `gpui-component bc174a7...` 與此 revision 有 11 個 API 編譯衝突，因此不納入 dependency graph；M1 以 GPUI-CE 原生 element 組合專案內的 button、tooltip、menu、input、scroll 與 divider helpers，維持 typed action、semantic token 與 accessibility contract。

Windows platform 直接使用同一 submodule 的 `gpui_windows`。GPUI-CE 內建 manifest 作為最終 exe 唯一的 manifest resource，app resource 只提供 VERSIONINFO，避免重複 ID 1；若上游 manifest 無法通過嚴格 Windows Manifest Tool 驗證，M0 必須保留 limitation 並在 5.4 修正，不能以 headful 可啟動取代 manifest closure。

選擇 GPUI-CE 是為了使用可獨立消費的社群 fork；仍固定 revision，因為 API 快速變動，可重現性比自動取得最新版重要。替代方案是 crates.io 版本、浮動 Git dependency或維護本地 `gpui-component` fork；前兩者不能精確重現 source checkout，後者會增加未經上游驗證的長期維護面，因此不採用。

### 3. Windows-only 編譯邊界明確化

binary 與 Windows integration crate 使用 compile-time target guard；CI 至少有 Windows job 執行完整 gates。錯誤平台必須快速失敗並顯示可理解訊息，而不是產生部分可編譯但不可執行的假支援。

替代方案是用大量 `cfg` 建立跨平台殼層；目前沒有跨平台產品目標，會增加測試矩陣並模糊 Windows API ownership，因此不採用。

### 4. Composition root 掌握啟停順序

`explorer-app` 依序初始化 diagnostics、DPI/Windows prerequisites、Shell STA、GPUI application 與 window；關閉時停止接受工作、要求 STA 結束、等待 join，最後 flush diagnostics。Shell STA 會執行 `COINIT_APARTMENTTHREADED`、維持 message pump，並以 RAII 保證 `CoUninitialize` 發生於同一執行緒。

M0/M1 先驗證 STA 生命週期；後續由同一 STA command endpoint 承接 location resolve、child enumeration、`IFileOperation`、OLE 與 context menu session。每個 command 帶 request/correlation ID、deadline 或 cancellation，所有 terminal path 都回報 event。替代方案是每個功能自行初始化 COM；這會破壞 apartment ownership、取消與 shutdown 一致性，因此不採用。

### 5. UI 依區域組合，狀態與繪製分離

`ExplorerWindow` 只組合 `WindowChrome`、`CommandBar`、`NavigationBar`、`ContentSplit` 與 `StatusBar`。M1 使用最小 `AppViewState` 保存 theme、pane width、focused surface 與靜態控制狀態；feature component 不呼叫 Win32 Shell 或同步 I/O。

替代方案是把整個靜態畫面寫在單一 render function；初期檔案較少，但會讓 focus、resize、visual fixtures 與未來替換區域變得難以獨立測試。

### 6. Theme 與 layout 全部由 semantic tokens 驅動

theme token 至少涵蓋 surface、control、hover、pressed、selected、divider、text、focus、danger 與 accent；layout token涵蓋 title/tab、command/address/status 高度、pane 寬度、padding、radius 與 focus stroke。feature component 不散落固定 RGB 或重複尺寸。

light/dark 在 M1 可切換；high contrast 先建立 semantic 映射與結構驗證，完整實機 accessibility closure 留到 M9。替代方案是先硬編碼 Explorer 色值再重構；這會讓視覺調整無法集中 review，也會提高 high contrast 改寫成本。

### 7. Typed actions 統一鍵盤與控制操作

Back、Forward、Up、new/close/switch tab、focus/submit address、focus/submit search、copy/cut/paste、delete、rename 與 theme toggle 等輸入先轉成 typed action，再由目前 focused surface 或 window coordinator 處理。M1 checkpoint 尚無真實 history 時 actions 可 disabled；導覽垂直切片完成後，availability 必須由 active tab model 和 selection capability 驅動。

替代方案是在每個元件直接比對 raw key；短期較直接，但會造成快捷鍵衝突、focus 優先序與 accessibility action 無法集中驗證。

### 8. 視覺驗收使用受控 baseline 與結構診斷

baseline key 包含 OS build、Explorer version、app commit、DPI、theme、window size 與 font configuration。測試同時保存 baseline、actual、diff 與 token/layout diagnostics；文字 antialiasing 使用 tolerance，layout bounds 與 semantic colors 使用較嚴格門檻。baseline 更新必須是人工 review 動作。

替代方案是只做人工目視或逐像素 gate。前者無法追蹤回歸，後者會被字型與 GPU 差異造成大量假陽性。

### 9. 文件與 parity matrix 是交付物

`docs/PARITY_MATRIX.md` 對每項納入範圍的 capability 記錄 milestone、狀態、自動/手動驗收、已知差異與 API 限制；`STATUS`、`MANUAL_TESTS` 與 `IMPLEMENTATION_PLAN` 必須與程式及測試同時更新。真實資料夾、破壞性操作與 Explorer 互通未實際執行時必須標記未驗證，不得以 fake 或預期結果代替。

替代方案是完成程式後補文件；這會讓 exit criteria 無法在每個 task 結束時判斷，因此不採用。

### 10. Per-tab model、stable identity 與導覽一致性

`explorer-model` 以 `TabId` 分隔 location、back/forward history、directory snapshot、selection、search mode、request generation 與 cancellation。每次 Navigate/Refresh/Search 建立 `RequestContext { request_id, tab_id, generation }`；UI 合併任何非同步 event 前都驗證三者仍有效。關閉分頁立即取消其所有 request，切換 active tab 不取消背景分頁，但會降低其 metadata/search 優先序。

檔案與資料夾使用 stable `ShellItemId`；本機路徑只是 descriptor，不能作為 selection/watch/operation 的唯一 identity。替代方案是讓每個 view 自行保存 path 與 history；多分頁同時更新時容易混入舊結果，且後續無 path Shell namespace 無法延伸，因此不採用。

### 11. 真實資料夾列舉、watcher 與測試 seam

Shell STA 解析 location，增量發送 bounded directory batches；filesystem watcher 使用 `ReadDirectoryChangesW` overlapped I/O，Shell identity/notification 必要時由 `SHChangeNotifyRegister` 補足。新導覽取消舊 generation，watcher event 先 coalesce 再轉 stable-ID diff；overflow 或 rename 無法配對時重新列舉並 diff，不直接清空 selection。

`explorer-test-support` 同時提供 deterministic fake service 與 temporary-folder fixture。真實 integration tests 使用測試建立且唯一擁有的暫存根目錄，涵蓋 Unicode、emoji、長名稱、hidden/system、reparse point、permission denied、快速新增刪除、rename storm、watcher overflow 與 100,000 項目。破壞性測試只能操作已驗證位於 fixture root 內的路徑。替代方案是只用 fake enumeration；它無法驗證 UTF-16 parsing、handle ownership、Windows notification 與實際權限錯誤。

### 12. 原生檔案操作與 journal

所有 create/rename/copy/move/recycle/permanent-delete 透過 typed `FileOperationRequest` 送到 Shell STA，由 `IFileOperation` 與 progress sink 發出 queued/running/progress/conflict/finished/cancelled/partial-failure terminal semantics。UI operation center 只消費 domain events，不能假設單一 item 成功代表整批成功。

operation journal 只記錄已完成且有安全 inverse 的動作；每筆保存 affected stable identity、原/新 parent/name、完成時間與 capability。外部變更讓 inverse 不安全時停用 undo 並說明原因。替代方案是直接使用 `std::fs`；它無法提供 Explorer 的 recycle、Shell namespace、衝突 UI 與 progress semantics，因此不作主要路徑。

### 13. Clipboard、OLE drag-and-drop 與 context menu

Clipboard 與 drag/drop 使用 Windows Shell formats、`IDataObject`、`IDropSource`、`IDropTarget` 和 `DoDragDrop`，由 OLE-ready STA 與 GPUI/原生 HWND 協調。drop effect 由 allowed effects、modifier、source preferred effect 與 target capability共同決定；cut 標記只有在成功 move/paste terminal event 後清除。拖曳超過 system threshold 才開始，支援 auto-scroll 與 right-drag menu。

Context menu 使用 `IContextMenu`/2/3，background、single-selection、multi-selection 分別建立 session，轉發 `WM_INITMENUPOPUP`、`WM_DRAWITEM`、`WM_MEASUREITEM`、`WM_MENUCHAR` 等必要訊息。第三方 extension 不得在 GPUI callback 中同步執行；本 change 至少提供 deadline、session cancellation、故障記錄與可恢復 UI，完整跨 process broker 留待後續隔離 change。替代方案是重製自有 menu commands；會失去 Explorer/第三方 extension 相容性，僅能作 timeout/error fallback。

### 14. 搜尋 AST 與 per-tab session

`explorer-search` 將純文字、quoted phrase、property filter、comparison、date/size shorthand 與 boolean operators 解析成 AST，提供位置化錯誤。Windows Search backend 只接受經 escape/bind 的 AST；無索引或不支援 location 時使用 bounded filesystem fallback，UI 必須顯示來源與能力差異。

每個 search session 帶 tab/generation、可取消、增量 results、stable-ID dedupe 與 terminal source status。地址列和搜尋列使用不同 parser；切換 query 或離開搜尋時取消舊 session，任何舊 event 都被 model 拒絕。替代方案是直接拼接 Windows Search query string；會造成 escaping/語意錯誤與不可測的 injection 風險，因此不採用。

### 15. 分階段可執行 checkpoint

實作仍依 M0 → M1 → multi-tab/local navigation → file operations → Shell interoperability → search 的依賴順序進行。每個 checkpoint 都必須通過四個 Cargo gates、對應 contract/integration tests、Windows manual matrix 與 parity 更新，才進入下一階段。這讓擴大範圍不會變成一次性大爆炸，也保留可回退的可執行狀態。

## Risks / Trade-offs

- [固定的 Git revision 無法在目前 Rust toolchain 建置] → 先做 dependency spike，記錄確切 toolchain；若必須調整，只能在 proposal/design 中更新 revision 並重新跑全部 gates。
- [GPUI custom chrome 無法完整支援 Snap、caption hit-test 或原生 HWND 行為] → M0/M1 建立 HWND capability smoke test，將缺口寫入 parity matrix；不以視覺遮蓋功能缺口。
- [跨八個 crate 的整合面過大] → 依 capability checkpoint 建立最小公開 contract，禁止循環/反向依賴，且每個 crate 都需有 production consumer 或 contract test。
- [靜態 placeholder 被誤認為可用功能] → disabled state、status text、manual docs 與 parity 狀態明確標示 M1 邊界。
- [視覺基準受 DPI、字型或 Explorer build 漂移] → baseline metadata 完整記錄環境，OS/Explorer 升級後執行明確 re-audit。
- [Shell STA 上的列舉、operation、OLE 或 menu call 阻礙 shutdown] → 所有 request 帶 cancellation/deadline，加入 bounded shutdown telemetry、fault injection、session cleanup 與 join 診斷。
- [high contrast 結構存在但未完整可用] → M1 只宣稱 token/focus foundation，完整 Narrator／IME／high contrast 驗收仍標記 M9，避免過度聲稱 parity。
- [多分頁的 stale event 污染 active tab] → 所有 event 驗證 tab/request/generation，並以關閉、切換、快速重導覽的 deterministic tests 覆蓋。
- [真實資料夾測試誤傷使用者資料] → 每次建立唯一 temporary fixture root，破壞前 canonicalize/identity 檢查目標仍在 root，測試不接受 workspace/home/root 作為 destructive target。
- [`IFileOperation` partial failure 或 undo 不安全] → 每個 item 保留 outcome，journal 只收錄已完成且 inverse 可重新驗證的 operation。
- [OLE drag/drop reentrancy 卡住 UI] → apartment/message-pump 合約、system threshold、session state machine、bounded diagnostics 與 Explorer integration fixtures。
- [第三方 context menu extension 掛起] → 不在 GPUI callback 同步 activate/invoke，加入 deadline、session recovery 與 handler-level parity limitation；完整 broker 為後續硬化項目。
- [Windows Search 索引不完整或不可用] → 顯示 source status，提供有界 fallback 並在 parity matrix 記錄語法/範圍差異。

## Migration Plan

1. 建立根 Cargo workspace、toolchain/target policy 與 Windows CI；先驗證固定依賴可解析。
2. 建立 `explorer-common` diagnostics/lifecycle primitives 與 `explorer-shell-win` STA lifecycle，加入啟停測試。
3. 建立 `explorer-app` composition root 與最小可 resize window，完成 M0 gates 與文件證據。
4. 建立 UI tokens、component regions、typed actions/focus routing 與 light/dark theme，完成 M1 靜態 checkpoint。
5. 建立 common command/event、model、test-support 與 Shell enumeration/watcher；完成多分頁真實資料夾導覽、selection 與 real-folder tests。
6. 在真實導覽/selection 基礎上加入 `IFileOperation`、progress/conflict/cancel/partial failure 與安全 operation journal；只在受控 fixture 先驗證 destructive cases。
7. 加入 Clipboard formats、Explorer 雙向 copy/cut/paste、OLE drag source/target、right-drag、auto-scroll 與 `IContextMenu3` sessions。
8. 加入 search AST、Windows Search backend、fallback、per-tab incremental/cancellation 與 stale-result tests。
9. 執行完整 Cargo gates、真實資料夾矩陣、Explorer interoperability、visual/performance/resource tests，更新 parity 與 handoff。
10. 若任何步驟無法穩定建置，可逐提交回退到上一個可啟動 checkpoint；破壞性操作只在明確受控 fixture 執行，不包含使用者資料 migration。

## Open Questions

目前沒有阻擋實作的產品問題。實作期間需以 capability spike 量測並記錄下列事實，而不是預先猜測：目標 GPUI revision 的 Windows chrome/HWND/OLE hooks、Explorer 支援的 Clipboard formats、第三方 context menu message requirements、Windows Search availability，以及 CI runner 能否執行有視窗與真實 Shell 的測試。這些結果可調整 adapter 與 parity 記錄，但不得移除使用者本次指定的多分頁、真實資料夾、檔案操作、Clipboard、OLE drag-and-drop、context menu 或 search 能力。
