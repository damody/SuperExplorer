## Why

Folder Options currently occupies an in-window overlay whose fixed viewport hides
long content and prevents it from behaving like an independent Windows settings
dialog. Moving it to one modeless native window makes every page reachable through
an explicit scrollbar while leaving normal file navigation responsive.

## What Changes

- Replace the Explorer-window Folder Options overlay with one application-owned,
  modeless GPUI window.
- Reuse the existing General, View, and Extensions page controls and typed setting
  reducers in the dedicated window.
- Keep the page tabs and OK/Cancel/Apply actions fixed while the selected page uses
  an independent, visible right-side vertical scrollbar.
- Preserve one scroll offset per page and consume wheel/pointer input inside the
  options window so the Explorer behind it cannot scroll.
- Activate the existing window on repeated open requests and recover safely from a
  stale handle or creation failure.
- Preserve Explorer-style draft, Apply-baseline, OK, Cancel, Escape, and title-close
  semantics across all live Explorer windows.
- Add Rust coverage and registered headful UITEST coverage for native window
  identity, lifecycle, scrolling, focus, DPI, and setting transitions.

## Capabilities

### New Capabilities

- `folder-options-window`: Dedicated Folder Options window ownership, modeless
  lifecycle, scrolling, focus, draft/apply behavior, and recovery.

### Modified Capabilities

None. The existing `extension-options-management` contract continues to govern the
Extensions page and its user-wide settings; this change governs the window that
hosts that page.

## Impact

- `explorer-app`: application-scoped window controller, creation/activation,
  cross-window setting synchronization, and shutdown cleanup.
- `explorer-ui`: extracted Folder Options view, page-local scroll handles,
  fixed-header/footer layout, focus and typed action routing.
- `uitest`: a registered headful case and DPI-aware evidence for the dedicated
  window and input isolation.
- No external API, plugin ABI, persisted setting schema, or filesystem behavior is
  changed.

## Non-goals

- Replacing GPUI controls with Win32 common controls.
- Redesigning the General, View, or Extensions setting inventory.
- Changing extension enablement semantics, search settings, or the About dialog.
- Supporting multiple concurrently editable Folder Options drafts.

## Compatibility

Existing persisted settings and typed option actions remain valid. The visible
container changes from an overlay to a native window, but successful Apply/OK
operations keep their existing user-wide effect.
