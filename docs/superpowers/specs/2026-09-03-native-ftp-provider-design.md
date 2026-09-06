# Native FTP provider design

SuperExplorer adds a native `FtpProvider` beside ADB and SFTP. FTP only implements protocol operations; navigation, clipboard, drag-and-drop, transfer center, cancel, and progress reuse `RemoteProvider`.

## Decisions

- Address-bar user-info is `ftp://username@host/path`, matching the standardized SFTP order. Canonical locations are `ftp://<alias>/path`.
- Passwords live only in Windows Credential Manager under `SuperExplorer/FTP/<alias>`.
- Plain FTP and Explicit FTPS are supported. Implicit FTPS is reserved and rejected.
- Passive mode is default (EPSV then PASV, with private-IP rewrite).
- `RemoteProviderCapabilities` hides FTP commands the server cannot support, including create-shortcut by default.
- `45.32.49.125` is a manual smoke host. Its password is never committed.
