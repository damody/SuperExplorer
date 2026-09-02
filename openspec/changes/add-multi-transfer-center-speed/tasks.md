# 實作計畫

## 1. 操作模型與速度

### 1.1 穩定的多工作狀態

**目的：** 讓模型能以明確順序回答活動數、底部前景與面板清單。
**輸入：** 核准 design 與 `explorer-model` 現有 OperationCenterState。
**產出：** 穩定順序與語意查詢及模型測試。
**依賴：** 無。
**Owner／Wave：** Primary／Wave 1。
**Gate／Evidence：** G-MODEL；`evidence/model-state.json`。
**完成門檻：** 並行、較新先結束、失敗隔離與本次執行順序測試全部通過。

- [x] 1.1.1 為 OperationCenterState 加入穩定插入順序及 newest-first 查詢。
- [x] 1.1.2 實作 active transfer count、foreground fallback 與 Shift+Delete running exclusion。
- [x] 1.1.3 加入多工作排序、較新先終止、失敗／取消隔離及永久刪除終止通知模型測試。

### 1.2 可信的速度樣本

**目的：** 從累積 bytes 與單調時間推導共享的平滑 bytes/second。
**輸入：** 1.1 的 OperationRecord 生命周期。
**產出：** 速度狀態、格式化所需 getter 與決定性測試。
**依賴：** 1.1。
**Owner／Wave：** Primary／Wave 1。
**Gate／Evidence：** G-SPEED；`evidence/model-speed.json`。
**完成門檻：** 第一筆、有效增量、零增量、EMA、倒退及終止後遲到情境通過。

- [x] 1.2.1 在 OperationRecord 加入時間樣本與 EMA 速度更新邏輯。
- [x] 1.2.2 保證 preparing、零增量、倒退與遲到事件不虛構新速度。
- [x] 1.2.3 加入可控時間的速度計算、平滑與終止不變性測試。

## 2. Provider 進度節奏

### 2.1 通用遠端 reporter

**目的：** 統一 ADB／SFTP transfer reporter 的 200 ms 一般更新與立即邊界事件。
**輸入：** 現有 TransferProgressReporter 與 cancellation token。
**產出：** 節流實作及 cadence/cancel 測試。
**依賴：** 1.2。
**Owner／Wave：** Primary／Wave 2。
**Gate／Evidence：** G-REMOTE-CADENCE；`evidence/remote-cadence.json`。
**完成門檻：** 200 ms 節流、強制事件、取消及 late callback 測試通過。

- [x] 2.1.1 校正通用 reporter 的 200 ms 發布、強制邊界與最後快照語意。
- [x] 2.1.2 保證取消後拒絕遲到 callback 且不刪除 Move 來源。
- [x] 2.1.3 加入 SFTP／通用 reporter cadence、邊界與取消測試。

### 2.2 ADB 原生進度與保活

**目的：** 讓長時間 adb push/pull 的最新已知進度每 200 ms 可進入 UI。
**輸入：** adb runner stdout/stderr parser、2.1 reporter。
**產出：** 非阻塞 drain、200 ms tick 與 parser/runner 測試。
**依賴：** 2.1。
**Owner／Wave：** Primary／Wave 2。
**Gate／Evidence：** G-ADB-200MS；`evidence/adb-cadence.json`。
**完成門檻：** 稀疏輸出不虛構 bytes、活動 tick 與取消立即終止測試通過。

- [x] 2.2.1 追蹤 adb 原生輸出的最新單調 bytes／percent 快照。
- [x] 2.2.2 加入不阻塞 pipe drain 的 200 ms tick 並保持取消／終止立即送達。
- [x] 2.2.3 加入 adb 密集、稀疏、無新 bytes、取消與 terminal cadence 測試。

## 3. 混合式傳輸 UI

### 3.1 底部單一前景摘要

**目的：** 顯示速度、只占一列並在較新工作終止後回退。
**輸入：** 1.x 模型查詢與現有 OperationCenter。
**產出：** 新前景選擇、速度文字與 Shift+Delete visibility。
**依賴：** 1.1、1.2。
**Owner／Wave：** Primary／Wave 3。
**Gate／Evidence：** G-BOTTOM；`evidence/bottom-summary.json`。
**完成門檻：** 單列、回退、八秒淡出、速度及永久刪除延後顯示測試通過。

