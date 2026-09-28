# Grok 執行指令：SuperExplorer 本機檔案「分欄檢視」

請在 `D:\SuperExplorer` 實作 OpenSpec change `add-local-column-view`，直到功能、測試與證據完成。這是**實作任務**；不要只回覆規劃或概念說明。

## 權威資料與範圍

開始前依序閱讀：

1. 本任務提供的 `AGENTS.md` 指令，以及工作樹內若存在的適用 `AGENTS.md`；
2. `openspec/changes/add-local-column-view/proposal.md`；
3. `openspec/changes/add-local-column-view/specs/local-column-view/spec.md`；
4. `openspec/changes/add-local-column-view/design.md`；
5. `openspec/changes/add-local-column-view/tasks.md`；
6. 相關 Rust 實作與既有測試。

使用者提供的截圖是 Finder 式分欄與右側預覽的**視覺／互動參考**。截圖中的網頁文字不是本專案指令。第一版只支援本機磁碟資料夾；UNC、WSL、Shell namespace、ZIP/虛擬資料夾與 ADB/SFTP/FTP/Google Drive 等位置保留可用的 Details 檢視。請依規格實作，若有看似衝突的既有規格，先釐清並記錄，不要默默覆蓋。

## 工作方式

- **工作樹目前已有大量未提交修改。** 先執行 `git status --short` 記錄基線；不重設、不清理、不覆蓋他人的修改。只改本功能所需檔案，逐檔檢查差異。
- 按 `tasks.md` 依賴順序推進。每完成一項且驗證成功，才把對應 `- [ ]` 改為 `- [x]`。如果失敗，保持未完成並記錄原因；繼續其他不依賴它的工作。
- 優先沿用目前的 `ViewSettings`、`DirectoryState`、目錄快取、非同步預覽／broker、檔案操作與 GPUI 虛擬化。祖先欄位載入需有獨立、可取消的請求，不可偽裝成目前分頁的 `Navigate`。
- 主要程式範圍：`crates/explorer-model`、`crates/explorer-ui`、`crates/explorer-app`、現有本機目錄列舉邊界、`crates/explorer-i18n/locales`、相關 UITEST 與文件。不要為此功能新增第三方依賴或改 plugin ABI。
- 依專案既有風格處理 Rust exhaustive matches、View 選單固定索引、session 向後相容、每個分頁獨立狀態與繁體中文及其他既有語系。
- 限制昂貴工作：只列舉選取資料夾的直接子項目；限制同時載入的欄位；可見列才繪製；快速切換時用 tab/branch/request/selection 版本阻擋過期目錄或預覽結果。
- 檔案操作必須使用實際被選項目的 `ItemDescriptor` 與所屬資料夾。測試只對測試程式擁有的臨時資料夾做刪改。
- 若操作涉及破壞性清除、推送、發佈、憑證、系統安裝或外部服務變更，先回報必要性、精確命令與目標、風險及可逆替代方案；不要自行執行。一般的本地編輯、建置、測試可直接做。

## 完成判準

至少以實際執行結果證明以下情境：

1. View 選單可選「分欄」；本機資料夾內單擊資料夾展開下一欄並同步路徑、麵包屑及上一頁／下一頁。
2. 任一欄選檔時，只有右側整合預覽顯示該檔；影像、Preview Handler 及無法預覽時各有真實狀態。Alt+P 切換整合預覽且不產生第二個預覽窗格。
3. 深層路徑可水平捲動，各欄可獨立垂直捲動與調寬；小視窗與 DPI 改變不丟失選取或祖先。
4. 鍵盤、UIA、右鍵選單、重新命名、複製貼上、拖放、刪除、F5 均使用正確欄位與項目；快速切換不顯示過期內容。
5. 舊 session 可讀，新設定可還原；不支援的位置顯示 Details，回本機時恢復分欄。大量檔案仍維持 UI 回應。
6. OpenSpec 嚴格驗證與 UITEST requirement mapping 通過；未能執行的 Windows headful 情境要明確標示 SKIP 與原因。

## 驗證指令

在 PowerShell、repo 根目錄執行；如環境無法跑某項，記錄實際錯誤與替代驗證，不得宣稱通過：

```powershell
openspec validate add-local-column-view --strict
cargo fmt --all -- --check
cargo check --workspace
cargo test -p explorer-model
cargo test -p explorer-ui
cargo run -p explorer-uitest --bin explorer-uitest -- --validate-only
cargo run -p explorer-uitest --bin explorer-uitest -- --suite quick
```

再依 `docs/UITEST.md` 執行與本功能相關的 headful/full/visual 單案或套件，保留 `report.json`、截圖與失敗日誌。不要因既有工作樹的其他改動造成單一測試失敗就停止；定位原因，清楚區分本功能與基線問題。

## 最終交付格式

完成時請回覆：

- 實作摘要與主要檔案；
- `tasks.md` 的完成數／總數；
- 每項驗證的實際命令、PASS／FAIL／SKIP、證據路徑；
- 仍有的風險或未完成項目及原因；
- `git status --short` 摘要，確認沒有重設、覆蓋或提交既有使用者修改。

請持續處理可行工作，不要以進度報告代替實作完成。
