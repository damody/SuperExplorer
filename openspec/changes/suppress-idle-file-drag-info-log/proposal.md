## Why

Ordinary pointer hover over file rows dispatches `UpdateFileDrag` at pointer frequency and currently writes every disabled result at INFO. This obscures useful operational events and grows normal logs even though no drag is taking place.

## What Changes

- Treat `UpdateFileDrag` dispatch records as high-frequency diagnostic telemetry and emit them at TRACE.
- Preserve INFO logging for drag lifecycle boundaries and all ordinary explorer actions.
- Preserve action dispatch, returned traces, drag behavior, and focus behavior.

## Capabilities

### New Capabilities

- `explorer-action-observability`: Defines normal-log and high-frequency diagnostic logging behavior for explorer actions.

### Modified Capabilities

None.

## Impact

- Affected code: `crates/explorer-ui/src/actions.rs`.
- No public API, persisted-state, dependency, or interaction changes.
- Normal INFO logs no longer contain pointer-frequency `UpdateFileDrag` records; targeted TRACE diagnostics still can.
