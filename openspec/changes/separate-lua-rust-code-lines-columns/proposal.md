## Why

The Lua and Rust code-line examples currently collide in host state and use ambiguous naming, so users cannot reliably display and compare both implementations at once. The Rust example also needs a deterministic main-language summary rather than a mixed or single-file result for folders.

## What Changes

- Name the Lua contribution `Code lines` and the Rust contribution `Main code lines` while retaining distinct stable identities.
- Allow both code-line columns to be enabled, populated, rendered, and sorted concurrently in Details view.
- Aggregate Rust folder statistics by language, select the language with the highest aggregate code count, and resolve ties by ascending language name.
- Render the Rust value as `Language: N` with comma-grouped line counts while preserving an unformatted numeric sort key.
- Add unit, integration, contract, and headful screenshot gates covering coexistence and exact visible output.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `source-example-plugin-suite`: Refine the Lua and Rust tokei example contracts, Rust folder aggregation, exact labels, and joint headful verification.
- `extension-jobs-values-and-dynamic-columns`: Require concurrently enabled extension columns with distinct stable IDs to retain independent values, render plans, caches, and sorting state.

## Impact

Affected areas include the Details-view dynamic-column host state, code-line column descriptors and render routing, the Lua and Rust tokei fixture metadata and tests, the Rust tokei folder classifier/renderer, smoke/UITEST scripts, package outputs, and retained local verification evidence. Public ABI shapes and third-party dependencies do not change.
