## Why

SuperExplorer already has a stable RemoteProvider stack for ADB and SFTP. Users also need native FTP/FTPS folder browsing, transfer, clipboard, and drag-and-drop without rewriting SFTP or shelling out to `curl.exe`.

## What Changes

- Add a native `FtpProvider` that implements the existing `RemoteProvider` contract.
- Accept `ftp://username@host/path` in the address bar, persist host-only `ftp://<alias>/path` after login, and store passwords only in Windows Credential Manager.
- Support plain FTP and Explicit FTPS. Implicit FTPS is reserved in the model but not offered in UI.
- Default to passive mode (EPSV then PASV, with private-IP rewrite).
- Report `RemoteProviderCapabilities` so FTP can hide chmod/symlink-create when unsupported.
- Keep SFTP/ADB behavior unchanged except for the small shared capability surface.

## Capabilities

### New Capabilities

- `native-ftp-provider`: Native FTP/Explicit FTPS filesystem access through the existing remote framework.

### Modified Capabilities

- `standard-sftp-userinfo-addresses`: FTP address-bar user-info uses the same `username@host` order as the standardized SFTP parser. The reversed `host@username` form is not inferred.

## Impact

- `crates/explorer-model`: `FileSystemKind::Ftp`, `FtpProfile`, `FtpAddressInput`, `RemoteProviderCapabilities`.
- `crates/explorer-remote`: `FtpProvider`, protocol parser, optional Explicit FTPS upgrade.
- `crates/explorer-app`: profile JSON, Credential Manager target `SuperExplorer/FTP/<alias>`, address-bar login.
- `crates/explorer-ui`: address-bar `ftp://`, left-nav FTP section, distinct bookmark icon, remote menus gated by capabilities.
- Tests use an in-process FTP fixture. `45.32.49.125` is a manual smoke host only; its password is never committed.
