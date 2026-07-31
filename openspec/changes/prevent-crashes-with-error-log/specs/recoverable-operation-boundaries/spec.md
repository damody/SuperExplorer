## ADDED Requirements

### Requirement: Production path 禁止 panic API
workspace 自有 non-test production targets MUST 不使用 `unwrap`、`expect`、`panic!`、`todo!` 或 `unimplemented!`，並以 source-policy gate 持續驗證；test-only code、build scripts 與 vendored dependencies 不在此限制內。

#### Scenario: 新增 production expect
- **WHEN** 變更在受管 production path 新增 `expect`
- **THEN** source-policy gate 失敗並指出檔案與位置

#### Scenario: 測試保留 assertion panic
- **WHEN** `#[cfg(test)]` module 或 integration test 使用 `unwrap`、`expect` 或 deliberate panic
- **THEN** source-policy gate 不將它判定為 production violation

### Requirement: 可恢復操作只終止自身 request
UI command、navigation、search、watcher、icon、clipboard、drag/drop、context-menu 與 file-operation 的可恢復錯誤 MUST 記錄到 `error.log`、發出或套用 terminal failure，並保留應用程式接受下一個操作的能力。

#### Scenario: Shell request 失敗後再次操作
- **WHEN** 一個 Shell request 因 Windows API、channel 或 provider 錯誤失敗，之後使用者送出有效 request
- **THEN** 第一個 request 以 failure 結束並留下 error record，第二個 request 仍被處理且視窗保持開啟

#### Scenario: UI command 缺少必要資料
- **WHEN** UI command 發現 row、selection、active tab 或 transient context 已失效
- **THEN** 系統取消該 command、清除相關 busy state、記錄錯誤並保留最後有效畫面

### Requirement: 模型 mutation 保留最後有效狀態
模型 MUST 在 commit 前驗證 window、tab、history、snapshot 與 request invariants；驗證失敗時 SHALL 回傳錯誤且不提交部分 mutation。

#### Scenario: Active tab invariant 失效
- **WHEN** mutation 無法取得有效 active tab 或 initial location
- **THEN** mutation 回傳具名錯誤，原 window model 保持不變且呼叫端可繼續處理後續輸入

#### Scenario: Stale request 嘗試 commit
- **WHEN** 過期 generation 的結果到達 mutation boundary
- **THEN** 系統拒絕該結果而不 panic，並保留目前 generation 的 snapshot 與 history

### Requirement: Parser 與轉換錯誤可回傳
搜尋 parser、整數尺寸轉換、handle 建立與 Shell identity 建構 MUST 使用 checked operation，並將無效輸入轉為具體錯誤或安全取消，不得假設 boundary 或值域後 panic。

#### Scenario: Parser cursor 無有效字元
- **WHEN** parser 在輸入結尾或無效 boundary 嘗試取得下一字元
- **THEN** parser 回傳 parse error 且應用程式仍可解析下一個有效 query

#### Scenario: Platform 值無法安全轉換
- **WHEN** Windows API 的 size、index、handle 或 identity 不符合目標型別 contract
- **THEN** 當次 platform operation 回傳錯誤且不建立假的預設值

### Requirement: Worker panic 隔離
每個可隔離的 background worker entry MUST 將非預期 Rust panic 轉成具 context 的 terminal failure，釋放該 worker 擁有的資源，且不得停止其他 worker 或 UI event loop。

#### Scenario: Worker handler panic
- **WHEN** background worker handler 發生可 unwind 的 Rust panic
- **THEN** worker boundary 將其寫入 `error.log`、送出 terminal failure 並結束該 worker，其他 worker 與 UI 仍可處理工作

### Requirement: 啟動與關閉失敗可診斷
diagnostics 初始化後的 startup 或 shutdown failure MUST 被記錄並依既有 reverse-order ownership 規則清理；無法建立初始 UI 的 prerequisite failure MAY 結束啟動嘗試，但不得無診斷消失。

#### Scenario: Shell STA 啟動失敗
- **WHEN** diagnostics 已就緒但 Shell STA 無法初始化
- **THEN** 系統寫入 startup error、逆序釋放已取得資源，並以受控失敗結束啟動嘗試

#### Scenario: Cleanup 部分失敗
- **WHEN** shutdown 中一個 owned resource 無法正常結束
- **THEN** 系統記錄該 failure 並繼續嘗試清理其餘 owned resources

### Requirement: 不可恢復程序終止明確排除
系統 SHALL 將 access violation、stack overflow、explicit abort、OS 強制終止與不可恢復 native corruption 視為 in-process recovery guarantee 之外，且測試與文件不得宣稱可安全繼續。

#### Scenario: 驗收恢復保證
- **WHEN** 驗收測試與文件描述「不閃退」範圍
- **THEN** 保證僅涵蓋 application-controlled recoverable errors 與可 unwind 的隔離 worker panic
