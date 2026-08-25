## Context

`LocationDescriptor::Virtual` already carries provider, opaque container
identity, generation, and normalized components, but the production Shell STA
only resolves Windows Shell locations.  The reference tree implements ADB and
SFTP behind a separate core; SuperExplorer needs the same capability while
preserving its owned request/event protocol and keeping remote credentials out
of session state.  The supplied test server is an integration target only.

## Goals / Non-Goals

**Goals:**

- Make ADB device roots and arbitrary phone paths first-class virtual locations.
- Make saved SFTP profiles first-class virtual locations with password login.
- Use the existing directory, navigation, clipboard, and operation semantics
  for list/create/rename/delete/copy/move, with cancellable progress.
- Authenticate SFTP securely: Windows Credential Manager contains the password;
  the profile holds no secret; unknown/changed SSH host keys require explicit
  trust and are persisted as a fingerprint, not silently accepted.

**Non-Goals:**

- FTP, SMB, SSH terminal/shell, SFTP public-key/agent authentication, ADB APK
  installation or screen control, recursive synchronization, offline caching,
  remote Recycle Bin, or atomic transactions across distinct filesystems.

## Decisions

### Provider boundary in the application host

Create `explorer-remote` as a platform-neutral runtime with typed providers and
an adapter in `explorer-app`. The Shell STA routes local/Shell locations as it
does today and routes `Virtual` descriptors with `provider_id` `adb` or `sftp`
to the remote runtime. This prevents a remote URI from ever reaching Win32 path
or COM APIs. An alternative of making paths look like `\\server` was rejected:
it would leak passwords, cannot model ADB, and confuses path validation.

### Stable virtual identity and paths

Each configured SFTP profile and each ADB serial gets a random persistent
16-byte container identity. ADB locations are addressed as `adb://<serial>/…`;
SFTP uses a profile alias in `sftp://<alias>/…`, never user or password in the
location. URI parsing normalizes slashes and rejects empty, traversal, NUL, and
absolute component injection. A profile/device refresh advances generation only
when its connection epoch changes; stale events are rejected by the normal
`RequestContext` gate.

### Network and process I/O

ADB invokes an explicitly resolved `adb.exe` with argument arrays, a bounded
stdout/stderr capture, timeout, cancellation kill, and no shell interpolation.
SFTP uses `russh` and `russh-sftp` on a dedicated Tokio runtime; connection
handles are keyed by profile and disconnect after bounded idle time. Directory
results stream in batches and every command produces exactly one terminal event.
The reference's pure Rust SSH choice avoids an OpenSSL runtime dependency.

### Mutation and transfer semantics

Remote delete is permanent and must use the existing destructive-operation
confirmation surface. Copy is supported for Local↔ADB, Local↔SFTP, and
ADB↔SFTP; move across provider boundaries is copy then verified source deletion
and reports a partial outcome if deletion fails. Conflicts retain the existing
Prompt/Skip/Replace/KeepBoth decisions. Remote files are streamed through a
bounded temporary spool only when a pair cannot stream directly; cleanup is
guaranteed by a scoped guard.

### UI and secrets

The navigation pane shows an Android Devices section and an SFTP section with
Connect/Add profile controls. Address entry accepts direct ADB paths and saved
SFTP profile aliases. SFTP profile data (host, port, user, label, pinned host
fingerprint) is non-secret configuration; the password is written/read through
Windows Credential Manager and never formatted in debug, telemetry, titles,
history, bookmarks, clipboard, or error strings.

### Clipboard and native drag interoperability

File commands are dispatched only while the file view owns keyboard focus.
Editable text keeps Ctrl+C/X/V through the GPUI text-input context. The Shell
clipboard adapter continues to recognize `CF_HDROP` for native local files and
adds a registered, versioned SuperExplorer format containing only opaque remote
descriptors and copy/cut intent. Text, HTML, bitmap, PNG, and unrelated formats
remain `Unsupported` for file paste and are not cleared or replaced. Remote
items dragged into Windows Explorer are materialized into a quota-managed staging
directory before starting the existing OLE drag loop; native files dragged into
a remote directory are routed to the cross-provider transfer engine.

## Risks / Trade-offs

- [ADB binary missing or device unauthorized] → Detect before navigation,
  render actionable states, and do not fabricate a local path.
- [Host impersonation/key rotation] → Pin first trusted fingerprint and block
  changed keys until the user explicitly replaces trust.
- [Remote mutations lack undo/atomicity] → Label deletion permanent; stage
  cross-provider moves and emit item-level partial outcomes.
- [Slow/failed networks] → Per-operation deadlines, cancellation, bounded
  queues, and stale-response rejection.
- [Rooted Android access varies] → List only paths the authorized device grants;
  permission-denied rows show a non-secret actionable error.

## Migration Plan

Feature-gate the remote runtime until its unit and opt-in integration gates pass.
Existing sessions remain valid because the new virtual provider IDs are additive.
On rollback, hide new navigation roots and release remote handles; profiles and
Credential Manager secrets are retained but unused, never migrated into files.

## Open Questions

None blocking: password authentication and the standard SSH port are the first
release baseline. The supplied SFTP server is used only by an opt-in test which
reads its password from the process environment or Credential Manager.
