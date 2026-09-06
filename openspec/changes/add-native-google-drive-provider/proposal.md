## Why

SuperExplorer already browses ADB, SFTP, and FTP through `RemoteProvider`. Users also need first-class Google Drive (My Drive) in the same Explorer chrome: OAuth login, list/CRUD, Office export of Google Docs, trash, and transfers with local/other remotes. Cyberduck's Drive module is the behavior reference, not a Java port.

## What Changes

- Add `gdrive` as a remote provider family beside ADB/SFTP/FTP.
- Desktop OAuth 2.0 with PKCE and a loopback redirect; refresh tokens live only in Windows Credential Manager.
- Navigation pane **Google Drive** section with a Connect control; canonical addresses `gdrive://you@gmail.com/path`.
- Drive API v3 list/download/upload/mkdir/rename/trash using file ids while the UI still shows folder paths.
- Download Google Docs/Sheets/Slides as `.docx`/`.xlsx`/`.pptx`. Delete moves to Drive trash.
- Reuse transfer, clipboard, and drag-and-drop. Hide chmod/symlink.

## Capabilities

### New Capabilities

- `native-google-drive-provider`: Native Google Drive My Drive access through the existing remote framework.

### Modified Capabilities

- Remote address parsing and navigation pane gain a `gdrive` family. SFTP/FTP/ADB behavior stays unchanged except additive provider-id matches.

## Impact

- `crates/explorer-model`: `FileSystemKind::Gdrive`, `RemoteProviderKind::Gdrive`, `GdriveProfile`, string `provider_entry_key`, capabilities.
- `crates/explorer-remote`: `GdriveProvider`, OAuth PKCE, Drive REST, mock-transport tests.
- `crates/explorer-app`: `gdrive-profiles.json`, Credential Manager `SuperExplorer/GDRIVE/<email>`, Connect/OAuth coordinator.
- `crates/explorer-ui`: Google Drive nav section, `gdrive://` address login, trash confirmation copy, capability-gated menus.
- Tests never call live Google. Client id is configured locally, never invented in git.
