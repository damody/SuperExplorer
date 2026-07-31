## ADDED Requirements

### Requirement: Single extensible Rust root module
Each Rust DLL SHALL export one `abi_stable` root module that reports metadata, SDK compatibility and a prefix-type registrar. A DLL MAY register multiple feature-scoped interfaces; SDK 1.x SHALL evolve the registrar only by appending optional functions or non-exhaustive data.

#### Scenario: Older 1.x plugin meets a newer host
- **WHEN** a plugin omits a registrar function appended later in SDK 1.x
- **THEN** the host loads its supported interfaces without interpreting the absent optional function

#### Scenario: Existing ABI field changes meaning
- **WHEN** a plugin's required layout or established numeric ID semantics differ from the host
- **THEN** layout/compatibility validation rejects the DLL before registration

### Requirement: Stable ABI data boundary
Stable callbacks SHALL use fixed-width primitives and `abi_stable` FFI-safe owned types. They SHALL NOT cross `std` collections, ordinary Rust trait objects, futures, closures, GPUI entities, private model types, native handle wrappers or allocator-ambiguous memory.

#### Scenario: SDK API exposes a forbidden type
- **WHEN** public ABI validation detects a forbidden Rust or private workspace type in an exported interface
- **THEN** the SDK build or validator fails before publication

### Requirement: Exact P0-0 toolchain baseline
The SDK SHALL fix Rust `1.97.1` for `x86_64-pc-windows-msvc`, Cargo from the same toolchain, and `abi_stable = 0.11.3` with the protected feature set recorded in `sdk-lock.json`. Builds SHALL verify compiler/Cargo commit hashes, not only display versions.

#### Scenario: Compiler commit differs
- **WHEN** a plugin is built by a compiler that reports 1.97.1 but has a different commit hash from the bundle
- **THEN** the UI fingerprint differs and GPUI contribution loading is rejected

### Requirement: Authorized GPUI source and immutable snapshots
The only authorized GPUI source SHALL be `https://github.com/damody/gpui-ce-explorer.git`. Development `main` SHALL be used only by an update job to resolve a complete commit; every actual host, plugin and CI build SHALL use an immutable snapshot bundle ID, canonical lock and vendored tree for that commit.

#### Scenario: GPUI main advances
- **WHEN** the update job resolves a newer `main` commit and all host, SDK and eight-example gates pass
- **THEN** it publishes a new snapshot bundle ID and atomically moves host and official consumers to it

#### Scenario: Candidate update fails
- **WHEN** any compatibility, UI, example or packaging test fails for a GPUI candidate
- **THEN** the previous approved snapshot remains active and no half-updated host/SDK state is published

### Requirement: Non-fast-forward update protection
The GPUI update job SHALL detect non-fast-forward history and SHALL require explicit approval before switching. Published snapshots SHALL remain offline-rebuildable from their own vendor source even if the remote commit becomes unreachable.

#### Scenario: Main is force-pushed
- **WHEN** the remote branch no longer descends from the current approved snapshot
- **THEN** automatic update is refused and the existing snapshot still rebuilds without network access

### Requirement: GPUI UI fingerprint
Any DLL registering a GPUI interface SHALL match an exact fingerprint derived from rustc/Cargo commits, target, resolved GPUI commit/tree, protected dependency graph, SDK public crate hashes, features, profile, panic strategy, allocator/CRT policy, LTO, codegen units, rustflags and ABI schema version. The fingerprint SHALL NOT include an unrelated SuperExplorer build ID.

#### Scenario: Dependency feature changes
- **WHEN** the plugin and host use the same GPUI commit but different protected features or profile inputs
- **THEN** the loader rejects the DLL before executing its first callback and reports both bundle IDs

#### Scenario: Host updates unrelated code
- **WHEN** a SuperExplorer update changes no fingerprint input
- **THEN** an already compatible plugin remains compatible without a rebuild-ID match

### Requirement: Release freeze
At RC cut, the system SHALL select a fully tested development snapshot, create/record a protected GPUI tag, set `release_frozen = true`, generate a signed release bundle and rebuild host, fixtures and eight examples offline. A post-freeze commit change SHALL create a new RC/bundle ID and repeat all gates.

#### Scenario: GPUI main changes after release
- **WHEN** the remote `main` advances after the release bundle is published
- **THEN** rebuilding that release continues to use its frozen commit, lock and vendor tree

### Requirement: Offline SDK bundle and protected dependency closure
The SDK SHALL ship `rust-toolchain.toml`, `Cargo.toml`, canonical `Cargo.lock`, `.cargo/config.toml`, `sdk-lock.json`, bundle manifest, offline vendor, SDK crates, fixtures, templates, AI prompt and build/validate/package scripts. Official builds SHALL use `cargo build --locked --offline` in an isolated empty Cargo home.

#### Scenario: Network and global cache are unavailable
- **WHEN** host and plugin fixtures build with an isolated empty Cargo home and network disabled
- **THEN** both build from bundle sources and the plugin loads into the fixture host

### Requirement: Plugin-private Rust dependencies
An author MAY add precisely locked private dependencies, but SHALL preserve the protected dependency closure and provide their own vendor, provenance and licenses. Validation SHALL follow actual metadata edges and SHALL reject a second GPUI/SDK type entering callback boundaries.

#### Scenario: EXIF parser is added privately
- **WHEN** an external author adds a static Rust EXIF parser without changing the protected closure
- **THEN** the plugin can build after its private dependency is locked, vendored and documented

### Requirement: GPUI callback execution boundary
GPUI callbacks SHALL run only on the GPUI thread with public immutable snapshots, a theme facade, action sink and scoped invalidation handle. They SHALL NOT perform file/network I/O or retain private host entities.

#### Scenario: Renderer performs slow work
- **WHEN** a renderer exceeds timing thresholds
- **THEN** diagnostics identify its package/interface and the host may reduce invalidation frequency without unsafe forced interruption
