## Context

SuperExplorer 已有 typed `LocationDescriptor`、`FileOperationRequest`、`DataTransferRequest`、Windows Shell clipboard／drag/drop，以及 `explorer-remote::TransferEngine`。ADB 與 SFTP provider 也已有 list、create-directory、rename、delete、upload 與 download 的低階能力，但 UI 仍以本機 Shell 操作為主要路徑；目前遠端目錄的右鍵、快捷鍵、資料夾遞迴傳輸及原生拖放沒有形成完整端到端契約。

權威來源是 `docs/superpowers/specs/2026-08-26-cross-provider-remote-file-operations-design.md`。使用者要求實作完成後才集中執行相關測試，不執行完整 workspace 回歸。Remote 刪除已核准為經確認後永久刪除；Local 繼續使用 Windows 資源回收筒。

## Goals / Non-Goals

**Goals:**

- 讓可寫 ADB／SFTP 背景與項目選單、`Ctrl+C/X/V` 及應用程式內拖放使用同一 capability 與 Transfer Engine。
- 支援 Local、ADB、SFTP 所有跨邊界方向的檔案與資料夾 Copy／Move。
- 以 scoped、RAII 管理且名稱不含遠端身分的本機暫存目錄中轉 Remote → Remote。
- 保證 Move 在完整複製成功後才刪除該來源，並提供 item-level Partial／Failed／Cancelled 結果。
- 保留文字／圖片 clipboard，並補齊 Windows Explorer 本機拖入遠端與遠端 staged 拖出。
- 保留現有 request context、generation、cancellation、conflict decision 與 exactly-one terminal event 契約。

**Non-Goals:**

- 遠端資源回收筒、跨檔案系統原子交易、離線同步、續傳、長期遠端快取或權限／時間戳完整複製。
- 以遠端 shell path 偽裝 Windows 本機路徑，或讓使用者設定任意 staging／清理根目錄。
- FTP、SMB、SSH shell、ADB 應用管理或新的 SFTP 驗證方式。
- 完整 workspace 回歸測試。

## Decisions

### 單一 capability predicate 與 typed request

UI 以 `LocationDescriptor` provider 身分、`NamespaceCapabilities` 與 provider registry 的能力計算背景及選取項目可執行命令。右鍵、工具列、快捷鍵與 drag/drop 都建立既有 typed request，不以網址或顯示名稱判斷。替代方案是在每個 UI surface 分別判斷；此方案會造成命令顯示與實際 dispatch 漂移，因此拒絕。

### 統一 Transfer Engine

Local → Local 保留 Windows Shell；Local → Remote 呼叫 upload；Remote → Local 呼叫 download；Remote → Remote 使用 scoped staging 後 upload。Transfer Engine 擴充為檔案與 bounded recursive tree，保留相對結構、拒絕父路徑跳脫，且不追蹤符號連結目標。替代方案是在 ADB、SFTP、Shell adapter 各自實作組合矩陣；其錯誤、取消及衝突語意難以一致，因此拒絕。

### Copy-then-delete 的 Move 邊界

每個來源項目只有在其完整目的樹完成後才允許來源刪除。刪除失敗回傳 Partial；複製、取消或衝突失敗時來源保持不變。已寫入的目的資料不做危險遞迴 rollback，因為目的可能與既有使用者資料合併。相同 provider 的 rename 可保留為明確安全的快速路徑，但不得改變結果契約。

### Scoped staging 與拖出 lease

一般 Remote → Remote 使用 `tempfile::TempDir`，操作 terminal 後由 RAII 清理。暫存名稱不含 host、user、serial 或 path。Windows Explorer 拖出則由 OLE data object／drag session 持有 staging lease，直到 Shell 完成消費後清理。建立或 materialize 失敗時不發布不完整 `CF_HDROP`。

### Clipboard 格式隔離

內部 Copy／Cut 保存 typed locations。原生 clipboard 只在可辨識的檔案格式（既有 `CF_HDROP` 或版本化 SuperExplorer remote descriptor format）存在時啟動檔案 Paste；文字、HTML、PNG、bitmap 與未知格式維持原內容且回傳 file-paste unsupported。編輯器擁有焦點時仍由 text input 處理 `Ctrl+C/X/V`。

