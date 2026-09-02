# Transient 傳輸視窗與 ADB 原生進度設計

日期：2026-09-02

## 目標

修正傳輸中心被主視窗內容遮擋、失焦後仍殘留並造成 UI 卡住、取消工作顯示為「部分完成」，以及 ADB push／pull 更新不如 CLI 即時的問題。

## 範圍

- 將右上角傳輸中心由主視窗內 deferred overlay 改成 transient owned tool window。
- 修正傳輸工作的取消 terminal 語意與遲到事件處理。
- 讓 ADB push／pull 直接以 PTY 原生 CLI 輸出驅動進度，再以 200 ms heartbeat 對 UI 發布最新快照。
- 保持本次程式執行期間的 newest-first 工作紀錄、多工作前景回退、逐工作取消與目的地導向。

不更改 SFTP 傳輸協定、檔案衝突策略或跨程式執行的歷史保存策略。

## 視窗架構

傳輸中心使用獨立 GPUI 工具視窗，owner 為目前 SuperExplorer 主視窗。Windows 原生視窗樣式必須符合：

- 不出現在工作列。
- 不出現在 Alt+Tab。
- 顯示時高於 owner 主視窗的一般內容與選單。
- 不高於登入、刪除確認等 modal 對話框。
- owner 最小化、關閉或切換到其他應用程式時隱藏或關閉。

傳輸按鈕是唯一顯示入口。再次點擊、Escape、或主視窗與工具視窗整個 owned-window 群組失去前景 focus 時隱藏。從主視窗點入工具視窗不算失焦，不得先隱藏再重開。

工具視窗錨定在傳輸按鈕下方並靠右對齊。定位會依目前 monitor work area 修正；右側不足時向左移，下方不足時改在按鈕上方。視窗使用 Fluent surface、圓角、邊框與陰影，內容可捲動。

## 狀態與生命週期

主 UI state 保存工具視窗 handle、目前 owner 與 visible 狀態，但工作資料仍以 `OperationCenterState` 為唯一真實來源。工具視窗只讀取 session records 並送出 typed actions，不建立第二份工作狀態。

顯示流程：

1. 點擊傳輸按鈕。
2. 若工具視窗不存在則建立並綁定 owner；存在則更新錨點。
3. 同步 newest-first 工作快照並顯示。
4. 工作更新時只 invalidates 工具視窗內容，不重新建立 native window。

隱藏流程統一經由一個 idempotent action，避免 root overlay click、focus callback 與 Escape 互相競爭造成卡住。

## 取消語意

使用者按下取消後，工作先進入 cancelling；provider 確認取消後 terminal 必須是 `Cancelled`，顯示「已取消」。取消不是部分成功或一般失敗。

- 取消後到達的 progress callback 全部忽略。
- terminal 事件只能接受一次；`Cancelled` 不得被 Finished、Failed 或部分完成摘要覆寫。
- 多項工作若在取消前已有成功項目，仍顯示「已取消（已完成 X/Y）」；零項成功時只顯示「已取消」。
- 取消按鈕在 cancelling 期間停用，terminal 後改為目的地導向或無操作。

## ADB 原生進度資料流

ADB push／pull 繼續透過 pseudo-terminal 執行，確保 adb 認為自己連到互動式終端並持續輸出原生進度。reader thread 必須即時 drain PTY，解析器以 carriage return、newline 與 ANSI control sequence 為 frame 邊界，支援 percent 與 byte pair 兩種輸出。

資料流：

`adb PTY bytes -> frame parser -> monotonic progress adapter -> latest snapshot -> 200 ms publisher -> OperationRecord/UI`

規則：

- 原生 parser 每取得較新的 bytes 或 percent 就更新 latest snapshot，不在 parser 層節流。
- 200 ms publisher 定時發布當下最新 snapshot；開始、phase 邊界、取消與 terminal 立即發布。
- 若 CLI 一段時間只回 percent，使用已知檔案總長換算 bytes；若有 byte pair 則以 byte pair 為準。
- 倒退、重複、破碎 frame、ANSI 查詢回應不得造成進度倒退或虛構速度。
- PTY reader、publisher 與取消 polling 各自獨立，任何一者不得阻塞其餘路徑。

## 錯誤處理

- 工具視窗建立失敗時記錄具體錯誤，傳輸本身繼續，底部 foreground 狀態仍可使用。
- owner 消失時安全關閉工具視窗，不保留失效 handle。
- ADB PTY 無法建立時可回退 pipe runner，但必須明確標記 fallback，並維持 200 ms latest-snapshot publisher。
- 取消 provider 失敗時顯示具體失敗原因；只有 provider 確認取消才標為「已取消」。

## 驗證

集中於實作完成後執行：

- 工具視窗 owner、tool-window/no-taskbar 樣式、定位、Escape、toggle 與整個 owner 群組失焦隱藏測試。
- newest-first、多工作回退、取消 terminal 與遲到 progress 隔離測試。
- ADB PTY carriage-return、ANSI、破碎 frame、percent、byte pair、200 ms cadence 與取消測試。
- ADB 與 SFTP 實機大檔案傳輸，對照 200 ms UI 快照、非零速度與立即取消。
- 使用者視角確認工具視窗不被主內容遮擋、不出現在工作列、失焦消失且不造成主 UI 卡住。
- 最後執行 `build_test_install.bat`，核對 release 與 installed SHA-256。

## 完成條件

- 傳輸中心在一般主視窗內容之上，modal 之下，且不出現在工作列或 Alt+Tab。
- 整個 SuperExplorer 視窗群組失焦後傳輸中心可靠隱藏，UI 可繼續操作。
- 取消完成後顯示「已取消」，不再顯示「部分完成 0/Y」。
- ADB UI 以 CLI 原生進度為資料來源，正常 push／pull 期間每 200 ms 顯示最新可用快照，主觀即時性與 SFTP 相當。
- 自動測試、ADB/SFTP 實測、打包、安裝及 hash 檢查全部通過。
