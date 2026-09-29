# 分欄壓縮檔瀏覽、純文字與圖片資訊預覽

日期：2026-09-29

實作：Codex 直接修改與驗證

## 1. 已實作的功能

| 需求 | 實作結果 |
| --- | --- |
| zip、7z、rar 當作資料夾瀏覽 | 本機分欄可展開壓縮檔、往下進入子目錄、切換同層項目；使用相同的分欄導覽與歷史事件 |
| RAR5 | 使用實際 RAR5 壓縮檔驗證列舉、完整成員讀取及部分讀取 |
| txt 預覽 | 內建純文字預覽，顯示前 3000 個 Unicode 字元 |
| md 純文字預覽 | 顯示原始標題符號、Markdown 語法和文字內容 |
| 壓縮檔內 txt／md | 同樣適用 3000 字上限，以記憶體串流讀取 |
| 圖片預覽 | 保留縮圖比例；壓縮檔內的圖片也可取得預覽像素 |
| 圖片資訊 | 顯示原始寬高、檔案大小、色彩深度、色彩格式及可解析的 EXIF |
| 前次修正 | 檔名最多兩行、超出截斷、36 px 列高、16 px 檔名行高，以及欄寬拖拉持續有效 |

### 使用方式

1. 執行新版 `D:\SuperExplorer\target\debug\SuperExplorer.exe`。
2. 在本機資料夾切換成分欄檢視。
3. 點選 zip、7z 或 rar，下一欄會顯示內容。
4. 點選壓縮檔內的資料夾，繼續往下展開。
5. 開啟整合預覽後，點選 txt／md 可讀取原文；點選圖片可查看圖片和資訊。
6. 預覽文字與圖片資訊區可垂直捲動，欄間拖拉仍可調整寬度。

## 2. 壓縮檔導覽的處理

### 路徑與服務

分欄使用可重建的複合路徑，例如：

```text
C:\archives\bundle.7z
C:\archives\bundle.7z\docs
C:\archives\bundle.7z\docs\readme.md
```

這些成員路徑由壓縮檔服務解讀。服務找出實際存在的壓縮檔，將後面的路徑作為包內成員名稱。

- 壓縮檔本體必須是實際檔案。名稱以 `.zip` 結尾的普通資料夾保留一般資料夾行為。
- 處理 `Navigate`、`Refresh`、`EnumerateColumn` 與成員 `OpenItem`。
- 導覽仍使用 `LocationResolved`、`DirectoryBatch`、`DirectoryFinished`。
- 祖先欄使用帶有分支版本的 `ColumnDirectoryBatch` 與 `ColumnDirectoryFinished`。
- `RequestContext.archive_browsing` 明確指定分欄讀取路由；其他檢視保留原有的原生 ZIP 與 7z 擴充提供者。
- 請求與事件持續以 tab、generation、request id 和 branch revision 驗證；路由旗標不改變請求身分。
- 包內項目使用獨立的提供者身分；壓縮檔檔案與包內資料夾會避開只適用真正磁碟目錄的循環解析。

### 讀取後端

優先使用程式旁或標準安裝位置的 7-Zip；可使用 Windows 內建 tar 作為替代讀取器。

此次在本機以兩個後端分別驗證 zip、7z、RAR 與 RAR5。中文檔名、含空白檔名、空資料夾及省略父目錄記錄的壓縮檔也有驗證。

背景程序使用 `Command` 的獨立參數，並設定隱藏視窗。瀏覽和預覽透過 stdout 讀取，沒有整包解壓到磁碟的步驟。

### 界線與上限

