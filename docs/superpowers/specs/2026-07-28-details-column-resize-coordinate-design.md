# Details 欄位拖曳座標修正設計

## 問題

Details header 的 GPUI mouse event 使用 logical px；原生 pointer capture 經
`GetCursorPos` 與 `ScreenToClient` 取得的是 Win32 physical client px。目前 resize action
先以 GPUI logical x 更新 reducer，取得 capture 後又把 Win32 physical x 直接送入同一
reducer。在 200% DPI 下，滑鼠移動 1 logical px 會被解讀為 2 px，因此欄寬移動距離是
滑鼠的兩倍；其他 DPI 也會依 scale factor 產生比例誤差。

## 核准方案

保留 Win32 pointer capture，因為欄位拖曳必須在滑鼠離開 separator 或 HWND client area
後繼續運作。所有送入 `DetailsColumnResizeSession` 的位置一律使用 logical client px：

1. GPUI `MouseDown`／`MouseMove` 的 `event.position.x` 已是 logical px，維持不變。
2. Win32 capture 回傳 physical client px 後，在 UI composition boundary 除以
   `window.scale_factor()`，只轉換一次。
3. begin 與 update 使用相同轉換 helper；scale 非有限值或小於等於零時拒絕 native
   sample，退回 GPUI action 已處理的座標。
4. reducer 繼續只計算 `current_logical_x - origin_logical_x`，不感知 DPI 或 Win32。
5. pointer capture ownership、滑鼠離窗、Escape、blur、capture-lost 與 mouse-up terminal
   行為不變。

不採用「取消 native capture」方案，因為會讓滑鼠離開 separator／視窗後無法繼續拖曳。
也不修改共用 `PointerCaptureSession` 回傳型別，避免同時改變 scrollbar 等既有消費者的
座標契約；本次只在 Details column resize 的明確邊界轉換。

## 測試

- 為 physical-to-logical helper 建立 100%、125%、150%、175%、200% table test。
- 建立 resize regression：各 DPI 下相同 40 logical px 拖曳都只增加 40 logical px。
- 驗證負 client x、非有限座標、無效 scale 不會污染 session。
- 保留 clamp、active-tab ownership、capture terminal 與滑鼠移出範圍測試。
- 執行 `smoke_sort_columns.ps1` headful case，以真實 separator drag 驗證欄寬和游標位移為
  1:1，並執行 fmt、UI tests、Clippy、OpenSpec strict 與 diff-check。

## 完成條件

在相同視窗與 Details view 中，使用者向左或向右拖曳欄位 separator 任意距離時，欄寬
的 logical 位移與滑鼠 logical 位移相同；在 100% 至 200% DPI 間不得再次出現 scale
factor 倍增，且滑鼠移出 separator 範圍後仍能持續拖曳直到 terminal event。
