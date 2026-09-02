## Context

先前 `fix-explorer-remote-drop-regression` 修正了 GPUI capture listener 吞掉 Windows OLE terminal `MouseUp` 的問題，並以一般 `.txt` 與資料夾 fixture 驗證 Explorer→Local／ADB／SFTP。使用者指定的 `D:\SuperExplorer\.tmp-full-meta.json` 是大小 34,629 bytes、屬性只有 `Archive` 的合法本機檔案，卻仍無法拖入 `adb://emulator-5554/sdcard/Download` 或 `sftp://45.32.49.125/home/linuxuser`。本變更以該真實來源隔離前導點檔名在 OLE、UI、state validation或remote dispatch的第一個失敗點。

限制如下：保留使用者來源檔，不輸出或儲存SFTP密碼；遠端清理只處理精確受控檔名；不做完整回歸；維持既有Copy語意與文字／圖片剪貼簿隔離。

## Goals / Non-Goals

**Goals:**

- 讓Windows Explorer提供的合法本機dotfile單檔進入既有共用external drop流程。
- 完整保留`.tmp-full-meta.json` basename，不因前導點或`.json`副檔名被拒絕或截斷。
- 以指定ADB與SFTP目錄驗證遠端存在、34,629 bytes、內容雜湊一致與本機來源保留。
- 讓拒絕或失敗可定位到OLE decode、UI target、state validation或transfer dispatch，且不洩漏credential。
- 建立可重跑的自動化與headful證據，防止合法dotfile回歸。

**Non-Goals:**

- 不對JSON或dotfile建立產品特例。
- 不接受虛擬Shell項目、空來源、非本機來源或不存在來源。
- 不改公開API、登入流程、剪貼簿、右鍵選單、拖出或整體傳輸架構。
- 不執行完整專案回歸測試。

## Decisions

### 1. 以指定真實檔案做分層實證

headful runner SHALL 直接選取`D:\SuperExplorer\.tmp-full-meta.json`，並記錄是否出現`UpdateExternalDrag`、`DropExternal`、`DataTransferRequest::DropExternal`及provider terminal結果。第一個缺失階段決定最小修正位置。

替代方案是先猜測前導點過濾並直接放寬驗證；拒絕此方案，因為可能破壞fail-closed且無法證明真實根因。

### 2. basename必須是無損共用值

OLE來源轉為本機`PathBuf`後，各層 SHALL 只用平台路徑API取得filename並原樣傳遞。不得使用會將前導點解讀為「沒有stem」或空名稱的副檔名邏輯建立目的名稱。ADB與SFTP SHALL 共用相同合法來源準備契約。

### 3. 驗證保持fail-closed

有效來源必須是非空、絕對、存在且為可傳輸實體檔案或資料夾；前導點不是拒絕條件。無法取得filename、來源消失、目的不可寫或effect不支援時仍拒絕，且不得建立部分錯誤目的名稱。

### 4. 實機oracle採basename、大小與SHA-256

ADB與SFTP驗證 SHALL 比較精確basename、長度及SHA-256；Copy完成後本機來源仍須存在且雜湊不變。報告只記錄sanitised URI、大小與hash，不含SFTP userinfo或密碼。

### 5. 受控清理與證據保留

遠端只刪除兩個指定父目錄下精確的`.tmp-full-meta.json`測試副本，並在刪除前確認其大小或hash符合本次來源。本機來源不得刪除；headful報告保留於`build/`作為證據。

### 6. 實證調整分級

- **A—任務精修：** 可調整命令、測試拆分或修正檔案位置，但不改需求、gate或公開契約。
- **B—設計／規格修正：** 若根因顯示仍在核准範圍內但原架構假設錯誤，暫停受影響分支，同步更新design/spec/tasks並重新strict validate。
- **C—實質變更：** 若需要放寬來源安全邊界、改公開契約、增加外部寫入範圍或降低blocking gate，必須先取得使用者核准。

已完成任務的依賴假設若被B/C調整推翻，相關任務 SHALL reopen，舊證據保留並標記stale或superseded，不得刪除證據沿革。

## Risks / Trade-offs

- **[headful命中不穩定]** → 使用UI Automation取得Explorer來源bounds，使用已校準的SuperExplorer背景目的點，並以日誌與遠端oracle雙重判定。
- **[dotfile被UI隱藏但仍可傳輸]** → 驗收以remote stat/read為準，不依賴列表是否顯示隱藏項目；另確認refresh後provider listing契約。
- **[同名遠端檔已存在]** → 測試前只在確認可安全清理時移除受控同名副本；不覆寫無法證明屬於測試的資料。
- **[SFTP credential洩漏]** → 密碼只經interactive stdin，報告與日誌掃描禁止password及URI userinfo。
- **[修正只適用JSON]** → 回歸以dotfile與一般檔案共同驗證，產品邏輯禁止副檔名判斷。

## Migration Plan

不需要資料遷移。修正以現有crate與runner的小範圍變更交付。回滾時還原本次產品程式變更與新增測試；不得回滾先前已通過的OLE capture修正。遠端fixture在驗證後由精確路徑清理。

## Open Questions

沒有需要使用者決定的開放問題。OLE、UI、state或transfer中的實際失敗層由指定fixture的實證決定，屬A級實作定位；若跨出已核准安全邊界則依C級規則處理。

## 實作後根因結論

分層實證顯示，指定dotfile經目前工作樹的OLE、UI、state validation與remote dispatch皆可成功傳輸；使用者實際執行的已安裝`SuperExplorer.exe`仍是2026-08-05舊產物，未包含先前完成的OLE terminal事件修正。`build_test_install.bat`會完成release建置與NSIS封裝，但預設僅啟動互動式安裝程式，封裝完成本身不代表使用者已完成安裝。因此本次除了強化真實來源runner與來源fail-closed驗證，也將最新release三個執行檔部署到既有安裝目錄並以hash確認一致；舊執行檔保留可回復備份。

SFTP列表的UI Automation可能因虛擬化或隱藏dotfile而找不到新列，不能作為內容正確性的唯一oracle。SFTP驗收改以受控provider下載、逐位元比較，確認後才精確刪除同名測試副本；UIA網址列或列表競態另列為runner限制，不覆蓋產品傳輸結果。
