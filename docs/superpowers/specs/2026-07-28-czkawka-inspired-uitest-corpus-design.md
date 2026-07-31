# Czkawka 啟發的完整 UTIT 檔案語料測試設計

日期：2026-07-28

## 目標

參考 Czkawka 對真實檔案樹、異常檔案、快取、取消及大資料量的測試方式，擴充本專案 UTIT。此變更只增加測試基礎設施、fixture、案例與證據，不增加重複檔案掃描、垃圾清理或媒體分析等產品功能。

測試必須直接驗證本程式對 Windows 真實檔案系統的列舉、顯示、導航、搜尋、選取與檔案操作行為。所有可變資料只能位於測試擁有的 GUID 目錄，禁止讀寫使用者既有檔案作為操作目標。

## 設計原則

- 以 Rust、Lua 與 PowerShell 為主，不建立 Python 或 Pillow 必要依賴。
- 測試結果以磁碟狀態、UI Automation 狀態、應用程式診斷及程序資源四種 oracle 交叉驗證。
- 每個案例只負責一組風險，讓失敗能直接指出語料類型與操作階段。
- 不支援的 Windows/NTFS 能力只跳過對應子情境，不得讓整個案例假通過。
- quick、full、interop 與 soak 有明確成本分層；一般 `UTIT.bat` 不執行 20,000 檔案壓力案例。
- 每個案例必須可重複執行、可單獨重跑，並在成功或失敗後安全清理。

## 架構

### 語料 manifest

新增版本化的語料 manifest，記錄每個節點的相對路徑、種類、內容類別、長度、雜湊、屬性、時間戳、連結目標及預期 UI 行為。manifest 使用 UTF-8 JSON，路徑只保存相對於 fixture root 的值，不記錄使用者身分或其它敏感絕對路徑。

語料建立器收到 runner 提供的 `{fixture_root}` 與案例識別後建立 GUID 子目錄。一般檔案與目錄由 Lua 建立；Windows 屬性、hard link、junction、symbolic link、ADS、稀疏檔案及長路徑由 PowerShell/.NET 或 Windows 原生命令處理。Rust 測試負責 manifest schema、雜湊、排序、邊界與前後快照的確定性檢查。

### 共用測試驅動

共用 PowerShell 模組提供：

- 受控 fixture root 建立與 containment 驗證。
- 應用程式啟動、window-ready 等待、UI Automation 查詢與乾淨關閉。
- 磁碟快照、SHA-256、檔案屬性及時間戳讀取。
- 子情境 PASS、FAIL、SKIP 與詳細 failure evidence 記錄。
- 安全清理；清理前重新解析完整路徑並確認仍位於測試擁有目錄。

不把所有行為寫入單一腳本。每個 UTIT 案例使用共用模組，但保有獨立入口、timeout、prerequisite、exclusive resource 與 evidence directory。

## 測試案例

### filesystem-corpus-contract

屬於 quick 套件，不啟動 UI。建立小型確定性語料並驗證建立器及 oracle：

- 零位元檔案、非零小檔案、相同大小但不同內容、完全相同內容。
- 空資料夾、只含空資料夾的巢狀樹、含檔案的非空父資料夾。
- 不同大小檔案、可快速建立的稀疏檔案與固定雜湊內容。
- 繁體中文、日文、韓文、emoji、組合字元、空白、括號、井字號、百分比及接近 Windows 上限的長路徑。
- manifest round-trip、排序確定性、內容雜湊、路徑 containment 與重跑隔離。

### filesystem-corpus-headful

屬於 full 與 visual 套件。真實啟動本程式並以 UI Automation 與磁碟 manifest 比對：

- 名稱、項目數、資料夾/檔案類型、大小及修改日期。
- Details、List、Content、Small/Medium/Large/Extra Large Icons 切換後項目集合不變。
- 名稱、日期、類型、大小的升冪與降冪排序。
- breadcrumb、chevron、網址列直接輸入、重新整理、Backspace 及多分頁切換。
- 搜尋 Unicode、大小寫、空白與無結果字串，並驗證清除搜尋可恢復資料夾快照。
- 捲動後 Details header 固定、選取身份與 scroll anchor 不因資料更新錯置。

### ntfs-semantics-interop

屬於 full 與 interop 套件：

