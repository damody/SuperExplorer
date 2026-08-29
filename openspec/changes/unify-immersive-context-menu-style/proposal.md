## Why

SuperExplorer currently presents the same Local Shell target with different visual treatment from ExplorerPatcher-skinned File Explorer because its independently hosted legacy `HMENU` never receives Windows immersive owner-draw styling. ADB/SFTP uses a separate GPUI renderer, so all three providers need a governed visual contract without sacrificing native Shell compatibility or safe fallback.

## What Changes

- Add a scoped, runtime-gated, SuperExplorer-owned Win32 popup host that materializes the authoritative `HMENU` after `QueryContextMenu` and returns its original command IDs.
- Detect unsupported extension-owned owner-draw rows, preserve `IContextMenu2/IContextMenu3`, dynamic submenus, command identity, state, bitmaps, and third-party handler data, and fall back to `TrackPopupMenuEx` without mutating the menu.
- Add bounded diagnostics and a rollout setting so popup-host failure or unsupported accessibility state cannot suppress context menus.
- Define shared context-menu visual tokens and apply their measured Local immersive values to ADB/SFTP custom menus without recoloring file/folder listing rows.
- Add automated and headful Windows evidence for theme, DPI, high contrast, lifecycle, nested menus, representative Shell extensions, and Local/ADB/SFTP visual parity.
- Do not inject into Explorer, require ExplorerPatcher, copy GPLv2 implementation code, or replace native Local commands with GPUI snapshots.

## Capabilities

### New Capabilities

- `native-immersive-context-menu-hosting`: Runtime capability discovery, scoped application-owned popup lifecycle, dynamic Shell message routing, compatibility preservation, diagnostics, and fallback for Local native context menus.
- `unified-context-menu-visual-style`: Observable typography, geometry, color, interaction, shadow, theme, DPI, accessibility, and listing-color isolation requirements shared by Local reference evidence and ADB/SFTP custom menus.

### Modified Capabilities

None. No existing main-spec capability defines these context-menu hosting or visual contracts.

## Impact

- `crates/explorer-shell-win`: application-owned popup host and integration with the native context-menu owner window.
- `crates/explorer-ui`: typed visual-token projection and ADB/SFTP renderer migration.
- `explorer-model` or existing settings/session contracts only if required for the typed rollout flag and diagnostics; no persisted-content migration.
- The documented Win32/GDI popup host remains optional with `TrackPopupMenuEx` as permanent fallback.
- Test/evidence scope includes Windows Shell extensions and headful theme/DPI comparisons; no new external service or system modification is required.
