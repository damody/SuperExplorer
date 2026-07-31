# 視覺基準與回歸測試

## 最終門檻

具名 region 使用 10% 相對容差；小於 10 logical px 的值使用 1 px rounding 容差。平坦色彩每個 sRGB channel 容差為 12；typography size/line-height/baseline 為 1 logical px。只允許 favorites 動態文字／數量與 ClearType 字緣 mask，mask 不得覆蓋 layout bounds。baseline 仍只能透過明確 `update_visual_baseline.ps1 -Approve` 更新，capture/compare 永遠唯讀 reference。

2026-07-27 最終 175% `D:\` 報告是 `target/explorer-reference-evidence/real-d-light-all-gates-parity-final/report.json`；重現時 app 使用 `-Width 1520 -Height 919 -RealPath 'D:\'`，並將 `docs/visual/explorer-d-drive-light-175-regions.json` 與 app `diagnostics.json` 傳給 comparator。

視覺測試使用固定的 M1 disconnected fixture，鎖定 logical window size、theme、DPI expectation、`Microsoft JhengHei UI` 與 placeholder state。應用程式完成第一幀後會寫入 `visual_fixture_ready`；擷取腳本必須同時等到這個 marker、diagnostics 檔與 HWND，不以固定 sleep 作為唯一同步機制。

## 擷取 actual

```powershell
./scripts/capture_visual_fixture.ps1 -Theme light -ExpectedDpiPercent 100 -State populated
```

輸出位於 `target/visual-actual/<timestamp-id>/`：

- `screenshot.png`：包含視窗框的實際畫面。
- `diagnostics.json`：fixture、semantic colors、layout contract 與 GPUI scale factor。
- `metadata.json`：Windows/Explorer/app/GPUI 版本、DPI、theme、尺寸、font、時間與檔案雜湊。
- `explorer.log`：包含 `visual_fixture_ready`、停止與 clean shutdown 證據。

正常擷取會用 `GetDpiForWindow` 驗證實際 DPI，必須在對應的 100/125/150/200% Windows session 執行。`-AllowDpiMismatch` 只供本機驗證擷取工具；其 metadata 會標為 `matches_expectation: false`，baseline update 必定拒絕這類成果。

## 比較

```powershell
./scripts/compare_visual_baseline.ps1 `
  -BaselineDirectory visual-baselines/windows-26200-light-100 `
  -ActualDirectory target/visual-actual/<capture>
```

比較不修改 baseline。輸出保留 `baseline.png`、`actual.png`、`diff.png`、雙方 diagnostics/metadata 與 `report.json`。DWM 外框使用明確 edge mask，文字 antialiasing 使用 channel tolerance；semantic RGBA 與 layout diagnostics 使用嚴格門檻。

## 更新 baseline

人工檢查 actual、metadata 與 diff 後，明確執行：

```powershell
./scripts/update_visual_baseline.ps1 `
  -ActualDirectory target/visual-actual/<capture> `
  -BaselineDirectory visual-baselines/windows-26200-light-100 `
  -Approve
```

未提供 `-Approve`、DPI 不符或缺少任何必要檔案都會拒絕更新。baseline 不會由 capture 或 comparison 流程自動覆寫。

## Deterministic interaction states

`capture_visual_fixture.ps1` 的 `-InteractionState normal|hover|pressed` 會以實際
Win32 pointer messages 驅動 production GPUI control。`pressed` 在截圖後先送出
`WM_CANCELMODE`，再於 hit target 外放開，避免 fixture 觸發檔案命令。`-State focused`
建立可重現的 Search keyboard focus；`empty` 與 `populated` 分別提供 disabled 與
selected 樣本。視窗以 `PrintWindow(PW_RENDERFULLCONTENT)` 擷取，避免其他桌面視窗
遮蔽或進入證據。

2026-07-26 實際證據位於 `target/interaction-evidence/{hover,pressed,focused-light,
focused-dark,disabled,selected}`。同尺寸 1984×1272 的 hover/pressed PNG 有 7,080
個差異像素（0.280546%），差異 bounding box 為 `(33,103)-(164,158)`，精確落在
`+ New` control。此工作階段的實際 DPI 是 168（175%），metadata 明確標記
`matches_expectation=false`，因此可證明 fixture/interaction state，但不能升格為
100/125/150/200% 正式 baseline。

`-WindowActivation active|inactive` 會送出 deterministic
`WM_ACTIVATEAPP/WM_NCACTIVATE/WM_ACTIVATE`，production component 仍透過
`Window::is_window_active()` 選擇 `SelectedActive` 或 `SelectedInactive`。175% 實測
active/inactive 差 77,169 pixels（4.864999%），bounds
`(33,32)-(1586,311)`；證據位於 `target/interaction-evidence/{active,inactive}`。

## Windows Explorer cross-app reference

```powershell
./scripts/capture_explorer_reference.ps1 `
  -LocationUrl 'file:///D:/' -Theme light `
  -OutputDirectory target/explorer-reference-evidence/d-drive-light-175

./scripts/compare_explorer_reference.ps1 `
  -ExplorerDirectory target/explorer-reference-evidence/d-drive-light-175 `
  -ApplicationDirectory target/explorer-reference-evidence/app-light-175 `
  -OutputDirectory target/explorer-reference-evidence/light-diff-175 `
  -PythonExecutable <python-with-Pillow>
```

Explorer 與 app 是不同 renderer，專用 comparison 只做相同 top-left physical region 的
pixel evidence，不比較不存在於 Explorer 的 app semantic diagnostics，也不把 non-zero diff
當作測試框架失敗。2026-07-26 light comparison 的尺寸為 2685×1621 對
2684×1620（1 px rounding），changed ratio 23.281977%。

## Deterministic feature states

`-State` 接受 `empty`、`populated`、`error`、`multi-tab`、`operation`、`drag-cue`、`search`。這些 fixture 直接建立 production model state，不使用測試 crate、真實 C:\ 內容或 Shell watcher，因此 screenshot 可重現。`populated` 的已選 row 同時是 context-menu 入口／selection style evidence；`drag-cue` 顯示 folder target；`operation` 顯示 operation center；`search` 顯示 partial fallback terminal。

```powershell
foreach ($state in @('empty','populated','error','multi-tab','operation','drag-cue','search')) {
    ./scripts/capture_visual_fixture.ps1 `
        -Theme light -ExpectedDpiPercent 100 -State $state
}
```

Harness 在 application ready marker 後呼叫 `DwmFlush`，讓 `CopyFromScreen` 等待 DWM present，避免擷取半完成 frame。2026-07-26 的實際 175% DPI state artifacts 位於 `target/visual-state-evidence/<state>`；它們只證明 state/harness，因 expected 100% 與 actual 175% 不符，不能成為正式 baseline。
