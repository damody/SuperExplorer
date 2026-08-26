## Why

Normal workspace compilation currently emits roughly 194–212 `unsafe_code` diagnostics from 113 canonical Windows FFI and raw-pointer locations. The warning volume obscures newly introduced unsafe boundaries and prevents the repository's warnings-as-errors quality gate from serving as an actionable regression signal.

## What Changes

- Audit every current `unsafe_code` diagnostic in normal workspace library and binary targets.
- Remove unnecessary unsafe blocks or use existing safe APIs where behavior remains identical.
- Add narrowly scoped `#[expect(unsafe_code, reason = "...")]` attributes to unavoidable unsafe operations, extern blocks, and unsafe functions.
- Require a specific expectation reason plus an adjacent soundness invariant for every accepted boundary.
- Preserve the workspace-level `unsafe_code = "warn"` lint and reject any new crate-wide or module-wide suppression in this change.
- Establish repeatable warning-inventory evidence and a zero-`unsafe_code` build gate without expanding this change into `dead_code` or general Clippy cleanup.

## Capabilities

### New Capabilities

- `unsafe-code-governance`: Defines the auditable unsafe-boundary policy and compilation gate for workspace-owned Rust targets.

### Modified Capabilities

None. This change does not alter product behavior or existing public capability requirements.

## Impact

- Affected code is limited to unsafe boundaries in `explorer-app` and `explorer-extension-host`, primarily the MFT query, service, focus, journal, migration, size-map, and SQLite modules.
- No public API, ABI, dependency, persistence format, installer, or runtime behavior changes are intended.
- Existing uncommitted user work must be preserved; edits are restricted to diagnostic locations and their safety documentation.
- Existing unrelated `dead_code`, Clippy, and all-target test compilation failures remain outside this change and must be reported accurately rather than hidden.
- Pre-existing broad unsafe suppressions outside the 113-location default-feature normal-target baseline are inventoried as deferred residual risk; removing them is a later cleanup wave.
