# Restore Address Bar Edit Mode

## Goal

Restore Windows File Explorer-compatible address-bar editing after the details-column drag implementation caused every left-button release to cancel a newly entered address editor. Clicking unused address-bar space, pressing `Ctrl+L`, or pressing `Alt+D` must expose the complete editable path, focus it, and select its text.

## Root Cause

The explorer window root currently installs an unconditional left-button release handler that dispatches `CancelDetailsColumnDrag`. Address editing is canceled before actions outside the address family are handled. Therefore a pointer click can enter address edit mode on mouse-down and immediately leave it when the same gesture bubbles to the root on mouse-up, even though no details-column drag exists.

## Design

### Scope drag cancellation to an active drag

The root-level release behavior remains available because releasing outside a details header must still terminate an actual drag. However, the render path will only dispatch the cancellation action when details-column drag state is active. Ordinary clicks elsewhere in the window will no longer manufacture drag lifecycle actions.

### Classify drag lifecycle actions as passive pointer activity

`UpdateDetailsColumnDragPreview`, `CommitDetailsColumnDrag`, and `CancelDetailsColumnDrag` will join the existing passive pointer-action classification used by resize, scrollbar, marquee, and file-drag lifecycle actions. These actions describe continuation or termination of an already established pointer interaction; they do not represent a new focus decision and must not independently close address editing or commit inline rename.

This second boundary protects editor behavior if a valid drag terminal action reaches the dispatcher while an editor state is being restored or synchronized. It also keeps the policy centralized instead of adding address-specific propagation workarounds.

## Explorer Behavior

- Clicking unused breadcrumb/address space enters editing, focuses the editor, and selects the complete parsing path.
- Clicking inside an already active editor reuses the editor and allows normal caret/selection behavior.
- `Ctrl+L` and `Alt+D` enter the same editing state.
- `Esc` cancels the draft and restores the breadcrumb.
- `Enter` submits the current draft through the existing address parser.
- A genuine details-column drag still previews, commits on a valid drop, and cancels when released outside its valid header target.

## Testing

### Unit and structural coverage

- Prove all three details-column drag lifecycle actions are passive pointer actions.
- Prove a mouse-sourced drag cancellation does not end active address editing or inline rename.
- Prove ordinary non-address click actions still end address editing, preserving click-outside behavior.
- Prove the root release cancellation is conditional on an active details-column drag rather than installed unconditionally.

### UTIT coverage

Add a manifest case that launches the real application and uses genuine Windows pointer and keyboard input. It will:

1. Click unused address-bar space and assert the editable complete path is exposed and focused.
2. Type a harmless marker and assert the editor receives it, proving the mode survives mouse release.
3. Press `Esc` and assert the resolved breadcrumb returns.
4. Exercise `Ctrl+L` and `Alt+D`, asserting complete-path selection by replacing the selected range with a marker.
5. Submit the current valid path with `Enter` and assert navigation remains on the resolved location.
6. Perform a details-column drag and an outside release to ensure drag cleanup remains functional.

The case will emit a report and screenshot evidence. Existing address selection, keyboard selection, and details-column drag tests remain active.

## Error and Lifecycle Handling

No navigation parsing, history, focus ownership, or text-input implementation changes are required. The fix only prevents unrelated drag terminal events from being generated and treats legitimate drag lifecycle actions as focus-neutral. If a drag is active, cancellation continues to clear the preview exactly once through the existing state transition.

## Out of Scope

- Redesigning breadcrumb layout or address typography.
- Changing address parsing or navigation-error behavior.
- Changing details-column ordering semantics or persistence.
- Refactoring unrelated pointer or focus handlers.
