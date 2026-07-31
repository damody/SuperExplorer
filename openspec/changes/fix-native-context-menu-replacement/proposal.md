## Why

Right-clicking a second visible item while a native Shell context menu is open can dismiss the old popup without opening a menu for the second item. The current synchronous `mouse_event` replay races native-menu teardown and foreground restoration, while existing UTIT does not prove that a safe command targets the replacement item.

## What Changes

- Capture and suppress the complete second physical right-button gesture while the old native popup owns its modal loop.
- End and fully release the old popup before replaying a tagged, DPI-correct `SendInput` right-click through the normal GPUI hit-testing path.
- Reject incomplete, wrong-owner, stale-window, and recursively tagged replacement gestures without retaining state.
- Strengthen UTIT to distinguish the old and replacement sessions, prove exact second-row command targeting, and repeat the flow ten times with bounded resources.
- Preserve current popup commands, Escape/outside dismissal, background menus, multi-selection, broker isolation, and third-party Shell extension behavior.

## Capabilities

### New Capabilities

- `native-context-menu-replacement`: Explorer-compatible replacement of an open native Shell context menu by a second genuine right-click, including exact target, input lifecycle, cancellation, and resource contracts.

### Modified Capabilities

None.

## Impact

- Native menu input observation and replay in `crates/explorer-shell-win/src/context_menu.rs`.
- Existing file-row pointer routing and context-menu lifecycle behavior, without a broker protocol change.
- `scripts/smoke_context_menu_replacement.ps1`, UTIT manifest coverage, and focused native-menu regression suites.
- Win32 input usage changes from synchronous `mouse_event` replay to bounded tagged `SendInput` after native-menu teardown.
