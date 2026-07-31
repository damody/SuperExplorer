## Context

專案已有成熟但分散的驗證資產：各 crate 的 Rust tests、`scripts/smoke_*.ps1`、真實 Shell/檔案系統 integration tests、Explorer interop、visual comparator、DPI/theme capture 與 capability soak。現況的 `run_headful_validation.ps1` 只有少數固定步驟，遇到第一個失敗即停止，也無法回答「哪個 OpenSpec requirement 沒有任何自動測試」。目前三個非 archived changes 合計約 100 個 requirements，且數量會持續增加。

測試程式只能在 Windows 執行；headful、Clipboard、OLE、context menu 與 Explorer interop 必須共享 interactive desktop，不能平行搶滑鼠、foreground window 或 clipboard。測試不得破壞使用者資料，所有 mutation 必須限制於 runner 建立的 owned fixture root。

## Goals / Non-Goals

**Goals:**

- 單一 executable 可列出、篩選、執行與重跑所有回歸案例。
- 每個非 archived OpenSpec requirement 在最終報告中具名，且至少映射一個可執行案例。
- quick suite 適合每次修改；full/interop/visual/soak suites 提供逐步加深的 Windows 真實驗證。
- 每個 subprocess 有 timeout、stdout/stderr capture、process-tree cleanup 與穩定 exit semantics。
- 一次執行即產生 JSON、JUnit XML、Markdown 和 requirement coverage artifacts。
- 既有專項 scripts 保持單一職責並可獨立重跑。

**Non-Goals:**

- 不以一個端到端案例取代 crate-level unit tests。
- 不宣稱在缺少第二磁碟、Explorer、指定 DPI/theme 或 shell extension 時通過對應案例；只能明確 SKIP 或在 strict 模式失敗。
- 不操作 fixture root 以外的真實檔案，不自動接受或更新 visual baseline。
- 不將 test runner 或 test-support 依賴加入 production `explorer-app` graph。

## Decisions

### 1. 使用獨立 Rust workspace crate 作為 orchestrator

新增 `crates/explorer-uitest` binary。Rust 提供強型別 manifest validation、穩定 exit code、跨 subprocess timeout、JSON/JUnit 產出與自身 unit tests；PowerShell 保留為 Windows UIA/Win32 操作 adapter。相較繼續擴充單一 PowerShell 腳本，此方案較容易測試 schema、coverage 與報告，並能避免 `$LASTEXITCODE`、scope 和例外流程互相污染。

### 2. Manifest 描述案例，OpenSpec 掃描器提供 requirements truth

`uitest/manifest.json` 只描述 suites、command、timeout、prerequisites、exclusive resources、coverage selectors 與 evidence policy。Runner 每次執行掃描 `openspec/changes/*/specs/*/spec.md` 的 `### Requirement:`，以 `<change>/<capability>/<normalized-title>` 建立 identity。Coverage selector 可指向精確 requirement 或 capability prefix，但報告永遠展開為逐 requirement mapping；零 mapping、selector 零命中、重複 case id、未知 suite 或非正 timeout 均為 manifest error。

### 3. 分層而非單一超長 suite

- `quick`: runner self-test、OpenSpec coverage、fmt/check、architecture、核心 crate tests。
- `full`: quick 加上 build、lifecycle、keyboard/mouse、tabs/address/search、sort/view/panes、rename、scroll、accessibility/IME/icon cache 等 headful regression。
- `interop`: 真實 Clipboard、OLE source/target、Explorer drag、context menu、file operations、watcher 與 search backend。
- `visual`: reference comparator、light/dark/high-contrast、DPI 與 region diagnostics；baseline 永遠只讀。
- `soak`: 100k folder/search、重複 multi-tab、resource/handle/process leak oracle。

Suite selection 取聯集且依 manifest 順序執行。GUI/clipboard/OLE/cursor 案例標示 exclusive resource；第一版保守地序列執行，未來才允許無共享資源的 pure cases 平行化。

### 4. Prerequisite 與 SKIP 是正式結果

Prerequisite 包含 Windows、interactive desktop、路徑/磁碟存在、命令存在、環境變數與 opt-in destructive/visual profile。缺少 prerequisite 產生 `SKIP` 和具體 reason；`--fail-on-skip` 將任何 SKIP 轉為整體失敗，適合 release gate。一般 failure、timeout、manifest/coverage error 永遠使 exit code 非零。

### 5. 安全 fixture 與 subprocess cleanup

Runner 建立 `target/uitest-runs/<run-id>/fixtures` 與 `evidence/<case-id>`，將絕對路徑透過環境變數傳入案例。測試只可刪除 resolved path 位於該 run root 的內容。每個 case 以隱藏 subprocess 啟動；timeout 時終止完整 process tree，並在報告記錄 terminal reason。Headful scripts 仍負責關閉自身 app/Explorer window，runner 負責最後防線。

### 6. 報告採 append-independent final snapshot

每次 run 產生：

- `report.json`: version、host、git revision、selected suites、case results、commands、durations、artifacts。
- `junit.xml`: CI testcases、failure/skipped output。
- `summary.md`: 人類可讀摘要與重跑命令。
- `coverage.json`: 每個 requirement 對應 cases 與本次最佳結果。
- 每 case 的 `stdout.log`、`stderr.log`。

寫檔採 temporary file + rename，避免中途中止留下看似完整的報告。

## Risks / Trade-offs

- [100 個 requirements 映射到錯誤或過寬案例] → coverage 報告逐 requirement 展開；manifest review 顯示 selector 與命中數；關鍵互動使用專項 case 而非只靠 workspace tests。
- [Headful automation 受前景視窗、DPI、語系影響而 flaky] → 強制序列、等待 UIA 狀態而非固定 sleep、保存 log/screenshot、案例可單獨重跑。
- [full suite 執行時間長] → quick/full/interop/visual/soak 分層，預設 quick，報告提供耗時排序。
- [測試誤改使用者檔案] → owned fixture root、resolved containment guard、不可用 workspace/root/profile 目錄、預設不執行 destructive external cases。
- [未安裝 shell extension 或缺少 D:] → 明確 prerequisite SKIP；release 使用 `--fail-on-skip`。
- [子程序 timeout 後殘留 app/Explorer] → process-tree kill、run 前後 PID census，殘留程序使案例失敗。

## Migration Plan

1. 新增 runner crate、manifest schema、OpenSpec scanner 與 self tests。
2. 先納入 pure/quick cases並使 coverage gate 完整。
3. 逐一接上既有 headful/interop/visual/soak scripts，保留原腳本入口。
4. 文件將舊的多個命令改列為 runner case 的底層重跑方式。
5. CI 先採 quick；Windows interactive release machine 採 full+interop+visual `--fail-on-skip`。

Rollback 只需移除 runner crate/manifest與文件；既有 scripts 和 production code不受影響。

## Open Questions

無阻塞問題。第一版採序列執行；案例平行化留待有實際時間需求及可靠 resource lock 後再設計。
