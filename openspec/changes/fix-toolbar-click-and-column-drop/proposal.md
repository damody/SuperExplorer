## Why

The details-column drag cancellation handlers now intercept ordinary pointer releases and race valid drop targets. As a result command-bar buttons can become inert and a valid column reorder visually previews but reverts on release, diverging from Windows File Explorer behavior.

## What Changes

- Stop generating details-column cancellation actions from unrelated command-bar pointer releases.
- Give a valid details header drop priority over the fallback outside-release cancellation.
- Preserve cancellation and original-order restoration when a drag ends outside every valid header.
- Preserve `Name` as the fixed leftmost column and persist successful movable-column reorders.
- Add unit/structural coverage and installed-app UTIT coverage using genuine pointer input for command clicks, committed drops, canceled drops, and persistence.

## Capabilities

### New Capabilities

- `explorer-pointer-interaction`: Defines command-bar click delivery and details-column drag/drop terminal ownership, persistence, cancellation, and fixed-column behavior.

### Modified Capabilities

None.

## Impact

- `crates/explorer-ui/src/chrome.rs`: root pointer handler and details header drag/drop callbacks.
- `crates/explorer-ui/src/lib.rs`: interaction/reducer tests if additional observable state assertions are required.
- `scripts/` and `uitest/manifest.json`: installed-app pointer regression automation and evidence.
- No public API, extension ABI, settings serialization format, or dependency changes.
