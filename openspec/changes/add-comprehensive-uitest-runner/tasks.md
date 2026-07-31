## 1. Coverage inventory and contracts

- [x] 1.1 保存目前非 archived changes、spec files、requirement 數量、既有 Cargo tests、smoke/visual/interop/soak scripts 與手動證據清單。
- [x] 1.2 定義版本化 manifest schema：case id、description、suites、program、arguments、timeout、prerequisites、exclusive resources、coverage selectors、evidence 與 environment。
- [x] 1.3 定義穩定 requirement identity `<change>/<capability>/<normalized-title>`、Unicode normalization、重複標題與 collision 規則。
- [x] 1.4 定義 case terminal enum PASS/FAIL/SKIP/TIMEOUT/ERROR、overall exit code、fail-fast 與 fail-on-skip 語意。
- [x] 1.5 定義 report JSON、coverage JSON、JUnit XML 與 Markdown schema/version/sample fixtures。

## 2. Runner crate foundation

- [x] 2.1 新增 `crates/explorer-uitest` workspace binary crate，只依賴 workspace common libraries且不被 production crates依賴。
- [x] 2.2 實作 CLI parser：`--manifest`、`--suite`、`--case`、`--list`、`--output`、`--fail-fast`、`--fail-on-skip`、`--validate-only`。
- [x] 2.3 實作 workspace root discovery、絕對/相對路徑正規化與 Windows-only 明確錯誤。
- [x] 2.4 實作 run id、`target/uitest-runs/<id>`、fixtures/evidence/logs 目錄與環境變數注入。
- [x] 2.5 實作 privacy-safe host metadata：Windows build、rustc/cargo、git revision、dirty flag、selected suites，不收集使用者檔名內容。

## 3. Manifest parsing and validation

- [x] 3.1 實作 serde manifest types、schema version gate、未知欄位拒絕與友善 path-aware errors。
- [x] 3.2 驗證 case id 唯一、description 非空、suite 非空且已知、timeout 正值、program/arguments 合法。
- [x] 3.3 驗證 environment key、evidence relative path、exclusive resource enum 與 prerequisite payload。
- [x] 3.4 實作 suite/case filter 取聯集、manifest ordering、零選取錯誤與 `--list` 表格。
- [x] 3.5 新增 parser/validation/filter snapshot tests，涵蓋 malformed JSON、duplicate、unknown suite、zero timeout、zero selected。

## 4. OpenSpec scanner and coverage gate

- [x] 4.1 遞迴掃描 `openspec/changes/*/specs/*/spec.md`，忽略 archive/hidden/target並解析所有 `### Requirement:`。
- [x] 4.2 實作 change/capability/title identity normalization、來源 path/line、duplicate identity diagnostic。
- [x] 4.3 實作 exact/prefix coverage selector expansion，零命中 selector 為 validation error。
- [x] 4.4 建立 requirement-to-cases 與 case-to-requirements 雙向 mapping，未覆蓋 requirement 使 coverage gate失敗。
- [x] 4.5 `--validate-only` 輸出 discovered/covered/uncovered counts及逐項 uncovered list，不啟動 subprocess。
- [x] 4.6 新增 scanner/normalization/selector/new-requirement-breaks-gate tests，包含繁中與英文標題。

## 5. Prerequisite engine and safety

- [x] 5.1 實作 Windows、interactive desktop、command、path/drive、environment opt-in prerequisites及具體 SKIP reason。
- [x] 5.2 實作 `--fail-on-skip` overall failure，普通 SKIP 不計 PASS也不隱藏 coverage。
- [x] 5.3 實作 runner-owned fixture resolved containment guard，拒絕 root/workspace/profile/fixture外 cleanup target。
- [x] 5.4 實作 exclusive resource validation及序列 scheduler；GUI/cursor/clipboard/OLE/Explorer cases不得重疊。
- [x] 5.5 新增 prerequisite、missing D drive、missing command、containment escape、fail-on-skip tests。

## 6. Process execution and cleanup

- [x] 6.1 以明確 program/argument array 啟動 subprocess，不經未轉義 shell string，設定 workspace cwd及per-case environment。
- [x] 6.2 每 case 保存 stdout/stderr、開始/結束UTC、duration、exit code與可重現 display command。
- [x] 6.3 實作 timeout polling與Windows process-tree termination，terminal標為TIMEOUT且繼續或fail-fast。
- [x] 6.4 實作 case evidence目錄、expected artifact glob收集與缺少必要artifact失敗。
- [x] 6.5 執行前後 process census並偵測 runner建立之殘留child PID；cleanup failure不得改寫原始 terminal。
- [x] 6.6 新增 pass/fail/timeout/large output/environment/working-directory/process cleanup integration tests。

## 7. Unified reports

- [x] 7.1 實作 versioned `report.json`，涵蓋host、selection、case result、counts、command/log/artifact paths。
- [x] 7.2 實作 `coverage.json`，逐 requirement列出mapped cases、本次best result與未執行原因。
- [x] 7.3 實作JUnit XML escaping、failure/skipped/system-out/system-err與一致counts。
- [x] 7.4 實作Markdown summary：總覽、失敗/跳過、耗時排序、uncovered、artifact與單case重跑命令。
- [x] 7.5 所有final report採同目錄temporary file + atomic rename，寫入失敗回傳非零。
- [x] 7.6 新增 mixed PASS/FAIL/SKIP/TIMEOUT golden tests，驗證四種格式 identities/counts一致。

