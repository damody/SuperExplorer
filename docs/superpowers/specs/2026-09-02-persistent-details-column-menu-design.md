# Persistent Details Column Menu Design

## Goal

Make the Details header column chooser behave like Windows File Explorer: users can toggle several columns in one popup session, see each check mark and the Details view update immediately, and close the popup only when they explicitly dismiss it.

## Interaction contract

- Right-clicking any Details column header opens the existing top-level immersive popup beside the pointer.
- Clicking a column row toggles that column immediately and keeps the popup open at the same position and scroll offset.
- The row check mark is repainted before the next interaction. The Details view receives the same toggle during the open popup session.
- Users may repeatedly check and uncheck any enabled column without reopening the popup.
- `Name` remains checked and disabled because the model requires it.
- Clicking outside, pressing Escape, deactivating the application, or right-click replacement dismisses the popup.
- Auto-size-this-column and auto-size-all-columns remain terminal commands and close the popup after execution.
- Target-specific display commands remain terminal unless explicitly classified as persistent toggles in a future change.

## Architecture

The existing application-owned popup renderer gains an opt-in persistent-selection mode. Each popup command is classified as either terminal or persistent-toggle. Activating a persistent toggle updates the owned menu state and materialized row state, invalidates the popup for immediate repaint, and publishes a selection event without ending the native message loop.

The popup continues running on its background worker. A bounded UI bridge transports persistent selection events to `ExplorerRoot`; the UI applies each `ExplorerAction` on the foreground context. Popup rendering never re-enters GPUI directly. When the popup closes, its terminal result is processed as today and all bridge resources are released.

The ordinary filesystem context-menu path keeps terminal command semantics. Persistent behavior is enabled only for the Details column popup.

## State and ordering

- Popup command indices remain stable for the lifetime of one popup.
- Separators never consume a command index.
- The UI-owned action vector and native popup command vector use the same command-row ordering.
- A persistent event carries the command-row index and resulting checked state.
- Duplicate events are harmless because the UI reconciles the requested checked state rather than blindly depending on timing-sensitive double toggles.
- Events arriving after the owning window or popup session closes are ignored.

## Failure handling

- Failure to publish one persistent event leaves the popup responsive but closes it to prevent the native check state and application state from diverging.
- Failure to update the native check state also closes the popup and reports the existing popup failure path.
- Invalid, disabled, separator, or out-of-range selections are ignored.
- No popup callback borrows or updates GPUI from the popup worker thread.

## Verification

- Unit-test activation rules for terminal versus persistent rows.
- Unit-test check-state mutation, repeated toggle sequences, disabled `Name`, index stability, and dismissal.
- Retain the immersive-popup resource and edge-clamping suites.
- Headful test one popup session that toggles a column on, verifies the check and visible column, toggles it off, verifies both again, and confirms the popup HWND/session did not change.
- Headful test Escape and outside-click dismissal after repeated toggles.
- Build and run the focused Details and immersive-popup test suites, then repeat the workflow from a user perspective in a deliberately small main window.
