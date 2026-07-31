## Why

The current native context-menu query uses a minimal flag set, so SuperExplorer exposes substantially fewer Windows and third-party commands than File Explorer. Delete failures also report only a generic error, leaving users unable to identify or safely close the application that owns a locked file.

## What Changes

- Query and display the complete target-appropriate classic Windows Shell context menu for files, folders, multi-selection, and backgrounds, including native submenus, owner-drawn items, disabled/default state, icons, and installed third-party extensions.
- Preserve Explorer modifier behavior: ordinary right-click uses the normal complete profile and Shift+right-click additionally requests extended-only verbs.
- Keep menu resolution, modal presentation, message forwarding, and invocation inside the disposable extension worker with bounded IPC, deadlines, cancellation, and crash isolation.
- Classify sharing/lock delete failures and use Windows Restart Manager to discover the current locking applications without scanning system-wide handles.
- Add an accessible locked-file dialog that lists bounded process identities and offers Retry, Cancel, and an explicit graceful “Close programs and retry” action.
- Revalidate process identity and protection/integrity before shutdown; never close SuperExplorer, system/protected/elevated-inaccessible processes, and never force-terminate a process.
- Add deterministic, real-Windows, interop, accessibility, focus, cancellation, destructive-fixture, and final UTIT coverage.
- Correct Properties, inline-rename pointer editing, and Pin to Start regressions discovered by installed-build verification.
- Add Explorer-like Back/Forward history menus and make pointer/keyboard new-tab creation inherit the active tab's complete committed navigation history.
- Close the remaining Properties coverage gap across file, folder, and compatible multi-selection targets, and restore Explorer-like pointer caret/selection behavior plus strong focused selection colors in address and search inputs.

## Capabilities

### New Capabilities

- `complete-shell-context-menu`: Complete Explorer-like native Shell context-menu discovery, modifier profiles, third-party submenu handling, focus/cancellation, and isolated invocation.
- `locked-delete-recovery`: Structured lock-failure classification, Restart Manager owner discovery, safe graceful close, retry/cancel behavior, and accessible failure recovery.

### Modified Capabilities

None.

## Impact

- Affects typed context-menu and file-operation contracts in `explorer-model`, broker wire payloads, app service routing, Shell/Restart Manager Windows adapters, GPUI reducer/actions/modal rendering, diagnostics, and UTIT manifests/scripts.
- Uses public Windows Shell and Restart Manager APIs; no new network service, elevation, credential storage, force termination, or system-wide handle enumeration is introduced.
- Existing filesystem navigation and delete failure behavior remain available as safe fallbacks when a Shell extension, broker, or Restart Manager operation is unavailable.
