## Why

Folder Options currently renders `Unavailable` whenever a cache telemetry sample is absent, even while a working owner is merely starting, reconnecting, or updating slowly. This falsely reports a failure and obscures the configured limit despite the user explicitly accepting slower refreshes.

## What Changes

- Introduce explicit available, pending/stale, and confirmed-unavailable telemetry presentation states.
- Retain the last successful usage sample through slow refreshes and reconnects.
- Show `— / configured limit` before the first successful sample instead of `Unavailable`.
- Reserve `Unavailable / configured limit` for authoritative terminal failures and recover automatically when the owner returns.
- Add model, sampler, UI, and installed-build regression evidence for delayed, stale, failed, and recovered telemetry.

## Capabilities

### New Capabilities

- `cache-telemetry-availability`: Defines cache usage availability semantics, stale-value retention, confirmed failure presentation, and recovery behavior.

### Modified Capabilities

None.

## Impact

- `explorer-ui` Folder Options cache usage snapshot, sampler, and presentation logic.
- `explorer-app` MFT/service telemetry mapping where authoritative availability is determined.
- UITEST scripts and OpenSpec evidence for installed Folder Options behavior.
- No public plugin ABI, cache budget, eviction, service permission, or sampling-frequency change.
