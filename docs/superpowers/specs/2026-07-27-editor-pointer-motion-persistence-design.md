# Editor Pointer-Motion Persistence Design

## Goal

Keep inline rename and address-bar editing active while the pointer merely moves. Editing may end from an explicit click action, the existing keyboard commit/cancel paths, or actual window deactivation.

## Root Cause

`ExplorerRoot::handle_action` currently treats broad action categories as focus loss:

- Address editing is canceled by every action outside the address action family.
- Inline rename is committed by every mouse-sourced action outside the rename action family.

File rows emit `UpdateFileDrag` on ordinary pointer movement even when no mouse button is pressed. That passive update therefore reaches both broad termination paths.

## Design

Add a small, pure action-classification helper at the UI root. It will identify pointer-motion and in-progress drag/resize updates that do not represent a new click or focus decision. `handle_action` will ignore those actions when deciding whether to end address or rename editing.

The helper will cover pointer update actions for file dragging, external dragging, marquee selection, scrollbars, details columns, side panes, and the navigation divider. Existing dispatch behavior for those actions will remain unchanged; only their ability to terminate an editor changes.

All non-passive mouse actions keep the existing behavior. Consequently, clicking a file row, the file-view background, navigation, or another control still ends the current editor through the action already emitted by that mouse-down or click handler. Enter, Escape, explicit submit/cancel, and window deactivation also retain their current semantics.

## Alternatives Considered

1. Recommended: classify passive pointer actions centrally. This is small, auditable, and preserves existing event and drag behavior.
2. Suppress `UpdateFileDrag` unless `event.dragging()` is true. This fixes the visible file-row trigger but leaves other pointer-update actions able to reproduce the same bug.
3. Add mouse-down and mouse-move variants to `ActionSource`. This is structurally precise but requires a broad callback API migration for a narrowly scoped bug.

## Testing

- Unit-test that all pointer-motion and drag/resize update actions are classified as non-terminating.
- Unit-test that representative click actions remain terminating.
- Exercise the targeted `explorer-ui` test suite and Clippy checks.
- Run formatting and workspace-level compile checks if targeted validation passes.

## Scope

This change affects only editor termination decisions. It does not alter rename validation, address submission, selection, drag-and-drop, marquee selection, resizing, native focus synchronization, or window-blur behavior.
