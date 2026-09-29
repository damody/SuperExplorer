# 分欄檔名兩行截斷與欄寬拖曳修正（2026-09-29）

## 結論

本輪由 Codex 接手完成、獨立審查與驗證。分欄檔名最多顯示兩行，超出的內容以省略號呈現；檔案欄右側邊界可拖曳，跨過原分隔線及多次重繪後仍能持續調整，放開後停止。已重建 `D:/SuperExplorer/target/debug/SuperExplorer.exe`。

## 使用者要求與問題

使用者截圖包含無空白的 UUID、APK、影片及圖片長檔名。要求超過兩行時自動截斷，並可拖拉左側檔案欄寬度。

- 舊分隔線每次 render 都重新建立區域拖曳狀態；按下後的 action 會觸發重繪，使拖曳起點遺失。
- 舊程式只在 4px 分隔線處理移動／放開，滑鼠跨到別欄時無法可靠持續調整。
- 上一輪已將共享列高修正為 36 logical px，檔名行高為 16px；先前的驗收只量測英文句子的文字框，缺少連續長檔名實際省略號的證據。

## 完成的行為

1. 一行可放下的檔名保持完整；兩行可放下時換行；超過兩行時第二行末端顯示 `…`。
2. 每列固定 36px，文字最多兩個 16px 行高；文字不侵入下一列。圖示、資料夾箭頭及單行重新命名保持列內對齊。
3. 以滑鼠左鍵拖曳檔案欄右側邊界，修改該欄的寬度，範圍沿用 180–480 logical px。命中區至少 6px，有縮放游標與 hover／active 顏色。
4. 拖曳起點、原寬度、欄索引、分頁 ID 及分支 revision 保存在狀態中。全視窗 capture 接收移動／放開，跨過分隔線及重繪後仍持續調整。
5. 放開、收到未按鍵移動、切換分頁／檢視、關閉分頁／視窗、原分支失效或視窗失去啟用時結束。`Esc` 也可結束目前拖曳，保留最後寬度。
6. 調寬／調窄後重新依可用寬度換行和截斷；每欄寬度寫入既有 `column_widths` 設定，第一欄同步舊 `column_width`。
7. 檔案模型保留完整名稱，重新命名和檔案操作不使用畫面上的省略文字；閒置 capture 不攔截普通點擊或 OLE drop 的放開事件。

以上尺寸是邏輯像素，實際顯示依 Windows DPI 比例放大。

## 實作檔案

| 檔案 | 修改 |
| --- | --- |
| `crates/explorer-ui/src/column_view.rs` | 分隔線 dispatch Begin action；移除區域拖曳狀態；擴大命中區與提示。檔名使用 GPUI 原生 `StyledText` 及既有兩行 clamp／ellipsis；補實際排版和拖曳事件測試。 |
| `crates/explorer-ui/src/state.rs` | 持續保存欄寬拖曳 session、有限數值／欄索引／分頁／分支守衛、寬度正規化、結束與失效取消；補單欄、上下限、設定及生命週期回歸測試。 |
| `crates/explorer-ui/src/actions.rs` | Begin／Update／End 欄寬 action 及頻繁指標事件分類。 |
| `crates/explorer-ui/src/chrome.rs` | 將欄寬拖曳接到既有全視窗 capture；active 拖曳可接收移動和放開，idle 事件繼續傳遞。 |
| `crates/explorer-ui/src/lib.rs` | 接入 action dispatch、分頁／檢視／視窗失效處理、視窗停用及 Esc 結束。 |

先前已接受的 `crates/explorer-model/src/column_view.rs` 共享列高修正保留。使用者既有 `chrome.rs` 書籤 fallback／Shell 圖示修改保留；本輪相對起始版本的 chrome diff 只包含 capture 路徑與對應測試。

## Codex 接手與審查

原 Grok 作業 `run-mum33akr-c1dq2b`，thread `b0c50ec3-d5b3-45bc-8512-088bdd1c712a`。作業進行 24 分 22 秒後由 Codex 停止並接手；未作為完成結果接受。Bridge 的 cancelled 事件使用通用訊息 `Stopped by user`，本次實際由 Codex 發出停止指令。停止後確認追蹤的程序已不存在。沒有可用的終止 token／費用數據，不能推定費用為零。

Codex 接手後：

- 移除 Grok 的自製文字量測、截斷 fallback、繪製及正式版記錄機制，改回原生文字元件。
- 修正 callback 被移走後仍使用的問題，以及未完成狀態測試的借用衝突。
- 完成真實 GPUI 欄界 mouse down／move／up 與原生文字排版測試，補 Esc 結束。
- 修正驗收程式與格式，消除新增的編譯警告，獨立跑完整本輪驗證。

