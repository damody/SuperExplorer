## Why

Focused text editors currently ignore the standard Windows `Shift+Home`, `Shift+End`, `Shift+Left`, and `Shift+Right` selection commands. This blocks familiar File Explorer editing workflows in the address field, search field, and inline rename editor.

## What Changes

- Add line-boundary selection actions and Windows key bindings to the shared editable-text component.
- Ensure shifted character movement extends, contracts, and reverses selection around a stable anchor.
- Apply the shared behavior consistently to all Super Explorer editable fields.
- Add component, binding-contract, and headful UTIT coverage for the four shortcuts.

## Capabilities

### New Capabilities

- `editable-keyboard-selection`: Windows File Explorer-compatible shifted keyboard selection in focused editable text controls.

### Modified Capabilities

None.

## Impact

The change affects the vendored `gpui_elements::editable_text` action and state layers, Explorer UI key-binding tests, the headful PowerShell smoke-test suite, and `uitest/manifest.json`. It does not change public application APIs or add dependencies.
