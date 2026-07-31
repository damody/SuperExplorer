## Context

目前 `explorer-common` 已提供一般事件日誌與 process panic hook，但固定寫入 `explorer.log`，而 panic hook 寫完後仍交回預設 hook 結束程序。workspace 正式路徑另有數個 `unwrap`／`expect`，分布於 parser、模型不變量、Windows handle、Shell worker 與 UI 邊界。這是跨所有 production crates 的修改，而且工作區大量依賴 Windows API 與 GPUI callback，不能用單一全域 `catch_unwind` 取代逐層錯誤處理。

設計沿用現有 typed error、request generation、terminal event 與 reverse-order shutdown 模式。既有 `explorer.log` 可繼續記錄一般 lifecycle；新的 `error.log` 專門承載 error 與 panic 診斷。

## Goals / Non-Goals

**Goals:**

- 所有應用程式可控制、可恢復的錯誤只終止當次操作，不關閉可用視窗或其他 worker。
- non-test workspace production path 不含 `unwrap`、`expect`、`panic!`、`todo!` 或 `unimplemented!`。
- 在執行檔旁優先建立 `error.log`，不可寫時依序退回 LocalAppData 與系統暫存目錄。
- 日誌初始化、鎖中毒與寫入失敗本身不得 panic 或遞迴呼叫 logger。
- 以靜態政策與錯誤注入測試證明失敗後仍能接受下一個有效操作。

**Non-Goals:**

- 不修改 vendored GPUI 或第三方相依套件。
- 不宣稱能在 access violation、stack overflow、`abort`、OS 強制終止或不可恢復的 native corruption 後繼續同一程序。
- 不把測試與 build script 的 deliberate panic 改成吞錯；這些環境仍須快速失敗。
- 不引入外部 telemetry、網路回報或自動重啟 supervisor。

## Decisions

### 1. 分離一般診斷與錯誤診斷

`DiagnosticsSession` 保留一般 event sink，並新增可獨立降級的 error sink。錯誤紀錄 API 接收 severity、subsystem、operation、完整 error chain 與可選 source location；共同補上 timestamp、thread 與 application version，再套用現有敏感路徑遮罩。

選擇獨立 sink 是因為使用者需要固定名稱 `error.log`，且錯誤診斷不應被大量正常 lifecycle event 淹沒。替代方案是直接把 `explorer.log` 改名，但會破壞既有工具與測試，也無法清楚區分 normal event 與 error。

### 2. 使用候選路徑鏈與 fallback sink

production 候選順序固定為執行檔目錄、`%LOCALAPPDATA%\RustGpuiExplorer\logs`、`%TEMP%\RustGpuiExplorer\logs`。每個候選都執行 create-directory/open-append 探測，失敗便嘗試下一個；測試可注入候選路徑，避免依賴主機權限。全部失敗時保留無檔案的 error reporter，僅向既有 tracing/stderr 做 best-effort 輸出。

不使用「啟動失敗即退出」，因為 logger 是輔助功能，不應反過來阻止 UI 啟動。不使用盲目 current-directory 路徑，因為捷徑或 Shell 啟動時 current directory 不穩定。

### 3. 在最小安全邊界回復，不全域吞 panic

每個 fallible operation 以 `Result` 或明確 `Option` 分支抵達最近的 UI command、request handler、worker entry 或 startup stage。該邊界負責記錄錯誤、發送 terminal failure、清理 transient busy state，並保持上次有效模型快照。

worker entry 可用 `catch_unwind(AssertUnwindSafe(...))` 作最後防線，把 worker panic 轉成 failure event；process panic hook仍保留以涵蓋 dependencies。主 UI callback 不以 catch-and-continue 掩蓋 panic，因為 unwind 後 GPUI/model 可能已不一致；正式碼須先消除可預期 panic 源。

替代方案 `unwrap_or_default` 會製造假 identity、invalid handle 或錯誤檔案操作，因此只允許在預設值確實符合 domain contract 時使用。

### 4. 模型 mutation 採 validate-then-commit

原本依賴「一定有 active tab/current location」的 accessor 分為 fallible accessor 與僅供已證明不變量的內部 helper。跨 request 的 mutation 先在暫存狀態驗證；成功才 commit，失敗保留既有 window/tab/history/snapshot。UI 收到錯誤後關閉 loading 或 editor transient state，但不清除最後有效內容。

### 5. 以 source policy 加上 behavior tests 防回歸

source-policy test 只掃 workspace 自有 production targets，排除 `#[cfg(test)]` module、`tests/`、build scripts 與 `vendor/`，避免把測試 assertion 當成產品風險。行為測試對 diagnostics path、不可寫候選、parser boundary、model mutation、Shell/worker panic 與 UI 後續操作做錯誤注入。

Clippy 仍作輔助 gate，但不單獨依賴 Clippy lint，因為 cfg target 與 lint coverage 可能漏掉 Windows-only path。

## Risks / Trade-offs

- [大量 API 改為 fallible 可能擴大修改面] → 先從 leaf conversion/parser/Windows helpers 往上傳遞，按 crate 小批次編譯與測試。
- [工作區已有重疊的未提交修改] → 每次修改前檢查 diff，保留既有行為並只做局部 patch，不進行 reset 或整檔重寫。
- [`error.log` 位於執行檔旁可能沒有寫入權限] → 即時嘗試 LocalAppData 與 temp fallback，並記錄選用路徑。
- [logger 自身失敗可能造成錯誤風暴] → error sink 不透過自身回報錯誤，單次 best-effort 寫入 fallback sink。
- [捕捉 worker panic 後資源可能中毒] → worker 視為終止，釋放其 owned resources 並回傳 terminal failure，不重用該 worker 的內部 mutable state。
- [全面禁止 `expect` 可能降低不變量可讀性] → 以具名 fallible helper、typed error 與明確 validation 取代，並用測試記錄 contract。

## Migration Plan

1. 先擴充 diagnostics 與測試，讓後續每個改動都有可用 error sink。
2. 由 leaf crates 到 composition root 移除 production panic API，逐層調整回傳型別與 caller。
3. 補齊 worker/UI/startup recovery boundary 與錯誤注入測試。
4. 啟用 source-policy gate，執行 format、workspace tests、Clippy 與 Windows smoke tests。
5. 若回歸無法在小批次內修正，可逐 crate 回復對應提交；`error.log` 與既有 `explorer.log` 可同時存在，不需要資料 migration。

## Open Questions

無。日誌位置、fallback 順序、production/test 範圍與不可恢復錯誤邊界皆已由核准設計決定。
