## Context

現有 UI 能產生 `DropExternal`／`BeginDrag`，Shell 能建立 `IDataObject` 並執行 `DoDragDrop`，remote service 也有 upload、download 與 request-scoped staging。然而 SFTP 的雙向拖放仍未端到端成立，主要風險是 remote route 未命中、staging 提早清理，以及 performed effect 未回傳至遠端來源刪除決策。

## Goals / Non-Goals

**Goals:**

- Windows Explorer 的實體檔案／資料夾可拖入 SFTP。
- SFTP 檔案／資料夾可經 staging 以標準 OLE 檔案拖放送到 Explorer。
- 預設 Copy，Shift Move；來源只在成功 Move 後刪除。
- 取消、失敗、部分成功與 staging 都有安全且可診斷的 terminal。

**Non-Goals:**

- 不實作 `FileGroupDescriptor`／`FileContents` 延遲虛擬檔案。
- 不接受文字、圖片與 URL external drop。
- 不擴張 ADB 功能，但共用行為不得回歸。

## Decisions

### 標準 OLE 邊界

Explorer→SFTP 只接受 `CF_HDROP` 並轉成 local sources；SFTP→Explorer 完整下載到 staging 後，以既有 Shell data object 與 `DoDragDrop` 暴露。Copy/Paste 模擬無法保留原生 drop negotiation，因此不採用。

### Effect 契約

跨檔案系統無修飾鍵與 Ctrl 均為 Copy。Shift 才允許 Move；Link 降級 Copy 或拒絕。遠端來源刪除必須同時滿足實際 performed effect 為 Move、OLE terminal 成功、對應本機目的已被 target 接受。

### Request-scoped staging

每個 drag request 建立獨立 `TempDir`，鍵為 request ID。remote service 在 Shell terminal 前保持 ownership；完成、取消與失敗皆移除。資料夾下載保持單一根名稱與遞迴內容。

### Terminal 與失敗

下載失敗不啟動 OLE；OLE 取消是 Cancelled；upload 或來源刪除部分失敗回傳逐項 outcome。日誌記錄 request、來源、目的地、effect 與 stage，但不記錄 credential。

### 調整級別

- A：可調整函式拆分、任務順序、測試命令與非公開內部型別。
- B：若現有 terminal 無法表達 performed effect，可在核准範圍內同步修正 design/spec/tasks 並重驗。
- C：改用虛擬檔案串流、降低 Move 安全門檻或移除 headful 證據需使用者核准。

## Risks / Trade-offs

- [大型遠端檔案拖曳前需等待下載] → 顯示進度並保持可取消；虛擬串流留待後續。
- [staging 過早刪除] → request-scoped ownership 保留到 OLE terminal。
- [Move 導致資料遺失] → performed Move 與成功 terminal 雙門檻，部分失敗只刪成功項目。
- [外部 target 讀取時間差] → `DoDragDrop` 返回前及 terminal 處理前不清 staging。

## Migration Plan

無資料遷移。以內部路由修正發佈；回滾本變更不會修改 credential 或遠端設定。測試建立的遠端 fixture 只在確認成功後清除。

## Open Questions

無。
