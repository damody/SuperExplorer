## Context

GPUI mouse events and scrollbar bounds are expressed in logical pixels. The Win32 pointer-capture
adapter uses `GetCursorPos` and `ScreenToClient`, which return physical client pixels. Details-column
resize already normalizes this captured coordinate through the current window scale factor, but
scrollbar dragging consumes it directly. This mismatch multiplies drag distance at non-100% DPI.

The file-view vertical, navigation vertical, and Details horizontal scrollbars share the same target
offset geometry and native capture lifecycle. The existing 240-file headful fixture validates capture
outside the HWND but only asserts that the offset changed.

## Goals / Non-Goals

**Goals:**

- Convert native captured scrollbar coordinates to logical pixels exactly once.
- Preserve one-to-one thumb movement from 100% through 200% DPI.
- Cover both vertical scrollbar kinds and the Details horizontal scrollbar.
- Quantitatively verify the drag ratio using the existing multi-file UITEST fixture.

**Non-Goals:**

- Change wheel sensitivity, track-click paging, scrollbar appearance, or thumb sizing.
- Replace native pointer capture or remove outside-window dragging.
- Change virtualization, overlays, or file enumeration behavior.

## Decisions

### Normalize at the native-capture consumption boundary

`ExplorerRoot` will convert both coordinates returned by `cursor_client_position` with
`physical_client_to_logical` and the current `Window::scale_factor`. The active scrollbar kind then
selects logical X or Y. This follows the existing Details-column resize contract and prevents the
shared geometry from needing to know whether its input came from GPUI or Win32.

Applying DPI compensation inside `scrollbar_target_offset` was rejected because ordinary GPUI event
coordinates are already logical and would be scaled twice. Removing native capture was rejected
because it would regress dragging outside the client area.

### Keep the geometry function DPI-independent

Thumb size, track travel, grab offset, and maximum offset remain logical values. Unit tests will
normalize equivalent physical inputs across five scale factors and assert identical target offsets.

### Make the headful gate quantitative

The existing `scrollbar-headful` case will measure the real physical cursor delta and window DPI,
derive its logical delta, and compare the RangeValue change with the value predicted by the exposed
track and thumb geometry. The file vertical scrollbar uses the 240-file fixture. Navigation vertical
and Details horizontal paths receive equivalent ratio assertions while their current capture and
terminal checks remain intact.

## Risks / Trade-offs

- **Rounding at fractional DPI can differ by one physical pixel** → Use a small tolerance derived
  from one physical pixel plus UI Automation numeric rounding.
- **The thumb can clamp at a track endpoint** → Start from a non-terminal position and limit the
  measured drag to an unclamped portion of the track.
- **Native capture can be unavailable** → Retain the existing GPUI logical event coordinate fallback.

## Migration Plan

Land the normalization and unit tests, extend the existing headful script, then run format, clippy,
unit tests, UITEST validation, and `scrollbar-headful`. Rollback is limited to these code and test
changes; there is no persistent state migration.

## Open Questions

None.
