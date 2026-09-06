## ADDED Requirements

### Requirement: Native FTP provider
The system SHALL expose a native `ftp` RemoteProvider that performs protocol operations without rewriting SFTP and without invoking `curl.exe`.

#### Scenario: Provider identity
- **WHEN** a virtual location has `provider_id` `ftp`
- **THEN** the FTP provider SHALL handle list, download, upload, mkdir, rename, delete, and metadata

### Requirement: Standard FTP address input
The system SHALL parse `ftp://username@host/path` as a transient username hint plus host, then canonicalize to a credential-free `ftp://<alias>/path` after login.

#### Scenario: Standard URI prefills login
- **WHEN** the user navigates to `ftp://test@45.32.49.125/`
- **THEN** login receives username hint `test` and host `45.32.49.125`

#### Scenario: Host-only URI remains compatible
- **WHEN** the user navigates to `ftp://45.32.49.125/uploads`
- **THEN** the system resolves a saved profile or opens login without a username hint

#### Scenario: Password-bearing URI is rejected
- **WHEN** the user submits `ftp://test:secret@45.32.49.125/`
- **THEN** navigation fails through the visible invalid-address path and no persisted or diagnostic output contains `secret`

### Requirement: Secret isolation
The system SHALL store FTP passwords only in Windows Credential Manager under `SuperExplorer/FTP/<alias>` and SHALL NOT serialize them into profile JSON, history, bookmarks, clipboard, or Debug output.

#### Scenario: Profile JSON has no password field
- **WHEN** an FTP profile is persisted
- **THEN** the JSON SHALL NOT contain a `password` field

### Requirement: Passive data connections
The system SHALL default to passive mode, prefer EPSV, fall back to PASV, and replace a private PASV address with the control-connection peer when the control peer is public.

#### Scenario: Private PASV rewrite
- **WHEN** PASV returns a private IP and the control peer is a public IP
- **THEN** the data connection SHALL use the control peer IP with the PASV port

### Requirement: Capability-gated commands
The system SHALL hide or disable FTP commands that the current server/session cannot support, including symlink creation when no safe extension is detected.

#### Scenario: FTP background menu omits create-shortcut by default
- **WHEN** the current location is an FTP folder and symlink creation is unsupported
- **THEN** the background menu SHALL NOT include `新增捷徑`
