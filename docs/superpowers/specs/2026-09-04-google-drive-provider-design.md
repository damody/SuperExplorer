# Native Google Drive provider design

SuperExplorer adds a native `GdriveProvider` beside ADB, SFTP, and FTP. Drive only implements Google API operations; navigation, clipboard, drag-and-drop, transfer center, cancel, and progress reuse `RemoteProvider`. Cyberduck's `googledrive` module is the semantics reference (OAuth, file ids, My Drive root, Docs export, trash).

## Locked product decisions

- First release is **My Drive explorer parity** only: list, upload, download, rename, create folder, delete-to-trash, and transfers with local/ADB/SFTP/FTP.
- Out of scope: Shared with me, Shared Drives, starred, versioning, sharing URLs, Drive search UI, Google Photos, service accounts.
- Auth is SuperExplorer's own Google Cloud **Desktop** OAuth client (PKCE + `http://127.0.0.1:<port>/`). Client id is not Cyberduck's.
- Connect lives in the navigation pane. Address bar shows `gdrive://you@gmail.com/…`.
- Google Docs/Sheets/Slides download as `.docx`/`.xlsx`/`.pptx`. Uploads stay binary files (no conversion to Google-native types).
- Delete patches `trashed=true`. Confirmation says this is Google Drive trash, not the Windows Recycle Bin, and is not permanent. No restore UI in v1.
- UI paths stay human-readable. Operations bind Drive file ids. Duplicate names all appear; address-bar path resolution picks the first non-trashed match.

## Remaining decisions (locked here)

- Scheme is `gdrive` (not `googledrive`). `gdrive://` with empty authority starts Connect.
- Profile alias is the account email. Canonical authority may contain exactly one `@`.
- My Drive root is `gdrive://email/` mapped to Drive id `root`. No synthetic `My Drive` path component.
- `VirtualLocationDescriptor.provider_entry_key: Option<String>` carries the Drive file id. Parent breadcrumbs clear it and re-resolve by path.
- Drive shortcuts are listed as their target kind and use the target id.
- Google Drawings export as PDF. Forms/Sites/Maps and other non-exportable native types list but fail download with a clear error.
- Names containing `/` or `\` are percent-encoded in path components.
- OAuth: `https://www.googleapis.com/auth/drive`, `access_type=offline`, `prompt=consent`. Refresh token at `SuperExplorer/GDRIVE/<email>`.
- Client id load order: `SUPEREXPLORER_GOOGLE_OAUTH_CLIENT_ID` (+ optional secret), then `%LOCALAPPDATA%\RustGpuiExplorer\remote\google-oauth.json`, then a bundled constant if non-empty. Missing config is an actionable error, not a panic.
- Rate limits: retry 429 and Drive `rateLimitExceeded`/`userRateLimitExceeded` with backoff (max 5).
- Unix permissions and symlink create are capability-off. No chmod/新增捷徑 on Drive.
- Multiple Google accounts are allowed as multiple profiles. Connect always opens the account picker. Re-authorizing the same email replaces the refresh token.

## Architecture

```
Navigation / address bar
        |
        v
explorer-app remote_service (OAuth coordinator, profile JSON)
        |
        v
GdriveProvider : RemoteProvider
        |
        +-- GdriveOAuth (PKCE, loopback, refresh)
        +-- GdriveTransport (reqwest blocking; mock in tests)
        +-- Drive v3 REST (list/get/export/upload/mkdir/rename/trash)
        |
        v
TransferEngine / clipboard / drag-drop (unchanged)
```

## Secret isolation

- Refresh tokens never appear in profile JSON, session, history, bookmarks, clipboard, titles, or Debug.
- Access tokens live only in memory and are refreshed on 401.
- `GdriveProfile` has no token fields.

## Testing

- Unit tests use an in-memory `GdriveTransport` that records HTTP and returns canned JSON.
- No live Google calls in CI.
- Model tests cover `gdrive://email/path` parsing, `@` in authority, and secret-free profile JSON.
