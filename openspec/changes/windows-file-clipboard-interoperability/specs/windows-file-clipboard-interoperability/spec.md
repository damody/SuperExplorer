## ADDED Requirements

### Requirement: 外部 Windows 檔案剪貼簿可被辨識
系統 SHALL 在 Windows clipboard sequence 改變後於 Shell STA 辨識有效的 `CF_HDROP`，解析本機來源項目與 Preferred DropEffect，並將最新可貼上狀態傳給 UI。

#### Scenario: 檔案總管複製本機檔案
- **WHEN** 使用者在原生檔案總管複製一個或多個實體檔案或資料夾
- **THEN** SuperExplorer 顯示可執行 Copy 貼上的外部檔案剪貼簿狀態

#### Scenario: 檔案總管剪下本機檔案
- **WHEN** 外部 `CF_HDROP` 同時提供 Move Preferred DropEffect
- **THEN** SuperExplorer 將操作模式辨識為 Cut，而非 Copy

#### Scenario: 系統剪貼簿只有非檔案內容
- **WHEN** clipboard sequence 改變且剪貼簿只有文字、圖片或 HTML
- **THEN** SuperExplorer 不啟用檔案貼上，且不清除或改寫該內容

### Requirement: 外部檔案可貼至所有支援目的地
系統 SHALL 讓有效外部 `CF_HDROP` 來源貼至可寫的 SuperExplorer local、ADB 與 SFTP 目錄，並使用各目的地既有的操作路由。

#### Scenario: 檔案總管複製後貼至 local
- **WHEN** 使用者把檔案總管複製的檔案貼至 SuperExplorer local 目錄
- **THEN** 系統透過 Shell file operation 在目的地建立對應項目

#### Scenario: 檔案總管複製後貼至 ADB
- **WHEN** 使用者把檔案總管複製的檔案貼至可寫 ADB 目錄
- **THEN** 系統透過既有 remote transfer service 將本機來源上傳至該 ADB 目錄

#### Scenario: 檔案總管複製後貼至 SFTP
- **WHEN** 使用者把檔案總管複製的檔案貼至可寫 SFTP 目錄
- **THEN** 系統透過既有 remote transfer service 將本機來源上傳至該 SFTP 目錄

#### Scenario: 貼上前剪貼簿已被取代
- **WHEN** UI 顯示外部檔案可貼上，但提交前 clipboard sequence 已改變
- **THEN** 系統不操作陳舊來源、刷新剪貼簿狀態並回報可理解的失敗原因

### Requirement: SuperExplorer local 複製可供檔案總管使用
系統 SHALL 在檔案檢視選取 local 項目並執行 Copy 或 Cut 時，發布含有效 `CF_HDROP` 與 Preferred DropEffect 的標準 Shell `IDataObject`。

#### Scenario: local 項目複製至檔案總管
- **WHEN** 使用者在 SuperExplorer local 檔案檢視選取項目並按 `Ctrl+C`
- **THEN** 原生檔案總管可在其 local 目錄貼上相同項目

#### Scenario: local 項目剪下至檔案總管
- **WHEN** 使用者在 SuperExplorer local 檔案檢視選取項目並按 `Ctrl+X`
- **THEN** 原生檔案總管取得 Move Preferred DropEffect 並可完成移動

### Requirement: 檔案與文字快捷鍵互不衝突
系統 SHALL 依目前焦點表面分派 `Ctrl+C`、`Ctrl+X` 與 `Ctrl+V`，使檔案檢視執行檔案命令，文字編輯器維持文字命令。

#### Scenario: 位址列複製文字
- **WHEN** 位址列或其他文字編輯器取得焦點且使用者按 `Ctrl+C`
- **THEN** 系統複製選取文字，不發布檔案 `CF_HDROP`

#### Scenario: 檔案檢視複製選取項目
- **WHEN** local 檔案檢視取得焦點、有檔案選取且使用者按 `Ctrl+C`
- **THEN** 系統發布檔案剪貼簿，不以路徑文字取代標準 OLE 資料物件

### Requirement: 失敗與部分完成可診斷
系統 SHALL 對剪貼簿忙碌、來源失效、權限錯誤及遠端傳輸失敗提供包含操作、來源、目的地與底層原因的詳細結果，並 SHALL NOT 暴露遠端密碼。

#### Scenario: 遠端上傳失敗
- **WHEN** 外部檔案貼至 ADB 或 SFTP 時 provider 拒絕或中斷傳輸
- **THEN** 狀態訊息指出失敗項目、目的路徑與 provider 原因，且不宣告整批成功

#### Scenario: 剪貼簿暫時忙碌
- **WHEN** OLE clipboard 暫時無法取得
- **THEN** 系統使用有界重試並保持 UI 可回應，重試耗盡後回報具體原因

### Requirement: 檔案拖放覆蓋所有來源與目的地
系統 SHALL 透過標準 OLE 檔案資料物件與既有 transfer router，支援 local、ADB 與 SFTP 在同一或不同 SuperExplorer 程序間的拖放，並支援 local 與原生檔案總管雙向拖放。

#### Scenario: 遠端項目拖至另一個支援目的地
- **WHEN** 使用者將 ADB 或 SFTP 項目拖至 local、ADB、SFTP 或另一個 SuperExplorer 視窗
- **THEN** 系統完整 materialize 所有來源、以 Copy 能力發布標準檔案資料物件，並在 drag terminal 後清理暫存

#### Scenario: 不支援或空白的 remote drop
- **WHEN** remote 目的地收到空來源、非本機來源、Link 或 None effect
- **THEN** 系統不執行任何 mutation、不得回報成功，並提供可診斷失敗

#### Scenario: 原生檔案總管與 SuperExplorer 雙向拖放
- **WHEN** local 項目在 SuperExplorer 與原生檔案總管之間任一方向拖放
- **THEN** Ctrl/Shift 與同磁碟/跨磁碟預設 effect 遵循 Windows Explorer 語意，且取消不改變來源或目的地
