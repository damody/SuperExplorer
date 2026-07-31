# Clipboard / Explorer 互通證據

測試日期：2026-07-26（Asia/Taipei）

測試環境由 `Get-ComputerInfo` 回報 Windows Product Name `Windows 10 Pro`、build `26200`、64-bit；`%WINDIR%\explorer.exe` FileVersion/ProductVersion 為 `10.0.26100.8875`。產品名稱與實際 build 的顯示差異屬 Windows 相容性資訊，此處同時保留兩者。

## Shell formats 與 ownership

- 選取項目以共同 parent PIDL 與 child PIDLs 呼叫 `SHCreateDataObject`，得到 Shell-compatible `IDataObject`；物件可提供 `CF_HDROP` 與 Shell IDList Array 等公開 Shell formats。
- `Preferred DropEffect` 使用 `TYMED_HGLOBAL` 的 DWORD：Copy 為 `DROPEFFECT_COPY`，Cut 為 `DROPEFFECT_MOVE`。
- 成功完成 cut/move paste 後寫入 `Performed DropEffect=DROPEFFECT_MOVE`；取消或失敗不會錯誤宣告已完成 move。
- `OleSetClipboard` 保留 COM reference。Shell STA 的 `ClipboardRuntime` 追蹤 owned reference、clipboard sequence 與 generation；ownership 改變或 clipboard clear 時會釋放 reference 並清除 cut visual。
- STA 初始化同時配對 `CoInitializeEx`/`CoUninitialize` 與 `OleInitialize`/`OleUninitialize`；關閉前以 `OleFlushClipboard` materialize delayed formats。
- `STGMEDIUM` 由 RAII guard 保證一次 `ReleaseStgMedium`，PIDL 以 `CoTaskMemFree` 釋放，`IDataObject::SetData(fRelease=true)` 成功後由 data object 接管 HGLOBAL。

## 自動化互通驗證

主要測試：

```text
cargo test -p explorer-shell-win real_ole_clipboard_copy_cut_paste_crosses_tabs_and_matches_disk -- --nocapture
cargo test -p explorer-shell-win real_explorer_single_multi_copy_cut_paste_matrix_matches_disk_effects -- --ignored --nocapture
```

通過內容：

- 同一應用程式不同 `TabId` 間執行 Copy→Paste 與 Cut→Paste，並以實際磁碟檔案、內容 bytes 與來源/目的地狀態作 oracle。
- mixed Cut→Paste 使用 Skip 造成 partial result 時，只保留失敗項目的 cut state；接著以 Replace 重試成功，才清除 clipboard cut state。
- 應用程式建立的 Shell `IDataObject` 交給系統 Clipboard，再由獨立 `powershell.exe -STA` 建立 `Shell.Application`，對真實 Explorer FolderItem 呼叫公開 `paste` verb；測試等待目的檔案出現並比對 bytes，證實本程式→Explorer 的實際輸出互通。
- 第二個 opt-in headful 測試在真實 Explorer `10.0.26100.8875` 開啟獨立視窗，以 FolderView automation 建立 single/multi selection，使用實際 HWND `SetForegroundWindow` 後送出 Ctrl+C／Ctrl+X。四個 case（single copy、multi copy、single cut、multi cut）均由 Explorer 發布 `CF_HDROP` 與 preferred Copy/Move effect，再由本程式執行 Paste。2026-07-26 本機明確執行通過（13.54 s）：copy 來源保留、cut 來源移除、所有 destination 名稱與 bytes 完全符合磁碟 oracle；沒有使用 FileDropList writer fallback。
- 預設非互動測試仍保留獨立 STA FileDropList writer 作為 CI 外部 ownership contract；它不替代上述真 Explorer headful 證據。

Fault 測試：

```text
cargo test -p explorer-shell-win external_shell_object_and_unsupported_clear_release_owned_state -- --nocapture
cargo test -p explorer-shell-win slow_clipboard_probe_becomes_recoverable_error -- --nocapture
```

涵蓋外部 Shell `IDataObject` 的多選 `CF_HDROP`、preferred effect、paste request、unsupported Unicode text、clipboard clear、stale FileDropList 與超過 250 ms 的 slow provider。錯誤都轉成 recoverable `ExplorerError`，且 medium/interface 由 RAII 釋放。

## 公開 API 限制

- OLE provider 呼叫不在 GPUI callback 執行，而固定在 Shell STA；背景 inspection 維持 250 ms slow-provider 門檻，使用者明確 Paste 的 `OleGetClipboard`／`IDataObject::GetData` 對 `CLIPBRD_E_CANT_OPEN` 使用最多 2 秒 bounded retry，以涵蓋 Explorer 剛完成操作時的短暫 clipboard lock。
- Windows 沒有保證任意第三方 `IDataObject::GetData` 能在固定時間內返回。若後續實測遇到阻塞 provider，必須改用可終止的獨立 process broker；不能以 UI thread timeout 假裝已取消 COM 呼叫。
- Explorer UI automation 受視窗重用、分頁、焦點與本地化選單影響，不是產品功能依賴；headful 測試因此以 HWND 與 Ctrl+C/Ctrl+X 避免選單文字。產品互通本身只依賴 Shell/OLE 公開介面。
