# Editable Selection Vertical Metrics Design

## Goal

Make selected text in every single-line editing surface visually match the approved C treatment: the selection background fills most of the editor's inner height, fully covers the glyphs, and leaves equal visible space above and below. The address editor, search editor, and inline rename editor must use one shared metric rule.

## Scope

This change covers the editable address bar, editable search box, and inline file or folder rename field. It changes only selection geometry and the padding needed to center that geometry. Existing foreground colors, selection colors, caret behavior, mouse selection, keyboard selection, focus borders, and commit or cancel behavior remain unchanged.

## Shared Metric Rule

Introduce a pure `editable_selection_metrics` helper whose inputs are the control height, focus-border width, and desired selection inset. The helper returns the text line height and equal top and bottom padding.

For a focused editor:

1. Subtract the top and bottom focus borders from the control height.
2. Reserve one small selection inset at both the top and bottom of the remaining inner height.
3. Use the rest as the editable text line height, because GPUI paints the selection background from the line box.
4. Use the same inset as vertical padding, so the selection box is centered and nearly fills the editor.

The invariant is:

`2 * border + top padding + selection line height + bottom padding == control height`

and `top padding == bottom padding`.

Metrics are clamped for unusually small controls so the line height never becomes negative and glyphs are not clipped. The helper is shared rather than duplicating numeric offsets in each editor.

## Integration

The address and search editors continue to use their existing typography size and colors, but obtain line height and vertical padding from the shared helper. The inline rename editor uses the same helper with its own control height and border width. This keeps the approved near-full-height treatment consistent even though rename has a different container height.

The unselected portion of the text keeps the normal primary foreground color. Selected text keeps the theme's selected-text foreground, including high-contrast themes. The change does not turn the entire field into a selected block; the background remains limited to the selected character range.

## Testing

Unit tests cover address, search, and rename dimensions and assert:

- top and bottom selection insets are equal;
- the metric equation exactly accounts for the control height;
- the selection line height is larger than the typography's normal line height and remains positive;
- small-height inputs clamp safely.

A structural test verifies that all three editor render paths use the shared helper. UTIT extends the editable pointer-input case to select text in the address bar, search box, and rename editor, capture each surface, and assert that the selection rectangle fully covers the glyph band with symmetric top and bottom margins within a one-physical-pixel tolerance after DPI scaling. The existing caret placement and mouse drag selection assertions remain active to prevent interaction regressions.

## Non-goals

This change does not alter non-editing breadcrumb rendering, file-row selection styling, multi-line text, typography size, input height, theme colors, or focus-ring thickness.
