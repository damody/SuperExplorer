# 真實資料夾驗證證據

## 2026-07-27 access-denied 與完整 fixture 回歸

- `real_temporary_acl_denial_returns_authorization_instead_of_empty` 在 owned temporary directory 內建立實體檔案，以目前 Windows 帳號套用可還原的 deny ACL，先確認 `ReadDirectory` 為 `PermissionDenied`，再由 production Shell STA 導覽。原先揭露 `E_ACCESSDENIED (0x80070005)` 被誤分為 Availability；修正後 terminal 為 `ExplorerErrorKind::Authorization`，不是 empty snapshot。RAII guard 於測試結束移除 deny、恢復 inheritance，並確認目錄重新可列舉後才交給 TempDir 清理。
- 同輪重跑 `real_folder_preserves_unicode_long_hidden_system_and_case_identity`、`real_watcher_detects_rename_storm_and_refresh_matches_disk_oracle`，均通過；empty/terminal contract 由 `fake_and_real_services_pass_the_same_navigation_contract` 與 empty fixture smoke 覆蓋。
- large-child-count 使用已實跑三輪的 100,000 實體檔案證據 `target/capability-soak-evidence/20260726-full-3run-v3/report.json`；每輪均 pass，未以合成 item count 取代磁碟 fixture。

## 2026-07-27 D 槽與 breadcrumb 回歸

- 只讀 D 槽 Explorer/application 同位置、175% DPI、近乎相同 capture bounds 證據：`target/explorer-reference-evidence/d-drive-light-175` 與 `app-light-175`；D 槽本身未建立、刪除或重新命名任何項目。
- `real_navigation_matrix_covers_child_back_forward_up_and_open_error` 與 `real_breadcrumb_protocol_resolves_owned_ancestry_and_direct_containers` 重跑通過。
- `target/breadcrumb-uia-evidence/20260727-keyboard-menu5/report.json` 由 UIA/實體點擊驗證「本機」列出 C:/D:/E:，`D:\test` chevron 列出 10 個直接真實子資料夾、每列 topmost hit-test、selection navigation、Home/End/typeahead；empty/error/cancel/stale terminal 由同一 typed component model test 覆蓋。

本文件只記錄已實際執行的結果；需要系統管理權限或外部 Windows 設定而未執行的案例，明確標為未驗證。

## 多分頁 end-to-end

2026-07-27 回歸新增 `two_tab_address_drafts_and_concurrent_menu_requests_reject_late_events`：兩分頁分別持有 `C:\fixture-a`／`D:\fixture-b` location 與不同未提交 draft，同時建立具獨立 request context 的 breadcrumb menu 列舉；切換分頁會取消離開分頁的 request，關閉分頁後其 terminal event 仍為 `IgnoredStale`，另一分頁 draft 與 location 不受影響。此 model/UI E2E 與真實 Shell `end_to_end_two_tabs_navigation_history_and_watcher_are_isolated`、`real_d_unicode_and_parsing_name_two_tab_state_isolation` 共同覆蓋地址列與實體資料夾邊界。

`cargo test -p explorer-shell-win end_to_end_two_tabs_navigation_history_and_watcher_are_isolated -- --nocapture` 於 2026-07-26 本機通過（0.27 s）。測試以兩個 `TabId` 導覽三個真實資料夾，逐 tab 比對 snapshot；tab A 實際進入 child 後執行 transactional Back／Forward，tab B 的 watcher 偵測新增檔案後走 correlated Refresh。切換兩個 tab 時 item count、current location、history 與 watcher mutation 互不覆寫。

UI root 的 Back／Forward／Up 已從原先只改 focus 修正為提交 `ExplorerCommand::Navigate`。Back／Forward 先 peek destination，只有 matching `LocationResolved` 才移動 history stack；Failed/stale terminal 只清除 pending traversal，保留原 committed history。

## 2026-07-26：Shell 列舉、刷新與 watcher

