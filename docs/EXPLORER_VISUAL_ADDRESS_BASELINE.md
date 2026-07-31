# Explorer 視覺與網址列實作前基準

日期：2026-07-27  
Change：`match-explorer-visual-address-parity`  
用途：保存變更前可重現版本、已核准參考環境、程式盤點、缺口與 evidence 分類；本文件不是完成證明。

## 1. 追蹤關係

- 核准產品設計：[`docs/superpowers/specs/2026-07-27-explorer-visual-address-parity-design.md`](superpowers/specs/2026-07-27-explorer-visual-address-parity-design.md)
- OpenSpec proposal：[`proposal.md`](../openspec/changes/match-explorer-visual-address-parity/proposal.md)
- OpenSpec technical design：[`design.md`](../openspec/changes/match-explorer-visual-address-parity/design.md)
- 視覺能力規格：[`explorer-visual-parity/spec.md`](../openspec/changes/match-explorer-visual-address-parity/specs/explorer-visual-parity/spec.md)
- 網址列能力規格：[`interactive-breadcrumb-address/spec.md`](../openspec/changes/match-explorer-visual-address-parity/specs/interactive-breadcrumb-address/spec.md)
- 詳細任務：[`tasks.md`](../openspec/changes/match-explorer-visual-address-parity/tasks.md)

所有 artifacts 的主要參考環境一致：Windows build 26200、Explorer `10.0.26100.8875`、繁體中文、淺色、175% DPI、`D:\` 根目錄、Details view。favorites／釘選內容可以不同，其區域幾何不可忽略。

## 2. Before snapshot

| 項目 | 實際值 | 備註 |
|---|---|---|
| App HEAD | `5e6466822cf32430093f2c17bf3455354d52519b` | 建立 OpenSpec change 後、production 修改前 |
| GPUI-CE gitlink | `f9740c88e5f799cef36c14662e3bccff9e0ca363` | `vendor/gpui-ce`；來源 `gpui-ce/gpui-ce` |
| Cargo.lock SHA-256 | `AA41B034FD6AFB97A31927C4F9149FBFDB28FD9BA6C802AE7951BCD4298961EE` | 變更前 lockfile |
| OS | Windows x64 build `26200` | `Get-ComputerInfo` 的產品名稱仍回報 Windows 10 Pro，build 與 reference metadata 確認為 Windows 11 專業版 |
| Explorer | `10.0.26100.8875 (WinBuild.160101.0800)` | 與 reference metadata 一致 |
| Explorer reference | `2685 × 1621` physical px、DPI 168（175%）、light | `file:///D:/`，顯示名稱「新增磁碟區 (D:)」 |
| App reference | `1520 × 919` logical px、`2684 × 1620` captured px、scale 1.75 | 舊 artifact commit `bd26116...` 且 dirty，僅作 before evidence |
| 字型 metadata | Windows system UI / Microsoft JhengHei UI | app fixture 由 `EXPLORER_VISUAL_FONT` 固定 |
| 工作樹邊界 | `codex_gpui_win11_explorer_prompt.md` 未追蹤 | 使用者檔案，不納入 change |

## 3. Explorer UI 現況盤點

### 3.1 Render tree 與 stable IDs

`crates/explorer-ui/src/chrome.rs` 已有下列主要 IDs：

- Window/title：`explorer-window`、`window-chrome`、`window-drag-region`、`tab-strip`、`active-tab`、`new-tab-button`、三個 caption control。
- Chrome rows：`command-bar`、`navigation-bar`、`breadcrumb-address-editor`、`search-box`。
- Body/status：`navigation-pane`、`navigation-divider`、`file-view-host`、`operation-center`、`status-bar`。

現況 render 順序是 title/tab、command、navigation、body、status；Windows 參考是 title/tab、navigation、command、body、status，因此後續必須重排。focus coordinator 已包含 WindowChrome、TabStrip、CommandBar、AddressBar、Search、NavigationPane、FileView、StatusBar；region diagnostics 尚未保存實際 layout 後矩形。

### 3.2 目前 logical geometry

`LayoutTokens::WINDOWS_11` 目前只有：title/tab 52、command 48、address 48、status 24、navigation pane min/default/max 180/240/360、spacing 8、horizontal/vertical padding 12/8、radius 8、focus 2、divider 4、minimum hit target 32、maximum glyph 20 logical px。缺少 navigation/command 個別按鈕、search/address flex、details columns/header/row、menu、caption 與 typography tokens。

### 3.3 Icon 暫代清單

