## ADDED Requirements

### Requirement: Versioned event envelopes
The system SHALL deliver events as owned envelopes containing name, payload version, source sequence, timestamp, applicable IDs, captured cwd context, and typed data.

#### Scenario: Event fields are unavailable
- **WHEN** an envelope field does not apply to a source
- **THEN** the field is absent rather than populated with a misleading default

### Requirement: Broad Explorer event coverage
The system SHALL expose application/window, navigation/tab/selection/search, file-operation, task/process/schedule, and AI lifecycle events defined by the approved event catalog.

#### Scenario: Explorer action completes
- **WHEN** a subscribed Explorer operation completes or fails
- **THEN** Lua receives the matching versioned completion or failure event with correlation identifiers

### Requirement: Script-scoped folder watches
The system SHALL let each script declare or receive UI overrides for multiple roots, recursion, include globs, and exclude globs.

#### Scenario: Matching folder change
- **WHEN** a file under an enabled script's configured root matches its filters
- **THEN** the script receives the corresponding create, modify, remove, rename, attribute, or security event

#### Scenario: Watcher overflows
- **WHEN** the operating-system watcher loses events
- **THEN** the script receives `watch.overflow` and the system does not claim that the stream is complete

### Requirement: Observation-only global hooks
The system SHALL expose global key, mouse, hotkey, foreground/window, clipboard, session, power, display, device, and network-change observations without allowing Lua to cancel or modify original events.

#### Scenario: Hotkey leaves input intact
- **WHEN** a configured chord is observed
- **THEN** Lua receives `hotkey.triggered` and the foreground application still receives the original key input

### Requirement: Non-blocking backpressure and privacy
Hook callbacks SHALL only capture minimal data and non-blockingly enqueue it. High-rate sources SHALL coalesce, queues SHALL be bounded, and raw keys, clipboard values, AI content, and paths SHALL not enter persistent diagnostics by default.

#### Scenario: Event flood
- **WHEN** mouse or window-location events exceed downstream capacity
- **THEN** the source callback remains responsive and the system coalesces/drops with explicit counters instead of blocking or growing without limit
