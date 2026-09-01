## ADDED Requirements

### Requirement: Unified real transfer progress
系統 SHALL 對 Local、ADB、SFTP 任意來源與目的的 file operation 發布同一 typed progress contract，包含已完成／總項目、實際已交付 bytes、可選總 bytes、phase 與目前項目；百分比不得由經過時間模擬。

#### Scenario: Large known-size file has intermediate progress
- **WHEN** 使用者傳輸一個可可靠取得大小且足以產生多個 stream chunk 的檔案
- **THEN** terminal 前 SHALL 至少發布一個大於 0% 且小於 100% 的確定進度，且所有進度單調不減

#### Scenario: All endpoint directions share the contract
- **WHEN** operation 為 Local↔ADB、Local↔SFTP 或 ADB↔SFTP 任一方向
- **THEN** UI SHALL 消費相同 progress fields，且不以 provider-specific 假百分比替代實際 bytes

### Requirement: Reliable totals and indeterminate fallback
系統 SHALL 只在所有必要來源 metadata 可靠且 aggregation 無 overflow 時發布 total bytes；否則 MUST 使用 indeterminate progress 並繼續顯示實際 completed bytes。

#### Scenario: Unknown source size
- **WHEN** 任一來源檔案大小無法可靠取得
- **THEN** operation SHALL 顯示 indeterminate progress，不顯示百分比，但持續更新已傳輸 bytes 與項目

#### Scenario: Actual bytes exceed estimate
- **WHEN** 實際成功交付 bytes 超過預掃描 total
- **THEN** reporter SHALL 降級為未知總量、保持 completed bytes 單調且不得讓顯示百分比倒退

#### Scenario: Empty files and directories
- **WHEN** operation 只包含零位元組檔案或空資料夾
- **THEN** 系統 SHALL 以 item progress 顯示進度，Finished terminal 顯示 100%

### Requirement: Delivered-byte accounting
系統 MUST 只在 bytes 成功寫入目前目的 stage 後增加 completed bytes，且 Local、ADB、SFTP streaming path SHALL 回報實際 chunk delta。

#### Scenario: Read succeeds but write fails
- **WHEN** source chunk 已讀取但目的 write 失敗
- **THEN** 該 chunk MUST NOT 計入 completed bytes，terminal SHALL 顯示失敗或部分成功原因

#### Scenario: Recursive folder transfer
- **WHEN** 使用者傳輸包含多層檔案與空資料夾的來源樹
- **THEN** bytes SHALL 聚合所有成功寫入檔案，items SHALL 在相應根項目完成時更新，且資料夾結構保持正確

### Requirement: Continuous staged remote progress
跨遠端 staging operation SHALL 顯示單一連續進度；已知來源 N bytes 時總工作量 MUST 為 2N，download 與 upload 各計一次成功交付 bytes。

#### Scenario: ADB to SFTP staged transfer
- **WHEN** ADB 項目先下載至本機 staging 再上傳 SFTP
- **THEN** 進度 SHALL 從 download 前進至中點，再由 upload 前進至 terminal，不得在 phase 切換時重設為 0%

#### Scenario: Second stage fails during Move
- **WHEN** staging download 成功但 destination upload 失敗
- **THEN** progress SHALL 保留最後實值、terminal SHALL 為 Partial 或 Failed，且原始遠端來源 MUST 保留

### Requirement: Bounded monotonic publication
Reporter SHALL coalesce high-frequency updates、使用 bounded nonblocking publication、強制發布重要邊界，並在 terminal 後拒絕 late progress。

#### Scenario: Many small chunks
- **WHEN** provider 在短時間回報大量小 chunk
- **THEN** published event 數量 SHALL 受節流限制，但最後 flushed counters MUST 等於實際成功交付總量

#### Scenario: Late callback after terminal
- **WHEN** provider callback 在 Finished、Cancelled、Partial 或 Failed terminal 後抵達
- **THEN** 系統 MUST 忽略該 callback，UI 與 operation record 不得再變更

### Requirement: Accurate terminal and cancellation presentation
只有完整 Finished terminal SHALL 呈現 100%；Cancelled、Partial 與 Failed MUST 保留最後真實進度並顯示逐項原因，取消不得觸發尚未授權的 Move cleanup。

#### Scenario: Cancellation during transfer
- **WHEN** 使用者在中間 byte progress 時取消 operation
- **THEN** transfer SHALL 停止、terminal SHALL 顯示 Cancelled、最後百分比不得跳至 100%，且不得發布 late progress

#### Scenario: Partial multi-item transfer
- **WHEN** 多項傳輸中部分成功而部分失敗
- **THEN** UI SHALL 顯示最後 byte/item progress、成功失敗數與各失敗 stage 原因，不得把 operation 標為完整成功

### Requirement: Detailed progress UI
下方 operation surface SHALL 顯示操作、來源、目的、目前項目、item counts、已傳輸 bytes，並在 total 已知時顯示百分比，在 total 未知時顯示 indeterminate bar。

#### Scenario: Known total rendering
- **WHEN** operation 有非零可靠 total bytes
- **THEN** progress bar 與文字 SHALL 使用 completed/total bytes 計算一致百分比，terminal 前最高為 99%

#### Scenario: Credential-safe remote rendering
- **WHEN** remote progress 或錯誤涉及 credential-backed SFTP location
- **THEN** UI 可顯示 public authority/path，但 persistent diagnostics MUST NOT 包含密碼、secret 或 token
