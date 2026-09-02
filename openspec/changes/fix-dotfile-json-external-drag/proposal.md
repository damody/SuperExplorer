## Why

Windows 原生檔案總管仍無法將真實的本機 dotfile JSON `D:\SuperExplorer\.tmp-full-meta.json` 拖入 SuperExplorer 的 ADB 與 SFTP 目錄，儘管一般 `.txt` fixture 已通過。這表示既有外部拖放回歸矩陣遺漏了合法的前導點檔名或真實單檔資料路徑，會讓跨程式遠端檔案操作依檔名失敗，因此必須以指定檔案找出並修正共用根因。

## What Changes

- 讓標準 Windows OLE `CF_HDROP` 中存在的本機 dotfile 與 JSON 單檔沿既有共用外部拖放流程進入 Local、ADB 或 SFTP，不建立副檔名特例。
- 在 UI、state validation、remote dispatch 與目的 basename 組合之間完整保留 `.tmp-full-meta.json`。
- 對外部單檔被拒絕或傳輸失敗的階段提供不含憑證的可行動診斷。
- 擴充 headful runner，以使用者指定的真實檔案驗證 ADB 與 SFTP 的檔名、大小、內容與來源保留。
- 增加 dotfile 單檔自動化回歸與受控遠端 fixture 清理。

## Capabilities

### New Capabilities

- `external-dotfile-drop-preservation`: 定義 Windows Explorer 外部拖放合法 dotfile 單檔時的來源接受、basename 保留、跨 provider 傳輸、診斷與實機內容驗證契約。

### Modified Capabilities

無；主規格目錄沒有既有 capability，本變更以新 capability 補強先前 `explorer-remote-drop-target-routing` 變更未覆蓋的合法檔名邊界。

## Impact

- 可能影響 `crates/explorer-shell-win` 的 OLE 路徑解碼、`crates/explorer-ui` 的 drop/state validation，以及 `crates/explorer-app` 的遠端 external drop 準備與目的檔名組合；實作只修改第一個有實證的共用失敗點。
- 驗證影響 `scripts/smoke_explorer_drag_interop.ps1`、受影響 crate 的聚焦測試及受控 ADB／SFTP fixture。
- 不新增外部依賴、不改公開 API、不改登入與憑證儲存、不改剪貼簿文字／圖片、右鍵選單或 SuperExplorer 拖出行為。
- 不執行完整專案回歸測試。
