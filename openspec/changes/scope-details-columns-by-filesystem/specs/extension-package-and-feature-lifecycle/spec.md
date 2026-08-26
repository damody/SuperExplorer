## MODIFIED Requirements

### Requirement: Unified package manifest
The system SHALL load Rust, Lua, Skin, locales, tools, licenses, content hashes, entry points, dependencies and feature declarations from a versioned `.sepack` manifest. The manifest SHALL use normalized stable package, publisher, feature and interface IDs and SHALL reject duplicate, unknown-required or over-length identifiers. Every data-column contribution SHALL contain an author-owned `file_systems` array whose allowed values are `local`, `adb`, and `sftp`; users SHALL NOT override it. Missing or empty arrays SHALL normalize to an empty, everywhere-inactive scope, duplicate allowed values SHALL normalize to one value, and an unknown value SHALL reject the affected contribution with one bounded diagnostic.

#### Scenario: Valid multi-content package
- **WHEN** a `.sepack` contains valid Rust, Lua and Skin entries whose files and hashes match the manifest
- **THEN** the package is represented as one atomic package version with independently declared features

#### Scenario: Manifest and payload disagree
- **WHEN** an entry point, content hash, required capability or feature ID differs from the manifest
- **THEN** the host rejects the complete package before executing any callback

#### Scenario: Data column declares multiple filesystems
- **WHEN** a data-column contribution declares `local`, `adb`, and a duplicate `adb`
- **THEN** validation succeeds with an immutable normalized scope containing `local` and `adb`

#### Scenario: Data column omits filesystem scope
- **WHEN** a legacy or new data-column contribution omits `file_systems` or declares an empty array
- **THEN** the contribution remains structurally loadable but is inactive on every filesystem

#### Scenario: Data column names an unknown filesystem
- **WHEN** a data-column contribution includes a value other than `local`, `adb`, or `sftp`
- **THEN** validation rejects the affected contribution with one actionable diagnostic and does not broaden its scope

#### Scenario: User settings attempt to broaden scope
- **WHEN** persisted user column settings make a contribution visible on a filesystem absent from its manifest scope
- **THEN** the manifest scope remains authoritative and the contribution is inapplicable
