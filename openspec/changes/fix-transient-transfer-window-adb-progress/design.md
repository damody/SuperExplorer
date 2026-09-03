## Context

現有多工作傳輸中心由 command bar 內的 `deferred` popup 呈現。它共享主視窗 paint/clip/focus tree，因此 details header 等後繪製區域能遮住 popup，root dismissal 與 popup focus callback 也可能競爭。`OperationCenterState` 已提供 session-only newest-first records 與 foreground fallback；這份狀態應繼續是唯一工作來源。

ADB 已用 PTY 執行 push／pull並有 frame parser，但目前 frame 切割與 snapshot 發布不能穩定反映 CLI carriage-return/ANSI 輸出。通用 reporter 雖每 200 ms heartbeat，若 parser 沒即時更新 latest snapshot，UI 仍會重送舊值。取消路徑另會把零成功摘要映射成 partial terminal，導致使用者看見「部分完成 0/Y」。

核准來源是 `docs/superpowers/specs/2026-09-02-transient-transfer-window-adb-native-progress-design.md`。這是 Windows/GPUI、model、app 與 ADB provider 的跨元件修正，不能降低既有立即取消、200 ms cadence、session-only history 或安裝 hash gate。

## Goals / Non-Goals

**Goals:**

- 使用 owner 綁定 transient tool window 呈現傳輸中心，使其高於主內容、低於 modal，且不出現在工作列或 Alt+Tab。
- 讓 toggle、Escape、owner lifecycle 與整個 owned-window 群組失焦可靠隱藏視窗，不阻塞主 UI。
- 取消確認後只產生 `Cancelled` terminal，並隔離遲到 progress／terminal。
- 直接解析 ADB PTY 的 percent/bytes 原生輸出，將最新單調快照每 200 ms送至 UI。
- 以自動測試、ADB/SFTP 實機、使用者視角與正式打包安裝證據關閉變更。

**Non-Goals:**

- 不改 SFTP 協定、衝突決策、跨執行歷史保存、公開擴充 ABI 或一般 modal 設計。
- 不新增 adb binary 或改用自製同步協定。
- 不讓工具視窗取得高於登入或刪除確認 modal 的層級。

## Decisions

### 1. 使用 owned tool window，而非提高 deferred priority

主視窗只保留傳輸按鈕與徽章。app composition 建立可重用工具視窗，window owner 指向呼叫它的 explorer window，並套用 Windows tool-window/no-activate-when-showing-in-taskbar policy。這從根本分離主視窗 clip/paint tree；單純提高 `deferred.with_priority` 仍無法跨 native surface 或保證 focus lifecycle，因此不採用。

工具視窗內容訂閱同一個 `OperationCenterState` 快照與 typed action callback，不複製工作模型。handle、owner identity、anchor bounds 與 visible 狀態由 bounded coordinator 管理。show/hide 是 idempotent；owner 關閉會 close，owner 最小化或 owner group 失去 foreground 會 hide。

### 2. 以 owner-group focus 判定失焦

從主視窗移入工具視窗不能視為失焦。coordinator 在 focus change 後延遲一個 UI turn，再檢查 foreground/focused native window 是否為 owner 或其 owned tool window；兩者都不是才 hide。Escape、再次點按鈕及 owner close 走相同 hide/close action，避免多個 callback 各自改 state。

### 3. 定位由 anchor 與 monitor work area 決定

command bar 在點擊時提供按鈕 screen-space bounds。預設工具視窗右緣對齊 anchor 右緣、top 位於 anchor bottom；結果 clamp 至 monitor work area，若下方不足且上方較充足則向上展開。DPI/monitor 切換時重新計算，不保存絕對座標。

### 4. `Cancelled` 是獨立且不可覆寫的 terminal

Operation reducer 對使用者取消 request 建立 cancelling 狀態。provider 回報 cancellation 後，以 typed `Cancelled` terminal 完成；progress reducer 在 cancelling 或 terminal 後拒絕事件，terminal reducer採 first-terminal-wins。顯示器對零成功取消輸出「已取消」，對部分已成功取消輸出「已取消（已完成 X/Y）」；一般 provider error 才顯示 failure/partial failure。

### 5. ADB parser 與 publisher 分層

PTY reader 持續 drain bytes；parser 移除 ANSI control sequences，並以 CR、LF 及完整控制序列邊界刷新 frame。parser 在每個可解析 frame 立即更新 monotonic adapter，byte-pair 優先於 percent 換算。adapter 只接受 completed bytes/percent 的單調增長並更新共享 latest snapshot，不執行時間節流。

獨立 publisher 每 200 ms讀 latest snapshot並發布；phase start、取消、terminal 立即發布。PTY reader 不等待 publisher，publisher 不等待 child exit，取消 polling 維持短週期並 kill/reap child。PTY 建立失敗可回退 pipe runner，但寫入明確 diagnostic 且仍走 latest-snapshot publisher。

### 6. Evidence 與變更校正

每個 L3 task 寫入 evidence index。A 類可調整 task 拆分或命令；B 類若發現 GPUI/Windows owner API 假設不成立，須同步修正 design/spec/tasks 並使相關 evidence stale；C 類若需普通工作列視窗、降低 200 ms gate、改外部協定或新增權限，必須取得使用者批准。任何校正不得靜默降低 blocking gate。

## Risks / Trade-offs

- [GPUI owned-window API 未直接暴露所有 Win32 style] → 先盤點既有 popup/secondary-window patterns；必要時用最小 Windows adapter 設定 owner 與 `WS_EX_TOOLWINDOW`，並以原生 style evidence 驗證。
- [focus callback race 造成一點即消失] → 延遲一個 UI turn後檢查 owner group，所有關閉路徑集中至 idempotent coordinator。
- [工具視窗 live update 造成 UI thread 壓力] → 只 invalidate 既有窗口，維持 200 ms coalescing，不因 progress 重建 window。
- [不同 adb 版本輸出格式不同] → parser 接受 CR/LF、ANSI、percent 與 byte pair，保存未知 frame diagnostics 並以 real adb probe 驗證。
- [取消後 adb device 端延遲 flush] → UI 只在 child kill/reap/provider terminal 後顯示已取消；測試同時檢查 process terminal 與 eventual remote cleanup，不以 UI 隱藏代替真正取消。

## Migration Plan

1. 新增 transient coordinator 與工具視窗 render，保留底部 foreground status 作為 fallback。
2. 切換傳輸按鈕 typed action 至 coordinator，移除主 command bar 內 transfer popup。
3. 正規化取消 terminal 與遲到事件 reducer。
4. 強化 ADB PTY parser/adapter/publisher，跑 focused tests。
5. 執行 ADB/SFTP 及 focus/layer 使用者視角驗證。
6. 以 `build_test_install.bat` 建置安裝並核對 hash。

回滾可恢復舊 command-bar popup action，而 OperationCenter 與傳輸本身仍可繼續；ADB parser 可回滾至前一 parser，但不得發布未通過 cadence/cancel gate 的版本。

## Open Questions

無。工具視窗層級、失焦語意、取消文字、200 ms cadence 與 session-only history 已由使用者核准。
