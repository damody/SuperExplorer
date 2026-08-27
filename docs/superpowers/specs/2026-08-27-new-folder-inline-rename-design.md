# 新增資料夾暫存重新命名設計

## 目標

SuperExplorer 在 Local、ADB 與 SFTP 執行「新增資料夾」時，先顯示尚未落地的資料夾列並進入 rename；只有名稱確認成功後才真正建立。

## 決策與資料流

1. 由目前 snapshot 產生不衝突的 `New folder` 或編號名稱。
2. 建立綁定目前 tab、generation 與 parent 的 provisional identity，不提交 provider 命令。
3. FileView 將暫存列納入顯示及虛擬化 item count，以既有 editor 全選名稱。
4. Enter 或失焦時驗證名稱與碰撞；有效才轉成 Folder `CreateItem` request。
5. Esc、導航或 context 失效時只移除草稿，不建立實體項目。
6. 建立失敗沿用既有 operation error，不留下舊名或半成品。

## 隔離

既有 F2／滑鼠 rename 仍產生 Rename request；批次或擴充套件建立不進 provisional 流程。Local、ADB 與 SFTP 共用 UI 狀態機。

## 驗證

聚焦測試確認預設命名、暫存列、Esc 取消、有效 commit request 與錯誤保留。真實視窗測試證明 Local 在 editor 開啟時路徑不存在、Enter 後才出現；ADB 與 SFTP 確認相同流程可建立並安全刪除。

