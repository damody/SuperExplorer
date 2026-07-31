# Shell context menu 實作與驗證證據

## 2026-07-27 Explorer reference regression

依使用者提供的繁中 Explorer 背景與檔案右鍵選單重新執行 production native path。證據位於 `target/context-menu-evidence/20260727-native-reference/`：

- background、single、multi selection 均由真實 `IShellFolder` 建立並有 commands；資源釋放回基線。
- popup cancel soak、`WM_INITMENUPOPUP`、`WM_DRAWITEM`、`WM_MEASUREITEM`、`WM_MENUCHAR` 與 reentrant `IContextMenu3` routing 全數通過。
- 真實安全 fixture 的 invoke 後由 `ReadDirectoryChangesW` watcher 收斂，extension failure 後下一次 session 可恢復。
- installed extension tree 實際列出 WinRAR、VS Code、7-Zip/CRC SHA、TortoiseGit、QQ、Defender、傳送到等項目；測試只安全 invoke temporary fixture 的 `加入 "third-party.7z"`，未操作使用者資料。

## 2026-07-26 installed third-party handler verification

- Registry：`HKCR\*\shellex\ContextMenuHandlers\7-Zip` →
  `{23170F69-40C1-278A-1000-000100020000}`；同機 menu 也實際列出 WinRAR、
  TortoiseGit、QQ/OneDrive 等 handler，但安全 invoke 只選 7-Zip。
- 命令：`cargo test -p explorer-shell-win
  context_menu::tests::installed_7zip_extension_queries_submenu_and_invokes_owned_archive_command
  -- --ignored --nocapture --exact`，exit 0，1.43 s。
- 測試在 owned temporary root 建立 `third-party.txt`，以 production
  `IShellFolder::GetUIObjectOf`/`IContextMenu::QueryContextMenu` 並開啟 keyboard/
  extended verbs；遞迴讀到 top-level `7-Zip` submenu、11 個 depth-1 commands，以及
  `CRC SHA` depth-2 submenu。
- 實際安全 invoke `加入 "third-party.7z"`（command id 138、relative offset 137），
  產生非空 `third-party.7z` 後由 owned fixture cleanup。沒有對使用者檔案執行命令。
- keyboard navigation contract 由 keyboard query flag、階層 submenu command IDs 與既有
  Escape popup loop 驗證；owner-draw/reentrant `WM_MEASUREITEM/WM_DRAWITEM/WM_MENUCHAR/
  WM_INITMENUPOPUP` 則由同一 native owner-window path 的 controlled IContextMenu3 fixture
  驗證。第三方 7-Zip 本身不是 owner-draw，文件沒有誤稱它是。

測試日期：2026-07-26（Asia/Taipei）

## Native session

- `ContextMenuRequest` 保存 background／items target、實際 owner HWND 數值、screen point、keyboard invocation、request correlation 與 deadline。
- domain session 嚴格走 resolve→query→show→invoke/cancel/fail→release；terminal 之後只有第一次 release 有效。
- filesystem parent/item 轉成 task-allocated PIDL；`IShellFolder::CreateViewObject` 建立 background menu，`GetUIObjectOf` 建立 single/multi item menu；PIDL 與 COM interfaces 都留在 Shell STA。
- `OwnedMenu` 唯一持有 `HMENU`，所有 cancel/error/normal terminal 都執行一次 `DestroyMenu`。
- 每次 session 建立一個 STA hidden owner window。其 wndproc 將 `WM_INITMENUPOPUP`、`WM_DRAWITEM`、`WM_MEASUREITEM`、`WM_MENUCHAR` 轉給可用的 `IContextMenu3::HandleMenuMsg2`，因此不需要 subclass GPUI HWND，也不把 apartment-affine interface 帶到 UI thread。
- `QueryContextMenu` 使用明確 command range 1..0x7fff 與 background/item 共用的公開 flags；選擇結果先驗證 range，再以相對 command offset 建立 `CMINVOKECOMMANDINFO` 呼叫 `InvokeCommand`。
- invoke 回傳只產生 `ContextMenuFinished`；directory snapshot 不被樂觀修改，檔案變更仍由既有 watcher/refresh 收斂。
- native session 現在由獨立 broker worker 擁有自己的 OLE apartment；Shell STA 與 GPUI callback 只排程工作，不直接執行 extension callback。deadline watchdog 與 worker 以 atomic terminal gate 競速，timeout/error/success/cancel 恰好送出一次 `ContextMenuFinished`，晚到 handler 結果只留下含 request correlation 的診斷。

