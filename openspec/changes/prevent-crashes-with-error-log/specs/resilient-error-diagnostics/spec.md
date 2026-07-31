## ADDED Requirements

### Requirement: 可降級的 error.log 路徑選擇
系統 SHALL 依序嘗試在執行檔目錄、`%LOCALAPPDATA%\RustGpuiExplorer\logs` 與系統暫存目錄的 `RustGpuiExplorer\logs` 建立可 append 的 `error.log`，且任一候選失敗不得 panic。

#### Scenario: 執行檔目錄可寫
- **WHEN** 應用程式啟動且執行檔所在目錄可建立並 append 檔案
- **THEN** 系統使用執行檔旁的 `error.log`，不再探測較低優先序候選

#### Scenario: 執行檔目錄不可寫
- **WHEN** 執行檔目錄拒絕建立或開啟 `error.log`
- **THEN** 系統依序嘗試 LocalAppData 與系統暫存候選，並以第一個可寫位置繼續啟動

#### Scenario: 所有檔案候選皆失敗
- **WHEN** 每個 `error.log` 候選都無法建立或開啟
- **THEN** 系統保留 best-effort tracing 或 stderr reporter，且正常執行其餘啟動流程而不 panic

### Requirement: 結構化且 append-only 的錯誤紀錄
系統 MUST 將每筆受控錯誤以單行 append 紀錄，包含 timestamp、severity、subsystem、operation、error chain、thread、application version，以及可取得時的 source location。

#### Scenario: 受控操作失敗
- **WHEN** UI、模型、搜尋、Shell、worker、startup 或 shutdown 操作回傳錯誤
- **THEN** `error.log` 新增一筆具備完整操作脈絡的紀錄，且既有紀錄不被覆寫

### Requirement: 敏感資訊遮罩
系統 MUST 在錯誤與 panic 資料寫入任何 sink 前遮罩設定的使用者 profile 路徑前綴。

#### Scenario: 錯誤包含使用者路徑
- **WHEN** error chain 或 panic payload 包含設定的敏感根目錄
- **THEN** 寫入紀錄以 `%REDACTED_ROOT%` 取代該根目錄且不包含原始前綴

### Requirement: Logger 失敗不得造成 panic 或遞迴
系統 MUST 以 best-effort 方式處理 error sink 初始化、mutex poisoning、serialization 與 write/flush 失敗，不得因記錄錯誤再觸發 panic 或遞迴記錄。

#### Scenario: 寫入期間發生 I/O 錯誤
- **WHEN** 已選定的 `error.log` 在寫入或 flush 時失敗
- **THEN** 呼叫端仍收到原始操作結果，logger 僅嘗試一次獨立 fallback sink 且不呼叫自身

#### Scenario: Error log mutex 中毒
- **WHEN** error sink 的 mutex 已 poisoned
- **THEN** 系統依安全策略取回或停用該 sink，且不以 `unwrap` 或 `expect` 造成第二次 panic

### Requirement: 最終 panic 診斷
系統 SHALL 保留 process panic hook，將 application version、thread、location、payload 與 backtrace availability 寫入 `error.log`，同時遵守遮罩與 non-recursive 規則。

#### Scenario: Dependency 發生非預期 panic
- **WHEN** 未被 typed recovery boundary 涵蓋的 Rust panic 到達 process panic hook
- **THEN** 系統在執行既有 terminal panic 行為前 best-effort 寫入完整且已遮罩的 panic 紀錄

### Requirement: 一般事件與錯誤事件可並存
系統 SHALL 允許既有一般 lifecycle diagnostics 繼續寫入其原有日誌，並確保所有 error 與 panic 另行寫入 `error.log`。

#### Scenario: 正常啟動後操作失敗
- **WHEN** lifecycle 初始化成功後某項操作失敗
- **THEN** 一般日誌保留 lifecycle event，且 `error.log` 包含該項失敗紀錄
