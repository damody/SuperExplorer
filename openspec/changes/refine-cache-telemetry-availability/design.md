## Context

`CacheUsageSnapshotV1` currently represents most usage values as `Option<u64>`, and the Folder Options renderer translates every `None` into `Unavailable`. The same representation is used for startup, slow sampling, pending MFT acknowledgement, reconnect, and terminal failure, so the UI cannot distinguish delay from true inability. The approved source design is `docs/superpowers/specs/2026-08-07-cache-telemetry-availability-presentation-design.md`.

## Goals / Non-Goals

**Goals:**

- Model cache telemetry availability independently from the optional current sample.
- Retain the last successful per-owner sample across pending refreshes.
- Show configured limits immediately and reserve `Unavailable` for confirmed terminal failures.
- Recover in the open Folder Options window without navigation or reopening.

**Non-Goals:**

- Changing cache limits, eviction policies, telemetry frequency, MFT permissions, IPC framing, or plugin ABI.
- Fabricating zero usage while no sample exists.

## Decisions

### Use an explicit availability enum

Add a small UI-facing state with `Pending`, `Available`, and `Unavailable`. Optional bytes remain separate so `Pending` can carry a retained value or no first sample. This is preferred over inferring state from `Option`, because absence is not evidence of failure.

### Retain last success in the sampler

The Folder Options sampler merges each new snapshot with its last successful snapshot. Pending fields keep their prior bytes; successful fields replace them; explicit unavailable fields remain unavailable while preserving their configured limits. Retention stays bounded to one snapshot and adds no growing cache.

### Determine terminal failure at the owner boundary

The app/service adapter marks unavailable only for authoritative conditions: missing/stopped service, incompatible/rejected IPC, or a terminal connection failure. Configuration awaiting acknowledgement and reconnect retry remain pending. This keeps transport knowledge out of the renderer.

### Render state and limit independently

The renderer always shows the configured/effective limit. Available and retained pending values show `used / limit`; first-sample pending shows `— / limit`; confirmed failure shows `Unavailable / limit`. Editors remain enabled in all states.

## Risks / Trade-offs

- **A stale value can remain visible longer during a slow update** → The user accepts slower updates; tests ensure it is replaced after recovery and is never presented after an explicit terminal failure.
- **Incorrect owner classification could hide a real outage** → Centralize terminal classification and cover stopped/missing/incompatible service fixtures.
- **Existing tests construct snapshots directly** → Provide defaults that preserve pending semantics and update fixtures intentionally.

## Migration Plan

No persisted-data migration is required. Ship the enum and renderer together, run targeted and installed-build tests, and roll back the UI/app changes together if telemetry classification regresses.

## Open Questions

None. The approved policy tolerates slow refresh and reserves `Unavailable` for confirmed inability.
