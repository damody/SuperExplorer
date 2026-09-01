## ADDED Requirements

### Requirement: ADB output is consumed incrementally
系統 SHALL 在 `adb push`／`adb pull` 執行期間透過隱藏 pseudo-terminal 持續 drain原生terminal output，並在 child 結束前將完整 progress frame 交給 parser，同時維持 bounded diagnostic capture；pseudo-terminal不可用時 SHALL 回退可取消的pipe runner與indeterminate progress。

#### Scenario: Carriage-return progress arrives before terminal
- **WHEN** ADB 以多個 read chunks 輸出由 `\r` 覆寫的 progress frames
- **THEN** 系統在 process terminal 前產生至少一個 progress observation，且 reader 不等待 EOF 才解析

#### Scenario: Pipe mode suppresses native progress
- **WHEN** 實機ADB在普通redirected pipe只輸出terminal summary
- **THEN** transfer runner使用pseudo-terminal取得中間frames，而不是啟動stat或tree-scan polling

#### Scenario: Callback panics
- **WHEN** progress consumer 在處理 frame 時 panic
- **THEN** 系統隔離 panic、繼續 drain pipes並完成 child cleanup

### Requirement: Parsed progress is monotonic and bounded
系統 SHALL 優先採用可靠 completed/total byte pair，否則可將 0–100 百分比映射到已知來源大小；越界、溢位、重複或倒退 observation MUST NOT 造成 completed bytes 倒退或重複累加。

#### Scenario: Frame spans reads
- **WHEN** 百分比或 byte pair 被拆在兩個 pipe reads
- **THEN** parser 保存殘片並只在 delimiter 完成 frame後發布一次有效 observation

#### Scenario: Per-file percentage resets
- **WHEN** 資料夾傳輸的 ADB 輸出百分比從較高值重設為較低值且無可靠 operation byte pair
- **THEN** 系統忽略倒退 observation並維持 indeterminate，而不是重設整體進度

### Requirement: Terminal semantics remain truthful
系統 SHALL 僅在 ADB process 成功且來源 total 可靠時補齊尚未回報 bytes；Failed、Cancelled 或 timeout MUST 保留最後實值且不得跳至 100%。

#### Scenario: Successful transfer has rounding remainder
- **WHEN** 最後原生百分比因取整未達來源 total但 ADB 成功退出
- **THEN** provider 補送剩餘 delta，使成功 terminal 能對應完整 total

#### Scenario: Transfer fails after intermediate progress
- **WHEN** ADB 在已發布中間進度後以非零狀態退出
- **THEN** provider 回傳既有詳細錯誤，且不補送剩餘 bytes

### Requirement: Progress polling overhead is removed
ADB provider MUST NOT 為進度週期性執行 remote metadata command或遞迴掃描本機 destination tree；格式未知時 SHALL 降級為 indeterminate。

#### Scenario: Unknown output format
- **WHEN** ADB output 不包含任何可驗證 percentage 或 byte pair
- **THEN** 傳輸繼續、UI 保持 indeterminate，且不啟動 progress-only `stat` 或 tree scan

### Requirement: Real-device fixture proves intermediate progress
針對可用的 `emulator-5554`，blocking fixture SHALL 證明 upload 與 download 在 terminal 前收到中間 progress，內容一致並清除受控測試資料。

#### Scenario: Emulator push and pull
- **WHEN** fixture 傳輸足以產生多個原生 progress frames的大檔
- **THEN** upload/download 都觀察到 terminal 前 progress、結果內容 hash 一致且 marker-owned fixture 被移除
