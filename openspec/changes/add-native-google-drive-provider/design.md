## Context

ADB, SFTP, and FTP already share `RemoteProvider`, `TransferEngine`, clipboard, and drag-and-drop. Google Drive must plug into that stack. Cyberduck implements Drive as OAuth + Drive API v3 + file-id mapping; SuperExplorer copies those semantics, not Cyberduck's Java types.

## Goals / Non-Goals

**Goals:**

- Native `GdriveProvider` for My Drive list/CRUD/recursive transfer/cancel/progress.
- Desktop PKCE OAuth, Credential Manager isolation, `gdrive://email/path`.
- Path UI with file-id operations; Docs export; trash; capability-gated chmod/symlink UI.
- Mock HTTP tests; no committed secrets.

**Non-Goals:**

- Shared with me, Shared Drives, starred, versions, share links, Drive search.
- Porting Cyberduck Java or using rclone/curl.
- Google verification of the OAuth consent screen (operators configure a test client).
- Rewriting SFTP into a generic NetworkProvider.

## Decisions

- Independent `GdriveProvider`, not an SFTP refactor.
- Authority is the Google email (one `@` allowed only for `gdrive`).
- `provider_entry_key` on `VirtualLocationDescriptor` holds the Drive file id.
- Passwords/tokens live only under `SuperExplorer/GDRIVE/<email>`.
- `RemoteProvider::capabilities()` hides symlink/unix mode.

## Risks / Trade-offs

- [Unverified OAuth client is limited to test users] → document Cloud Console setup; fail Connect with an actionable error.
- [Drive names and duplicate names] → encode hostile characters; bind ids from listing.
- [Google Docs are not bytes] → export on download; refuse non-exportable native types with a clear error.
- [Rate limits] → bounded retry on 429 / rateLimitExceeded.

## Migration Plan

No existing Drive data. Rollback is code-only; SFTP/FTP/ADB profiles stay untouched. Drive profiles and Credential Manager secrets remain unused if the provider is removed.

## Open Questions

None. Product choices are locked in `docs/superpowers/specs/2026-09-04-google-drive-provider-design.md`.
