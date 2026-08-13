## ADDED Requirements

### Requirement: Icon and thumbnail memory budgets are independent
The system SHALL persist, normalize, apply, and enforce an icon memory-cache budget independently from a thumbnail memory-cache budget. The defaults SHALL be 32 MiB for icons and 128 MiB for thumbnails.

#### Scenario: Prior session receives thumbnail default
- **WHEN** a valid prior-session payload contains an icon budget but no thumbnail budget
- **THEN** the restored icon budget equals the persisted value and the restored thumbnail budget equals 128 MiB

#### Scenario: Icon limit changes independently
- **WHEN** the user changes the icon cache limit in Folder Options
- **THEN** only the icon cache budget changes and icon LRU eviction settles at or below that limit

#### Scenario: Thumbnail limit changes independently
- **WHEN** the user changes the thumbnail cache limit in Folder Options
- **THEN** only the thumbnail cache budget changes and thumbnail LRU eviction settles at or below that limit

### Requirement: Host publishes bounded cache telemetry
The Host SHALL publish an immutable bounded cache telemetry snapshot containing stable identity, category, availability, current bytes, optional limit, entry count, and applicable hit/miss counters for registered SuperExplorer caches. Telemetry SHALL NOT contain user paths, file names, or individual MFT records.

#### Scenario: Available cache is reported
- **WHEN** a registered cache supplies a valid sample
- **THEN** the snapshot reports its current owned bytes and optional configured limit under its stable identity

#### Scenario: Unavailable source is explicit
- **WHEN** a disk sampler, extension Host source, or MFT Service cannot supply a sample before its deadline
- **THEN** the corresponding telemetry entry is `Unavailable` rather than zero and navigation continues

#### Scenario: Totals saturate safely
- **WHEN** available cache byte counters would overflow their aggregate integer type
- **THEN** the subtotal saturates at the type maximum and the snapshot remains valid

### Requirement: Folder Options displays live cache usage
Folder Options SHALL display Memory, Disk, and MFT Service cache sections and refresh their snapshot once per second while the window is open. Refresh work SHALL be single-flight and recursive disk accounting SHALL NOT execute on the UI thread.

#### Scenario: Live values refresh
- **WHEN** Folder Options remains open across two refresh intervals and a cache value changes
- **THEN** the displayed value updates without closing or reopening the window

#### Scenario: Slow sample does not accumulate
- **WHEN** a background cache sample remains active across another one-second tick
- **THEN** no second concurrent sample starts and the UI retains the latest completed snapshot

#### Scenario: Window closure stops refresh
- **WHEN** Folder Options closes
- **THEN** its refresh timer and subscription are cancelled and no further window updates are delivered

#### Scenario: Partial total is labelled
- **WHEN** at least one member of a displayed subtotal is unavailable
- **THEN** the subtotal includes only available byte values and is visibly identified as partial

### Requirement: MFT Service exposes aggregate diagnostics locally
The MFT Service SHALL answer a fixed-size local diagnostics request with aggregate cache bytes, configured limit, count, persisted index bytes, hits, misses, and generation. It SHALL NOT return paths, file names, individual file sizes, or MFT index contents.

#### Scenario: Authorized local diagnostics succeeds
- **WHEN** an authorized local interactive client sends a valid diagnostics request
- **THEN** the service returns one bounded fixed-size aggregate response

#### Scenario: Malformed diagnostics fails closed
- **WHEN** a diagnostics request has an invalid discriminator, version, length, or bounded field
- **THEN** the service rejects it without returning cache contents or terminating service query handling

#### Scenario: Service unavailable is non-blocking
- **WHEN** the service is absent, disconnected, or misses its deadline
- **THEN** Folder Options displays MFT Service as `Unavailable` and remains interactive

### Requirement: Registered extension cache telemetry remains Host-owned
The Host SHALL report memory and persistent disk usage for Host-managed extension data-column cache storage without allowing plugins to set cache policy or publish arbitrary UI telemetry values.

#### Scenario: Host reports extension storage
- **WHEN** extension data-column values are present in Host memory or persistent storage
- **THEN** their owned bytes appear in the Host cache telemetry snapshot

#### Scenario: Plugin cannot override accounting
- **WHEN** a plugin returns extension data values
- **THEN** the Host derives cache accounting from owned storage and ignores plugin-supplied telemetry fields
