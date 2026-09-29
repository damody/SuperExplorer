# 分欄檔名兩行間距修正（2026-09-29）

## 問題與原因

使用者截圖中，PDF 檔名的第二行文字與下一個檔案的第一行擠在一起。原因是分欄允許文字換行，但共享 `COLUMN_ROW_HEIGHT` 仍為 24 logical px；檔名繼承的行高是 16 logical px，兩行需要 32 px，超過每列配置的高度。只調整文字行距無法同步修復列邊界、選取區域及虛擬捲動。

## 本輪修正契約

- 保留最多兩行檔名，行高 16 px、字級 12 px，超出的內容以省略號呈現。
- 共享列高改為 36 px，容納兩行及合計 4 px 的垂直空間；每列不可被 flex 壓縮。
- 同一檔案的兩行留在該列內，不侵入下一個檔案的範圍；圖示、箭頭及重新命名編輯器維持列內對齊。
- 虛擬列配置、空白跨度、捲動上限、鍵盤 reveal 及定位沿用同一共享常數。
- 不做變動列高或全目錄文字量測，保留大型目錄的有界可見列建構。

以上是邏輯像素，Windows 會依 DPI 比例放大。

## 委派與驗收

Grok 作業：`run-mum2bl9o-ffzrq8`；thread：`b0c50ec3-d5b3-45bc-8512-088bdd1c712a`；使用 `grok-4.7-build-fast`。Grok 負責兩個分欄原始碼檔案及必要的定向測試，Codex 負責獨立審查與驗證。

狀態：Grok 完成，Codex 已獨立審查及驗證接受。基準 `master`／`ce466cf5b998071d5cce1554d4e6913c730d3b6f`，分支與 HEAD 保持一致；使用者在 `chrome.rs` 的書籤圖示修改保留。

### 實際修改

| 檔案 | 修改 |
| --- | --- |
| `crates/explorer-model/src/column_view.rs` | 共享列高 24 → 36 px；原有捲動上限及 reveal 測試按共享列高驗證。 |
| `crates/explorer-ui/src/column_view.rs` | 列的 min／max height 同為 36，禁止 flex shrink，裁切列外內容；檔名 min-width 為 0，明確 16px 行高、兩行 clamp／ellipsis、最大文字高度 32px。單行 rename 保持在列內；補上真實 GPUI 排版回歸測試及量測 selector。 |

### 獨立驗證

| 驗證 | 結果 | 證據 |
| --- | --- | --- |
| `wrapped_column_filenames_stay_inside_adjacent_rows` | 1 PASS | `target/column-label-render-codex-20260929.txt` |
| `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8` | 模型 25、UI 39 PASS（含上述排版測試） | `target/column-label-column-tests-codex-20260929.txt` |
| `cargo check --workspace --locked --offline` | PASS | `target/column-label-check-codex-20260929.txt` |
| 修改的兩個檔案 `rustfmt --edition 2024 --check` | PASS | `target/column-label-fmt-codex-20260929.txt` |
| `git diff --check` | PASS | `target/column-label-diff-check-codex-20260929.txt` |
| `cargo build -p explorer-app --locked --offline` | PASS | `target/column-label-build-codex-20260929.txt` |

排版測試使用 180px 欄寬、200px viewport、12 個實際 row model，讓內容超出 viewport，以驗證列不會被壓縮。外層刻意設不同的 22px 行高，確定檔名自己的 16px 行高生效。測試量測實際 GPUI bounds，確認：

- 短檔名的文字盒為 16px；長 PDF 檔名實際換行，文字盒不超過 32px。
- 每列高 36px，相鄰列從下一個固定邊界開始；文字盒完整留在所屬列內。
- 文字、圖示及資料夾箭頭垂直置中。
- 單行重新命名欄位及文字保持在該列內。

這項排版證據來自 GPUI 測試視窗的量測。本輪已安裝程式的桌面畫面未重驗；重建的版本位於 `D:/SuperExplorer/target/debug/SuperExplorer.exe`。

### Grok 作業記錄

作業完成時間 11 分 42 秒。Bridge 回報：輸入 683,497 tokens、快取輸入 6,247,936、輸出 66,264，總計 6,997,697；回報費用 USD 3.32421128。這是本次作業的量測。

## 範圍

本輪只修正分欄檔名與列幾何。之前記錄的 PDF Handler、UIA 停用／勾選語意，以及命令矩陣的桌面前景阻礙仍按原驗收文件管理，不因本輪排版修正而關閉。