| 使用點 | 現況 | 目標來源 |
|---|---|---|
| Back / Forward / Up | Unicode `U+2190/U+2192/U+2191` | 集中式 Fluent navigation icon |
| More | Unicode `U+2026` | Fluent MoreHorizontal |
| Minimize / Maximize / Close | `U+2014/U+25A1/U+00D7` | 集中式 caption vector/glyph，保持 Win32 hit test |
| Tab close / New tab | 文字 `×` / `+` | 集中式 Close/Add icon |
| New command | 文字 `+ New` | Add icon 加本地化名稱 |

command bar 的 Cut/Copy/Paste/Delete 目前以文字名稱為主，尚未具備 Explorer icon contract。production 完成條件禁止以任意 Unicode glyph 作最終 chrome icon。

### 3.4 Theme 與 typography

現有 theme 有 14 個 semantic slots：surface、subtle surface、control fill/hover/pressed、selected active/inactive、divider、primary/secondary/disabled text、focus、danger、accent。light/dark 為固定測量值；high contrast 已有 Windows system-color mapping。

缺口：沒有 toolbar/address/search/menu/caption 專屬 semantic slots；沒有 per-surface typography tokens。production UI 未設定 tab/command/address/search/navigation/header/row/status 的 family、size、weight與line height，只有 visual/headful scripts以 `Microsoft JhengHei UI` 記錄 fixture font。

## 4. 網址列與 typed contract 現況

- `BreadcrumbAddressEditor` 名稱尚未反映行為：目前只渲染固定 272 logical px 的 `EditableTextState`，沒有 segment、chevron、overflow 或 browsing/editing 狀態機。
- `Ctrl+L` 已綁 `FocusAddressBar`；`Alt+D` 尚未註冊。Enter/Esc 共用 `SubmitFocusedInput`／`CancelFocusedInput`，模式與錯誤語意未獨立。
- address/search 都支援 GPUI editable-text與 IME focus，但 active/background tab 的 draft 尚未模型化。
- `ExplorerAction` 尚缺 Enter/Update/Submit/Cancel address、segment、chevron menu actions。
- `LocationDescriptor`、`RequestContext`、Navigate/Refresh/Cancel、LocationResolved/DirectoryBatch/terminal events 已可重用。
- typed protocol 尚缺 Shell ancestry、container capability、child-container request/batches/menu terminal 與 owned icon request/payload。
- `explorer-shell-win::navigation` 已能將 descriptor 解析為 `IShellItem`，可作 ancestry、child enumeration 與 icon pipeline 的實作入口；PIDL/COM interface 不得直接跨執行緒。

## 5. Reference evidence 分類

| Artifact | 分類 | 理由／下一步 |
|---|---|---|
| `target/explorer-reference-evidence/d-drive-light-175/` | 可重現 Explorer reference | OS build、Explorer version、location、DPI、theme、font、window bounds與screenshot hash完整，且目前Explorer version一致 |
| `target/explorer-reference-evidence/app-light-175/` | before evidence only | 條件完整，但app commit是舊dirty revision；production變更後必須重擷取 |
| `target/explorer-reference-evidence/light-diff-175/` | cross-app before evidence | 全圖changed ratio 23.28%，report已明示不能作pass/fail；後續改用具名region comparator |
| app-only interaction/theme/high-contrast evidence | regression evidence | 可驗證狀態與資源，但不能代替同條件Explorer geometry gate |

baseline scripts 預設不得覆寫上述 reference。若Windows或Explorer build改變，建立新profile並保留舊baseline。

## 6. 變更前品質 gate

執行日期：2026-07-27。production 修改前所有必要 gate 通過：

| Gate | 命令 | 實際結果 |
|---|---|---|
| Rust format | `cargo fmt --all --check` | 通過 |
| Workspace check | `cargo check --workspace --all-targets --locked` | 通過 |
| Clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | 通過，0 warning |
| Workspace tests | `cargo test --workspace --all-targets --locked` | 通過；明確標示的 interactive/100k/machine benchmark tests 保持 ignored |
| Headful lifecycle | `.\scripts\smoke_windows_lifecycle.ps1 -Profile debug -SkipBuild -TimeoutSeconds 20` | 通過；resize `1984 × 1272 → 2104 × 1352`、WM_CLOSE exit 0、cleanup ordered |

Headful evidence：`target/smoke-evidence/20260727T011137472Z-e4f4136c214f4fb0a68016cc837e710b/`。關窗過程的 GPUI/Windows log 出現預期 terminal window-handle errors，但 harness 驗證 exit code 0 與 cleanup ordered；此現象保留為 before evidence，若後續改變則另行分類。

## 7. 具名區域診斷 before report

第 2 階段加入 schema version 2 的 GPUI post-layout diagnostics；headful capture 在 175% DPI 實際取得 28 個唯一區域，logical bounds 乘一次 `1.75` 產生 physical bounds，validator 同時拒絕重複 id、非有限值、負尺寸與重複縮放。擷取結果位於 `target/explorer-visual-address-evidence/region-recorder-v2/`。

