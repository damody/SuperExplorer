## 1. 工具視窗基礎與生命週期

### 1.1 盤點並建立 transient window coordinator

**目的：** 建立單一 owner-aware coordinator，管理工具視窗 handle、anchor、visible 與 idempotent show/hide/close。
**輸入：** 核准 design、既有 secondary window/popup patterns、Explorer 主視窗 composition。
**產出：** coordinator、typed actions、owner lifecycle integration 與 focused tests。
**依賴：** 無。
**Owner／Wave：** Primary／Wave 1。
**Gate／Evidence：** G-WINDOW-LIFECYCLE；`evidence/window-lifecycle.json`。
**完成門檻：** 同一 owner 重複 toggle 不重建窗口；owner close 清除 handle；所有關閉路徑 idempotent 且測試通過。

- [x] 1.1.1 盤點 GPUI secondary-window 與 Windows owner/tool-window API，記錄採用介面及 fallback adapter。
- [x] 1.1.2 實作 transfer tool-window coordinator 與 show、hide、reposition、close typed lifecycle。
- [x] 1.1.3 將 owner close/minimize、Escape 與重複按鈕 toggle 接到統一 lifecycle 並加入測試。

### 1.2 實作層級、focus 與定位

**目的：** 讓工具視窗高於 owner 一般內容、低於 modal、不進工作列/Alt+Tab，並可靠處理 owned-window group focus 與 monitor bounds。
**輸入：** 1.1 coordinator、transfer-button screen bounds、Windows monitor work area。
**產出：** native style/owner composition、focus-group detector、anchor placement 與 evidence probe。
**依賴：** 1.1。
**Owner／Wave：** Primary／Wave 1。
**Gate／Evidence：** G-WINDOW-STYLE、G-FOCUS、G-PLACEMENT；`evidence/window-style-focus-placement.json`。
**完成門檻：** native style/owner probe、主↔工具視窗 focus、外部失焦隱藏與上下展開/邊界 clamp 全部通過。

- [x] 1.2.1 套用 owned tool-window/no-taskbar/no-Alt+Tab style，並保證 modal z-order 高於工具視窗。
- [x] 1.2.2 實作延遲一個 UI turn 的 owner-group focus 判定與外部失焦隱藏。
- [x] 1.2.3 實作 anchor screen-bounds、monitor work-area clamp、上/下展開與 DPI 重新定位。
- [x] 1.2.4 加入 style、focus race、owner 終止與 placement boundary 的自動化 probe/tests。

## 2. 傳輸中心內容與取消 terminal

### 2.1 將 session transfer UI 移至工具視窗

**目的：** 移除 command-bar 內會被裁切的 transfer popup，讓可重用工具視窗直接呈現現有 session records。
**輸入：** 1.x 工具視窗、OperationCenterState、現有 transfer panel rows/actions。
**產出：** 工具視窗 render、live invalidation、按鈕/徽章 integration。
**依賴：** 1.2。
**Owner／Wave：** Primary／Wave 2。
**Gate／Evidence：** G-TRANSFER-UI；`evidence/transfer-tool-window-ui.json`。
**完成門檻：** newest-first、活動數、逐工作取消/導向、session-only 與 live refresh 不重建 native window 的測試通過。

- [x] 2.1.1 抽出可供獨立窗口使用的 transfer-center view model/render，維持 OperationCenterState 單一來源。
- [x] 2.1.2 將 toolbar button/badge 改為 coordinator action並移除舊 deferred popup composition。
- [x] 2.1.3 連接工作更新 invalidation、逐工作取消/導向、newest-first 與 session-only 測試。

### 2.2 正規化取消與遲到事件

**目的：** 使使用者取消成為不可覆寫的 `Cancelled` terminal，並顯示正確的零/部分成功摘要。
**輸入：** 現有 operation reducer、remote cancellation events、transfer display formatter。
**產出：** first-terminal-wins reducer、late-event guards、cancelled formatting/tests。
**依賴：** 無；可與 1.x 並行，2.1 整合時消費。
**Owner／Wave：** Primary／Wave 1。
**Gate／Evidence：** G-CANCEL-TERMINAL；`evidence/cancel-terminal.json`。
**完成門檻：** 零成功顯示「已取消」、部分成功顯示完成數、遲到 progress/terminal 不覆寫且 provider 真正終止。

- [x] 2.2.1 追蹤 cancelling request 並將 provider cancellation 正規化為 typed `Cancelled` terminal。
- [x] 2.2.2 在 reducer/reporter 拒絕 cancelling/terminal 後的遲到 progress 與第二 terminal。
- [x] 2.2.3 更新取消顯示格式與 row actions，加入零成功、部分成功、late callback 與多工作隔離測試。

## 3. ADB 原生 CLI 進度

### 3.1 強化 PTY frame parser 與 monotonic adapter

