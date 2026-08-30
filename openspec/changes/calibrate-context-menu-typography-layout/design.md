## Context

The application-owned remote context menu currently shares geometry and fallback constants with the Windows owner-draw popup. Its GPUI renderer explicitly uses Microsoft JhengHei UI but takes a 15px font size from the visual metrics and leaves line height and weight implicit. The application typography contract already defines the Windows zh-TW menu style as 12px, 16px line height, weight 400.

The approved source is `docs/superpowers/specs/2026-08-30-context-menu-typography-layout-calibration-design.md`.

## Goals / Non-Goals

**Goals:**

- Match the native Windows menu text scale and vertical rhythm at every DPI.
- Make the remote renderer consume one complete menu typography style.
- Keep geometry, command behavior, themes, and local system-font preference stable.

**Non-Goals:**

- Change command order, icons, colors, shadows, or popup lifecycle.
- Replace `NONCLIENTMETRICS.lfMenuFont` for native owner-draw menus.
- Adjust global application typography.

## Decisions

### Use menu typography tokens as the GPUI authority

`context_menu_visual_tokens` will project font size, line height, and weight from `tokens.typography.menu`; the menu surface already applies the configured family. The row explicitly applies all four properties. This keeps font metrics together and automatically preserves logical-pixel DPI scaling.

Alternative: only change the numeric `font_size`. Rejected because the browser still derives an implicit line box and weight.

Alternative: encode physical pixels for the reference screenshot. Rejected because physical pixels fail across monitor DPI settings.

### Correct the shared owner-draw fallback to 12px

`WINDOWS_CONTEXT_MENU_VISUAL_METRICS.font_size` becomes 12. The Windows popup continues to prefer `NONCLIENTMETRICS.lfMenuFont`; only the existing fallback `CreateFontW` path consumes this number.

### Preserve 23px rows and current gutters

A 16px text line fits inside the 23px row with seven pixels of total leading space and is vertically centered by the flex row. Existing 42px icon gutter, 16px icon, and 13px offset remain unchanged because the reference screenshots show the text origin and icon column already align.

## Risks / Trade-offs

- [Fallback fonts have different glyph metrics] → Retain the existing Windows-oriented family fallback chain and explicit line height.
- [A 12px font could expose clipping if line height is omitted] → Apply and test the 16px line height on every remote command row.
- [Changing the shared fallback could alter rare local owner-draw fallback output] → This is intentional parity; the normal system-font path remains first choice and receives focused tests.

## Migration Plan

No data migration. Update constants and renderer projection, then run focused model, UI, and shell-win compilation/tests. Rollback restores the prior fallback number and removes explicit row typography fields.

## Open Questions

None.