- 分欄內採唯讀瀏覽。包內修改、刪除、貼上和拖入操作會受到限制。
- 列舉上限為 50,000 筆，原始清單輸出上限為 16 MiB。
- 每個背景讀取程序最多執行 30 秒；取消或超額會終止程序。
- 拒絕絕對路徑、磁碟／串流冒號、`..`、控制字元和重複成員路徑。
- 略過符號連結及硬連結。RAR5 的空白連結屬性不會誤判成真正的連結。
- 成員雙擊開啟可使用受限的暫存檔；單一檔案最多 64 MiB、最多保留 32 個暫存檔、總量 128 MiB，由服務持有其生命週期。
- 包內再包含壓縮檔時，目前可作為成員檔案開啟，尚未遞迴展開成第二個壓縮容器。
- 分欄流程尚未加入壓縮檔密碼輸入；加密、損壞、讀取器不支援或超額時會呈現失敗狀態。
- Windows tar 的檔名輸出使用系統 ANSI 編碼；7-Zip 使用 UTF-8 輸出，可處理較完整的 Unicode 檔名。

## 3. txt／md 純文字預覽

- 支援 `.txt` 和 `.md`，副檔名大小寫不影響判斷。
- 支援 UTF-8、UTF-8 BOM、帶 BOM 的 UTF-16 LE／BE，以及 Windows 系統 ANSI 編碼。
- 最多讀取約 16 KiB 的來源前綴，再保留前 3000 個 Unicode 字元；換行也算字元。
- 中文及 Emoji 使用完整 Unicode 字元截斷，不會截到 UTF-8 的半個字元。
- md 的內容直接作為文字顯示。
- 大檔案顯示「僅顯示前 3000 字」；空檔案顯示空白檔案狀態。
- 二進位 NUL、編碼異常、權限錯誤及讀取失敗有明確的預覽錯誤。
- 文字區可垂直捲動，正常垂直滾輪不會拿來移動整條分欄。

文字預覽使用內建讀取流程。圖片與文字資訊背景載入最多兩個工作；切換選取、關閉預覽、換 tab 或換檢視會取消原有工作。更新同一檔案的 generation 會重新讀取。

## 4. 圖片資訊

目前整合圖片路由涵蓋 JPEG、PNG、BMP、GIF、WebP、TIFF。

| 資訊 | 來源／規則 |
| --- | --- |
| 長寬 | 原始圖片標頭的像素尺寸 |
| 大小 | 本機檔案長度；包內圖片使用成員的未壓縮大小 |
| 色彩深度 | 原始色彩資訊，單位為 bits/pixel |
| 色彩格式 | 例如 RGB8、RGBA16、Indexed8 |
| EXIF | 可解析的相機、時間、方向、曝光、鏡頭、GPS 等原始欄位；值包含可用單位 |

- PNG 的灰階／索引色深度、GIF 色盤深度與 BMP 色深直接由來源標頭補正，避免把解碼後的 RGBA 縮圖格式當成原始色深。
- 16-bit RGBA PNG 可正確顯示 64 bits/pixel。
- 預覽資訊區可捲動；無 EXIF 或 EXIF 異常會顯示對應說明。
- 資訊讀取最多 4 MiB；EXIF 最多呈現 256 個欄位、每個值最多 1024 字。
- 壓縮檔圖片像素來源最多 32 MiB，解碼配置維持 128 MiB 上限及 32768 px 尺寸限制。
- 圖片縮圖仍會依 EXIF 方向調整並保持比例。

## 5. 主要程式修改

| 檔案 | 責任 |
| --- | --- |
| `crates/explorer-common/src/archive.rs` | 壓縮檔路徑解析、後端選擇、清單解析、子目錄建構、限額讀取、取消與逾時 |
| `crates/explorer-app/src/archive_service.rs` | 分欄列舉／導覽事件、唯讀操作限制、成員暫存開啟 |
| `crates/explorer-app/src/brokered_service.rs` | 壓縮檔服務路由、包內圖片縮圖解碼 |
| `crates/explorer-model/src/domain.rs` | 分欄壓縮檔路由旗標 |
| `crates/explorer-model/src/navigation.rs` | 壓縮檔項目 metadata |
| `crates/explorer-model/src/column_view.rs` | 本機壓縮檔分欄資格與循環判斷 |
| `crates/explorer-ui/src/preview_content.rs` | 純文字前綴讀取、圖片標頭與 EXIF、背景載入控制 |
| `crates/explorer-ui/src/column_view.rs` | 文字預覽區、圖片資訊區、捲動與預覽標題界線 |
| `crates/explorer-ui/src/lib.rs` | 選取同步、取消、背景結果更新與服務路由 |
| `crates/explorer-ui/src/state.rs` | 壓縮檔分欄導覽、metadata 保留及禁止包內貼上／拖入 |
| `crates/explorer-ui/src/chrome.rs` | 預覽內容傳遞與回歸測試設定 |

