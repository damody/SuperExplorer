# Unified Immersive Context Menu Style Design

## Goal

Make Local file context menus use the same Windows immersive styling already visible on Local folder/background menus, while preserving native Shell behavior. Make the custom ADB/SFTP menu visually equivalent through shared measured tokens where native Windows styling cannot apply.

## Scope

- Local item, folder, and background menus hosted by `explorer-shell-win`.
- The existing GPUI ADB/SFTP context menu.
- Light/dark theme, high contrast, per-monitor DPI, nested menus, keyboard input, third-party owner-draw handlers, cancellation, and repeated invocation.
- Runtime capability detection, safe fallback, diagnostics, tests, and headful evidence.

## Non-goals

- Replacing Local Shell commands with a GPUI command snapshot.
- Injecting code into `explorer.exe` or requiring ExplorerPatcher to be installed.
- Copying ExplorerPatcher GPLv2 source or shipping its binary/pattern database.
- Reimplementing or calling private Windows immersive-menu helpers.
- Making ADB/SFTP expose Local-only Shell extensions.

## Reference Finding

ExplorerPatcher proves that the legacy `HMENU` command model can be retained while owner-draw changes its visual presentation. SuperExplorer adopts that architecture, but independently draws normal HMENU rows with documented Win32/UxTheme/GDI APIs. It does not call ExplorerPatcher or Windows private immersive helpers. SuperExplorer owns its popup call site, so it needs a scoped adapter rather than process-wide API hooks.

## Architecture

### 1. Scoped native immersive adapter

Add a Windows-only module under `explorer-shell-win` that exposes a small session API:

```text
ImmersiveMenuCapability::probe()
        -> Unsupported(reason)
        -> Available(adapter)

adapter.apply(hmenu, owner, origin, theme, dpi)
        -> ImmersiveMenuSession

ImmersiveMenuSession::handle_owner_message(...)
ImmersiveMenuSession::finish()
```

The adapter dynamically resolves the required Windows implementation for the running build. Resolution is cached, never fatal, and records a structured reason when unavailable. The session owns all rendering data and cleanup; `Drop` is a final safety net, not the primary lifecycle path.

No global IAT hook or process injection is needed. The adapter runs only around SuperExplorer's existing `TrackPopupMenuEx` call.

### 2. Existing-skin and compatibility gate

Before applying a skin, inspect the menu/session for existing owner-draw state and third-party owner-draw entries. Existing owner-draw entries and their `dwItemData` remain authoritative. The adapter converts only ordinary string/separator/check/bitmap items it can snapshot and restore; incompatible items remain extension-owned or cause an evidence-backed session fallback.

The change must not overwrite command IDs, submenu handles, bitmaps, verbs, or Shell handler data.

### 3. Owner message routing

The hidden native owner window routes menu messages in this order:

1. Active immersive session for `WM_MEASUREITEM` and `WM_DRAWITEM`.
2. Existing `IContextMenu3` forwarding for extension-owned messages and dynamic submenus.
3. Default window procedure.

A message claimed by one renderer is not sent to another renderer. `WM_INITMENUPOPUP` and `WM_MENUCHAR` continue to reach `IContextMenu3`. Session teardown occurs after `TrackPopupMenuEx` on selection, cancellation, error, panic boundary, or replay scheduling.

### 4. Safe fallback

Capability discovery, application, message handling, or cleanup failure must never suppress the menu. Before tracking begins, any failure falls back to the existing unskinned native path. A failure discovered during a session disables immersive styling for later sessions in that process and records diagnostics; the current menu remains cancellable and Shell resources are released exactly once.

High-contrast mode always selects system-native rendering.

### 5. Shared visual tokens for ADB/SFTP

ADB/SFTP remains GPUI-rendered because it has no Shell `HMENU`. Add a `ContextMenuVisualTokens` projection containing surface, border, divider, text, danger, hover, pressed, font, row height, icon gutter, horizontal inset, width policy, and shadow geometry.

The values are calibrated from headful Local immersive-menu measurements at each supported DPI/theme combination. The projection is consumed only by custom remote menus; it does not recolor file or folder listing rows.

### 6. Observability

Record bounded diagnostics for capability status, Windows build, apply result, fallback reason, cleanup result, existing owner-draw detection, theme, and DPI. Do not record file paths, command labels, user names, or handler-specific private data.

## Licensing and Platform Decision

ExplorerPatcher is a behavioral reference only. Its GPLv2 implementation is not copied. The SuperExplorer implementation uses independently written Rust/Win32 bindings and its existing Windows isolation boundaries.

The renderer uses documented public Windows APIs and still remains capability-gated for theme handles, owner-draw compatibility, and resource creation. The current unstyled legacy menu remains the permanent fallback.

## Testing

### Automated

- Capability probe success, missing theme service, incompatible owner-draw state, and cached failure.
- Apply/finish exactly-once lifecycle for selection, cancellation, query failure, and replay.
- Message routing precedence and no double handling.
- Preservation of command IDs, submenu handles, bitmaps, `dwItemData`, and `IContextMenu3` messages.
- Existing immersive menu and third-party owner-draw fallback.
- Remote token projection for light, dark, high contrast, and DPI scaling.
- No changes to Local listing-row colors.

### Headful Windows evidence

- The same `C:\Windows\System32\appverifUI.dll` in File Explorer and SuperExplorer.
- Local file, folder, and background menus at 100%, 125%, 150%, and 200% DPI.
- Light, dark, and high-contrast modes.
- 7-Zip, WinRAR, TortoiseGit, VS Code, Defender, nested submenus, Shift-extended verbs, and multi-selection.
- ADB and SFTP item/folder/background menus compared with the accepted Local style.
- Repeated open/cancel, right-click replacement, keyboard invocation, Escape, and monitor-edge placement.

## Rollout and Rollback

Ship behind a typed runtime setting defaulted off until the compatibility and headful gates pass. Once enabled by default, capability or session failure automatically falls back without changing stored user data. Rollback is removal/disablement of the adapter flag; no migration is required.

## Planning Adjustment Rules

- **A — task refinement:** task split, command, fixture, or ordering changes that do not change behavior, gates, or public contracts.
- **B — design/spec correction:** an implementation discovery within this approved scope; pause affected work, update design/spec/tasks, mark dependent evidence stale, and revalidate.
- **C — material change:** copying GPL code, adding injection/global hooks, weakening a gate, removing fallback, changing supported platforms, or adding externally visible/destructive behavior; requires user approval.

## Acceptance

- Supported systems show the accepted immersive Local style for file, folder, and background menus without losing Shell commands or behavior.
- Unsupported or failed systems show the existing native menu and remain fully usable.
- ADB/SFTP matches the accepted Local typography, spacing, colors, dividers, hover states, and shadow at the active theme and DPI.
- All lifecycle, compatibility, accessibility, strict OpenSpec, build, test, and headful evidence gates pass.
