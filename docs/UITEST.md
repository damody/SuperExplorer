# Explorer UITEST 使用與維護指南

## Shell context menu and locked-delete cases

- `complete-shell-context-menu-direct`: real native provider inventory and ordinary/Shift baselines.
- `complete-shell-context-menu-broker`: exact direct-worker versus broker differential for file, folder, multi-selection, and background targets.
- `locked-delete-recovery`: deterministic reducer, Restart Manager adapter, eligibility, cancellation, PID-reuse, refused, timeout, and cleanup tests.
- `locked-delete-recovery-headful`: real owned lock holders with UIA, keyboard, pointer, Cancel, Retry, multi-owner, and Close-and-retry evidence.
- `context-lock-resource-soak`: ten native popup and locked-delete lifecycle cycles with resource/session baselines.

The headful cases operate only on runner-owned temporary fixtures. They do not close unrelated processes or delete user data.

## Post-parity roadmap cases

`roadmap-session-persistence`、`roadmap-thumbnail-cache`、`roadmap-shell-namespace`、`roadmap-extension-broker`、`roadmap-preview-handler` 是正式 manifest cases。`--validate-only` 必須維持所有 active OpenSpec requirements 100% 覆蓋；combined report 位於 `target/uitest-runs/roadmap-combined-final/report.json`，10 輪資源 soak 位於 `target/roadmap-combined-soak10-v3/report.json`。

`explorer-uitest` 是本專案的統一回歸測試入口。它會掃描作用中的 OpenSpec requirement，以 manifest 對應測試案例，執行 Rust contract tests 與 Windows headful 測試，並產生可供本機與 CI 使用的結構化報告。

## 常用指令

Windows 本機可直接執行根目錄的 `UTIT.bat`。它固定使用 `D:\test\build\tools\lua\lua.exe` 彙整結果，預設執行 quick、full、interop、visual，並在根目錄產生當日的 `UTIT-年-月-日.log`。成功案例只記錄標題；非成功案例會記錄完整原因、stdout/stderr、重跑命令與最終統計。也可以指定單案，例如 `UTIT.bat --case runner-unit-tests`。

```powershell
# 僅驗證 manifest、OpenSpec 掃描與 100% requirement coverage
cargo run -p explorer-uitest -- --validate-only

# 快速守門：fmt、workspace check/tests、Clippy、doc tests、架構與 runner 自測
cargo run -p explorer-uitest -- --suite quick

# 真實視窗、鍵盤、滑鼠、F2、捲軸、排序、檢視模式與 accessibility
cargo run -p explorer-uitest -- --suite full

# Clipboard、OLE drag-and-drop、Shell context menu、TortoiseGit
cargo run -p explorer-uitest -- --suite interop

# 圖示模式、DPI、配色、高對比與參考畫面證據
cargo run -p explorer-uitest -- --suite visual

# 僅重跑單一案例
cargo run -p explorer-uitest -- --case icon-view-layout-headful
```

`--list` 列出案例；`--output <path>` 指定輸出；`--fail-fast` 遇到第一個失敗即停止；`--fail-on-skip` 將 prerequisite 不符造成的 SKIP 視為整體失敗。需要互動桌面的 release gate 應使用 `--fail-on-skip`，一般開發機可保留 truthful SKIP。

## 測試層級

- `quick`：純程式契約與 workspace 品質守門，不依賴滑鼠或前景視窗。
- `full`：啟動 `SuperExplorer.exe`，使用真實資料夾與 UI Automation 驗證鍵盤、滑鼠、焦點、F2、選取、分頁、欄位、捲軸與檢視模式。
- `interop`：驗證 Windows Shell、Clipboard、OLE、Explorer drag-and-drop、context menu 與 TortoiseGit overlay。
- `visual`：保留 PNG、diagnostics 與 metadata，涵蓋圖示模式、100–200% DPI、高對比及多螢幕條件。
- `soak`：重複啟動、檔案操作與資源釋放的長時間測試。

## Fixture 與安全性

跨磁碟案例只會在 runner 擁有的目錄建立資料：

- `%LOCALAPPDATA%\Temp\RustGpuiExplorerUITest\explorer-uitest-<guid>`
- `<workspace>\target\uitest-drive-fixtures\explorer-uitest-<guid>`

