# 實作計畫

## 1. Model 契約

- [x] 1.1 新增 `FileSystemKind::Ftp`、`RemoteProviderKind::Ftp`、`FtpProfile`、`FtpAddressInput` 與 `RemoteProviderCapabilities`。以 `cargo test -p explorer-model --lib remote` 驗證。
- [x] 1.2 `ColumnFileSystems` 加入 FTP bit 並納入 REMOTE。以 `filesystem_scope_is_closed_and_fail_closed` 驗證。

## 2. 協定與 Provider

- [x] 2.1 實作 FTP reply／PASV／EPSV／MLSD／路徑／遮蔽 parser 測試。
- [x] 2.2 實作原生 `FtpProvider`（被動模式、取消、原子上傳或誠實降級）。以本機 fixture CRUD 測試驗證。
- [x] 2.3 `RemoteProvider::capabilities()` 預設與 FTP 覆寫。

## 3. 應用整合

- [x] 3.1 Credential Manager `SuperExplorer/FTP/<alias>`、`ftp-profiles.json`、登入／匿名／明文警告。
- [x] 3.2 網址列 `ftp://` 背景登入與左側 FTP 區段。
- [x] 3.3 所有 `adb | sftp` 遠端判斷加入 `ftp`；書籤圖示與 SFTP 區分。
- [x] 3.4 FTP 右鍵依能力隱藏 `新增捷徑`。

## 4. 驗證

- [x] 4.1 `cargo test -p explorer-model --lib remote`、`cargo test -p explorer-remote --lib ftp`、聚焦 UI 測試。
- [x] 4.2 `cargo test -p explorer-app` 與 `openspec validate add-native-ftp-provider --strict`。
- [x] 4.3 使用者視角：標準 URI 預填、僅主機相容、密碼拒絕、導覽與傳輸契約。
