# GPUI Windows capability spike

查核日期：2026-07-26
GPUI-CE：`gpui-ce/gpui-ce` @ `6c799b8e994266233014cea66d7769675ec1967c`

GPUI source 以 `vendor/gpui-ce` Git submodule 固定。`gpui-component bc174a7...` 與此 revision 實測出現 11 個 API 編譯衝突（`container_query`、`flex_grow_1`、`flex_shrink_1` 與舊 `flex_grow(f32)`），因此已移出 dependency graph；UI 計畫改用 GPUI-CE 原生 elements 與專案內 semantic helpers。

## 已由指定 revision 原始碼確認

| 能力 | 可用 API／證據 | 結論 |
|---|---|---|
| Window options | `WindowOptions`、`WindowBounds::Windowed`、`window_min_size`、`WindowKind::Normal` | 可建立有初始/最小尺寸的 normal window |
| 自訂 titlebar | `TitlebarOptions { appears_transparent, title, .. }` | 可建立 client-rendered titlebar |
| Caption controls | `WindowControlArea::{Min,Max,Close}` | Windows 控制區可交由 GPUI/native window event 處理 |
| Drag region | `WindowControlArea::Drag` | 可標記 titlebar drag hit region |
| 程式化移動 | `window.start_window_move()` | 自訂 pointer 手勢可開始 native window move |
| Window menu | `window.show_window_menu(position)` | 可在支援平台顯示原生 window menu；Windows 實機仍需 smoke test |
| Focus | `FocusHandle`、`Focusable`、`track_focus`、`focus`、`tab_stop` | 可建立集中式 focus coordinator 與 tab traversal |
| Key bindings | `KeyBinding::new`、`cx.bind_keys`、key context | 可用 typed actions 與 scope/context 做快捷鍵 routing |
| Window activation/title | `window.activate_window()`、`window.set_window_title()` | 可控制啟動焦點與 native window title |
| 原生 window handle | `Window: raw_window_handle::HasWindowHandle`；Windows backend 產生 `Win32WindowHandle` | 可由 app/platform adapter 借用 HWND；UI public state 不保存 Win32 type |

## 待 capability test

| 能力 | 狀態 | 驗證方式 |
|---|---|---|
| Windows 11 Snap Layout hover parity | 部分完成 | `WindowControlArea::Max` 已進入 component contract 並由 Windows backend 映射 `HTMAXBUTTON`；Snap flyout hover 仍待人工實機擷取 |
| `WM_NCHITTEST`/caption message hook | 部分完成 | Drag／Min／Max／Close 均使用 GPUI-CE native non-client hit-test；OLE/context host 所需額外 hook 尚待後續 spike |
| OLE drop-target registration 所需 owner HWND | 待驗證 | 在完成原生 handle spike 後，以最小 `RegisterDragDrop` fixture 驗證 |
| `IContextMenu3` owner-draw message forwarding | 待驗證 | 以可控制 fake handler 與 native owner window fixture 驗證 |

## 風險判讀

指定 GPUI-CE revision 已提供 M0/M1 所需的 window、titlebar、caption region、focus、key-binding 與 raw window handle 基礎。Snap、OLE owner HWND 實作與 context-menu message forwarding 仍屬後續明確 spike，不能在 parity matrix 中視為已完成。GPUI-CE 內建 manifest 可實際啟動且包含 PerMonitorV2，但缺少頂層 definition `assemblyIdentity`；專案的成品 finalization 會保留上游設定、補入 x64 definition identity、原位更新唯一 manifest ID 1 並執行 `mt -validate_manifest`，不修改固定的 submodule revision。

M1 自繪 titlebar 已使用 `WindowControlArea::{Drag,Min,Max,Close}`；GPUI-CE Windows backend 會將其轉成 `HTCAPTION`／`HTMINBUTTON`／`HTMAXBUTTON`／`HTCLOSE`，因此 maximize/restore 與 Snap 入口交由 Windows 處理。自動測試能保證 region contract 與 headful resize/close lifecycle，但無法判定 Snap flyout 的視覺與 hover timing，該項保持人工未驗收。
