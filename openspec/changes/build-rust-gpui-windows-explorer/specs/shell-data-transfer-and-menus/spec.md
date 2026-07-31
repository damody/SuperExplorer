## ADDED Requirements

### Requirement: Explorer 相容 Clipboard copy/cut/paste
系統 SHALL 以 Windows Shell data object/formats 實作 copy、cut 與 paste，並可與 Windows Explorer 雙向交換單一/多選本機項目；paste availability MUST 由 clipboard content 與 target capability 決定。

#### Scenario: 從 Explorer 貼入本程式
- **WHEN** 使用者在 Windows Explorer 複製多個受控測試項目並在本程式貼到可寫入資料夾
- **THEN** 本程式必須使用可接受的 Shell formats 建立 operation、顯示 progress/outcome，且目的地出現相同項目

#### Scenario: 從本程式貼入 Explorer
- **WHEN** 使用者在本程式複製多個項目並在 Windows Explorer 貼上
- **THEN** Explorer 必須能讀取 data object 並完成標準 copy，除非 parity matrix 記錄具體公開 API 限制

### Requirement: Cut 狀態與完成語意
Cut items SHALL 以 stable identity 呈現 pending cut 視覺；只有成功 move/paste 的項目才能清除或移除其 cut state，cancel/partial failure MUST 保留正確的逐項狀態。

#### Scenario: Cut paste 部分失敗
- **WHEN** 多選 cut/paste 只有部分項目移動成功
- **THEN** 成功項目必須清除 cut state，失敗項目保留或回復可重試狀態，UI 不得一律清空

### Requirement: OLE drag source
系統 SHALL 在 pointer 超過 Windows system drag threshold 後建立 OLE drag source，使用 `IDataObject`/`IDropSource` 與 allowed effects，並在 drag terminal outcome 後更新 model。

#### Scenario: 拖到 Explorer
- **WHEN** 使用者從本程式拖曳受控項目到 Windows Explorer 目的地
- **THEN** Explorer 必須接收相容 data object，cursor/effect 必須反映 copy/move/none，完成後本程式依實際 effect 更新狀態

### Requirement: OLE drop target
系統 SHALL 將 file view、資料夾 item 與 navigation targets 註冊為 capability-aware drop targets，處理 DragEnter/Over/Leave/Drop、modifier/effect negotiation 與 reentrancy。

#### Scenario: 從 Explorer 拖入
- **WHEN** 使用者從 Explorer 拖曳多個項目進本程式的可寫入資料夾
- **THEN** target 必須顯示正確 drop cue/effect，Drop 後建立原生 file operation，離開或取消時清除所有 hover/capture state

### Requirement: Right-drag 與 auto-scroll
系統 SHALL 支援 right-drag 的 copy/move/cancel 決策，並在拖曳靠近可滾動 file view 邊緣時依速度受限地 auto-scroll；drag leave/terminal MUST 停止 scroll。

#### Scenario: Right-drag 取消
- **WHEN** 使用者 right-drag 到有效目的地後選擇 Cancel
- **THEN** 不得建立檔案操作，所有 drag indicator、hover 與 auto-scroll state 必須清除

### Requirement: Shell context menu sessions
系統 SHALL 對 background、single-selection 與 multi-selection 建立 `IContextMenu`/2/3 session，顯示原生選單並轉發必要 owner-draw/menu messages；命令 availability MUST 反映 item/target capability。

#### Scenario: Multi-select context menu
- **WHEN** 使用者對多個真實項目開啟 context menu
- **THEN** 選單必須使用多選 data object/parent contract，標準與相容 extension commands 可呈現並由正確 selection 執行

#### Scenario: Owner-draw message
- **WHEN** context menu extension 要求 measure/draw/init/menu-char message
- **THEN** host 必須在 session 有效期間轉發並處理結果，選單關閉後不得再使用已釋放 interface/HMENU

### Requirement: Extension 故障邊界
第三方 context menu activation/invoke MUST NOT 在 GPUI callback 中無界阻塞；session SHALL 記錄 correlation/deadline，於錯誤、timeout 或 host failure 後恢復可操作 UI 並清理 owner window/menu state。

#### Scenario: Handler timeout
- **WHEN** 可控制測試 handler 在 activation 或 command invoke 掛起
- **THEN** UI 必須保持或恢復回應、顯示安全錯誤、關閉 session 並記錄 handler/phase；無法強制終止的限制必須進 parity matrix

### Requirement: Explorer 互通測試矩陣
專案 SHALL 以受控真實檔案驗證與 Explorer 雙向 copy/cut/paste、left/right drag、copy/move/none effects、single/multi/background context menu 及 owner-draw fixture。

#### Scenario: 互通 case 無法自動化
- **WHEN** CI 無法控制 Explorer 或原生 drag loop
- **THEN** manual test 必須記錄逐步操作、actual result、OS/Explorer build 與證據，不得以 fake-only 測試宣告 parity
