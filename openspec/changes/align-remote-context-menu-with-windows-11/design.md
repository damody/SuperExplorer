## Context

Local filesystem items use the native Windows Shell context menu. ADB and SFTP items cannot use that menu because they are provider-backed virtual locations rather than Shell namespace items. Their current GPUI menu is a single plain-text list, although the application already has Fluent icons, semantic theme colors, remote capability checks, action dispatch, popup state, and clamped positioning.

The approved source design is `docs/superpowers/specs/2026-08-29-remote-windows-context-menu-design.md`. This change is limited to remote context-menu presentation and focused interaction verification. Existing dirty-worktree changes are user-owned and must be preserved.

## Goals / Non-Goals

**Goals:**

- Match the recognizable Windows 11 File Explorer menu hierarchy: a compact icon command strip for common item actions, grouped text rows for remaining actions, separators, Fluent spacing, rounded surface, border, and shadow.
- Apply one visual contract to ADB and SFTP item and background menus.
- Preserve provider-aware command availability, action dispatch, dismissal, placement, clipboard, and error reporting.
- Support light, dark, high-contrast, hover, pressed, disabled, and keyboard-focus presentation.

**Non-Goals:**

- Register ADB or SFTP as Windows Shell namespace extensions.
- Load native or third-party Shell extension commands for remote items.
- Modify provider transfer, mutation, authentication, persistence, or local Shell menu behavior.
- Perform unrelated full regression testing.

## Decisions

### Use a GPUI Windows 11 presentation layer over existing remote actions

The renderer will construct a small command model containing action, label, icon, danger semantics, enabled state, and placement in either the icon strip or text section. Both ADB and SFTP use the existing `RemoteContextMenuState` and `ExplorerAction` pipeline.

Alternative: invoke the native Shell menu. Rejected because remote URIs have no valid PIDL or Shell item and Shell verbs would target the wrong namespace.

Alternative: retain the text list and only adjust colors and corners. Rejected because it omits the Windows 11 command hierarchy and remains visibly inconsistent.

### Reuse existing icons and semantic theme tokens

The implementation will use existing `ExplorerIcon` variants for Cut, Copy, Paste, Rename, Delete, New, and an appropriate open/folder icon. Existing `menu_fill`, divider, text, hover, pressed, focus, and danger tokens remain the color authority. No hard-coded light-only palette or external icon dependency will be added.

### Separate item and background command composition from rendering

Item menus place Cut, Copy, Rename, and permanent Delete in the top icon strip, then expose Open and applicable Paste as text commands. Background menus expose New folder and applicable Paste as text commands. Composition filters commands using the current remote capabilities before rendering.

This boundary lets tests verify command grouping without exercising provider I/O and prevents ADB/SFTP visual drift.

### Preserve popup lifecycle and clamp against measured contract dimensions

The full-window overlay continues to dismiss on outside left or right click. Mouse-down inside the menu stops propagation. Command activation uses the existing callback. Position clamping will use constants that cover the revised menu width and the maximum item-menu height so menus opened near window edges remain visible.

### Keep validation focused and late

Implementation proceeds without running broad checks after each edit. At the end, focused formatting/build/unit checks and representative headful checks cover ADB and SFTP item/background menus and ensure local native menus remain untouched.

## Risks / Trade-offs

- [Pixel-level Windows changes vary by OS build and scale factor] → Match the application's current Windows 11 Fluent tokens and structural hierarchy rather than undocumented Shell internals; validate at the active scale factor.
- [A wider icon strip can overflow near small window edges] → Use a fixed tested menu width and clamp the surface to client bounds.
- [Overlay event propagation can regress the prior menu-disappearing fix] → Keep stop-propagation at the menu surface and add a focused lifecycle/source-contract test.
- [Danger styling may over-emphasize the icon strip] → Use semantic danger color for the destructive command while retaining the same hover surface and confirmation behavior.
- [Existing uncommitted edits overlap `chrome.rs`] → Apply a narrow patch around the remote menu renderer and never revert unrelated hunks.

## Migration Plan

No data migration is required. Replace only the remote menu renderer and add focused tests. Rollback is the removal of the new presentation helper and restoration of the prior renderer; providers and stored state remain compatible.

## Open Questions

None. The user delegated remaining UI details and selected direct implementation.

