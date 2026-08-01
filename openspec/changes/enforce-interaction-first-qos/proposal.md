## Why

Background enumeration, file operations, search, thumbnails, previews, and Shell integrations can contend with foreground interaction even when individual features are asynchronous. SuperExplorer needs one enforceable interaction-first policy so overload delays content instead of freezing navigation or input.

## What Changes

- Introduce a central bounded QoS policy with explicit foreground-to-maintenance priority lanes.
- Enforce non-blocking UI dispatch, cancellable work, and generation-safe result delivery.
- Budget result integration per frame and shed low-value background work under pressure.
- Isolate navigation, file operations, thumbnails/previews, and search/index work so one blocking domain cannot starve another.
- Add privacy-safe latency, queue, cancellation, stale-result, and degradation observations.
- Add deterministic unit and integration stress tests for responsiveness during competing and stalled work.

## Capabilities

### New Capabilities

- `interaction-first-qos`: Defines foreground responsiveness, bounded scheduling, workload isolation, backpressure, degradation, recovery, observability, and stress-test requirements.

### Modified Capabilities

None.

## Impact

This affects `explorer-jobs`, `explorer-ui`, `explorer-shell-win`, the application composition root, performance diagnostics, and UT/IT runners. Existing typed commands and result identities remain compatible; internal scheduling and result-delivery APIs will gain QoS metadata and bounded-drain behavior.
