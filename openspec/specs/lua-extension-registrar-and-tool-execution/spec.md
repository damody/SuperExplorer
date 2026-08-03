# lua-extension-registrar-and-tool-execution Specification

## Purpose
TBD - created by archiving change build-extensible-plugin-platform. Update Purpose after archive.
## Requirements
### Requirement: Restricted Lua registrar
The Lua registration phase SHALL support single/batch data columns, commands, extension buttons, host forms and operation-plan providers. Every registration SHALL bind to a manifest feature/capability and produce an immutable descriptor and registry callback.

#### Scenario: Lua registers a batch column
- **WHEN** an authorized script registers a batch column returning typed byte/integer/text values
- **THEN** the host exposes the column using built-in renderers without allowing Lua to create arbitrary GPUI elements

### Requirement: Capability-only Lua environment
Distributed Lua packages SHALL have no arbitrary filesystem, network, process or private-model APIs. All reads, mutations, tools and network requests SHALL require declared host capabilities and authorized handles.

#### Scenario: Script calls undeclared filesystem delete
- **WHEN** a Lua callback requests a capability absent from its feature manifest
- **THEN** the host denies the call, records a scoped diagnostic and performs no mutation

### Requirement: Bundled tool manifest and package validation
Any executable required by a Lua plugin SHALL be packaged at `.sepack/tools/<target>/<tool-id>/` and described with target, relative path, exact version, size, SHA-256, output protocol, source and license/NOTICE. Validation SHALL reject missing, altered, wrong-target or path-escaping tools before dependent callbacks run.

#### Scenario: Tokei is missing from the package
- **WHEN** the Lua tokei package lacks its declared `windows-x64` executable
- **THEN** its dependent feature is blocked before Lua registration/callback and the host does not search for another tokei

### Requirement: Opaque Tool Resolver
Lua SHALL submit only a manifest tool ID, argument array and bounded options. Tool Resolver SHALL return/use an opaque package-generation-scoped handle and SHALL NOT expose the executable path or search PATH, Registry, common install locations, network or user-selected substitutes.

#### Scenario: System PATH contains tokei
- **WHEN** the package tool is invalid but another tokei exists on PATH
- **THEN** execution remains blocked and the system executable is not used

#### Scenario: Package is updated
- **WHEN** package generation changes after a tool handle was issued
- **THEN** the old handle becomes invalid and cannot execute the previous payload

### Requirement: Shell-free bounded process request
Tool execution SHALL use an executable handle plus argument array, authorized working directory, environment allowlist, stdin policy, timeout, stdout/stderr limits and cancellation token. It SHALL NOT construct a cmd.exe or PowerShell command string.

#### Scenario: Filename contains shell metacharacters
- **WHEN** a selected path contains quotes, ampersands or command-substitution characters
- **THEN** it is passed as one literal argument and cannot inject a shell command

### Requirement: Child process lifecycle
The host SHALL own a ProcessLease and, on cancellation, timeout, feature disable or folder change, terminate and reap the full child process tree using a Windows Job Object. Terminal results SHALL distinguish exit, timeout, cancelled, spawn failed and output truncated.

#### Scenario: Lua future is dropped
- **WHEN** the extension callback is cancelled while its tool and child process are running
- **THEN** the Job Object terminates/reaps the process tree and returns a cancelled terminal state

### Requirement: Batched Lua tokei mapping
The Lua tokei example SHALL batch authorized item handles according to count and Windows command-line limits, request JSON output and map results back to stable handles. It SHALL avoid one process per item and treat unknown/binary files as unsupported rather than zero.

#### Scenario: One thousand files are analyzed
- **WHEN** the example receives 1,000 supported paths
- **THEN** it executes bounded groups (default maximum 128 subject to command length), maps typed numeric results and does not spawn 1,000 processes

### Requirement: Shared typed semantics
Lua serializers SHALL mirror public `PluginValueV1`, terminal outcomes and `OperationPlanV1` rather than defining incompatible value, error or mutation semantics.

#### Scenario: Lua plan reaches the executor
- **WHEN** a Lua callback returns a valid create-directory plan
- **THEN** the same host validator, preview, executor and undo rules used by Rust plans apply
