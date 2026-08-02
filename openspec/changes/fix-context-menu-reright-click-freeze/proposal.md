## Why

Right-clicking a second file while a native Shell context menu is open can freeze the menu and application because the low-level mouse hook synchronously tears down the modal menu loop. The replacement must remain responsive and target the second item exactly, matching Windows File Explorer.

## What Changes

- Replace synchronous native-menu termination inside the mouse hook with one posted asynchronous cancellation request.
- Keep the complete second right-button gesture suppressed until the old popup has fully released, then replay it once through the normal Win32/GPUI input path.
- Coalesce rapid replacement attempts to the latest complete target and reject stale terminal events.
- Add deterministic lifecycle tests and genuine-pointer UTIT that proves responsiveness and exact second-target command invocation.
- Preserve current Shell extension, selection, submenu, Escape, outside-dismissal, and broker-isolation behavior.

## Capabilities

### New Capabilities

- `context-menu-replacement-liveness`: Non-blocking, exact-target replacement of an open native Shell context menu by a second genuine right-click.

### Modified Capabilities

None.

## Impact

- Native popup cancellation and input replay in `crates/explorer-shell-win/src/context_menu.rs`.
- Context-menu request replacement state in `crates/explorer-ui/src/state.rs` and its event submission boundary.
- Focused context-menu replacement scripts and `uitest/manifest.json` coverage.
- No public API, broker protocol, or extension manifest change.
