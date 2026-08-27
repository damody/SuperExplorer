## ADDED Requirements

### Requirement: Plugin faults latch global Safe Mode
A caught plugin panic, abnormal plugin callback termination, or stale durable callback marker SHALL atomically latch global Safe Mode for the next startup before the fault can be treated as recovered.

#### Scenario: Host catches a plugin panic
- **WHEN** a plugin callback panics and the host catches the unwind
- **THEN** the callback fails without unwinding into core code and global Safe Mode is durably latched

#### Scenario: Process exits during a callback
- **WHEN** SuperExplorer terminates with a durable plugin callback marker uncleared
- **THEN** the following startup converts the stale marker into the global latch

### Requirement: Latched startup executes no plugin code
While global Safe Mode is latched or its durable state cannot be validated, startup SHALL execute no plugin DLL entrypoint, Lua registrar, skin callback, or bundled plugin tool regardless of desired-state settings.

#### Scenario: Enabled packages exist while latched
- **WHEN** startup finds enabled packages and a valid latched Safe Mode record
- **THEN** it keeps core file management and non-executing extension diagnostics available without loading plugin code

#### Scenario: Safe Mode state is corrupt
- **WHEN** startup cannot validate the Safe Mode state
- **THEN** it fails closed and exposes repair diagnostics instead of enabling plugins

### Requirement: Explicit recovery preserves individual choices
Global Safe Mode SHALL remain latched across restarts until the user explicitly confirms **Re-enable all plugins** in Extensions options. Successful recovery SHALL preserve every global/package/feature desired state and SHALL require restart before native plugins can execute.

#### Scenario: User merely restarts again
- **WHEN** Safe Mode is latched and the user closes and reopens SuperExplorer without confirming recovery
- **THEN** all plugins remain blocked

#### Scenario: User confirms recovery
- **WHEN** the user confirms recovery and the latch/incident cleanup commits successfully
- **THEN** the UI reports restart required and the next clean startup loads only individually enabled admissible plugins

#### Scenario: Recovery persistence fails
- **WHEN** the latch clear or incident cleanup cannot commit
- **THEN** Safe Mode remains effective and the UI reports the failure
