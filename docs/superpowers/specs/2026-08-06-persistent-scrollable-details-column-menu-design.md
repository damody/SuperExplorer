# Persistent Scrollable Details Column Menu

## Goal

Make the Details column chooser behave like Windows File Explorer: users can repeatedly check and uncheck columns without reopening the menu, and a menu taller than the available window area can be scrolled vertically to every registered built-in or extension column.

## Current Behavior and Root Cause

The reducer already preserves `details_column_menu` for `ToggleDetailsColumn`, but the action reports `FocusSurface::CommandBar` even though the popup belongs to the file-view details header. The focus transition changes popup ownership during the click/rerender cycle and allows the menu to be dismissed even though its state was preserved. The menu itself also has no maximum height or vertical overflow policy, so registered extension columns can extend beyond the bottom of the window.

## Considered Approaches

### Keep file-view ownership and add bounded scrolling — selected

Column-menu actions remain owned by `FocusSurface::FileView`. Toggle actions update visibility immediately while preserving the same open popup and its scroll position. The popup receives a viewport-bounded maximum height and GPUI vertical overflow scrolling. This is local, matches the actual popup owner, and automatically applies to built-in and extension columns.

### Reopen the popup after every toggle

The action could close and reopen the menu around every state update. This would visibly flicker, reset the scroll position, and make rapid multi-selection awkward.

### Move the chooser into a separate dialog

A dedicated dialog would avoid popup lifetime concerns but diverges from File Explorer and adds unnecessary window, focus, and dismissal behavior.

## Interaction Design

- Clicking any enabled column row immediately toggles its checked state.
- The popup remains open after toggling and the same row can be clicked repeatedly: checked → unchecked → checked.
- `Name` remains checked and disabled.
- Auto-size and column-specific display actions remain available without closing the chooser.
- Clicking outside the popup, right-clicking outside it, pressing `Esc`, navigating away, changing tabs, or replacing the active popup closes it through the existing dismissal rules.
- The popup remains anchored in the file view even when the header that originally opened it is hidden.
- Built-in and extension columns share one ordered scrollable list.

## Scrolling and Layout

The popup keeps its existing width, padding, typography, row height, and anchor. Its maximum height is constrained to the existing menu-height token so it remains inside the usable explorer viewport. When content exceeds that height, GPUI `overflow_y_scroll` provides mouse-wheel, touchpad, and scrollbar movement. Toggling a visible row must not reset the current scroll offset, allowing several offscreen extension columns to be configured in one session.

At short window heights, the menu must still expose its top actions and permit reaching the final row. Content must not paint or receive pointer hits outside the popup bounds.

## State and Data Flow

1. A header context action opens `details_column_menu` with the target column used by auto-size and column-specific settings.
2. A menu-row click dispatches `ToggleDetailsColumn`.
3. The reducer updates the active tab's existing ordered details layout and persists it through the current settings path.
4. The action remains on `FocusSurface::FileView`; the menu state and popup identity survive rerender.
5. The same popup rerenders the row with its new checked state and retains its scroll position.

No extension ABI, registry ordering, or settings format changes are required.

## Testing

### Rust tests

- Prove `ToggleDetailsColumn` preserves the open chooser and file-view focus.
- Prove repeated toggles produce checked → unchecked → checked without closing the chooser.
- Prove `Name` remains visible and cannot be toggled.
- Structurally verify the popup has a bounded maximum height and vertical scrolling.
- Preserve existing tests for per-tab visibility and extension column ordering.

### UTIT

Add an installed-app genuine-pointer case that:

1. Opens the Details column chooser.
2. Clicks the same enabled row to uncheck, re-check, and uncheck/re-check again while asserting the popup remains visible after every click.
3. Runs with enough registered extension columns or a short enough app window to overflow the popup.
4. Scrolls to the last column, toggles it, and proves the checked state changes without closing or jumping back to the top.
5. Scrolls back upward and confirms earlier state changes remain applied.
6. Captures screenshots of the persistent toggle state and the scrolled bottom boundary.

## Failure Handling and Scope

If a column becomes unavailable while the menu is open, the next registry-driven render omits that row without disturbing the remaining menu. This change does not alter column drag/drop, sorting, resize behavior, filter menus, Folder Options, or extension loading.
