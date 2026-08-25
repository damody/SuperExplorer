## ADDED Requirements

### Requirement: SuperExplorer parent diagnostics console
On Windows, the SuperExplorer application executable SHALL use the console subsystem in debug and release builds while the development diagnostics policy is active.

#### Scenario: Debug application starts
- **WHEN** a developer starts a debug SuperExplorer executable
- **THEN** the SuperExplorer parent process has a visible diagnostics console

#### Scenario: Release application starts
- **WHEN** a developer starts a release SuperExplorer executable
- **THEN** the SuperExplorer parent process has a visible diagnostics console

### Requirement: Hidden production background processes
Every console-subsystem process launched internally by SuperExplorer or a shipped helper on Windows SHALL be created with `CREATE_NO_WINDOW`, including startup discovery, remote operations, automation tools, diagnostic probes, extension brokers, workers, and worker-owned helper processes.

#### Scenario: Startup discovers ADB devices
- **WHEN** SuperExplorer invokes `adb.exe devices -l` during startup
- **THEN** ADB produces no visible console window and its exit status, stdout, and stderr remain available to the existing runner

#### Scenario: Automation starts a console tool
- **WHEN** an authorized automation action launches a direct console-subsystem executable
- **THEN** no child console becomes visible and the existing arguments, working directory, environment, timeout, cancellation, stdout, and stderr contracts remain in force

#### Scenario: Extension helper starts a child
- **WHEN** the app, broker, or worker starts an internal extension helper or diagnostic probe
- **THEN** neither the helper nor its child process creates a visible console and existing typed terminal behavior is preserved

#### Scenario: Background spawn fails
- **WHEN** a hidden background executable is missing or cannot start
- **THEN** the caller receives its existing spawn failure and SuperExplorer does not open a fallback console or shell

### Requirement: Explicit visible Command Prompt exception
The user-facing Open Command Prompt action SHALL continue to launch `cmd.exe` with `CREATE_NEW_CONSOLE`; no other production launcher SHALL use the visible-console classification unless a later specification explicitly authorizes it.

#### Scenario: User opens Command Prompt
- **WHEN** the user invokes Open Command Prompt for a filesystem folder
- **THEN** one visible Command Prompt opens with that folder as its working directory

#### Scenario: Internal operation runs cmd
- **WHEN** an internal helper needs to invoke `cmd.exe` for a reviewed background operation
- **THEN** it uses the hidden-background classification and does not reuse the visible Open Command Prompt path

### Requirement: Process launch inventory
The repository SHALL maintain a blocking classification gate for production process launch sites. New or changed production launch sites MUST be classified as hidden-background or explicitly visible, and test-only or build-time exclusions MUST be distinguishable from production coverage.

#### Scenario: Unclassified production command is added
- **WHEN** a production source adds a process launch without a recognized hidden-background configuration or approved visible-console exception
- **THEN** the inventory gate fails and identifies the source location

#### Scenario: Test fixture launches a command
- **WHEN** a command exists only under a test or build-time configuration
- **THEN** the inventory records or excludes it without treating it as proof that the corresponding production launcher is compliant

### Requirement: Runtime window verification
Blocking Windows verification SHALL exercise debug and release application profiles and representative background launchers, inspect actual process-owned top-level windows, and preserve hashed evidence for parent and child visibility plus command outcomes.

#### Scenario: Profile runtime verification passes
- **WHEN** the debug or release verification launches SuperExplorer and representative background commands
- **THEN** it observes the visible parent diagnostics console, no visible background child console, successful captured output, and an evidence record for each independently failing assertion

#### Scenario: Optional ADB prerequisite is absent
- **WHEN** the verification host has no approved ADB executable
- **THEN** a controlled console-subsystem fixture exercises the production runner and the ADB-specific branch receives an evidence-backed `not-applicable` disposition rather than being silently skipped

#### Scenario: A background child hangs
- **WHEN** a representative hidden child exceeds its timeout or is cancelled
- **THEN** existing termination and reap behavior completes without showing a console, and the timeout or cancelled outcome is recorded
