## Context

目前 `ShowContextMenu` 只攜帶 Win32 screen point。Local 選單把它交給 Windows Shell 是正確的，但 ADB／SFTP state 將同一數值保存給 GPUI absolute overlay，造成座標空間混用。核准設計來源為 `docs/superpowers/specs/2026-08-26-remote-context-menu-coordinate-design.md`。

## Goals / Non-Goals

**Goals:**

- 從事件建立點開始明確保存 client 與 screen anchor。
- 依選單實作者選擇座標，而非在 render 階段反推。
- 滑鼠與鍵盤開啟路徑一致，並保留邊界避讓。

**Non-Goals:**

- 不更改選單命令、樣式或 permanent-delete 行為。
- 不重寫 Windows Shell context menu。
- 不處理與右鍵定位無關的 UI。

## Decisions

1. 擴充 UI action 的 context-menu payload，加入明確命名的 client point，同時保留既有 screen point。這比在 state 以視窗 origin 反向換算可靠，因為 state 不應依賴 render-time window geometry。
2. 滑鼠路徑直接保存 GPUI event position；鍵盤路徑先建立聚焦列的 client anchor，再透過既有 `context_menu_coordinates` 得到 screen anchor。
3. `begin_context_menu_request` 接受兩套座標。判定 ADB／SFTP 時寫入 client point；其他 provider 與 Local request 仍寫入 screen point。
4. overlay clamp 依其實際寬高常數計算，避免右下溢出。既有命令 routing 不變。
5. Remote overlay 使用獨立且明確的 close action；dispatcher 不再把任意非 `ShowContextMenu` action 視為關閉訊號。只有 overlay 點擊、Esc、再次開啟或已知 remote menu command 關閉它。
6. Background context hit 以完整 file-view viewport 與 `file_origin` 判定，不以 scroll-content bounds 判定。row handler 繼續停止傳播，維持 Items target 優先權。
7. Background secondary-button handler 必須由外層 full-height viewport owner 註冊；content-only scroll element 不得成為唯一事件 owner，否則短清單下方不會產生事件。

替代方案是將 screen point 轉回 client point，改動較少但會把視窗幾何耦合到 state；另一方案是全面改用 client point，會破壞 Windows Shell API 契約，因此不採用。

## Risks / Trade-offs

- [Risk] 新增 payload 欄位可能漏改某個鍵盤或 fallback 建立點 → 以編譯錯誤和 focused action tests 枚舉所有建構點。
- [Risk] client 與 screen 欄位再次被誤用 → 使用具名欄位並讓 remote／Shell 分支在同一 state 函式內選擇。
- [Risk] 選單靠近邊界仍溢出 → 以純定位 helper 測試一般、負值與右下邊界。
- [Risk] pointer／hover action 意外關閉選單 → 以 action lifecycle focused test 固定不變條件。
- [Risk] 短清單的 scroll content 小於 viewport，空白區被錯誤拒絕 → 使用 viewport-based 純 helper 測試空白與邊界。

## Migration Plan

這是內部 UI payload 的原子修改，不需資料遷移。若 focused tests 失敗，可回退 payload 與 routing 修改，不影響遠端檔案資料。

## Open Questions

無。
