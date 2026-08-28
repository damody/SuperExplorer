## Why

ADB and SFTP currently use a plain text GPUI context menu that visually diverges from the Windows 11 File Explorer menu used for local items. Remote locations should feel like first-class Explorer locations without routing their non-Shell paths through commands that cannot operate on them.

## What Changes

- Render ADB and SFTP item context menus with a Windows 11-style command strip, icons, grouped text commands, separators, spacing, corners, border, shadow, and semantic interaction states.
- Render remote-folder background menus with the same visual contract while retaining only applicable commands such as New folder and Paste.
- Preserve the existing remote action dispatch, capability checks, clipboard integration, popup positioning, dismissal behavior, and detailed operation errors.
- Cover light, dark, high-contrast, mouse, and keyboard-focus states without changing the native Windows Shell menu used for local filesystem items.
- Do not expose third-party Shell extensions or register remote namespaces with Windows Shell.

## Capabilities

### New Capabilities

- `remote-windows-context-menu`: Visual structure, applicable command presentation, interaction states, placement, dismissal, and behavior preservation for ADB and SFTP context menus.

### Modified Capabilities

None.

## Impact

- Primary code: `crates/explorer-ui/src/chrome.rs`, with focused supporting changes to icon or theme primitives only if the existing public primitives are insufficient.
- Tests: focused `explorer-ui` unit/source-contract tests and headful checks for ADB/SFTP item and background menus.
- Compatibility: no public API, manifest, persistence, provider, transfer, or local Shell context-menu changes.
- Dependencies: no new external dependencies.
