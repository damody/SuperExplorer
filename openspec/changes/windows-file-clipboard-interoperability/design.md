## Context

SuperExplorer 已有 Shell STA、OLE clipboard runtime、local file operation 與 ADB/SFTP 暫存傳輸能力，但外部 Windows 檔案剪貼簿同步和 local 選取項目的標準 OLE 發布尚未形成可靠的端到端契約。所有 OLE 操作必須留在 STA；UI 不得直接持有 COM 物件；文字編輯快捷鍵也必須與檔案快捷鍵隔離。

## Goals / Non-Goals

**Goals:**

- 原生檔案總管的 copy/cut 可貼至 SuperExplorer local、ADB、SFTP。
- SuperExplorer local 的 copy/cut 可由原生檔案總管貼上。
- 以 `CF_HDROP`、Preferred DropEffect 與 clipboard sequence 維持標準 Shell 語意。
- 非檔案剪貼簿格式不被檔案操作解讀、清除或覆寫。
- 操作失敗提供來源、目的地、操作及底層原因。

**Non-Goals:**

- 不把 ADB/SFTP 遠端項目發布為 Windows 虛擬檔案資料物件。
- 不改寫衝突處理、遠端登入或 provider 協定。
- 不新增私有格式作為外部互通的必要條件。

## Decisions

### 1. Shell STA 是唯一 OLE 剪貼簿所有者

`OleGetClipboard`、`OleSetClipboard`、`IDataObject` 與 `STGMEDIUM` 的生命週期全部留在 `explorer-shell-win` STA。跨層只傳遞純資料描述、模式、generation 與事件，避免 UI 執行緒 COM apartment 錯誤。替代的 UI 直接讀取方案會擴散 COM 所有權且增加凍結風險，因此不採用。

### 2. 外部狀態先同步，提交時再驗證

clipboard sequence 改變時，runtime 檢查 `CF_HDROP` 並更新 `ClipboardState::External`，供工具列、快捷鍵與右鍵選單判定。真正貼上時再次取得原始 OLE 物件並確認 sequence，防止 UI 顯示狀態與實際剪貼簿不同步。若剪貼簿已變更，操作安全失敗並刷新狀態。

### 3. 使用標準 Shell 格式雙向互通

外部來源解析 `CF_HDROP` 路徑，Preferred DropEffect 缺失時預設 Copy。SuperExplorer local copy/cut 發布標準 `IDataObject`，含 `CF_HDROP` 與相應 drop effect。私有 token 僅能作為內部來源辨識輔助，不能成為檔案總管互通前提。

### 4. 目的地沿用既有操作路由

local 目的地使用 Shell file operation；ADB/SFTP 目的地把外部本機路徑交給既有 remote transfer service 和 staging lifecycle。這保持 provider 邊界一致，也讓 local→remote、remote→remote 與 Explorer→remote 共用錯誤及清理語意。

### 5. 快捷鍵由焦點表面決定

檔案檢視有選取項目時 `Ctrl+C`/`Ctrl+X` 是檔案命令；位址列、搜尋、重新命名及其他 editor 焦點維持文字命令。檔案 `Ctrl+V` 只在檔案檢視和可寫目的地生效。此判定不得依剪貼簿同時含有文字格式而改變。

### 6. 拖放與貼上共用 fail-closed transfer 邊界

local 拖出使用標準 Shell `IDataObject`；ADB/SFTP 拖出先完整下載到有界暫存目錄，再以唯讀 Copy 能力發布相同的本機資料物件。SuperExplorer drop target 將 local 暫存來源交給既有目的地 router，因此 local、ADB、SFTP 與其他 SuperExplorer 程序不需要私有跨程序協定。remote drop 只接受非空、全為本機路徑且 effect 明確為 Copy 或 Move 的請求；`None`、Link、混合或無效來源必須明確失敗，不能降級成 Copy 或回報空批次成功。暫存只保留到 native drag terminal，之後按 request id 清理。

## Risks / Trade-offs

- [剪貼簿被其他程式快速取代] → sequence 更新與貼上提交時雙重驗證，陳舊請求不執行。
- [OLE clipboard 暫時忙碌] → 使用既有短暫、有界重試；不在 UI 執行緒等待。
- [外部 cut 到遠端的部分成功] → 只有各項目上傳及來源移除皆成功才回報移動完成；保留逐項詳細失敗。
- [非檔案資料同時帶有多種格式] → 只有存在有效 `CF_HDROP` 才啟用檔案貼上；其餘格式不讀不改。
- [遠端認證資訊洩漏] → 錯誤及證據只記錄 sanitised 路徑與 provider 原因，不記錄密碼。

## Migration Plan

以內部行為修正方式佈署，不需資料遷移。先補齊 runtime 與路由，再接通 UI 狀態，最後集中執行聚焦與 headful 測試。若需回滾，可還原本變更而不影響書籤、遠端登入資料或檔案內容。

## Open Questions

無；本次範圍與成功條件已由核准設計固定。
