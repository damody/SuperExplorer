## Why

Session restore rebuilds saved background tabs without directory contents, but activating one does not connect it to the directory service. The tab therefore remains blank with `Directory service is not connected` until the user manually refreshes, unlike Windows File Explorer.

## What Changes

- Load the restored active tab immediately during application startup.
- Lazily load each restored background tab the first time it becomes active through pointer selection, keyboard cycling, or closing another tab.
- Submit at most one automatic navigation request while a tab is loading and never turn a genuine terminal error into an activation retry loop.
- Add state-level regression coverage and a two-process headful UTIT that restores multiple tabs, activates the background tab, and proves contents appear without F5.
- Preserve the existing session schema, directory service contract, request cancellation, stale-generation rejection, and explicit refresh behavior.

## Capabilities

### New Capabilities

- `restored-tab-directory-autoload`: Defines immediate active-tab loading and first-activation loading for restored background tabs, including duplicate suppression, failure behavior, and restart UTIT evidence.

### Modified Capabilities

None.

## Impact

- `crates/explorer-ui`: restored-tab state inspection, shared post-activation load submission, and unit tests.
- `crates/explorer-model`: no public contract or session schema change is expected; existing directory states and request contexts remain authoritative.
- `scripts` and `uitest/manifest.json`: real restart coverage and artifacts for active and background restored tabs.
- No dependency, installer, extension ABI, or persisted-data migration impact.
