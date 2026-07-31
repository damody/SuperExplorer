# OLE drag-and-drop 實作與驗證證據

## 2026-07-27 regression

- app 內跨分頁真實 OLE Clipboard copy/cut/paste 與 Explorer single/multi copy/cut/paste matrix 均通過；證據位於 `target/clipboard-evidence/20260727-regression/`。
- 指定 Explorer→app drag fixture 的 `explorer-left-copy.txt` 仍存在且 SHA-256 為 `51854EAB6238CD5C7973FA95655F2771B59670B9A95596A2365811B17129CB8A`；本次回歸只讀取並雜湊該檔，未修改或清除原始證據 fixture。

測試日期：2026-07-26（Asia/Taipei）

## 實作邊界

- Shell STA 在同一執行緒配對 `CoInitializeEx(COINIT_APARTMENTTHREADED)`、`OleInitialize`、`OleUninitialize`、`CoUninitialize`。
- 來源以真實 selection descriptors 呼叫 `SHCreateDataObject`，寫入 `Preferred DropEffect`，再以自訂 `IDropSource` 呼叫 `DoDragDrop`。Esc 回傳 `DRAGDROP_S_CANCEL`，放開起始滑鼠鍵回傳 `DRAGDROP_S_DROP`，cursor 使用 `DRAGDROP_S_USEDEFAULTCURSORS`。
- 起拖門檻直接讀取 Windows `SM_CXDRAG`／`SM_CYDRAG`，在 composition root 依 GPUI window scale 換算一次 logical pixels；100%、125%、150%、200% 都有 contract test。
- GPUI-CE submodule revision `f9740c88e5` 已在 Windows `IDropTarget` 讀取 source allowed effects、`Preferred DropEffect`、Ctrl/Shift/Alt 與 right-button，將共享 negotiation metadata 放入 `ExternalPaths`；每次 `DragEnter`／`DragOver`／`Drop` 都在同步 GPUI hit-test 後把 negotiated effect 回寫給 OLE cursor。window teardown 仍以 `RevokeDragDrop` 配對註冊。
- 本程式將同一原生 drop target 的 hit routing 分成 file view background、folder row 與 navigation pane；read-only target 回傳 None，copy/move cue 與回給 OLE 的 cursor effect來自同一個 `negotiate_effect`。UI crate 不持有 COM interface。
- 外部 drop 轉成 `DataTransferRequest::DropExternal`，由 Shell boundary 建立真實 stable filesystem identity，再轉用既有 `FileOperationRequest` 與 `IFileOperation` pipeline；沒有第二套 copy/move/conflict 實作。
- right-drag drop 先保存 target tab/generation 與 source capabilities，顯示 Copy here／Move here／Cancel terminal menu；Cancel 或 disabled choice 不建立 operation，target generation/tab 改變會拒絕 stale choice。
- drag cue 與 edge auto-scroll 由 session state 控制。32 logical-pixel edge zone 每個 drag tick 最多移動一個可見項目；離開視窗時 GPUI 清除 active drag，16 ms service pump 清除 cue/scroll state；drop、cancel、tab switch、navigation、window close 也走相同 cleanup。Shell shutdown 同步取消所有 active request token，`IDropSource::QueryContinueDrag` 會結束 nested OLE loop。

## 自動測試

```text
cargo test -p explorer-shell-win real_do_drag_drop_cancel_soak_releases_process_resources -- --nocapture
cargo test -p explorer-shell-win -- --nocapture --test-threads=1
cargo test -p explorer-model -p explorer-ui -p explorer-shell-win -p explorer-app -- --nocapture
```

`real_do_drag_drop_cancel_soak_releases_process_resources` 在真實 OLE apartment 建立 Shell `IDataObject`，執行 5 次 warm-up、25 次量測、再 25 次量測；每次由另一執行緒向 OLE loop 投遞 Esc，使真實 `DoDragDrop` 走 cancel terminal。獨立執行的實際穩定計數為：

```text
before = handles 345, GDI 22, USER 10
middle = handles 345, GDI 22, USER 10
after  = handles 345, GDI 22, USER 10
```

其他 tests 覆蓋：candidate→dragging→dropped/cancelled/failed、Windows threshold、modifier/preferred/allowed/can-write effect negotiation、gpui-ce clone-shared negotiation、right-drag copy/move/cancel、外部 background/folder drop request、來源 cancellation、target tab/generation 切換、shutdown cancellation、DPI、bounded auto-scroll 與既有 file-operation reuse。

## 2026-07-27 跨程序 desktop matrix 實際結果

- `scripts/smoke_explorer_drag_interop.ps1` 會建立四個 owned 真實資料夾，開啟 production app 與真正 Explorer HWND，排列左右視窗，再執行 app→Explorer／Explorer→app、left/right、copy/move/none 的嚴格磁碟 oracle。single 路徑已實際進入 production `BeginFileDrag`／`DoDragDrop`；反向則由真正 Explorer `IDataObject` 觸發本程式多次 `UpdateExternalDrag(Handled)`。
- 這個 Codex desktop session 的 `mouse_event`／`SendInput` release 不會讓跨程序 OLE source 產生最終 `Drop`；app→Explorer 停在 source modal loop，Explorer→app 停在 DragOver。證據保留於 `target/explorer-interop-evidence/20260727-{drag-clean-sta,drag-v27-explorer-to-app}`，沒有把 None 冒充 Copy/Move。
- 2026-07-27 最終回歸修正 harness 對「隱藏已知副檔名」與 AccessKit 延遲啟用的假設後，已能從新版 production UIA tree 找到來源列並進入 `BeginFileDrag`；`target/explorer-interop-evidence/20260727-drag-debug5` 仍停在 app→Explorer 的 OLE modal source，合成 mouse-up 沒有形成 Explorer Drop。測試程序由外層 timeout 終止，owned app HWND 與測試 Explorer window 隨後清理；10.8 保持未完成，不把分層 effect tests 代替跨程序實體 Drop。
- ignored test `real_explorer_drop_target_matrix_records_desktop_capability` 另以真正 Shell `IDataObject`、真正 Explorer HWND、真實 button down/up 與 controlled `IDropSource` 驗證本 session；Explorer 明確回 `DROPEFFECT_NONE`，測試將它分類為 desktop/input-driver capability 結果。
- 可獨立成立的 terminal/effect 證據均已通過：真實 OLE cancel/resource soak、left/right source、single/multi Shell data object、copy/move/none negotiation、right-drag Copy here／Move here／Cancel、真正 Explorer Clipboard 單檔/多檔 copy/cut，以及 app data object→Explorer paste。

結論：22.12 matrix 已實際執行並逐項記錄；功能與 OLE ownership 實作完成，但此 runner 的實體跨程序 Drop 結果列為「input-driver 不可用」，不是 parity pass。具硬體輸入或互動式專用 GUI runner 時，直接重跑同一 strict script，不需改測試 oracle。
