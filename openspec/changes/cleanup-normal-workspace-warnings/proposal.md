## Why

The normal Rust workspace build still emits 17 non-`dead_code`, non-`unsafe_code` warnings after the warning-governance work. These diagnostics are mechanically correct but require manual review because two unused bindings are RAII guards whose removal would change resource lifetime.

## What Changes

- Manually remove redundant path qualifications, an unused import, and an unnecessary mutable binding.
- Preserve Win32 event-handle lifetime by retaining and explicitly marking the RAII guard bindings as intentionally unread.
- Remove the unused cancellation error binding without changing match selection or terminal state.
- Require both normal and all-target locked/offline workspace builds to complete with zero rustc warnings; do not use `cargo fix` or lint suppression.

## Capabilities

### New Capabilities

- `normal-workspace-warning-hygiene`: Defines behavior-preserving cleanup and a zero-warning gate for the normal Rust workspace build.

### Modified Capabilities

None.

## Impact

The change touches only compiler-reported sites in `explorer-ui`, `explorer-mft` source modules, `explorer-app`, `explorer-extension-host`, and the MFT service binary. It changes no public API, dependency, data format, runtime branch, or packaging behavior. Clippy-only warnings remain outside scope because resolving pedantic API and numeric-policy lints requires separate behavioral design.