參考區域定義為 `docs/visual/explorer-d-drive-light-175-regions.json`。新版 comparator 使用各自 window coordinate space 正規化 left/top/right/bottom、center、width/height 與明示的 sibling gap；小於 10 px 的參考值改採 1 physical px 容差。favorites 與 ClearType mask 只遮蔽動態內容或字緣，不得遮蔽 layout bounds。報告另輸出 reference/application overlay、raw/masked diff、最差誤差排序、icon/typography metadata 覆蓋率及固定座標色彩樣本。

目前 before report 位於 `target/explorer-visual-address-evidence/named-before-report/`，結果為 13 個共同區域中僅 `explorer-window` 全欄通過；12 個區域超過 10%，三個零間距超過 1 px，masked changed-pixel ratio 為 `0.1214818955`。主要落差是 address/search flex 寬度、navigation pane 寬度、caption/chrome 高度與 status 高度，後續 layout token 校準以這份具名報告排序處理。

所有 compare scripts 對 baseline 都是唯讀；`scripts/update_visual_baseline.ps1` 必須明示 `-Approve`，缺少旗標時會在建立或複製輸出前終止。已以不存在的 probe 目錄驗證拒絕路徑不會產生任何檔案。

## 8. Layout 與 chrome icon 校準

`LayoutTokens` 已改為帶 reference profile 的區域化 logical-px contract。175% 同尺寸 capture 先將共同區域從 12 個失敗降至 0 個；加入 details header 與第一列後，15/15 具名區域仍通過 10%／1px gate。Refresh 採真正 action 與 F5 binding，網址列用 flex 取得剩餘空間，search、caption、pane、divider、row、status 與 details columns 都由 root token 提供。

Unicode／文字 chrome placeholder 已換成集中式 GPUI vector renderer，涵蓋 navigation、command、tab、caption 與 status view controls。`target/explorer-visual-address-evidence/vector-icons-v6/` 實際記錄 37 個 icon bounds，application icon metadata coverage 為 100%；同時移除不符合參考狀態的全 file-view 藍框與預選列後，masked changed-pixel ratio 從 `0.1214818955` 降到 `0.0539599087`。唯一 gap 差異是舊 reference profile 對已移除 full-panel focus border 的預期，已依相同無焦點狀態修正為 0 px，未使用 layout mask。

## 9. Per-tab 網址列與 breadcrumb

`AddressBarState` 現在由每個 `TabState` 獨立持有 Browsing、Editing、EnumeratingMenu、NavigationError、draft、resolved ancestry、menu generation 與 recoverable error。filesystem ancestry 以 `LocationDescriptor` 的 hash identity 建立 stable segment id，不依 display text 或 row index；成功 resolve 才更新 committed ancestry，失敗保留草稿且不污染 history。

Browsing UI 已呈現「本機 > 磁碟 > 資料夾」，segment、chevron 與右側空白各有獨立 action/hit target；`Ctrl+L`／`Alt+D` 以 parsing path 重建 editor entity並全選，Esc 還原。`target/explorer-visual-address-evidence/breadcrumb-v8/` 的同尺寸 capture 保持 15/15 region gate 通過，masked changed-pixel ratio 為 `0.0538968924`。

## 10. Navigation pane 與 Windows Shell icon

左側導覽列已改成 typed navigation items，依序提供常用、圖庫、OneDrive、Windows Known Folders、This PC、實際存在的磁碟機與網路；每列具有穩定 ID、32 logical-px row、indent、chevron、pin、selected/hover、separator 與可捲動容器。`IShellItemImageFactory` 在 Shell STA 取得真實 Windows icon，經 RAII `HBITMAP`／`HDC` 讀回為 owned RGBA/stride payload，GPUI 僅建立 texture，不持有 Win32 handle。

`target/explorer-visual-address-evidence/navigation-shell-icons-v3/` 為 175% DPI 實機擷取；`navigation-shell-icons-v3-compare/report.json` 維持 15/15 region、4/4 gap、4/4 color samples 通過，masked changed-pixel ratio 為 `0.0540040662`。

同一條 Shell icon 管線已延伸到檔案列，僅對每個 tab/generation 的前 64 個 viewport-priority items 建立請求；100,000 rows contract test 證明不會替全部 offscreen rows 排隊。`target/smoke-evidence/20260727T024437890Z-4319f49b476849b78f4d4defd5493d5c/01-resized.png` 使用真實 `D:\`，可見 folder、WinRAR RAR/ZIP association、This PC、C/D/E drive 與 namespace icon，resize/minimize/maximize/restore/close lifecycle 全部通過。
