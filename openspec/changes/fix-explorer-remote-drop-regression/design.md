## Context

SuperExplorer 已有 GPUI `ExternalPaths`、typed `DropExternal`／`UpdateExternalDrag` actions、`AppViewState::queue_external_drop`、application transfer routing，以及 ADB／SFTP provider upload。既有 headful 證據曾證明 Windows Explorer→ADB 可工作；相同事件所有權回歸目前使 DragOver 仍可見但 terminal Drop 沒有到達背景 target。修正必須保留 OLE STA 邊界、標準 `CF_HDROP`、tab generation、Move 成功後才刪來源、剪貼簿隔離與遠端憑證保密。

核准的來源設計為 `docs/superpowers/specs/2026-09-01-explorer-remote-drop-regression-design.md`。

## Goals / Non-Goals

**Goals:**

- 恢復 Windows Explorer 的本機檔案／資料夾拖入 ADB、SFTP 背景與可寫資料夾列。
- 對一般檔案列、資料夾列與背景建立互斥且可測試的 external drop ownership。
- 保留 Copy／Move negotiation、目的地 generation 與既有 transfer／progress／terminal 路徑。
- 讓「有 DragOver、無 Drop」可由結構化診斷判斷在哪一層被拒絕。
- 以 Local、ADB、SFTP 真實 Windows Explorer matrix 作為 blocking gate。

**Non-Goals:**

- 不重寫 `IDataObject`、`DoDragDrop` 或 TransferEngine。
- 不改變 SuperExplorer→Windows Explorer、文字／圖片 clipboard 或內部欄位／書籤拖放。
- 不新增 Link drop、虛擬檔案 streaming 或 Shell namespace 支援。
- 不執行 workspace 完整回歸。

## Decisions

### 1. Drop ownership 由元素能力決定

file view background 在目前目的地可寫且 external paths/effect 有效時接收 drop；只有可寫資料夾列註冊 child `.can_drop`、`.on_drop` 與 external `.on_drag_move`；一般檔案列完全不註冊 external child target，使事件向背景解析。這延續既有 GPUI bubbling，避免新增 window-level interceptor 或第二套 HWND drop target。

### 2. 目的地在 Drop action 建立前固定

背景使用目前 tab location；資料夾列用 presentation resolver 還原 stable snapshot row，再以當前 tab/generation 建立目的地。若 generation、entry kind、provider write capability 或搜尋結果語意已改變，action fail-closed 並清除 cue，不依後續 selection 猜測目的地。

### 3. Effect 與來源只驗證一次語意、兩次時效

UI negotiation 僅接受非空、全為本機絕對路徑且 Copy／Move allowed 的 `ExternalPaths`。state 建立 command 時再次驗證目的地與 filesystem self/descendant/same-parent move 規則。Link、None、混合或空白來源不降級。Move 的本機來源刪除仍由既有 transfer terminal 控制，本變更不另行刪檔。

### 4. 診斷靠 typed rejection reason，不靠路徑或憑證內容

Drag/Drop 診斷只記 request context、target kind、provider scheme、來源數量、allowed/performed effect、generation 與拒絕分類。不得記錄 SFTP password、URI userinfo 或完整敏感來源清單。Drop action 成功排隊、被 UI target 拒絕、被 state validation 拒絕與 transfer dispatch 失敗必須能區分。

### 5. 真實 OLE oracle 是 blocking gate

unit tests 無法證明 Windows hit testing 與 GPUI terminal event ownership，因此 headful Explorer→Local／ADB／SFTP 是 blocking。ADB 使用 `emulator-5554` marker-owned fixture；SFTP 使用既有登入 profile並以互動方式取得憑證。Copy 需目的存在且來源保留；Move 需目的存在且來源移除；所有遠端 fixture 驗證 marker 後清理。

### 6. Evidence-driven adjustment

- **A — task refinement：** 可調整測試命令、診斷文字、私有 helper 拆分與 leaf 順序，不改需求或 gate。
- **B — design/spec correction：** 若重現證明根因位於同一核准的 GPUI/OLE event ownership 範圍，可同步更新 design/spec/tasks、重開受影響 leaf並重新 strict validation。
- **C — material change：** 新增 HWND DropTarget、改公開契約、降低 Move 安全門檻、取消 SFTP/ADB headful gate、使用未授權破壞性目標或擴張 Link/virtual-file 範圍，必須取得使用者核准。

## Risks / Trade-offs

- [已填滿 viewport 的 row 截走背景事件] → 一般檔案列不註冊 external drop target，並以 source contract test 與 populated headful fixture驗證。
- [資料夾列 drop 同時 bubble 到背景造成重複操作] → folder terminal handler `stop_propagation` 且每次 drop 只產生一個 command。
- [拖曳期間導覽造成 stale destination] → generation 與 stable row validation fail-closed，terminal 清除 cue。
- [Move 部分成功導致資料遺失] → 不改既有 success-only source deletion；實測 Copy／Move oracle。
- [headful runner 座標漂移] → 優先使用 UI Automation bounds；報告保存實際 source/target rectangle 與窗口 DPI。
- [遠端測試洩漏憑證或留下資料] → 憑證只走互動輸入；fixture 帶 marker且清理前重驗 ownership。

## Migration Plan

不需資料遷移。先重現並保存失敗證據，再修正 element ownership/state validation，最後執行聚焦測試及 Local／ADB／SFTP headful matrix。若需回滾，還原 UI/state/diagnostic 變更即可；不得刪除使用者遠端資料，僅清理 marker-owned fixture。

## Open Questions

無。路徑、provider、effect、安全門檻與測試目標已由核准設計固定。
