## ADDED Requirements

### Requirement: Windows-only 可重現 workspace
系統 SHALL 提供 Windows-only Rust Cargo workspace，以 Git submodule 固定 GPUI-CE revision 並提交 `Cargo.lock`；非支援 target MUST 在編譯期以可理解訊息失敗。

#### Scenario: Windows 目標解析固定依賴
- **WHEN** 開發者在支援的 Windows x64 target 執行 dependency resolution
- **THEN** Cargo 必須解析到規格指定 revisions，且重複解析不改變已提交的 lockfile

#### Scenario: 非支援 target 快速失敗
- **WHEN** 開發者嘗試為非 Windows target 建置 application binary
- **THEN** 建置必須在編譯期停止並指出本專案只支援 Windows，而不是產生不可用 binary

### Requirement: 可啟動與可關閉的 GPUI application
系統 SHALL 啟動單一 GPUI application window，允許調整尺寸，並在使用者關閉視窗後正常結束程序。

#### Scenario: 正常啟停
- **WHEN** 使用者啟動 application、調整視窗尺寸後關閉視窗
- **THEN** 視窗必須持續回應、程序必須結束，且不得留下 application-owned thread 或 handle

### Requirement: 程序診斷
系統 SHALL 在建立 UI 前初始化結構化 logging 與 panic hook，並在 panic 時留下足以識別 app version、thread 與失敗位置的診斷資訊，同時避免記錄 credential、檔案內容或不必要的完整敏感路徑。

#### Scenario: 啟動後產生日誌
- **WHEN** application 正常啟動與關閉
- **THEN** diagnostics 必須記錄啟動階段、版本、主要生命週期事件與乾淨關閉結果

#### Scenario: 受控 panic
- **WHEN** 測試入口觸發受控 panic
- **THEN** panic hook 必須保存診斷並允許程序受控退出，不得以空白或只含原始敏感資料的訊息結束

### Requirement: Shell STA 生命週期
系統 SHALL 建立專用 Shell STA thread，在該執行緒初始化 apartment、維持 message pump，並在 application shutdown 時於同一執行緒解除初始化及完成 join。列舉、檔案操作、OLE 與選單 command MUST 透過 typed endpoint 進入 STA，且只有實際 capability 與 contract test 需要的 API 才能公開。

#### Scenario: STA 正常啟停
- **WHEN** composition root 啟動後立即執行正常 shutdown
- **THEN** STA 必須回報 ready、接受 shutdown、停止 message pump、於原執行緒解除 COM 初始化並在期限內完成 join

#### Scenario: 重複關閉
- **WHEN** shutdown 路徑因視窗與程序事件被呼叫多次
- **THEN** STA shutdown 必須保持 idempotent，不得 double-uninitialize、panic 或永久等待

### Requirement: 明確啟停順序
系統 SHALL 依 diagnostics、Windows/DPI prerequisites、Shell STA、GPUI、window 的順序啟動，並以相反順序關閉；任何階段失敗 MUST 回收已取得資源並回傳可觀測錯誤。

#### Scenario: 中途初始化失敗
- **WHEN** 測試注入 Shell STA 或 window 建立失敗
- **THEN** 已啟動的前置服務必須被關閉，錯誤必須包含失敗階段，程序不得留在半初始化狀態

### Requirement: 最小且單向的 crate 邊界
production code SHALL 維持 `explorer-app` 作為 composition root、`explorer-ui` 不依賴 `explorer-shell-win`、Win32/COM 型別不進入 UI 公開介面，且 MUST NOT 建立沒有 production consumer 或 contract test 的空 API。

#### Scenario: Dependency boundary 檢查
- **WHEN** 執行 workspace metadata 或架構測試
- **THEN** 不得出現 `explorer-ui -> explorer-shell-win` dependency，且 UI 公開型別不得包含 `windows` crate 的 Shell/COM 型別

### Requirement: Cargo 品質閘門
系統 SHALL 通過 formatting、workspace check、all-target/all-feature clippy with warnings denied，以及 workspace tests。

#### Scenario: 執行四個 gates
- **WHEN** 開發者依文件執行四個標準 Cargo commands
- **THEN** 每個 command 必須以成功狀態結束，且結果必須記錄於當前 milestone 證據