Clipboard ownership 以最新 Copy／Cut 為準。當全 Local Copy／Cut 交給 Windows Shell 時，host 必須先失效先前的 internal remote clipboard record 與 staging；其後貼到 ADB／SFTP 必須讀取目前 `CF_HDROP`，不得因舊 remote token／record 攔截或取代 Local sources。

### 永久刪除與確認

Remote Delete 一律使用不可復原確認並 dispatch provider recursive delete；Local Delete 保留 recycle semantics。確認內容只顯示安全的項目數與既有顯示名稱，不揭露密碼或暫存內容。

任何 Remote delete 必須拒絕 empty components、`/`、`.`、`..`、provider／authority／container identity 或 generation mismatch。確認 session 保存 immutable typed targets、operation nonce 與 request generation；確認後只可 dispatch 該集合。每個項目在 destructive commit 前再檢查 cancellation／deadline；已開始的項目取得真實 outcome，已成功項目不可改報 Cancelled，尚未開始項目才標示 Cancelled。SFTP 以 `lstat` 判型，symlink 只刪 link 本身。

### Native clipboard authenticity 與 operation generation

版本化 native remote clipboard 不攜帶可直接執行的 descriptor／Cut 權限，而只攜帶 host-minted 256-bit、process/session-bound token。host 以 token 查詢 immutable internal record；foreign、malformed、replayed、previous-process 或已消耗 token 預設拒絕。每次 dispatch 仍重新驗證 provider、authority、container generation 與 capability。外部程序提供的 payload 永遠不能授權 source delete。

view generation 與 operation／clipboard generation 分離。stale view terminal 不得更新 snapshot／selection，但匹配 operation ID 的成功 Move 必須冪等消耗已完成 Cut items；Partial 僅保留 Failed、Skipped、Cancelled 或未開始來源，terminal replay 不得再次刪除或傳輸。

### Skipped、Replace 與 complete-copy 定義

Item outcome 保留既有 `Skipped`。Move 只有來源項目的所有必要 descendants 都成功寫入才構成 complete copy；任一 skipped descendant 都禁止來源刪除。Replace 不得以無界遞迴刪除混合目的樹；目的已有資料的合併／替換須依 conflict plan 精確作用，部分失敗保留已寫入目的且不 rollback 使用者既有內容。

### Windows staging containment

Remote → Local／Explorer 的每個 child 必須是單一合法 Windows component。拒絕 `/`、`\\`、NUL、colon／ADS、root／drive prefix、`.`／`..`、保留裝置名與尾端 dot／space。Unicode-normalized 或 case-fold collision 進入 conflict result。建立前後均驗證 canonical path 位於 owned staging root，且 traversal 不穿越 symlink、junction 或 reparse point。

### 固定資源界限

Traversal 硬界限為深度 64、每來源樹 100,000 nodes、單檔實際寫入 32 GiB、每操作 staging 實際寫入 64 GiB、全 process 並行 staging 128 GiB，並保留 `max(2 GiB, volume capacity 的 5%)` 可用空間。provider 宣告大小只供預檢，實際寫入 bytes 是權威計數；N+1 必須在下一次寫入及任何 source delete 前失敗。這些 blocking thresholds 不得在 apply 時靜默降低。

### OLE ownership 與 remote drag-out effect

Remote → Windows Explorer 第一版只提供 `DROPEFFECT_COPY`。staging lease 由 COM data object 與 drag source 的共享 owner 持有，至少等待 `DoDragDrop` terminal 且 final `IDataObject::Release`；window teardown 只能取消並釋放自己的 reference，不得刪除仍被 COM 持有的 lease。`QueryInterface`／`AddRef`／`Release`、`STGMEDIUM` ownership、STA affinity 與 callback `catch_unwind` → HRESULT 都必須遵守 exactly-once release。

### Deadline

ADB、SFTP、recursive enumeration、download、upload 與 source delete 共用 request deadline。deadline 在 dispatch 前、enumeration／transfer 中或 delete commit 前到期時，不得啟動後續步驟；已進入單項 destructive commit 的操作完成該項並回報真實 outcome，其他未開始項目為 Cancelled／deadline failure，request 仍只有一個 terminal。

### 衝突、刷新與終止