依使用者最新要求，後續本項工作由 Codex 自己實作，不再委派 Grok。

## 獨立驗證與證據

| 驗證 | 結果 | 證據 |
| --- | --- | --- |
| 原生排版與跨欄拖曳整合測試 | PASS，完整分欄套件亦再次執行此測試 | `target/column-truncate-resize-render-codex-20260929.txt`、下列完整分欄 log |
| `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8` | 模型 25、UI 41 PASS，共 66 項 | `target/column-truncate-resize-column-tests-codex-20260929.txt` |
| `idle_pointer_capture_leaves_external_ole_mouse_up_for_drop_target` | 1 PASS | `target/column-truncate-resize-ole-codex-20260929.txt` |
| `cargo check --workspace --locked --offline` | PASS，最終執行沒有新增警告 | `target/column-truncate-resize-check-codex-20260929.txt` |
| 涉及的六個 Rust 檔案 `rustfmt --edition 2024 --check` | PASS | `target/column-truncate-resize-fmt-codex-20260929.txt` |
| `git diff --check` | PASS | `target/column-truncate-resize-diff-check-codex-20260929.txt` |
| `cargo build -p explorer-app --locked --offline` | PASS | `target/column-truncate-resize-build-codex-20260929.txt` |

共 67 項相關測試通過。

### 實際排版／事件證據

`column_view_long_names_truncate_and_divider_drag_survives_rerenders` 渲染真實 `ColumnStrip`、檔案列及分隔線，使用正式的 `pointer_drag_capture_listener` 和持續保存的 AppViewState 欄寬狀態。測試沒有直接送 SetColumnWidth 來代替拖曳。

- 測試短檔名、連續 APK 長字串、UUID 圖片名稱及足夠長的中文名稱。
- 讀取原生 `TextLayout.text()`、`wrapped_text()`、bounds 及省略號的 shaped position；確認超長名稱真的以 `…` 結尾，最多兩行，省略號位置位於文字框內，短名稱保持原文。
- 外層刻意使用 22px 行高，確認檔名仍採自己的 16px 行高。
- 在真實欄界按下，重繪，向左移動 40px：240 → 200；再次重繪後跨到別欄向右移動：→ 420。
- 確認加寬後能顯示更多字，所有列仍為 36px。
- 在欄外放開，重繪，再無按鍵移動，確認寬度保持 420 且 session 已結束。

`column_view_width_drag_clamps_one_column_and_cancels_when_stale` 驗證只修改指定欄、180／480 上下限、設定同步、非有限座標、切換檢視、切換／新增／關閉分頁、開啟子資料夾造成分支失效及另一個 resize 接手。

這是 GPUI 測試視窗的排版與合成事件驗證，文字度量來自測試平台；未將它當成 Windows 桌面像素截圖或系統 SendInput 驗收。

## 版本與啟動

本輪確定重建並驗證的檔案：

- 路徑：`D:/SuperExplorer/target/debug/SuperExplorer.exe`
- 修改時間：2026-09-29 11:32:00（Asia/Taipei）
- SHA-256：`B3C0BF0A6F88F3B838501A039898345A02F57A8D3417C6C0375C24C23981939C`
- 執行檔 metadata：`target/column-truncate-resize-build-metadata-codex-20260929.json`

要使用這份新編譯版，先關閉目前的 SuperExplorer，再執行上述檔案。

本輪開始時，已安裝版是 10:23:16 的檔案；收尾時發現 `C:/Program Files/SuperExplorer/SuperExplorer.exe` 也已更新為 11:28:43，且有新的執行程序。本輪沒有執行安裝或替換該檔案，不能只由時間戳確認它包含所有最後修改。起始與收尾 metadata 已保存；本文件的驗收對應上面列出的 debug 執行檔。

## 工作區狀態與界限

基準及收尾皆為 `master` / `ce466cf5b998071d5cce1554d4e6913c730d3b6f`，沒有 commit、push 或切換分支。起始檔案副本位於 `target/column-truncate-resize-baseline-20260929/`；chrome 保留證據為 `target/column-truncate-resize-chrome-preservation-codex-20260929.diff`。

本輪接受的是檔名截斷與欄寬拖曳修正；先前功能文件中另行記錄的 PDF Handler、UIA 語意與其他整體驗收項目不因本輪測試而變成完成。
