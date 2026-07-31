## Why

Real Windows Shell Properties sheets invoked by SuperExplorer currently open at the desktop's
top-left corner, disconnected from the window and item that launched them. They should follow
Explorer-like owner-aware placement while retaining the real Shell handler and persistent STA
lifecycle.

## What Changes

- Center each host-owned Properties sheet over the active SuperExplorer window before activation.
- Fall back to the invocation-point monitor's work area when the app owner is invalid.
- Clamp placement to the monitor work area without changing native size, focus, Z-order, pages, or
  Shell ownership.
- Extend result-based UTIT coverage across file, folder, multi-selection, executable, and script
  Properties sheets, including post-dismissal context-menu usability.

## Capabilities

### New Capabilities

- `properties-window-placement`: Owner-relative, work-area-safe placement and lifecycle validation
  for native Windows Shell Properties sheets.

### Modified Capabilities

None.

## Impact

- `explorer-shell-win` persistent context-menu STA and Windows hook/monitor FFI.
- Existing built-in context-command headful UTIT, evidence report, and manifest coverage.
- No protocol, broker-count, installer-format, or public API change.

