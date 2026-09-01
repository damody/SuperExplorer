# ADB 原生進度串流設計

## 目標

將 ADB upload/download 的進度來源從目的端 `stat` 與本機遞迴掃描，改為直接解析 `adb push`／`adb pull` 的增量輸出。進度必須單調、可取消、不洩漏路徑或憑證，並沿用既有 Local／SFTP／ADB 共用的 `OperationProgress` 與 terminal 規則。

## 範圍

- 增量讀取 ADB stdout 與 stderr，不等待程序結束才收集完整輸出。
- 支援以 `\r` 覆寫同一行及以 `\n` 結束的 progress frame。
- 從百分比或 transferred/total 欄位產生 delivered-byte delta。
- 已知來源大小時將百分比映射為 byte progress；未知或不可靠時維持 indeterminate。
- 移除 ADB upload 的週期性 remote `stat` 與 download 的週期性本機 tree scan。
- 保留既有取消、timeout、bounded diagnostic capture、錯誤清理及 terminal barrier。

不在本次範圍：自行實作 ADB sync protocol、傳輸速率／ETA、修改公開 extension ABI。

## 架構

### 串流 runner

`AdbCommandRunner` 增加內部 progress-capable 執行入口。production runner 對 stdout/stderr 各使用一個 bounded reader thread，reader 每次取得 bytes 後立即交給 parser，同時只保留受限大小的診斷輸出。fake runner 可沿用預設實作或注入決定性 frames。

callback 必須輕量且不能阻塞 pipe drain。parser 只產生最新 cumulative observation；上層將它轉為 delta 後交給既有 reporter 節流。

### Frame parser

parser 保存跨 read 邊界的殘留 bytes，以 `\r` 或 `\n` 完成 frame。它接受：

- 百分比，例如 `42%`；
- transferred/total 數值，例如 `44040192/104857600`；
- 同一 frame 中同時存在兩者時優先使用明確 byte pair。

parser 不依賴固定檔名或完整英文句子。無效 UTF-8、溢位、total 為零、完成值大於 total、百分比超過 100 或數值倒退均被忽略或降級，不得造成 UI 倒退。

### Byte 映射

單一已知來源總量 `N`：百分比 `p` 映射為 `floor(N*p/100)`，使用 checked arithmetic。只有大於上次 observation 的部分會形成 delta。ADB 程序成功結束後，若來源 metadata 仍可靠，上層補齊尚未回報的剩餘 bytes；失敗或取消不補齊。

資料夾若 ADB 輸出對每個檔案重新從 0% 開始，而沒有可靠 operation total，保持 indeterminate 並只使用可證明的 byte pair。不能證明 cumulative semantics 時不偽造百分比。

## 錯誤與生命週期

- 取消或 timeout：kill/wait child，停止接受 progress，回傳既有錯誤。
- pipe reader／parser failure：傳輸本身可繼續，但進度降級 indeterminate；診斷仍受 capture 上限控制。
- ADB 非零退出：保留已觀察進度，不能跳至 100%。
- 成功退出：flush 最後 frame，才允許補齊可靠已知總量。
- callback panic 必須隔離，不得中止 pipe reader 或 ADB child cleanup。
- parser 與 log 不保存密碼；路徑只存在既有 bounded ADB diagnostics，不加入 progress event。

## 測試與完成條件

- parser：分段 frame、`\r` 覆寫、`\n`、百分比、byte pair、重複／倒退、溢位、無效格式。
- runner：stdout/stderr 同時 drain、bounded capture、取消、timeout、callback panic。
- provider：成功補齊、失敗不補齊、未知 total indeterminate、無 `stat`／tree scan。
- integration：Local↔ADB 以及 ADB↔SFTP 保持單調且 terminal 前有中間 progress。
- final gates：相關 crates format/check/test、credential scan、`git diff --check`、OpenSpec strict validation與 `emulator-5554` 實機 push/pull。
