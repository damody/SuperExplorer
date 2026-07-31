## Why

Restored tabs can retain a bare Windows drive designator such as `D:` instead of the absolute drive root `D:\`. The file list may still render, but Shell context-menu resolution interprets the parent relative to the process drive current directory and refuses to create an item menu, producing the manual-only regression that isolated UTIT fixtures missed.

## What Changes

- Canonicalize only bare filesystem drive designators to absolute Windows drive roots after Shell resolution.
- Ensure restored history commits and subsequently persists the canonical root descriptor.
- Add a headful UTIT that restores a multi-tab, high-DPI session containing a bare drive designator and opens an item context menu with a physical secondary-button gesture without topmost assistance.
- Preserve ordinary explicit filesystem paths and opaque Shell namespace identities unchanged.

## Capabilities

### New Capabilities

- `restored-drive-root-context-menu`: Restored drive-root tabs retain absolute parent identity and can invoke native item context menus under real window activation conditions.

### Modified Capabilities

None.

## Impact

- `explorer-shell-win` location canonicalization and unit coverage.
- Session-restoration and native-context-menu headful UTIT coverage.
- UTIT manifest result mapping and packaged debug/release validation.
