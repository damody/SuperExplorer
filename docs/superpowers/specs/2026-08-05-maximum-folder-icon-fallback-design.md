# Maximum Folder Icon Fallback Design

## Problem

At maximum icon zoom, a failed exact-size shared folder Shell-icon request is recorded as a
permanent base-icon failure. The file view then renders its fixed 20 px yellow generic fallback for
every folder, even when a smaller valid Windows Shell folder icon is already available.

## Required behavior

The file view must use this presentation order for folders:

1. The exact current-size, current-DPI, current-theme Shell icon for the real folder.
2. The largest compatible Shell icon already available for that same folder, upscaled with
   `ObjectFit::Contain` into the current icon box.
3. The largest compatible shared Windows Shell folder icon, also upscaled into the current box.
4. The generic yellow fallback only while no Shell pixels exist.

A failed request for an exact large size must not permanently suppress compatible smaller Shell
pixels or prevent a bounded retry through the real folder path. Results from another DPI, theme,
association generation, or overlay generation are not compatible.

## Implementation boundaries

- Add a cache lookup that selects the largest compatible same-item texture when the exact key is
  absent.
- Allow the shared folder base cache to select the largest compatible size for the active display
  context.
- Treat an exact-size shared-base failure as a missing size, not a permanent failure of the folder
  class. Submit a bounded real-item request so custom folder icons and overlays remain correct.
- Keep the existing cache memory limit, current-demand admission, virtualization, and two-worker
  thumbnail concurrency unchanged.
- Do not stretch the fixed generic fallback; it remains only a temporary last resort.

## Verification

- Unit tests prove exact-size preference, largest compatible same-item/shared-base selection, and
  recovery after an exact-size base failure.
- The icon-view headful UTIT zooms to 512 logical pixels and rejects the small yellow fallback for
  folders. It accepts a valid lower-resolution Shell folder texture enlarged into the icon box.
- Existing stale-result, cache-budget, DPI, theme, and mode-switch tests continue to pass.

## Error handling

Shell failures remain non-blocking. The UI keeps the best compatible Shell texture, schedules no
duplicate request for an already pending key, and uses the generic fallback only until compatible
Shell pixels arrive.
