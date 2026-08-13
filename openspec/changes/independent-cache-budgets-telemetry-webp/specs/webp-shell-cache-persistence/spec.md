## ADDED Requirements

### Requirement: Icon disk cache uses lossless WebP
The Shell icon disk cache SHALL persist decoded icon pixels as lossless WebP and SHALL preserve alpha values across a valid encode/decode round trip.

#### Scenario: Alpha icon round trip
- **WHEN** an icon containing opaque, translucent, and transparent pixels is stored and loaded
- **THEN** decoded dimensions and every RGBA channel equal the original values

### Requirement: Thumbnail disk cache uses quality-80 WebP
The Shell thumbnail disk cache SHALL persist thumbnails as lossy WebP using quality 80 and SHALL validate the decoded payload against the request resource bounds before returning it.

#### Scenario: Valid thumbnail round trip
- **WHEN** a bounded thumbnail is stored and loaded under the same key
- **THEN** the decoded dimensions match, decoded bytes remain within the configured maximum, and the source is reported as the project disk cache

### Requirement: WebP entries use a bounded versioned envelope
Each WebP cache entry SHALL use a versioned checksummed envelope containing cache kind, key digest, decoded dimensions, and encoded length. Icon and thumbnail roots, limits, accounting, and clear operations SHALL remain independent.

#### Scenario: Matching entry loads
- **WHEN** envelope version, kind, digest, checksum, dimensions, lengths, and decoded resource limits are valid
- **THEN** the entry loads as an owned payload and updates only its cache's access metadata

#### Scenario: Corrupt entry is removed and regenerated
- **WHEN** an entry is truncated, has a checksum mismatch, has a mismatched key or kind, contains invalid WebP, or decodes outside resource limits
- **THEN** the entry is rejected and removed and the normal Shell provider path remains available to regenerate it

#### Scenario: Decompression bomb is rejected
- **WHEN** encoded input declares or produces decoded dimensions or bytes above the request limit
- **THEN** decoding fails closed without publishing the entry or retaining an unbounded allocation

### Requirement: WebP publication and quota enforcement are atomic
The system SHALL publish WebP entries through same-directory temporary files and atomic replacement semantics, and SHALL enforce each cache's entry and byte quota without deleting session, log, or sibling cache data.

#### Scenario: Concurrent writer does not corrupt entry
- **WHEN** two jobs publish the same cache key concurrently
- **THEN** one complete valid entry remains and no partial entry is observable as a hit

#### Scenario: Quota cleanup remains isolated
- **WHEN** icon or thumbnail WebP storage exceeds its own quota
- **THEN** LRU cleanup removes entries only from that cache root until it satisfies its quota

### Requirement: Raw-RGBA cache migration is lazy and safe
The new cache SHALL NOT treat prior `.rgba` entries as WebP and SHALL regenerate new entries lazily without a recursive startup conversion.

#### Scenario: Old entry is not misdecoded
- **WHEN** only a prior raw-RGBA entry exists for a requested key
- **THEN** the new WebP reader reports a miss and the request can regenerate a WebP entry through the provider

#### Scenario: Startup avoids bulk conversion
- **WHEN** an existing cache root contains prior raw-RGBA entries
- **THEN** application startup performs no bulk decode or encode pass over those entries

### Requirement: WebP dependency passes repository gates
The selected WebP implementation SHALL support locked offline Windows builds, approved licensing, lossless alpha icons, quality-80 thumbnails, and bounded decode behavior before production writers are enabled.

#### Scenario: Codec gate passes
- **WHEN** dependency, license, offline build, alpha, quality, corruption, and decoded-resource evidence all pass
- **THEN** the WebP writer may become the active cache schema

#### Scenario: Codec gate fails
- **WHEN** any required dependency or safety evidence fails
- **THEN** production writer activation remains blocked and the gate SHALL NOT be weakened without user approval
