## Why

The selected tab currently looks like a blue file selection and remains separated from the content
by a divider, while breadcrumb child menus can leave a blue keyboard row highlighted instead of
visually following the pointer. Both behaviors diverge from File Explorer and make the active
surface and current menu target ambiguous.

## What Changes

- Render the active tab and the content surface as one continuous white region without a bottom
  divider.
- Render inactive tabs with the same gray fill as the surrounding tab strip.
- Make breadcrumb child-menu focus follow physical pointer movement between rows.
- Use the semantic gray hover fill for the current breadcrumb menu row instead of blue selection.
- Add unit and headful UTIT coverage with pixel and pointer-movement evidence.

## Capabilities

### New Capabilities

- `tab-breadcrumb-interaction-parity`: Defines active/inactive tab surface continuity and
  pointer-following breadcrumb child-menu highlights.

### Modified Capabilities

None.

## Impact

The change affects `explorer-ui` chrome rendering and actions/state for breadcrumb menu focus, plus
the UTIT manifest and a focused Windows headful visual-interaction script. It adds no dependencies
and does not change filesystem, Shell enumeration, or navigation semantics.