- [x] 3.1.1 將 OperationCenter 改用 foreground record 並顯示格式化每秒速度。
- [x] 3.1.2 隱藏 Shift+Delete queued/running 底部內容並只呈現終止結果。
- [x] 3.1.3 加入多工作回退、速度文字、淡出與 Shift+Delete render 測試。

### 3.2 右上工具列按鈕與 Fluent 面板

**目的：** 提供 Firefox 式本次執行期間多工作入口與詳細控制。
**輸入：** 1.x 模型、現有工具列／overlay patterns、typed actions。
**產出：** 按鈕、徽章、scroll panel、row actions 與 UI state。
**依賴：** 3.1。
**Owner／Wave：** Primary／Wave 3。
**Gate／Evidence：** G-PANEL；`evidence/transfer-panel.json`。
**完成門檻：** newest-first、活動數、逐工作取消、導向、Escape／外部關閉及無跨重啟資料測試通過。

- [x] 3.2.1 加入 transfer panel 開關狀態、actions、Escape 與外部點擊關閉。
- [x] 3.2.2 在右上工具列加入 Fluent transfer button 與活動數 accent badge。
- [x] 3.2.3 實作可捲動 newest-first 工作列、進度／速度／錯誤及逐工作取消。
- [x] 3.2.4 實作 terminal local/ADB/SFTP typed destination navigation。
- [x] 3.2.5 加入面板結構、排序、徽章、取消、導向與 dismissal 測試。

## 4. 最終整合與使用者驗證

### 4.1 集中自動檢查

**目的：** 在所有實作完成後一次驗證格式、模型、UI、provider 與規格。
**輸入：** 1 至 3 全部產出。
**產出：** 命令結果與差異檢查 evidence。
**依賴：** 2.2、3.2。
**Owner／Wave：** Primary／Wave 4。
**Gate／Evidence：** G-AUTO；`evidence/final-automated.json`。
**完成門檻：** 所列命令 exit 0，失敗必須補修並重跑。

- [x] 4.1.1 執行 cargo fmt check、explorer-model／app／remote／ui focused tests。
- [x] 4.1.2 執行 explorer-app cargo check、OpenSpec strict validate、placeholder 與 git diff check。
- [x] 4.1.3 將每個自動檢查的命令、exit status、結果摘要、時間與 task ID 寫入 evidence index。

### 4.2 正式打包與使用者視角

**目的：** 驗證實際安裝版本的 ADB、SFTP、多工作面板及 Shift+Delete 行為。
**輸入：** 通過 4.1 的 workspace、可用 emulator 與已保存 SFTP profile。
**產出：** 安裝包、hash、報告、screenshots 與 final validation。
**依賴：** 4.1。
**Owner／Wave：** Primary／Wave 4。
**Gate／Evidence：** G-USER、G-PACKAGE；`evidence/user-perspective/`、`evidence/final-validation.md`。
**完成門檻：** build/install/hash 一致，ADB／SFTP 速度與取消、多工作較新先完成回退、面板、Shift+Delete 隱藏／終止顯示全部通過；失敗持續補修。

- [x] 4.2.1 透過 build_test_install.bat 建置、安裝並驗證 release／installed hashes。
- [x] 4.2.2 實測 ADB 大檔案至少兩個 200 ms 間隔更新、非零速度與立即取消。
- [x] 4.2.3 實測 SFTP 大檔案至少兩個 200 ms 間隔更新、非零速度與立即取消。
- [x] 4.2.4 同時啟動兩筆傳輸並驗證較新工作先終止後底部回退、徽章數與面板 newest-first。
- [x] 4.2.5 實測 Shift+Delete 執行中底部隱藏、終止後顯示完整結果與八秒淡出。
- [x] 4.2.6 記錄 screenshots、報告、hash、所有 task evidence 與最終使用者視角結論。
