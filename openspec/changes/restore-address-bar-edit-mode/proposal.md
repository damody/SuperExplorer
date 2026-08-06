## Why

The details-column drag implementation dispatches a drag-cancel action on every left-button release, so a pointer click enters address editing and immediately cancels it. This regression blocks normal Explorer-style path editing and requires a lifecycle boundary plus automated coverage before further pointer interactions are added.

## What Changes

- Keep root-level details-column drag cancellation capture-safe while making inactive cancellation a focus-neutral no-op.
- Treat details-column drag update, commit, and cancellation as passive pointer lifecycle actions that do not independently close text editors.
- Preserve ordinary click-outside address cancellation and genuine details-column drag cleanup.
- Add focused unit/structural tests and a real-input UTIT scenario covering pointer entry, keyboard entry, editing survival, cancel/submit, and drag cleanup.

## Capabilities

### New Capabilities

- `address-bar-edit-lifecycle`: Defines Explorer-compatible address-edit entry and termination behavior when other pointer interaction lifecycles coexist in the window.

### Modified Capabilities

None.

## Impact

- `crates/explorer-ui`: pointer action classification, root release dispatch, address-edit and details-column drag regression tests.
- `uitest/manifest.json` and its headful runner coverage: a real Windows input regression scenario and evidence output.
- No public API, persistence format, navigation parser, dependency, installer, or plugin ABI changes.
