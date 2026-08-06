## Why

Details headers currently treat an entire target column as `insert before`, so adjacent leftward
drags work while adjacent rightward drags appear unchanged until the pointer crosses an additional
column. The drag also withholds the resulting row layout until drop, preventing users from seeing
where the column will land.

## What Changes

- Resolve a header drag against the target column midpoint, with symmetric left and right insertion
  slots.
- Reproject headers and every visible Details row into the prospective order while the pointer is
  still down.
- Commit the prospective order once on drop and restore the original order on Escape or pointer
  cancellation.
- Preserve the fixed-first `Name` column, click-to-sort behavior, resize-grip isolation, filters,
  accessibility order, and existing session persistence.
- Add unit, state, projection, and blocking UTIT coverage for adjacent rightward and leftward drags,
  including a before-mouse-up assertion of header and data-cell movement.

## Capabilities

### New Capabilities

- `details-column-live-reordering`: Midpoint-based, symmetric Details-column drag preview, commit,
  cancellation, invariants, and observable test behavior.

### Modified Capabilities

None.

## Impact

- `crates/explorer-ui`: drag actions/state, midpoint resolution, effective-order projection, GPUI
  header handlers, cancellation, and focused tests.
- `crates/explorer-model`: only if the existing ordered-layout API needs a pure prospective-order
  helper; no persistence-schema change is intended.
- `scripts` and `uitest/manifest.json`: real-pointer live-preview and symmetry regression coverage.
- No public extension ABI, file-operation, provider, dependency, or packaging change.
