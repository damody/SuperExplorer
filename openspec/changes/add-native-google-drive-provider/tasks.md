# 實作計畫

## 1. Model 契約

- [x] 1.1 新增 `FileSystemKind::Gdrive`、`RemoteProviderKind::Gdrive`、`GdriveProfile`、`gdrive://` 位址（email authority）、`provider_entry_key` 與 `gdrive_defaults` 能力。以 `cargo test -p explorer-model --lib remote` 驗證。
- [x] 1.2 `ColumnFileSystems` 加入 GDRIVE bit 並納入 REMOTE。以 `filesystem_scope_is_closed_and_fail_closed` 驗證。

## 2. 協定、OAuth 與 Provider

- [x] 2.1 實作 Drive REST 對應：list query、export MIME、shortcut target、path encode、trash PATCH。以 mock transport 測試。
- [x] 2.2 實作 PKCE + loopback URL 組裝、token JSON 解析、refresh。不以真實瀏覽器當 CI。
- [x] 2.3 實作 `GdriveProvider`（list/download/upload/mkdir/rename/trash/metadata、401 refresh、429 retry）。以 mock CRUD 測試驗證。

## 3. 應用整合

- [x] 3.1 Credential Manager `SuperExplorer/GDRIVE/<email>`、`gdrive-profiles.json`、Connect OAuth 協調器。
- [x] 3.2 網址列 `gdrive://` 與左側 Google Drive 區段（連線列 + 帳號列）。
- [x] 3.3 所有遠端判斷加入 `gdrive`；書籤圖示區分；刪除確認改 Drive 垃圾桶文案；隱藏 `新增捷徑`。

## 4. 驗證

- [x] 4.1 `cargo test -p explorer-model --lib remote`、`cargo test -p explorer-remote --lib gdrive`、聚焦 UI 測試。
- [x] 4.2 `cargo test -p explorer-app --lib` 編譯通過；`openspec validate add-native-google-drive-provider --strict`。
- [x] 4.3 使用者視角：連線列、email 位址、文件 export、垃圾桶確認、缺 client id 錯誤、token 不進 JSON。
