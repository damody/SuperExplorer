## Context

The light theme exposes generic `row_hover`, `selected_active`, and `focus` colors. File rows currently use only `row_hover` for unselected hover and `focus`/`divider` as selection outlines, leaving selected rows unfilled. The supplied Windows Explorer screenshots establish different file-row-specific colors: hover `#E5F3FF`, active selection `#CCE8FF`, and an opaque black one-pixel focus outline.

The change must not recolor menus/navigation, alter selection state, or undermine dark/high-contrast themes.

## Goals / Non-Goals

**Goals:**

- Match the supplied Explorer light-theme hover and focused-selection colors and outline.
- Keep state projection deterministic: hover for unselected rows, active/inactive fills for selected rows, and no hover overlay on selection.
- Preserve dark-theme readability and Windows system-color ownership in high contrast.
- Isolate file-row styling from other semantic surfaces.

**Non-Goals:**

- Change focus/selection/input behavior, row bounds, row height, spacing, icons, or typography.
- Recolor navigation, menus, tabs, dialogs, or generic controls.
- Reproduce screenshot antialiasing fringe colors.

## Decisions

### Add file-row-specific theme slots

`SemanticColorSlot`, `SemanticColors`, and `HighContrastMappings` will gain file-row hover, active selection, inactive selection, focus outline, and selected-text roles. Light values use the measured RGB colors; dark values retain high-contrast dark equivalents; high contrast resolves all roles through the Windows system-color table.

This is preferred over changing global slots, which would affect unrelated surfaces, and over literals in `chrome.rs`, which would bypass theme completeness and accessibility tests.

### Keep one state-to-style projection

`FileRowVisual` will explicitly carry background fill, optional hover fill, optional outline, selected text color, and outline width. `file_row_visual` remains the sole mapping from selected/active state to those properties. Rendering applies that projection without changing callbacks or selection ownership.

The active selected state receives the Explorer fill and focus outline. Inactive selection receives its own theme fill and subdued outline. Unselected rows retain the normal surface and receive the hover fill only through the GPUI hover style.

### Use one logical pixel for the file-row focus outline

The reference outline is one physical pixel at the captured scale. The renderer will use a dedicated one-logical-pixel file-row outline rather than the broader generic focus stroke, which currently produces the visibly thick blue frame.

### Target visual-fixture interaction evidence

The existing capture harness will accept optional logical interaction coordinates while preserving its current defaults. This lets evidence inject hover into a deterministic file row at any DPI instead of capturing an unrelated command-bar point.

### Bound Details rows to their data-column extent

`FileViewHost` already computes `render_item_width` from every visible built-in and extension column. Details rows will use that exact width instead of `w_full()`, while other view modes retain their current sizing. The scroll surface continues to own the full viewport background and background context-menu hit area.

This is preferred over clipping only the paint or adding a wrapper because header, cells, horizontal scrolling, row paint, and row hit testing remain governed by one existing width calculation.

The visible header separator is centered inside the trailing resize grip. Therefore the painted row boundary is the visible-column sum plus `control_padding_horizontal - divider_width / 2`: that is the exact coordinate of the visible final header-column divider. The remainder of the resize grip and trailing container padding are not part of the data-column boundary. This token-based calculation avoids a fixed-pixel correction and keeps DPI scaling exact.

## Risks / Trade-offs

- [Risk] New semantic slots are omitted from a theme constructor → Update exhaustive slot lists, getters, light/dark constructors, high-contrast mappings, and completeness tests together.
- [Risk] Selected text loses contrast in high contrast → Add a selected-text role mapped to `HighlightText` and apply it at the row container.
- [Risk] Fill changes non-details views because they share the row renderer → This is intentional file-surface consistency; geometry and view-specific layout remain unchanged.
- [Trade-off] Dark theme is not pixel-matched to a supplied Explorer reference → Preserve current dark contrast and verify invariants rather than inventing light RGB reuse.
- [Risk] Restricting every view mode would shrink List/Content rows unexpectedly → Apply explicit column width only when `ViewMode::Details` is active and cover the branch with a render contract test.

## Migration Plan

Land theme slots, row projection, renderer changes, and tests atomically. No persisted migration or staged rollout is required. Rollback is a source revert.

## Open Questions

None. Reference images and prior autonomous implementation authorization determine the light-theme target and scope.
