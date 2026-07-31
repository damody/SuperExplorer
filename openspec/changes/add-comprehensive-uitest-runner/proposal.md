## Why

目前約 100 個 OpenSpec requirements 的驗證分散在 Cargo tests、PowerShell headful smoke、真實檔案系統測試、視覺比較與人工證據，沒有單一可重複入口，也沒有機器可判定的 requirement-to-test coverage gate。新增功能後即使局部測試通過，仍可能悄悄破壞舊功能而未被現有流程發現。

## What Changes

- 新增 manifest-driven `explorer-uitest` Rust 測試程式，提供 `quick`、`full`、`interop`、`visual` 與 `soak` suites。
- 自動掃描所有非 archived OpenSpec change specs，為每個 `### Requirement:` 建立穩定 requirement identity；未映射到測試案例即讓 coverage gate 失敗。
- 統一執行 Cargo unit/integration/doc tests、architecture/format/Clippy gates、真實可寫資料夾檔案操作、GPUI headful UIA、Clipboard、OLE drag-and-drop、Shell context menu、搜尋、圖示、視覺/DPI 與跨磁碟互動。
- 每個案例具備 prerequisite、timeout、互斥 GUI 資源、可重現命令、輸出目錄與 evidence artifacts；不可用環境須回報有理由的 SKIP，不得偽裝 PASS。
- 輸出版本化 JSON、JUnit XML 與 Markdown summary，包含 PASS/FAIL/SKIP、耗時、stdout/stderr、requirement coverage、未覆蓋 requirement 與失敗重跑命令。
- 新增 runner 自身的 parser、validation、timeout、process cleanup、report 與 coverage tests。

## Capabilities

### New Capabilities

- `openspec-regression-runner`: 定義從 OpenSpec requirements 到可執行回歸案例、分層 suites、真實 Windows/UI interop、結果報告與 coverage gate 的完整契約。

### Modified Capabilities

無。此變更整合並驗證既有 capability，不改變檔案總管產品行為。

## Impact

- 新增 workspace crate `crates/explorer-uitest`、測試 manifest 與 runner 文件。
- 重用並參數化 `scripts/` 下既有 smoke/visual/soak scripts；新增缺少的跨功能案例時仍使用受控 fixture 與安全 cleanup。
- CI/本機可用單一命令執行；full/interop/visual suites 需要 Windows 桌面 session，部分案例需要實際 C:/D: 磁碟、Explorer、已安裝 shell extension 或指定 DPI/theme。
- 不新增 production runtime dependency，也不讓測試程式進入 `explorer-app` dependency graph。
