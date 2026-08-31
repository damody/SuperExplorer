## ADDED Requirements

### Requirement: Ordinary repeated launches create resident windows
SuperExplorer SHALL maintain one resident ordinary application process per
interactive Windows user, and each accepted later ordinary launch SHALL create
exactly one new independent top-level explorer window in that process.

#### Scenario: Second ordinary launch
- **WHEN** an ordinary SuperExplorer process is ready and the same user executes SuperExplorer again
- **THEN** the resident process creates exactly one additional top-level explorer window and the later process exits after acknowledgment

#### Scenario: Simultaneous initial launches
- **WHEN** two ordinary launches race before a resident endpoint is ready
- **THEN** one launch becomes resident and the other delivers exactly one request without creating duplicate resident owners

### Requirement: Relaunch windows start at the system drive root
Every explorer window created from a repeated launch SHALL contain a fresh
single-tab state whose active location is the filesystem path `C:\`.

#### Scenario: Relaunch location
- **WHEN** the resident process accepts a repeated-launch request
- **THEN** the newly created window displays `C:\` regardless of the resident window's active location or restored session

#### Scenario: First launch restoration remains intact
- **WHEN** no resident process exists and a valid session is available
- **THEN** the initial window uses the existing session-restoration behavior rather than being forced to `C:\`

### Requirement: Launch coordination is bounded and user scoped
The launch endpoint SHALL be scoped to the current Windows user, SHALL accept
only a versioned bounded command contract, and SHALL perform all GPUI window
creation on the GPUI foreground thread.

#### Scenario: Invalid request
- **WHEN** the resident endpoint receives an oversized, malformed, unknown-version, or unknown-command request
- **THEN** it rejects the request without opening a window and remains available for later valid requests

#### Scenario: Different Windows user
- **WHEN** a process running as another interactive user attempts to use the endpoint
- **THEN** Windows access control denies the request and no window is created

### Requirement: Coordination failure does not prevent startup
An ordinary invocation SHALL use bounded connection and acknowledgment waits and
SHALL continue as an independent normal startup when no healthy resident accepts
its request.

#### Scenario: Stale or unavailable endpoint
- **WHEN** resident coordination cannot connect or acknowledge within the configured bound
- **THEN** the invocation records a fallback diagnostic and continues through normal application startup

### Requirement: Special launches remain isolated
SuperExplorer SHALL bypass resident launch coordination for launches with
explicit diagnostic, visual-fixture, auto-close, or plugin DLL configuration.

#### Scenario: Automated fixture launch
- **WHEN** SuperExplorer starts with visual-fixture or auto-close configuration
- **THEN** it creates its own process-local window lifecycle and sends no resident open-window request

#### Scenario: Explicit plugin development launch
- **WHEN** SuperExplorer starts with one or more `--plugin-dll` arguments
- **THEN** it creates its own process-local composition and sends no resident open-window request

### Requirement: Multiple explorer windows have independent lifetime
Closing one explorer window SHALL leave other explorer windows usable, and the
resident application SHALL terminate only after its final window closes.

#### Scenario: Close one of two explorer windows
- **WHEN** two explorer windows are open and the user closes one
- **THEN** the remaining explorer window stays open and responsive

#### Scenario: Close final window
- **WHEN** the user closes the final application window
- **THEN** SuperExplorer stops the launch listener and completes normal shutdown
