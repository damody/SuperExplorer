# File row hover and focus visual alignment design

## Goal

Align SuperExplorer file-row hover and focused-selection visuals with the supplied Windows File Explorer references without changing selection, focus, keyboard, context-menu, or drag behavior.

## Reference measurements

- Windows File Explorer hover fill: `#E5F3FF`.
- Windows File Explorer focused-selection fill: `#CCE8FF`.
- Windows File Explorer focused-selection outline: opaque black, one logical pixel.
- Current SuperExplorer hover fill: `#F5F5F5`.
- Current SuperExplorer focused selection: white fill with a `#0078D4` outline.

The measurements come from dominant flat-color pixels and interior/boundary samples in the four user-provided screenshots. Antialiased edge pixels are not treated as source colors.

## Visual behavior

- In the light theme, an unselected hovered file row uses `#E5F3FF` across the row bounds.
- In the light theme, an actively selected/focused file row uses `#CCE8FF` plus a one-logical-pixel black outline.
- A selected row does not receive an additional hover fill.
- Inactive selection retains a distinct, lower-emphasis treatment.
- Dark-theme colors retain suitable dark contrast rather than copying light-theme RGB values.
- Windows high-contrast mode resolves file-row selection and focus through system highlight roles rather than fixed RGB values.
- Text and icons retain their existing layout, typography, spacing, and activation behavior.

## Architecture

Add file-row-specific semantic colors instead of changing global control, menu, or navigation colors. `file_row_visual` remains the single state-to-style projection and gains an explicit selection fill alongside hover fill and outline. The file-row renderer consumes only that projection.

This isolates the calibration from other uses of `row_hover`, `focus`, and global selected colors. The selection model and `file_row_selection_active` contract remain unchanged.

## Alternatives considered

1. **File-row semantic tokens (chosen).** Precise and isolated, with explicit light, dark, and high-contrast values.
2. **Change global semantic colors.** Smaller edit but unintentionally recolors unrelated rows, controls, and menus.
3. **Hard-code colors in `chrome.rs`.** Visually direct but bypasses theme ownership and Windows high-contrast behavior.

## Verification

- Theme tests cover every new semantic slot in light, dark, and high contrast.
- File-row visual-state tests cover unselected hover, active selection, inactive selection, and selected-row hover suppression.
- Source/render contract tests verify the fill and one-pixel outline are applied by the file-row renderer.
- Focused UI tests and visual evidence compare the rendered light-theme row against the supplied Explorer references.

## Non-goals

- Changing hover/focus visuals for navigation, menus, tabs, toolbars, or dialogs.
- Changing row height, column widths, icon rendering, typography, or selection mechanics.
- Emulating screenshot antialiasing artifacts as palette colors.
