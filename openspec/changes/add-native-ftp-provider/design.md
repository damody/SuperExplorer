## Context

ADB and SFTP already share `RemoteProvider`, `TransferEngine`, clipboard, and drag-and-drop. FTP must plug into that stack. The SFTP URI order was standardized to `username@host`; FTP follows that contract rather than the obsolete reversed example.

## Goals / Non-Goals

**Goals:**

- Native `FtpProvider` for list/CRUD/recursive transfer/cancel/progress.
- Plain FTP + Explicit FTPS, passive default, Credential Manager isolation.
- Canonical `ftp://<alias>/path` with transient username hints.
- Capability-gated chmod/symlink UI.
- Local fixture tests; no committed secrets.

**Non-Goals:**

- Rewriting SFTP into a generic NetworkProvider.
- `curl.exe`.
- Implicit FTPS in the first UI.
- Cross-application resumable REST/APPE transfers.
- Shipping a built-in profile for `45.32.49.125`.

## Decisions

- Independent `FtpProvider`, not an SFTP refactor.
- Address-bar user-info is `ftp://username@host/path` (same as current SFTP). After login the alias is the host string, matching SFTP.
- Passwords live only under `SuperExplorer/FTP/<alias>`. Anonymous logins store no credential.
- Passive: EPSV, then PASV; if PASV returns a private IP while the control peer is public, use the control IP.
- Upload: hidden sibling temp name, `RNFR`/`RNTO`; if rename is missing, direct STOR and report non-atomic.
- `RemoteProvider::capabilities()` is the only shared refactor.

## Risks / Trade-offs

- [Plain FTP is insecure] → one-time save warning and persistent 未加密 label; never mark it secure.
- [FTP metadata is unreliable] → prefer MLSD/MLST, fall back to LIST, never invent owner/ctime/permissions.
- [Servers differ wildly] → capabilities hide unsupported commands instead of failing after the user clicks them.

## Migration Plan

No existing FTP data. Rollback is code-only; SFTP/ADB profiles stay untouched.

## Open Questions

None. Remaining product choices follow the approved 32-item list, except URI order which follows the already-shipped SFTP `username@host` standard.
