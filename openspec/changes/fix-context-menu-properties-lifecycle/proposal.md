## Why

Selecting a file currently paints an opaque full-row band, and invoking Properties can show a generic unavailable dialog or leave the next native right-click unusable. The prior shortcut changed the Properties entry point without preserving the Shell STA and owner-window lifetime, so this regression needs a lifecycle-correct fix backed by genuine mouse evidence.

## What Changes

- Render selected files and folders with an Explorer-like focus outline and no selected/hover full-row fill.
- Replace per-request host-owned Shell threads with one bounded application-owned STA executor while retaining the existing single context-menu broker process.
- Invoke Properties through a host-resolved native `IContextMenu` using the immutable popup target, a real SuperExplorer owner HWND, and extended Unicode invocation metadata.
- Add PID-bound, genuine Win32 mouse UTIT that rejects generic unavailable dialogs and proves a second right-click and command work after Properties closes.
- Add ten-cycle resource and lifecycle coverage for app, broker, worker, host STA, owner window, menu, thread, and handle bounds.

## Capabilities

### New Capabilities

- `file-row-selection-outline`: Explorer-like selected-row outline behavior across focus, hover, native popup, view mode, theme, and high contrast.
- `context-menu-properties-lifecycle`: Persistent host Shell STA ownership and target-correct Properties invocation that cannot poison later native context menus.

### Modified Capabilities

None. The repository has no promoted base specs; this change records self-contained regression capabilities while preserving the completed umbrella change contracts.

## Impact

- Affects GPUI file-row styling, Shell STA dispatch, native `IContextMenu` invocation, app shutdown, and context-menu UTIT scripts/manifest mappings.
- Does not replace the persistent broker, change third-party provider invocation, add a broker process, or perform Shell COM work on the UI thread.
- Debug, release, and installed binaries must be rebuilt and validated as one app/broker/worker set.