清理前會做 resolved-parent containment 檢查，拒絕 workspace root、磁碟根目錄、使用者 profile 或相鄰目錄。測試不應直接修改既有的 `C:\`、`D:\` 使用者資料。

## Manifest 約定

案例可宣告 Windows、互動桌面、命令、路徑、環境變數與最少螢幕數 prerequisite。條件不符時會記錄具體 SKIP 原因。`required_artifacts` 支援相對 glob，例如 `report.json`、`**/*.png`；找不到必要證據時原本的 PASS 會轉為 FAIL。

每個子程序會記錄啟動 PID、執行前後程序數、殘留 descendant PID 與清理嘗試。逾時會終止整棵 process tree；殘留程序也會使整體結果失敗。

## 報告與除錯

預設輸出在 `target/uitest-runs/<run-id>/`：

- `report.json`：版本化的主機、選取條件、案例結果、命令、artifact 與 process census。
- `coverage.json`：OpenSpec requirement、對應案例與最佳執行結果。
- `junit.xml`：CI 測試報告。
- `summary.md`：人類可讀摘要與精確重跑命令。
- `logs/<case>/stdout.log`、`stderr.log`：完整子程序輸出。
- `evidence/<case>/`：截圖、UIA tree、diagnostics 與案例 report。

失敗時先使用 `summary.md` 提供的 `--case` 指令重跑，再查看該案例的 stderr、report 與 screenshot。測試輸出不得包含使用者名稱或電腦名稱；host metadata 只保留 Windows build、CPU architecture、rustc/cargo、Git revision 與 dirty flag。

## 本機分欄檢視

`local-column-view-contract` 執行分欄模式的模型與 UI reducer 測試。`local-column-view-headful` 需有互動桌面與已建置的 `SuperExplorer.exe`；它先確認 runner 擁有的啟動夾具已顯示，再操作 View 選單切換分欄、點開資料夾確認下一欄子檔可見，最後驗證 Alt+P 隱藏整合預覽。缺少執行檔時報告 SKIP；操作或斷言失敗時報告 FAIL。JPEG 像素、水平捲軸與 F2 重新命名另由專用腳本驗證。`local-column-view-commands-headful` 在四個獨立的 runner 擁有夾具中逐項驗證祖先欄 Delete、複製貼上、跨欄拖放及右鍵選單；每項有程序時間上限，只有實際檔案結果吻合才報 PASS。使用者操作說明見 `docs/COLUMN_VIEW.md`。

分欄案例在 `uitest/manifest.json` 使用逐項的 OpenSpec requirement ID。`local-column-view-keyboard-headful` 用獨立本機夾具驗證鍵盤切換分欄、方向鍵與位址、欄內 Shift/Ctrl 選取、UIA 名稱／角色及焦點返回；檢視選單勾選狀態在 UIA 不提供 TogglePattern 時標示 SKIP。`local-column-view-empty-headful` 驗證空資料夾的狀態文字、父欄保留及重試控制；權限不足的資料夾只經單元測試，腳本不更動 ACL。`local-column-view-handler-headful` 使用已註冊的本機 PDF Preview Handler，驗證初始邊界、預覽寬度拖曳、Alt+P 卸載／重開與切換選取後卸載；視窗同步縮放、真實 DPI 切換、handler 崩潰與 UIA 焦點查詢在報告中明確標示 SKIP，對應的狀態轉移另有單元測試。全庫 `--validate-only` 仍因其他進行中變更的 548 項未覆蓋要求而失敗；使用 `--case <id>` 執行單一分欄案例，並檢查其報告的 PASS、FAIL 或 SKIP 與證據路徑。

## 新增案例檢查表

1. 優先新增 deterministic Rust test，再以 headful 腳本補足 Windows 行為。
2. 腳本接受 `-OutputDirectory` 與 `-SkipBuild`，把證據寫入 runner 的 `{evidence_dir}`。
3. 設定有界 timeout、正確 prerequisite、必要 exclusive resources 與 artifacts。
4. 以 `covers` 對應 OpenSpec requirement，執行 `--validate-only` 確認 coverage 維持 100%。
5. 讓案例自行建立並清理 fixture；不得依賴或刪除使用者既有檔案。
