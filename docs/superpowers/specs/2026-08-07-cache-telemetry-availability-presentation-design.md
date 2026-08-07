# Cache telemetry availability presentation

## Goal

Folder Options must not label a cache owner `Unavailable` merely because telemetry is delayed, reconnecting, or has not produced its first sample. Slow updates are acceptable. `Unavailable` is reserved for a confirmed inability to use the owner.

## State model

Each telemetry value is presented from three explicit states:

1. **Available**: show the newest used bytes and effective limit as `used / limit`.
2. **Pending or stale**: retain and show the last valid `used / limit`. If no valid sample has ever arrived, show `— / configured limit`. Do not show `Unavailable`.
3. **Confirmed unavailable**: show `Unavailable / configured limit` only after an authoritative failure, such as a missing or stopped service, incompatible IPC protocol, rejected configuration, or an explicit terminal connection failure.

Ordinary sampling delay, a configuration update awaiting acknowledgement, a reconnect attempt, or a temporarily absent sample is not a confirmed failure.

## Data flow

The cache telemetry sampler retains its last successful snapshot. A new successful sample replaces it. Pending samples preserve the retained value. Confirmed failures set an explicit availability state without discarding the configured limit.

The presentation layer receives the availability state separately from optional used bytes. It does not infer service failure solely from `Option::None`.

## UI behavior

- Available: `314.9 MB / 1.0 GB`
- Pending with prior data: keep `314.9 MB / 1.0 GB`
- Pending without prior data: `— / 1.0 GB`
- Confirmed unavailable: `Unavailable / 1.0 GB`

The number textbox and slider remain usable in every state. Applying a new limit immediately updates the displayed configured limit while the usage sample may update later.

## Failure handling

The service/configuration path remains retryable. A confirmed failure may later transition back to pending and then available. The UI must recover without reopening Folder Options.

## Tests

- Unit tests cover first-sample pending, stale-value retention, confirmed failure, recovery, and limit changes while pending.
- UI tests verify `Unavailable` is absent during delayed telemetry and present only for an explicit failure fixture.
- Installed-build headful evidence captures an MFT service value during delayed refresh and after recovery.

## Scope

This change only refines cache telemetry availability semantics and presentation. It does not change cache limits, sampling frequency, service permissions, or eviction behavior.
