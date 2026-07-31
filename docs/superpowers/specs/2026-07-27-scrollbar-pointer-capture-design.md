# Windows 檔案總管捲軸拖曳捕捉設計

## 目標

左右垂直捲軸的 thumb 被滑鼠左鍵按住後，即使游標橫向離開捲軸、移到內容區或暫時離開應用程式視窗，仍依游標的垂直位置連續更新捲動進度。放開左鍵、收到 capture-lost、按 Esc、視窗失焦或目標 ScrollHandle 失效時，拖曳必須只結束一次。

## 已評估方案

1. **Win32 pointer capture 加 GPUI 全窗 drag surface（採用）**：`SetCapture` 保證視窗外仍收到滑鼠訊息；GPUI 全窗透明 drag surface 保證視窗內游標離開原 scrollbar element 後，事件不會改投遞給內容列。語意最接近 Windows Explorer，代價是需要一個小型 audited Win32 RAII 邊界。
2. **只建立 GPUI 全窗透明 drag surface**：能涵蓋應用程式視窗內，但游標離開 HWND 後不能保證收到 move/up，不符合完整 Windows 行為。
3. **只在根元素監聽 bubbling mouse move**：改動少，但子元素可能停止 propagation，overlay、menu 與 row drag 也會造成不可靠的事件路徑。

## 架構

`AppViewState` 保存唯一 `ScrollbarDragSession`，內容為 scrollbar identity（navigation 或 file view）、開始時的 thumb grab offset，以及 active mouse button。session 不保存 `ScrollHandle` 或 native HWND，避免 presentation state 攜帶 UI/native handle。

`ExplorerAction` 增加 begin、update、end 三種 typed action。`ExplorerRoot` 是 `ScrollHandle` 的既有 owner，因此由它依 scrollbar identity 選擇正確 handle，將全域 pointer Y 轉成 track-local Y，使用目前 viewport、max offset 與 thumb height 計算目標 offset並 clamp。開始拖曳只允許 pointer 位於 thumb；點擊 track 仍執行既有 page-up/page-down且不建立 session。

拖曳開始後，`ExplorerWindow` 在最高層 render 一個透明 drag surface，覆蓋整個 client area，提供 `col-resize` 以外的標準 pointer cursor，接收 mouse move、mouse up 與 mouse-up-out。Win32 capture adapter 對 HWND 呼叫 `SetCapture`，以 RAII／單一終止路徑呼叫 `ReleaseCapture`；capture 被其他視窗奪走時視為 cancel。所有終止原因都可重複呼叫而不產生第二次狀態變更。

## 座標與行為

- 只使用 pointer Y；橫向距離不影響捲動。
- 保留開始時 pointer 相對 thumb top 的 grab offset，避免 thumb 在 mouse-down 時跳到置中。
- pointer 高於 track 時 clamp 到 0；低於 track時 clamp 到 maximum。
- resize、資料量縮小或 DPI 改變時，每次 move 都重新讀取 handle bounds/max offset，不使用過期幾何。
- navigation pane 與 file view 共用同一計算函式；同一時間只容許一個 session。
- wheel、track paging、Home/End 與 row drag 不得被非拖曳狀態的透明層攔截。

## 終止與錯誤處理

Mouse Up、Mouse Up Outside、Esc、window deactivate、capture lost、切 tab、關閉視窗及 handle 無有效 scroll range都呼叫同一個 idempotent end transition。`SetCapture` 失敗時退化成 client-area drag surface，仍可在視窗內拖曳，並寫入非敏感診斷；不因 capture 失敗終止應用程式。

## 驗證

- 純函式測試：grab offset、上下越界 clamp、零 viewport、零 maximum、resize 後重算。
- reducer 測試：begin/update/end、重複 end、Esc、tab switch、失焦及兩 scrollbar互斥。
- GPUI interaction 測試：從 thumb 開始，游標移到內容區後仍改變 offset；track click不建立 drag session。
- Windows headful smoke：在真實長資料夾中拖曳右側 thumb，將游標移到視窗中央與 HWND 外側後繼續上下移動，再於外側放開；驗證 offset持續變化、放開後停止，並對左側 navigation scrollbar重複一次。

## 範圍

本變更只處理垂直 scrollbar pointer capture與終止語意，不改 scrollbar寬度、配色、內容排序或水平 scrollbar設計。
