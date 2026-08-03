## ADDED Requirements

### Requirement: Unified package manifest
The system SHALL load Rust, Lua, Skin, locales, tools, licenses, content hashes, entry points, dependencies and feature declarations from a versioned `.sepack` manifest. The manifest SHALL use normalized stable package, publisher, feature and interface IDs and SHALL reject duplicate, unknown-required or over-length identifiers.

#### Scenario: Valid multi-content package
- **WHEN** a `.sepack` contains valid Rust, Lua and Skin entries whose files and hashes match the manifest
- **THEN** the package is represented as one atomic package version with independently declared features

#### Scenario: Manifest and payload disagree
- **WHEN** an entry point, content hash, required capability or feature ID differs from the manifest
- **THEN** the host rejects the complete package before executing any callback

### Requirement: Structured publisher contacts
Every package SHALL provide a stable publisher ID, display name and at least one public contact; at least one contact SHALL declare a `support` or `security` purpose. Supported structured kinds SHALL include email, website, support forum, GitHub Issues, Discord server/user, QQ group and other.

#### Scenario: Community-only contact is insufficient
- **WHEN** a manifest provides only contacts whose purposes are `community`
- **THEN** package validation fails with a contact-purpose diagnostic

#### Scenario: Signed publisher mismatch
- **WHEN** a signed package's manifest publisher ID differs from the signing identity
- **THEN** the package is rejected as an identity mismatch

### Requirement: Atomic package resolution
The Package Manager SHALL select at most one version for each package ID and SHALL resolve dependencies, cycles, hashes, signatures, target and compatibility before registration. It SHALL NOT partially load a package whose validation fails.

#### Scenario: Unsatisfied dependency
- **WHEN** a package's required dependency range has no compatible installed version
- **THEN** the complete package is marked blocked and none of its contributions are registered

### Requirement: Feature and capability binding
Every registrar contribution SHALL reference one manifest feature ID, and each feature SHALL declare all capabilities used by its callbacks. Undeclared, duplicate or capability-exceeding contributions SHALL reject the package. Runtime authority handles SHALL additionally bind package, feature, interface, package incarnation, capability, authorized resource root and relevant generations; the host SHALL revalidate that envelope at dispatch/use and SHALL reject it after disable, update or generation change.

#### Scenario: Renderer uses undeclared capability
- **WHEN** a feature registers a GPUI renderer but does not declare the corresponding GPUI render capability
- **THEN** validation rejects the package before the renderer factory runs

#### Scenario: Previously authorized handle is used after update
- **WHEN** a callback submits a handle issued for an older package incarnation or resource generation
- **THEN** runtime authorization rejects the call without opening or mutating the referenced resource

### Requirement: Desired and effective states
The host SHALL persist global, package and feature desired states separately from effective states. Effective state SHALL include enabled, disabled, pending-restart, disabling, blocked and faulted, and disabling a parent SHALL preserve child desired states.

#### Scenario: Parent is re-enabled
- **WHEN** a user disables a package whose children have mixed desired states and later re-enables the package
- **THEN** each child returns to its previously persisted desired state subject to current dependencies and compatibility

### Requirement: Native plugin load lifecycle
Rust DLLs SHALL load only during application startup and remain resident until process exit. The host SHALL complete package/hash/signature/target/PE-policy validation and write a durable load-attempt marker before `LoadLibrary`; after loading it SHALL validate root layout, compatibility and fingerprint before any SDK accessor, factory, registrar or callback. Successful validation/registration SHALL atomically clear or mark the matching attempt registered; typed post-load rejection SHALL leave the DLL resident but non-dispatchable and mark the attempt `rejected-resident`; abnormal load termination SHALL leave the attempt incomplete for next-start Safe Mode. Runtime disabling SHALL atomically gate new dispatch, cancel jobs/streams/processes, detach UI contributions on the GPUI thread, resolve impacted virtual tabs and bounded-drain correlated active calls without unloading the DLL. Installing, replacing, removing or enabling an unloaded DLL SHALL require restart.

#### Scenario: Loaded feature drains successfully
- **WHEN** a user disables a loaded Rust feature and its jobs and callbacks drain within the limit
- **THEN** its contributions stop immediately while its DLL remains resident

#### Scenario: Callback does not drain
- **WHEN** a Rust callback remains active beyond the bounded drain period
- **THEN** the feature becomes pending-restart and the host does not force-unload the DLL

#### Scenario: DLL aborts during operating-system load
- **WHEN** a DLL terminates the process from `DllMain` or a TLS initializer before root validation can run
- **THEN** the durable load-attempt marker causes the next startup to offer Safe Mode with that package suppressed, without claiming that root validation sandboxed load-time code

#### Scenario: Loaded DLL fails post-load validation
- **WHEN** the operating system loads a DLL but root, compatibility, fingerprint or registration validation returns a typed rejection
- **THEN** the DLL remains resident and non-dispatchable for the process lifetime, and the matching attempt records a `rejected-resident` terminal rather than an uncleared crash marker

#### Scenario: Callback returns after disable starts
- **WHEN** an in-flight callback or incremental sink publishes after the dispatch gate closes
- **THEN** its correlation/generation is rejected and no removed contribution or stale result becomes current

### Requirement: Contribution call guard and Safe Mode
The host SHALL write a package/interface/operation call marker before native callbacks and clear it after normal return. Recoverable panics SHALL become typed plugin errors; an uncleared marker after abnormal termination SHALL cause the next startup to offer Safe Mode with the suspected contribution disabled.

#### Scenario: Previous process died inside a plugin
- **WHEN** startup finds an uncleared native call marker
- **THEN** Safe Mode identifies the suspected package/interface and prevents its callback until the user confirms re-enable

### Requirement: Package source abstraction
The first implementation SHALL provide built-in and local-developer package sources and SHALL expose replaceable Package Source and Entitlement Provider boundaries without linking Steamworks.

#### Scenario: First-stage build runs without Steam
- **WHEN** SuperExplorer is built and tests this change
- **THEN** package discovery works for built-in and local packages without a Steamworks dependency
