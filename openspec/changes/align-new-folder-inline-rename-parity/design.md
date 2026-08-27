## Context

Local、ADB 與 SFTP 已共用建立資料夾操作與行內重新命名元件，但名稱確認前不得建立實體資料夾。因此互動式新增資料夾必須先呈現暫存列，確認名稱後才提交 provider 操作。

## Goals / Non-Goals

**Goals:**

- 按下新增後立即顯示暫存資料夾列並全選預設名稱。
- Enter 或失焦確認後才建立；Esc 取消時 provider 不產生任何項目。
- Local、ADB、SFTP 使用同一狀態機與驗證規則。
- 保留既有 F2 rename 與非互動式擴充套件建立流程。

**Non-Goals:**

- 變更 provider 檔案操作 API 或持久化格式。
- 對批次建立操作自動開啟編輯器。

## Decisions

### 使用 UI 暫存列

互動式建立不立即送出命令。State 建立綁定目前 tab、generation、parent 的暫存資料夾 identity，FileView 將它附加到目前 presentation 並以既有 `RenameEditorState` 呈現。

### 確認時轉成建立操作

commit 先執行檔名與不區分大小寫碰撞檢查，再產生 Folder `CreateItem` request。預設名稱即使未修改也必須建立。驗證失敗保留 editor；Esc 清除 editor 與暫存列。

### 預設名稱與 context 安全

名稱依序使用 `New folder`、`New folder (2)`，以目前 snapshot 不區分大小寫計算。導航、分頁切換或不相符的 generation 會終止草稿。

## Risks / Trade-offs

- [確認後 provider 建立失敗] → 沿用既有 operation error，不建立半成品。
- [編輯期間目錄刷新] → 同 parent 的 refresh 更新 generation；不同位置清除草稿。
- [確認前出現外部同名項目] → commit 再檢查 snapshot，KeepBoth 作最後競態保護。
- [大型目錄虛擬化] → 暫存列納入虛擬 item count。

## Migration Plan

移除先提交、完成後定位 row 的流程，改為 provisional draft。無持久化資料遷移。

## Open Questions

None.
