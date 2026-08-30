# Details row visual column boundary design

## Goal

In Details view, stop file-row hover and selection visuals at the trailing edge of the last visible data column instead of extending across unused viewport space.

## Behavior

- A Details row's background fill and focus outline span exactly the sum of its visible column widths.
- Resizing, showing, hiding, or reordering columns immediately changes the row visual boundary through the existing computed width.
- When visible columns are narrower than the viewport, the trailing area remains ordinary file-view background.
- When visible columns are wider than the viewport, the row and header continue to share the same horizontal scrolling extent.
- Row cells, selection state, activation, drag/drop, marquee, and background context-menu behavior remain unchanged.
- Non-Details view modes retain their current row/tile width behavior.

## Architecture

`FileViewHost` already computes `render_item_width` with `view_item_width_with_registry`, including visible built-in and extension columns and the This PC Details special width. The Details row container will use that width explicitly instead of applying `w_full()`. No new width calculation or state is introduced.

The scroll surface remains the owner of full viewport background and background pointer handling. Only the child row's painted and hit-tested width changes to the data-column boundary, matching the fixed Details header extent.

## Alternatives considered

1. **Use the existing computed Details width (chosen).** One source of truth shared by header, cells, horizontal extent, and rows.
2. **Clip only the border/background.** Leaves row hit testing and future visual states inconsistent with the painted boundary.
3. **Add a nested visual wrapper.** Works but adds another layout/hit-test layer without providing new behavior.

## Verification

- Unit/source-contract tests assert Details rows use the computed visible-column width and do not use full viewport width.
- Tests cover built-in and extension-column width changes and widths smaller/larger than the viewport.
- Headful hover and focused-selection screenshots verify the right edge ends at the Size column and trailing space stays white.
- Existing file-row color samples remain `#E5F3FF`, `#CCE8FF`, and black for the active outline.

## Non-goals

- Changing Details header width or horizontal-scroll policy.
- Changing column defaults, row height, padding, typography, or interaction state.
- Changing List, Content, Tiles, or icon view geometry.
