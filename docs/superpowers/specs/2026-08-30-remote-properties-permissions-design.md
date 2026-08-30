# Remote Properties and Permissions Design

## Scope

ADB and SFTP item context menus expose a functional Properties command for exactly one selected item. The application-owned dialog follows the Windows Explorer General-page hierarchy: name, type, remote location, size, timestamps, and attributes. It additionally exposes POSIX owner/group/other read, write, and execute bits. The backend also preserves support for the setuid, setgid, and sticky bits in its typed mode contract.

## Architecture

The UI snapshots the selected remote `FileEntry`; it never sends a path string or shell command directly. Applying changes submits `FileOperationKind::SetUnixMode` with the immutable item descriptor and a permission-only mode (`0000..7777`). `RemoteExplorerService` resolves the registered provider. ADB invokes `chmod` through its argument-array runner with validated/quoted remote paths; SFTP sends a protocol-native `SETSTAT` containing only the permissions field. Local Shell dispatch rejects this remote-only operation.

The dialog is unavailable for multiple selections or entries without Unix mode metadata. Provider failures flow through the existing operation center, and successful completion triggers ordinary directory reconciliation so the refreshed row shows the authoritative mode.

## Alternatives

Launching an external terminal was rejected because it would expose credentials, quoting, and lifecycle behavior outside the typed service boundary. Separate ADB and SFTP dialogs were rejected because they would duplicate state, validation, and accessibility behavior.

## Verification

Unit tests cover mode formatting, ADB argument safety, invalid mode rejection, remote operation routing, menu membership, and dialog controls. Validation includes formatting, focused crate tests, all-target compilation, and strict OpenSpec validation.
