## ADDED Requirements

### Requirement: Single extensible Rust root module
Each Rust DLL SHALL export one `abi_stable` root prefix module that reports metadata and SDK compatibility and directly contains the required SDK-owned registrar factory. The earlier handwritten raw-callback/custom-root layout was unpublished and experimental, so it does not constitute SDK 1.x. The first published SDK 1.x baseline SHALL be the fixed Rust-first `ExtensionRootModuleV1` with SDK-owned factory and panic trampoline: plugin authors implement ordinary Rust traits and SHALL NOT hand-write `extern "C"` callbacks or ABI layouts. After publication SDK 1.x SHALL NOT append or reinterpret root, factory, or trait-object layout fields; compatible evolution SHALL use the baseline descriptor/capability data contract and approved non-exhaustive values, while structural ABI changes require a new SDK major.

#### Scenario: Superseded pre-release raw root meets the Rust-first host
- **WHEN** a DLL exports the superseded unpublished raw-callback/custom-root layout
- **THEN** `abi_stable` layout validation rejects it before any accessor, factory, callback or native-call marker executes

#### Scenario: Published baseline plugin meets a newer 1.x host
- **WHEN** a plugin compiled against the published Rust-first baseline uses only descriptor/capability values understood by an older 1.x SDK
- **THEN** the newer host loads the identical checked root shape and preserves or rejects unknown non-exhaustive values according to their typed contract, without layout guessing

#### Scenario: Published ABI field changes meaning
- **WHEN** a plugin's required published-baseline layout or established numeric ID semantics differ from the host
- **THEN** layout/compatibility validation rejects the DLL before registration

### Requirement: Stable ABI data boundary
Stable callbacks SHALL use fixed-width primitives and `abi_stable` FFI-safe owned types. They SHALL NOT cross `std` collections, ordinary Rust trait objects, futures, closures, GPUI entities, private model types, native handle wrappers or allocator-ambiguous memory. The SDK contract SHALL define allocation origin, returned-value destruction, registrar/trait-object ownership, permitted drop thread and library lifetime. Factory, registrar, provider, renderer, service and destructor boundaries SHALL NOT unwind, and the bundle fingerprint SHALL identify the panic strategy used by both sides.

#### Scenario: SDK API exposes a forbidden type
- **WHEN** public ABI validation detects a forbidden Rust or private workspace type in an exported interface
- **THEN** the SDK build or validator fails before publication

#### Scenario: ABI-owned object is destroyed
- **WHEN** the host releases a registrar, returned value or trait object created by a plugin
- **THEN** destruction uses the SDK-defined owner and permitted thread while the DLL remains resident, and no panic unwinds across the ABI boundary

### Requirement: Exact P0-0 toolchain baseline
The SDK SHALL fix Rust `1.97.1` for `x86_64-pc-windows-msvc`, Cargo from the same toolchain, and `abi_stable = 0.11.3` with the protected feature set recorded in `sdk-lock.json`. Builds SHALL verify compiler/Cargo commit hashes, not only display versions.

#### Scenario: Compiler commit differs
- **WHEN** a plugin is built by a compiler that reports 1.97.1 but has a different commit hash from the bundle
- **THEN** the UI fingerprint differs and GPUI contribution loading is rejected

### Requirement: Authorized GPUI source and immutable snapshots
The only authorized GPUI source SHALL be `https://github.com/damody/gpui-ce-explorer.git`. Development `main` SHALL be read only during an explicit primary-agent update operation, which is the sole network operation permitted by this change and resolves a complete commit; every actual host/plugin build, fixture, test, promotion, rollback and release validation SHALL run locally from the checked-out repository against an immutable snapshot bundle ID, canonical lock and vendored tree for that commit.

#### Scenario: GPUI main advances
- **WHEN** the explicit primary-agent update operation resolves a newer `main` commit and every required local offline host, SDK, contract, UITEST and eight-example gate passes
- **THEN** it records a new snapshot bundle ID and atomically moves the local host and official consumers to it

#### Scenario: Candidate update fails
- **WHEN** any compatibility, UI, example or packaging test fails for a GPUI candidate
- **THEN** the previous approved snapshot remains active and no half-updated host/SDK state is published

### Requirement: Non-fast-forward update protection
The explicit primary-agent GPUI update operation SHALL detect non-fast-forward history and SHALL require explicit approval before switching. Recorded snapshots SHALL remain offline-rebuildable from their own vendor source even if the remote commit becomes unreachable.

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
At RC cut, the system SHALL select a fully validated local development snapshot, record its protected source revision, set `release_frozen = true`, create a signed local release evidence bundle, and rebuild host, fixtures and eight examples with `--locked --offline`. The release evidence bundle SHALL bind the exact commands or manual procedures, task and unique subcheck IDs, expected and actual results, source/environment metadata, SHA-256 inventory, RC identity and retention metadata; it SHALL be verified under the release-integrator trust policy before release readiness. A post-freeze commit change SHALL create a new RC/bundle ID and repeat every required local gate.

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

### Requirement: True GPUI callback execution boundary
Any feature explicitly declared as a true GPUI callback SHALL run only on the GPUI thread with public immutable snapshots, a theme facade, action sink and scoped invalidation handle. It SHALL NOT perform file/network I/O or retain private host entities. Data-only column and view render-plan callbacks are not GPUI callbacks: they SHALL follow their worker-safe bounded-dispatch contracts, and GPUI SHALL only paint their returned current-revision plans.

#### Scenario: Renderer performs slow work
- **WHEN** a renderer exceeds timing thresholds
- **THEN** diagnostics identify its package/interface and the host may reduce invalidation frequency without unsafe forced interruption