同名目的沿用 Prompt／Skip／Replace／KeepBoth 決策，不靜默覆寫。每個 request 只產生一個 terminal event；完成後刷新實際受影響的來源與目的分頁。導覽或取消後的結果由 request context 與 generation 拒絕。

### 實作調整分級

- A：不改變 requirements、blocking gates 或公開契約的任務拆分、順序、檔案或測試命令調整，可直接更新 tasks 與 evidence lineage。
- B：核准範圍內的設計／spec 修正，必須暫停受影響工作、更新 design／spec／tasks、將相依 evidence 標成 stale 並重新 strict validate。
- C：新增 provider、改變 Remote 永久刪除、staging／安全邊界、必要測試或外部／破壞性權限，必須取得使用者核准。

## Risks / Trade-offs

- [大型資料夾耗用暫存空間] → bounded traversal、逐項取消檢查、staging quota／空間失敗回報，terminal 後清理。
- [惡意 Remote descriptor 或名稱導致根目錄刪除／staging escape] → immutable confirmation targets、root guard、authority／generation revalidation 與 Windows component containment。
- [偽造 native clipboard Cut 導致任意來源刪除] → 只接受 host-minted session token；外部 payload 不具 source-delete authority。
- [Move 複製完成但來源刪除失敗] → Partial 並保留來源，不宣告完整成功。
- [目的已有同名資料] → 進入既有 conflict decision，不靜默覆寫或遞迴刪除目的。
- [符號連結循環或路徑跳脫] → 不追蹤 link target、驗證相對 component、限制 traversal 節點／深度。
- [OLE 拖出過早清理] → staging lease 綁定 data object／drag session，而非函式區域變數。
- [clipboard 與文字／圖片衝突] → 只辨識檔案格式，editable focus 優先，unsupported 不清除 clipboard。
- [憑證或遠端內容進入診斷] → error redaction、無秘密 Debug、staging 名稱不含 authority/path。
- [現有本機操作回歸] → Local → Local 仍走 Shell；聚焦測試固定 recycle、conflict 與 clipboard 行為。

## Migration Plan

1. 先擴充共用 capability／request 與 Transfer Engine 契約，但保持 Local → Local 路由不變。
2. 接通 provider mutation、recursive transfer 與 remote service terminal／refresh。
3. 接通 UI command、typed clipboard、Remote delete confirmation 與應用程式內拖放。
4. 最後接通 OLE drag-in／staged drag-out lease，集中執行聚焦測試與相關 crate check。

Rollback 可停止 remote mutation dispatch 並隱藏 capability；沒有 session schema 或秘密遷移。scoped staging 由 process／lease 清理，既有 SFTP profiles 與 ADB identity 保留。

## Blocking Gates and Evidence

- `REMOTE-MUTATION`: ADB／SFTP create-directory、file/tree delete、取消及路徑拒絕測試通過。
- `TRANSFER-MATRIX`: Local／ADB／SFTP 跨邊界 Copy／Move、recursive tree、Partial、衝突及 staging cleanup 測試通過。
- `CLIPBOARD-ISOLATION`: file clipboard、editable focus、text/image/unknown format 不受干擾測試通過。
- `DRAG-INTEROP`: internal drag、Explorer drag-in、staged drag-out lease／failure 測試通過。
- `DESTRUCTIVE-FIXTURE`: 真實 ADB／SFTP delete 測試只可在唯一、預先建立且 marker 驗證的 owned subtree 執行；containment 不符即拒絕清理。
- `HEADFUL-OLE`: 以真實 Windows Explorer 與 disk/content oracle 驗證 drag-in／drag-out；環境無真實輸入能力時 gate 為 Blocked，不得用 synthetic test 取代。
- `FINAL-FOCUSED`: 相關 crates 編譯、聚焦測試、`git diff --check` 與 OpenSpec strict validation 通過；不得執行完整回歸。

每個 L3 任務在 `evidence/index.jsonl` 使用唯一 `task_id`，記錄命令／程序、預期與實際結果、exit status／reviewer、時間戳、相關 gate、適用時的 hash 與 adjustment ID。

## Open Questions

無阻塞問題。Remote 永久刪除、scoped 本機 staging 與不執行完整回歸均已由使用者核准。