## 8. Quick suite manifest

- [x] 8.1 新增 `uitest/manifest.json` 與 schema fixture，納入 runner self tests與OpenSpec coverage validation。
- [x] 8.2 納入 cargo fmt、workspace check、workspace tests、Clippy、doc tests、architecture與UI token audits。
- [x] 8.3 將 windows-app-foundation、model/navigation/search/operations與error diagnostics requirements映射到具名quick cases。
- [x] 8.4 執行 `explorer-uitest --validate-only`，保證所有目前requirements均有至少一個mapping。
- [x] 8.5 執行完整quick suite並保存第一份JSON/JUnit/Markdown/coverage evidence。

## 9. Full headful suite

- [x] 9.1 納入 Windows lifecycle/repeated startup、panic report與一般權限/UAC/logging驗證。
- [x] 9.2 納入 keyboard、mouse、accessibility、IME、breadcrumb/address、multi-tab與cross-drive F2 cases。
- [x] 9.3 納入 sort/column resize、view modes/panes、details pinned header、scrollbar capture、inline rename cases。
- [x] 9.4 納入 Shell icon/overlay/cache、navigation pane、command bar、selection/range/marquee/context focus cases。
- [x] 9.5 所有headful cases宣告gui/cursor exclusive resources、必要artifact與bounded timeout。
- [x] 9.6 執行full suite並確認app/window/process清理、FAIL/SKIP可單獨重跑。

## 10. Interop, visual and soak suites

- [x] 10.1 納入真實file operations、watcher、multi-tab real folder及search backend tests，fixture只位於owned root。
- [x] 10.2 納入OLE Clipboard copy/cut/paste、drag source/drop target/right-drag與Explorer雙向drag cases。
- [x] 10.3 納入background/single/multi context menu、installed extension、timeout/cancel/resource cleanup cases。
- [x] 10.4 納入Explorer reference compare、region/color/icon/typography、dark/high-contrast與baseline read-only gate。
- [x] 10.5 納入100/125/150/175/200% DPI、mixed monitor prerequisite及無法實機驗證的truthful SKIP。
- [x] 10.6 納入100k folder/search、repeat tabs、file operations、Clipboard/OLE/context resource soak。
- [x] 10.7 執行可用interop/visual/soak cases；缺少環境者保存SKIP reason並用fail-on-skip測試release gate。

## 11. Documentation and final quality

- [x] 11.1 新增 `docs/UITEST.md`，說明安裝、quick/full/interop/visual/soak、filter、report、rerun與安全模型。
- [x] 11.2 更新README、STATUS、IMPLEMENTATION_PLAN、PARITY_MATRIX與MANUAL_TESTS，將單一runner列為主要回歸入口。
- [x] 11.3 更新architecture audit，禁止production crates依賴explorer-uitest並禁止runner進release artifact。
- [x] 11.4 執行fmt、workspace check/tests/doc tests、Clippy warnings-as-errors、OpenSpec strict及diff-check。
- [x] 11.5 執行quick與環境可用的full/interop/visual smoke，保存最終report與coverage=100%。
- [x] 11.6 檢查git status/submodule/generated evidence policy，只提交本change追蹤檔並排除使用者未追蹤檔。

## 12. 跨磁碟快捷鍵與 TortoiseGit 強驗證

- [x] 12.1 在 `%LOCALAPPDATA%\Temp\RustGpuiExplorerUITest` 與 `D:\test\target\uitest-drive-fixtures` 建立每次執行唯一的 C:/D: 隔離資料，清理前重新驗證 resolved parent containment。
- [x] 12.2 透過真實前景視窗輸入驗證 C: 與 D: 各一次 F2 rename，並以磁碟路徑變更作為成功 oracle，不只檢查 editor 是否出現。
- [x] 12.3 驗證 Shift-click 連續選取與 Ctrl+A 全選，從 UI Automation `SelectionItemPattern` 讀回實際 selected rows 數量。
- [x] 12.4 驗證 Ctrl+C/Ctrl+V 的真實 `CF_HDROP`/OLE data object 能跨 C:→D: 導覽後建立檔案，並以目的檔案存在性判定。
- [x] 12.5 驗證 Enter、Backspace、Delete、F5、Ctrl+T、Ctrl+W、Ctrl+F；Delete 只操作 runner 建立的 `delete-me.txt`，並等待 watcher 與磁碟結果收斂。
- [x] 12.6 盤點本機 TortoiseGit overlay handlers 與 Windows 前 15 slots，建立真實 Git clean/modified/added/unversioned 狀態並比較 Shell RGBA hash。
- [x] 12.7 驗證 Shell 圖示已寫入 `%LOCALAPPDATA%\RustGpuiExplorer\icon-cache\v1`，保存 handler、hash、cache entry count 與 cargo test log。
- [x] 12.8 將跨磁碟快捷鍵與 TortoiseGit 案例加入 full/interop/visual manifest、required artifacts、exclusive resources、prerequisites 與 OpenSpec coverage mapping。
- [x] 12.9 實際執行兩個強驗證案例並保存可重跑證據；缺少必要元件時由 runner 明確 SKIP，release gate 以 `--fail-on-skip` 拒絕略過。
