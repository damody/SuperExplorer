## ADDED Requirements

### Requirement: Native Google Drive provider
The system SHALL expose a native `gdrive` RemoteProvider that performs Drive API v3 operations without rewriting SFTP/FTP/ADB and without shelling out to `rclone` or `curl.exe`.

#### Scenario: Provider identity
- **WHEN** a virtual location has `provider_id` `gdrive`
- **THEN** the Google Drive provider SHALL handle list, download, upload, mkdir, rename, trash, and metadata for My Drive

### Requirement: OAuth login and secret isolation
The system SHALL authenticate with Google OAuth 2.0 (Desktop PKCE, loopback redirect) and SHALL store refresh tokens only in Windows Credential Manager under `SuperExplorer/GDRIVE/<email>`.

#### Scenario: Connect from navigation pane
- **WHEN** the user activates the Google Drive Connect control
- **THEN** the system SHALL open the system browser for Google consent and, on success, persist a non-secret profile keyed by the account email and navigate to `gdrive://<email>/`

#### Scenario: Profile JSON has no token field
- **WHEN** a Google Drive profile is persisted
- **THEN** the JSON SHALL NOT contain `refresh_token`, `access_token`, or `password`

#### Scenario: Missing OAuth client is actionable
- **WHEN** Connect runs and no client id is configured
- **THEN** the user SHALL see an error telling them to set `SUPEREXPLORER_GOOGLE_OAUTH_CLIENT_ID` or `%LOCALAPPDATA%\RustGpuiExplorer\remote\google-oauth.json`

### Requirement: Canonical gdrive addresses
The system SHALL parse `gdrive://you@gmail.com/path` as a credential-free location whose authority is the account email. Empty `gdrive://` SHALL start Connect.

#### Scenario: Email authority is canonical
- **WHEN** the user navigates to `gdrive://you@gmail.com/Work`
- **THEN** the location authority is `you@gmail.com` and components are `["Work"]`

#### Scenario: Password-bearing URI is rejected
- **WHEN** the user submits `gdrive://you:secret@gmail.com/`
- **THEN** navigation fails through the invalid-address path and no persisted or diagnostic output contains `secret`

### Requirement: File id operations with path display
The system SHALL display folder paths and SHALL perform mutations using Drive file ids carried on listed entries.

#### Scenario: Duplicate names remain distinct
- **WHEN** My Drive contains two files named `Report.docx` in the same folder
- **THEN** both SHALL appear, and delete/rename of one SHALL NOT affect the other

#### Scenario: Address bar duplicate resolution
- **WHEN** the user types a path that matches more than one non-trashed item
- **THEN** the system SHALL open the first non-trashed match

### Requirement: Google native document export
The system SHALL export Google Docs, Sheets, and Slides to Office Open XML on download, and SHALL upload local files as their original bytes without converting them to Google-native types.

#### Scenario: Document download
- **WHEN** the user downloads a Google Doc named `Notes`
- **THEN** the local file SHALL be `Notes.docx`

### Requirement: Trash delete
The system SHALL move Drive items to Google Drive trash (`trashed=true`) instead of calling permanent `files.delete`.

#### Scenario: Delete confirmation names Drive trash
- **WHEN** the user deletes a Drive item from SuperExplorer
- **THEN** confirmation SHALL state the item goes to Google Drive trash, not the Windows Recycle Bin, and SHALL NOT claim the action is permanent

### Requirement: Capability-gated commands
The system SHALL hide Unix permission and create-shortcut commands on Google Drive locations.

#### Scenario: Drive background menu omits create-shortcut
- **WHEN** the current location is a Google Drive folder
- **THEN** the background menu SHALL NOT include `新增捷徑`