## UI selection contract

- 右鍵已選項目會保留 stable-ID multi-selection。
- 右鍵未選項目才切換成單選。
- 右鍵 file-view 空白會清除選取並建立 background request。
- Ctrl+左鍵可建立 additive stable-ID selection；context menu request 不使用 row index 作為 item identity。

## 真實自動測試

```text
cargo test -p explorer-shell-win real_background_single_and_multi_shell_menus_have_commands_and_release -- --nocapture
cargo test -p explorer-shell-win real_popup_cancel_soak_forwards_messages_and_releases_menu_resources -- --nocapture
cargo test -p explorer-shell-win controlled_owner_draw_handler_forwards_reentrant_messages_and_releases_in_order -- --nocapture
cargo test -p explorer-shell-win bounded_broker_isolates_slow_hung_and_error_handlers_with_correlation -- --nocapture
cargo test -p explorer-shell-win end_to_end_context_menu_query_invoke_watcher_and_failure_recovery -- --nocapture
cargo test -p explorer-ui context_menu_failure_is_recoverable_and_rejects_stale_correlation -- --nocapture
```

第一項在真實 temporary folder 建立兩個檔案，依序向 Windows Shell 查詢 background、single-selection、multi-selection menu，三者均取得大於零的原生命令數，且 session 後釋放 PIDL/interface/HMENU/HWND。

第二項實際開啟 10 次 `TrackPopupMenuEx`，由測試執行緒投遞 Esc 走 cancel terminal；驗證 owner wndproc 確實收到並轉送 menu messages，最後 active HMENU 與 owner HWND counter 回到測試前數值。

第三項使用真正實作 `IContextMenu3` 的可控 COM fixture，插入 owner-draw command，逐一驗證 `WM_MEASUREITEM`、`WM_DRAWITEM`、`WM_MENUCHAR`、`WM_INITMENUPOPUP`；handler 在 init-popup callback 內同步重入 `WM_MENUCHAR`，實際觀察到五筆有序訊息。釋放順序固定為 HMENU→owner HWND→最後 COM handler reference，資源 counter 回到基線。

第四、五項注入 slow、超過 deadline 的 hang 與立即 error。呼叫端在 handler 完成前立即返回；20 ms watchdog 產生含 correlation/deadline 的 recoverable outcome，150 ms 晚到結果被抑制；隨後的第二個 context menu 仍能成功建立。UI 只接受目前 pending request 的完整 correlation，將錯誤顯示在 status bar，下一次 cancel/success 會清除錯誤。

`end_to_end_context_menu_query_invoke_watcher_and_failure_recovery` 於 2026-07-26 本機通過（1.05 s）：在同一 owned 真實資料夾依序取得 Windows 內建 background/single/multi menu；再由可控 `IContextMenu3` owner-draw extension 執行 command 0，建立真實檔案並比對 bytes，`ReadDirectoryChangesW` watcher 收到相同 tab/generation 的變更。最後注入 extension error，再提交第二個 broker job 成功 cancel，證明 failure terminal 後仍可恢復。

## Extension 隔離限制

activation/query/show/invoke 已移到獨立 OLE worker，永久 hang 不再阻塞 GPUI 或主要 Shell STA，deadline 後 UI 可繼續操作。不過 Windows in-process `IContextMenu` handler 若永久 hang，該 worker thread 本身仍無安全的強制中止 API；目前會隔離並遺棄該 worker。要回收惡意/永久卡死 handler 所占的 thread/COM/HMENU/HWND，仍需要可終止的獨立 process broker。

因此 OpenSpec 23.9、23.10 已由可控 fixture 與 UI correlation 測試完成；23.11 仍以本文件明確記錄 process-isolation 限制與 fallback。至少一個可控第三方 extension 的實機安裝/Explorer 比對結果（23.13）仍待補，沒有以 Windows 內建 handler 冒充。
