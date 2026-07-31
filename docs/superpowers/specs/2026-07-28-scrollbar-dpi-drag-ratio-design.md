# Scrollbar DPI Drag Ratio Design

## Problem

Native pointer capture returns physical Win32 client coordinates, while GPUI scrollbar bounds,
mouse events, thumb geometry, and scroll offsets use logical pixels. The captured coordinates are
currently passed directly into the logical scrollbar calculation. At 175% display scaling, a
logical pointer movement is therefore interpreted as roughly 1.75 times its actual distance.

## Scope

Correct one-to-one thumb dragging for these custom scrollbars at 100%, 125%, 150%, 175%, and 200%
DPI scaling:

- File-view vertical scrollbar.
- Navigation-pane vertical scrollbar.
- Details-view horizontal scrollbar.

Native capture, dragging outside the window, grab-offset preservation, track paging, overlays, and
the existing scrollbar appearance remain unchanged.

## Design

Captured Win32 client coordinates will be converted from physical pixels to GPUI logical pixels at
the boundary where `ExplorerRoot` reads the native capture sample. The conversion uses the current
window scale factor exactly once. Normal GPUI mouse-event coordinates remain untouched because they
are already logical.

Both vertical scrollbar kinds select the converted logical Y coordinate. The Details horizontal
scrollbar selects the converted logical X coordinate. The shared scrollbar geometry continues to
map the logical pointer position to the available thumb track:

`target_offset = (pointer_local - grab_offset) / (viewport - thumb_size) * maximum_offset`

This keeps the existing proportional relationship between thumb travel and content offset while
removing the accidental DPI multiplier.

Invalid or non-positive scale factors will make the captured sample unavailable, allowing the
existing logical GPUI event coordinate to remain the fallback. Offset and track clamping behavior
does not change.

## Testing

Unit coverage will verify that equivalent logical pointer movements produce identical scrollbar
targets at scale factors 1.0, 1.25, 1.5, 1.75, and 2.0 for vertical and horizontal axes. Existing
grab-offset and outside-track clamping tests remain in place.

The `scrollbar-headful` UITEST will retain its real 240-file folder fixture and add ratio assertions
for all three scrollbar kinds. It will record the physical pointer delta, convert it using the
window DPI, derive expected thumb and range-value movement from the exposed track geometry, and
fail when the observed movement differs beyond a small rounding tolerance. Existing native capture,
outside-window dragging, capture release, horizontal overflow, and hidden-scrollbar cases remain.

## Acceptance Criteria

- A physical drag corresponding to N logical pixels moves each scrollbar thumb by N logical pixels,
  subject only to endpoint clamping and pixel rounding.
- Behavior is consistent from 100% through 200% DPI.
- The 240-file headful fixture verifies vertical file-view dragging quantitatively.
- Navigation vertical and Details horizontal dragging use the same coordinate normalization.
- Format, clippy, unit, UITEST validation, and `scrollbar-headful` gates pass.