另外補齊 Shell metadata 初始化的新欄位。較廣的回歸測試發現 Details 寬度測試仍假設預設只有四欄；現有預設還包含檔案／資料夾計數。該測試已明確設定四欄範圍，維持其 1900 px 總寬和 8 px inset 驗證，沒有修改正式寬度計算。

## 6. 驗證證據

已通過：

- `cargo test -p explorer-model --lib`：193 項。
- `cargo test -p explorer-ui --lib column_ -- --nocapture`：82 項。
- `cargo test -p explorer-ui --lib preview -- --nocapture`：28 項。
- `cargo test -p explorer-common archive -- --nocapture`：4 項，包含真正 zip、7z、RAR、RAR5 讀取，且每種以 7-Zip 與 Windows tar 驗證。
- `cargo test -p explorer-app --lib archive -- --nocapture`：5 項，涵蓋實際 RAR 導覽／子目錄、空資料夾、唯讀阻擋、其他檢視路由、包內圖片縮圖與取消。
- GPUI 真正渲染測試：文字區內容高於可視區時維持捲動界線，圖片資訊區維持界線，切換圖片後清除舊文字；純文字不建立原生 handler 區。
- 文字測試：3000 Unicode 字元、Emoji、UTF-16 LE／BE、空檔案、二進位內容、快速選取、重新整理、關閉預覽及包內 md。
- 圖片測試：原始尺寸、16-bit RGBA PNG、來源色盤／灰階／BMP 色深、JPEG EXIF 相機欄位、包內 PNG。
- 前次兩行截斷與跨重繪欄寬拖拉測試。

測試數量有交集，不能將各篩選直接相加當成不重複總數。

紀錄位於 `target`：

```text
column-archive-common-tests-20260929.txt
column-archive-service-tests-20260929.txt
column-archive-preview-model-tests-20260929.txt
column-archive-preview-column-tests-20260929.txt
column-archive-preview-preview-tests-20260929.txt
column-preview-content-tests-20260929.txt
column-preview-native-render-20260929.txt
column-archive-preview-check-20260929.txt
column-archive-preview-build-20260929.txt
column-archive-preview-diff-check-20260929.txt
column-archive-preview-fmt-20260929.txt
column-archive-preview-build-metadata-20260929.json
```

測試覆蓋 GPUI 渲染、狀態機與實際壓縮檔服務讀取。本次沒有自動操作目前正在執行的安裝版視窗；現有安裝版程序仍使用 `C:\Program Files\SuperExplorer\SuperExplorer.exe`。

編譯與靜態檢查已通過：`cargo check --workspace`、`cargo build -p explorer-app --bin SuperExplorer`、本次修改的 Rust 檔案 rustfmt 檢查，以及 `git diff --check`。應用程式測試編譯仍有既有的 6 個 qualification 警告。

## 7. 新版執行檔

輸出位置：`D:\SuperExplorer\target\debug\SuperExplorer.exe`。

這是本次原始碼的 debug 版本。關閉舊版後從上述路徑開啟，便可驗收新功能。

建議手動驗收案例：

1. 分別用 zip、7z、rar 點進兩層資料夾，使用上一頁及上一層，再切換同層項目。
2. 在壓縮檔內點選超過 3000 字的 md，確認原文、上限提示及垂直捲動。
3. 點選含 EXIF 的 JPEG 與無 EXIF 的 PNG，確認圖片比例及資訊。
4. 快速切換 txt、md、圖片與資料夾，確認內容對應最新選取。
5. 拖窄與拖寬欄位，確認長檔名維持兩行且沒有重疊。
