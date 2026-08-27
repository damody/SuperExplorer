# 檔案操作訊息詳細資訊與淡出設計

## 目標

SuperExplorer 的操作訊息欄不再只顯示籠統的 `File operation completed`，而是顯示足以辨識操作內容的摘要，包含操作類型、來源、目的地、項目數、目標路徑與結果。進行中的操作持續可見；終止訊息顯示八秒，最後一秒淡出，完成後移除訊息欄並釋放其高度。

## 顯示內容

訊息依 `FileOperationKind` 產生，不直接拼接未受控的除錯文字：

- 複製／移動：操作名稱、項目數、來源摘要與目的地完整路徑。
- 新增資料夾／新增項目：操作名稱與建立後的完整目標路徑。
- 重新命名：舊路徑與新名稱；能安全組合時顯示完整新路徑。
- 刪除／永久刪除：操作名稱、項目數與來源路徑摘要。
- 建立捷徑：操作名稱、項目數與來源路徑摘要。
- 進行中：上述操作摘要加上完成數／總數。
- 成功：上述操作摘要加上「完成」。
- 取消：上述操作摘要加上「已取消」。
- 部分成功：顯示成功數／總數，並列出最多五筆失敗或略過結果。
- 失敗：顯示操作摘要、使用者可理解的錯誤原因，以及必要的重試提示。

多項來源以第一個完整路徑加上「另有 N 個項目」摘要，避免單列被大量路徑撐高。Local 路徑使用 Windows 顯示形式；ADB 與 SFTP 使用 canonical URI。SFTP 使用已去除密碼與其他敏感驗證資料的 location descriptor，不顯示登入密碼。

## 生命週期與淡出

State 在收到最新 operation terminal event 時記錄該 request identity 與單調時間。新操作開始或另一筆操作成為最新項目時，舊訊息立即被取代並重新依新操作計時。

- 非終止操作：持續顯示，opacity 為 1。
- 終止後 0 至 7 秒：完整顯示，opacity 為 1。
- 終止後 7 至 8 秒：opacity 由 1 線性降至 0。
- 終止滿 8 秒：不渲染操作訊息欄，並釋放其版面高度。

淡出由視窗動畫 frame 驅動，只在最後一秒要求後續 frame。滑鼠 hover 不暫停倒數。若視窗暫停繪製，重新繪製時以單調時間直接計算正確狀態，不累積或延長八秒期限。

## 元件邊界

- `explorer-model` 的 operation record 保留純資料與既有終止結果，不加入 UI 時鐘。
- `explorer-ui::state` 管理最新終止通知的 request identity 與時間，並拒絕 stale event 改寫顯示期限。
- `OperationCenter` 負責將 typed request 格式化成摘要、計算 opacity 與條件渲染。
- 路徑格式化使用獨立純函式，讓 Local、ADB、SFTP 與各種 operation kind 可做聚焦測試。

## 錯誤與邊界處理

- 無法取得來源葉節點時顯示完整 location，而不是退回籠統的 `item`。
- 無法安全推導 rename 的新完整路徑時，顯示原始完整路徑與新名稱，不猜測 provider 結構。
- 同時存在執行中與剛完成操作時，最新 operation record 仍是訊息欄唯一來源；不建立通知堆疊。
- operation center 沒有 record、終止訊息超過八秒，或 terminal timestamp 與 latest request 不相符時，不渲染訊息欄。
- 部分成功明細最多五筆，保留目前防止訊息欄無限制增高的界線。

## 驗證

只執行本功能相關檢查，不做完整迴歸：

1. 純函式測試涵蓋 Create、Rename、Copy、Move、Delete 的 Local／ADB／SFTP 摘要。
2. 時間測試涵蓋進行中、7 秒前、7 至 8 秒 opacity、8 秒後隱藏，以及新操作重設計時。
3. 渲染測試確認終止後欄位存在、淡出後完全移除。
4. 執行 `cargo fmt --check`、`cargo check -p explorer-ui`、聚焦測試、`git diff --check` 與嚴格 OpenSpec validation。
5. 真實視窗執行一筆檔案操作，擷取詳細訊息與八秒後欄位消失的證據。
