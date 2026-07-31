# 原生檔案操作驗證證據

驗證日期：2026-07-26（Asia/Taipei）

## 實作邊界

- 所有 `IFileOperation`、`IShellItem` 與 `IFileOperationProgressSink` 都只存在於 Shell STA。
- progress sink 在同一 apartment `Advise`／`Unadvise`；callback 只進行取消檢查、HRESULT 保存與 bounded `try_send`。
- progress 依原生 work 百分比 coalesce，單次 operation 最多送出 101 個中間更新；terminal 另行送出。
- name collision 必須使用 `Prompt`、`Skip`、`Replace` 或 `KeepBoth` typed decision；`Prompt` 遇到已存在目的地時不執行 operation。
- Access denied、collision、cancelled HRESULT 分別映射成 Authorization、Conflict、Cancellation。

## 真實磁碟 oracle

OpenSpec 25.2 的 end-to-end operation flow 由同一組 production Shell STA／model contract 測試構成：`real_file_operations_match_safe_disk_oracle` 依序執行 create、rename、multi-copy、multi-move、mixed conflict partial、Recycle Delete、confirmed Permanent Delete 與 rename undo/redo；`large_real_copy_cancellation_has_one_terminal_and_no_late_progress` 驗證進度後取消、恰好一個 terminal 與 late-progress 抑制；operation-center model tests 驗證 progress 單調與 terminal phase。每一步都以 owned fixture 的來源、目的地、檔名及 bytes 作 oracle，而非只檢查 HRESULT。

執行：`cargo test -p explorer-shell-win real_file_operations_match_safe_disk_oracle -- --nocapture`

實際結果：通過。涵蓋建立資料夾、Unicode 路徑 rename、多選 copy、多選 move、mixed collision partial、Recycle Delete、明確確認後的多項 Permanent Delete，以及 rename 的 undo/redo 磁碟往返。每個 destructive target 在提交前重新 canonicalize，並驗證仍位於帶 ownership marker 的 fixture root。

2026-07-27 chrome regression 重跑：`real_file_operations_match_safe_disk_oracle` 與 `large_real_copy_cancellation_has_one_terminal_and_no_late_progress` 均通過；輸出位於 `target/file-operation-evidence/20260727-regression/`。

執行：`cargo test -p explorer-shell-win large_real_copy_cancellation_has_one_terminal_and_no_late_progress -- --nocapture`

實際結果：通過。建立 128 個 1 MiB 真實檔案，在收到原生 operation progress 後取消；terminal 為 Cancelled、目的地未複製完 128 項，terminal 後沒有舊 request progress。

執行：`cargo test -p explorer-shell-win real_move_covers_cross_volume_and_reparse_capability -- --nocapture`

實際結果：通過。reparse 建立權限可用時驗證移動 link 本身且 target 未移動；來源與 `D:\test\target` fixture 位於不同 volume 時驗證跨 volume move。若 OS 禁止建立 symlink 或只有單一 volume，測試明確記錄 capability unavailable，不虛構成功。

## 安全負向矩陣

執行：`cargo test -p explorer-test-support`

實際結果：6 passed、1 個明確標示的 100k soak ignored。自動拒絕 fixture root、drive root、workspace root、user profile、unresolved parent、fixture 外路徑與 reparse escape。

## Windows / API 限制

- Recycle Bin 保留時間、容量政策、使用者清空與最終可還原性由 Windows 管理；應用程式只保證使用 `FOFX_RECYCLEONDELETE | FOF_ALLOWUNDO` 回收語意，不宣稱一定可永久還原。
- 跨 volume move 取決於來源／目的地 provider 與 Windows Shell capability；失敗保留逐項 HRESULT，不回退成未告知的 copy-delete。
- 第三方 Shell namespace 若無 filesystem path，無法使用路徑 preflight；仍由無 UI 的 `IFileOperation` 結果產生 typed failure，不靜默覆寫。