- Hidden、System、ReadOnly、Archive 屬性以及顯示隱藏項目設定。
- hard link identity、junction 導航、symbolic link 導航與失效 symbolic link。
- junction/symlink 循環不得造成無限列舉、無限搜尋或無界資源成長。
- NTFS alternate data stream 不得被誤列為一般子項目。
- 外部刪除、改名、替換及時間戳更新後，Refresh 必須收斂到磁碟真實狀態。
- 非 NTFS、缺少 symbolic-link 權限或系統政策不允許時，對應子情境記錄具體 SKIP 原因。

### mutation-safety-matrix

屬於 full 與 interop 套件。操作前後都保存磁碟快照：

- F2 rename、Delete、Ctrl+C、Ctrl+X、Ctrl+V、Backspace、Shift range selection 及 Ctrl toggle selection。
- Unicode、深層路徑、空資料夾、零位元檔案、唯讀檔案及同名衝突。
- 同磁碟 move、跨 C:/D: move/copy、取消與失敗後復原。
- 來源與目的地存在性、內容 SHA-256、檔案長度與未選取項目不變性。
- 項目在確認前被外部替換、刪除或由空資料夾變成非空時，不得操作錯誤身份或遞迴刪除新內容。

所有 destructive 驗證只針對 fixture；刪除案例以可觀察的產品行為驗證，清理階段才使用測試腳本強制移除 fixture。

### large-directory-cancel-and-cache

分為兩個成本等級：

- full：大約 2,000 個項目，包含多層資料夾、重複大小、混合副檔名與 Unicode 名稱。
- soak：大約 20,000 個項目，不由無參數 `UTIT.bat` 預設執行。

驗證快速資料夾切換、連續重新整理、搜尋取消、關閉分頁及關閉程式。舊 generation 的批次或終止事件不得覆蓋新資料夾。冷啟動與熱快取的項目集合、排序及圖示 fallback 必須一致；損壞測試專用快取後必須安全重建。記錄啟動時間、首批項目時間、完成時間、working set、thread、process/GDI/User handle，並使用寬鬆但明確的防退化上限，避免以硬體相依的微小時間差造成假失敗。

## 錯誤處理與證據

每個案例至少輸出：

- `report.json`：案例、子情境、狀態、時間與 failure reason。
- `fixture-manifest.json`：預期語料及 capability probes。
- `before.json`、`after.json`：需要 mutation 的磁碟快照。
- UI 案例的必要 screenshot、UIA tree 或 action log。
- 資源案例的 `resources.json` 與 timing 資料。

成功案例在當日 `UTIT-YYYY-M-D.log` 只輸出穩定案例識別。FAIL、TIMEOUT、ERROR 與 SKIP 輸出原因、命令、evidence、stdout/stderr 及可單獨重跑命令。fixture 建立失敗、capability 不支援、產品行為失敗與清理失敗使用不同狀態/原因，不互相掩蓋。

## Manifest 與執行分層

新增獨立 manifest cases，使用 runner 現有 `{fixture_root}`、`{evidence_dir}`、prerequisite、exclusive resource 與 required artifacts：

- quick：`filesystem-corpus-contract`。
- full/visual：`filesystem-corpus-headful`。
- full/interop：`ntfs-semantics-interop`、`mutation-safety-matrix`。
- full：2,000 項目的 `large-directory-cancel-and-cache`。
- soak：20,000 項目的 `large-directory-cancel-and-cache-soak`。

一般 `UTIT.bat` 繼續執行 quick、full、interop、visual，因此會涵蓋除 soak 以外的新案例。soak 由 `UTIT.bat --suite soak` 或 runner 直接選取。

## 驗收標準

- 新案例可從乾淨 fixture root 單獨執行並產生完整 required artifacts。
- 在支援的 Windows/NTFS 主機上，所有非 soak 子情境通過；不支援能力有精確 SKIP 原因。
- C:/D: fixture 僅在兩磁碟存在時執行，且清理範圍始終受 containment 驗證。
- 一般完整 UTIT 為零 FAIL、零 TIMEOUT、零 ERROR；環境能力不足只允許事先定義的 truthful SKIP。
- 不新增 Python module prerequisite，也不下載或執行 Czkawka 二進位檔；Czkawka 僅作為測試設計參考。
- Rust fmt、workspace tests、warnings-as-errors Clippy、manifest validation 與 OpenSpec coverage gate 全部通過。

## 非目標

- 不在產品內新增重複檔案、空資料夾、壞副檔名、媒體相似度或垃圾清理功能。
- 不複製 Czkawka 原始碼或測試資產。
- 不掃描、修改或刪除使用者既有資料夾。
- 不以 sleep 或放寬斷言掩蓋非同步、快取或 UI 競速。