- Fixture ownership：每個測試使用 `tempfile::TempDir` 建立獨占 temporary root；測試只在該 root 建立、改名與刪除項目，drop 後確認由 TempDir 清理。
- Commands：`cargo test -p explorer-shell-win -- --test-threads=1`。
- 初始列舉：146 個真實項目，先收到 location metadata，再收到 count/estimated-byte 有界 batches，最後恰好一個 terminal event。
- Refresh：在磁碟新增項目後重新列舉，history 不增加、selection 與 scroll anchor 保留，snapshot 與磁碟數量一致。
- Watcher：`ReadDirectoryChangesW` 使用 overlapped handle；25 次快速 create/delete 加上一筆 emoji rename 後收到通知，overflow/reconciliation refresh 最終只留下磁碟 oracle 的 `renamed-😀.txt`，Windows file ID 與 selection 維持不變。
- Unicode matrix：繁體中文、emoji、組合字元、180 字元名稱、hidden+system attribute 與大小寫不敏感 probe 均由真實 Shell 列舉；display names、唯一 identity 與 final count 通過。
- Navigation matrix：真實 root → child → Back → Forward → Up、Refresh 及不存在檔案的 Shell open error 均通過；open error 未破壞既有 directory snapshot。
- Parser fault matrix：added/removed/modified、rename old/new pairing、odd UTF-16 length、截斷 header/name、錯誤 next offset 與 unpaired rename 均有自動測試。
- Cleanup：workspace gates 後沒有殘留 watcher thread；watcher drop 會取消 pending I/O、join thread，再關閉 event/directory handles。
- Reparse fixture：在 Windows 允許建立 symlink 時，owned root 內的 directory link 可導覽並只列舉 owned target；無權建立 symlink 時測試會輸出明確 skip 原因。導覽本身不遞迴 traversal，因此不會跟隨循環離開目前 location。
- Fake/real contract：`ImmediateNavigationService` 與真實 `ShellStaHandle` 執行同一套 Navigate contract，兩者皆先 metadata、完整 correlation、恰好一個 terminal。

## 2026-07-26：100,000 項目實跑

- Command：`cargo test -p explorer-shell-win real_100k_dataset -- --ignored --nocapture --test-threads=1`。
- Dataset generation：11.7167 s；fixture generator 產生 `item-000000.dat` 至 `item-099999.dat`，磁碟 oracle 為 100,000。
- First item／first viewport：53.6528 ms。
- Terminal enumeration：3.7006 s，model final count 100,000。
- Batching／queue evidence：1565 個 events，收到 100,000 rows，最大 batch 64；bounded event queue capacity 4096，測試期間未 overload。
- Process working set：7,933,952 bytes → 63,991,808 bytes，增量 56,057,856 bytes。
- 實跑先發現並修正兩個問題：snapshot O(n²) merge 改為 stable-ID hash index；canonical fixture 的 `\\?\` extended path 在 Shell parser 回 `E_INVALIDARG`，目前在 Shell parsing boundary 正規化 drive/UNC prefix。

## Permission-denied 案例

目前的自動測試程序以目前登入使用者執行，不能可靠建立「連建立者本身也不可讀」的 ACL，而不使用系統管理權限或冒險讓 temporary root 無法清理。因此此案例未宣告自動通過。

手動步驟：

1. 以另一個低權限測試帳號建立一個不授予目前測試帳號 `LIST_DIRECTORY` 的資料夾。
2. 在本程式 address bar 導覽該路徑。
3. 驗證只收到一個保留 HRESULT 的 recoverable error、UI 顯示安全訊息、原 snapshot 不被假空結果取代。
4. 由建立者帳號恢復 ACL 並刪除 fixture；在本文件補上 Windows build、ACL 命令、actual result 與 evidence path。

狀態：**未驗證（需要外部帳號／ACL 前置條件）**。

## Headful production evidence

- Evidence：`D:\test\target\smoke-evidence\20260726T114941584Z-36f533d8ff24407683a4310f6f32417b`
- 實際結果：真實 `C:\` first-item/first-viewport 約 46 ms、terminal 約 46 ms；1134×727 resize 至 1254×807，WM_CLOSE exit code 0，cleanup events 順序正確。
- 已知差異：GPUI-CE 在 WM_CLOSE 後仍記錄非致命 invalid-window-handle 訊息；程序 exit、STA join 與 diagnostics cleanup 均成功，此差異仍需上游/後續 hardening。