**目的：** 即時解析 adb push/pull 的 CR/LF、ANSI、fragmented percent 與 byte-pair frame，不等待 child exit。
**輸入：** 現有 `run_adb_in_pty`、AdbProgressParser/Adapter、真實 adb captured frames。
**產出：** non-blocking parser/adapter、fallback diagnostics 與 parser fixtures。
**依賴：** 無。
**Owner／Wave：** Primary／Wave 1。
**Gate／Evidence：** G-ADB-PARSER；`evidence/adb-native-parser.json`。
**完成門檻：** CR、ANSI、chunk fragmentation、percent、byte pair、repeat/regression 與 PTY fallback fixtures 全部通過。

- [x] 3.1.1 擷取並索引目前 adb CLI progress frame 形式，補齊不含秘密的 parser fixtures。
- [x] 3.1.2 實作 ANSI-aware CR/LF fragmented frame drain 與 percent/byte-pair parsing。
- [x] 3.1.3 校正 byte-pair precedence、percent-to-bytes 與 monotonic repeat/regression guards。
- [x] 3.1.4 加入 PTY 建立失敗的明確 diagnostic 與 bounded pipe fallback 測試。

### 3.2 解耦 200 ms publisher、reader 與取消

**目的：** 以 parser 最新快照驅動 200 ms UI publication，同時維持 reader drain 與 child cancellation 即時性。
**輸入：** 3.1 latest snapshot、現有 TransferProgressReporter、CancellationToken。
**產出：** shared snapshot publisher、boundary emission、kill/reap contract 與 cadence tests。
**依賴：** 3.1、2.2 terminal contract。
**Owner／Wave：** Primary／Wave 2。
**Gate／Evidence：** G-ADB-CADENCE、G-ADB-CANCEL；`evidence/adb-cadence-cancel.json`。
**完成門檻：** 正常排程下相鄰發布不超過 200 ms、最新 observation 被 coalesce、reader 不阻塞、取消 kill/reap 並立即 terminal。

- [x] 3.2.1 實作 thread-safe latest native snapshot，parser 更新不做時間節流。
- [x] 3.2.2 將 200 ms publisher 改為讀取最新快照並在 phase/cancel/terminal 邊界立即發布。
- [x] 3.2.3 保證 PTY drain、publisher 與 cancellation polling 獨立，取消時 kill/reap child。
- [x] 3.2.4 加入密集/稀疏/不變輸出 cadence、reader backpressure、取消與 late-output 測試。

## 4. 集中驗證、實機與打包

### 4.1 最終自動檢查

**目的：** 在所有實作完成後集中驗證格式、model、UI、app、ADB provider 與 OpenSpec traceability。
**輸入：** 1 至 3 全部產出。
**產出：** focused test/check 結果與 task evidence index。
**依賴：** 2.1、3.2。
**Owner／Wave：** Primary／Wave 3。
**Gate／Evidence：** G-AUTO；`evidence/final-automated.json`。
**完成門檻：** 所列命令 exit 0、strict validation 通過、每個 L3 有唯一 evidence task_id；失敗持續補修。

- [x] 4.1.1 執行 cargo fmt check、model cancel、window/UI、app reporter 與 ADB parser/cadence focused tests。
- [x] 4.1.2 執行 explorer-app cargo check、OpenSpec strict validate、placeholder、traceability 與 git diff check。
- [x] 4.1.3 寫入每個 task_id 的命令/程序、預期/實際、exit status、hash、gate 與 timestamp evidence。

### 4.2 使用者視角、實機與正式安裝

**目的：** 用正式 release 驗證視窗層級/focus、取消文字、ADB/SFTP 即時性與安裝一致性。
**輸入：** 通過 4.1 的 workspace、emulator、已保存 SFTP profile、明確測試目的地。
**產出：** screenshots、native style/focus probes、ADB/SFTP reports、installer/hash/final validation。
**依賴：** 4.1。
**Owner／Wave：** Primary／Wave 4。
**Gate／Evidence：** G-USER、G-REAL-ADB、G-REAL-SFTP、G-PACKAGE；`evidence/user-perspective/`、`evidence/final-validation.md`。
**完成門檻：** 工具視窗不被遮擋/不進 taskbar/Alt+Tab/失焦隱藏，取消顯示已取消且真正停止，ADB/SFTP 200 ms與速度實測通過，release/installed hash 一致。

- [x] 4.2.1 以 headful probe 驗證 tool-window owner/style、modal z-order、定位、toggle/Escape 與外部失焦隱藏。
- [x] 4.2.2 以真實 ADB push/pull 驗證原生快照至少兩個 200 ms 間隔、非零速度、立即取消、已取消 terminal 與 eventual cleanup。
- [x] 4.2.3 以真實 SFTP 傳輸驗證相同 200 ms/速度/取消顯示，確認共用 UI 未退化。
- [x] 4.2.4 驗證多工作 newest-first、badge、較新 terminal 後 foreground 回退及工具視窗 live refresh。
- [x] 4.2.5 執行 `build_test_install.bat`，核對 installer、release 與 installed SHA-256 並重跑安裝版使用者視角 smoke。
- [x] 4.2.6 清理明確測試檔、索引 screenshots/reports/hashes，完成 final validation 並確認所有 blocking gate 通過。
