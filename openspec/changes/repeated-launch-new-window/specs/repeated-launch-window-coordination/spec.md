## ADDED Requirements

### Requirement: Ordinary repeated launches create independent windows
SuperExplorer SHALL detect existing ordinary windows in the current interactive
Windows login session, and each later ordinary launch SHALL create exactly one
new independent top-level explorer window.

#### Scenario: Second ordinary launch
- **WHEN** an ordinary SuperExplorer window exists and the same login session executes SuperExplorer again
- **THEN** the later process creates exactly one additional top-level explorer window

#### Scenario: Simultaneous initial launches
- **WHEN** two ordinary launches race before a resident endpoint is ready
- **THEN** one launch is classified as initial and the other as repeated, and both create exactly one window

### Requirement: Relaunch windows start at the system drive root
Every explorer window created from a repeated launch SHALL contain a fresh
single-tab state whose active location is the filesystem path `C:\`.

#### Scenario: Relaunch location
- **WHEN** the resident process accepts a repeated-launch request
- **THEN** the newly created window displays `C:\` regardless of the resident window's active location or restored session

#### Scenario: First launch restoration remains intact
- **WHEN** no resident process exists and a valid session is available
- **THEN** the initial window uses the existing session-restoration behavior rather than being forced to `C:\`

### Requirement: Launch detection is atomic and session scoped
The launch marker SHALL use an atomic Windows named object scoped to the current
interactive login session and SHALL remain held for each ordinary process's
lifetime.

#### Scenario: Oldest window closes first
- **WHEN** multiple ordinary windows exist and the oldest process exits
- **THEN** a subsequent invocation still detects an existing ordinary window and opens at `C:\`

#### Scenario: Different Windows user
- **WHEN** another Windows login session launches SuperExplorer
- **THEN** its launch marker is independent and its first window follows normal first-launch behavior

### Requirement: Detection failure is controlled
An ordinary invocation SHALL report launch-marker creation failure through the
existing controlled startup error path without corrupting session state.

#### Scenario: Marker creation fails
- **WHEN** Windows cannot create or open the launch marker
- **THEN** the invocation exits through controlled error reporting and leaves persisted session data unchanged

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
Closing one explorer window SHALL leave explorer windows owned by other
SuperExplorer processes usable.

#### Scenario: Close one of two explorer windows
- **WHEN** two explorer windows are open and the user closes one
- **THEN** the remaining explorer window stays open and responsive

#### Scenario: Close final window
- **WHEN** the user closes a process's only window
- **THEN** that process completes normal shutdown without closing windows owned by other processes

### Requirement: Concurrent windows share extension scratch safely
Every process-owned window SHALL be able to initialize the extension host while
another SuperExplorer process holds the verified private `.sepack-staging` root.

#### Scenario: Two extension hosts open the staging root
- **WHEN** two SuperExplorer processes initialize against the same user profile
- **THEN** both open the staging root without a Windows sharing violation and retain unique import-candidate ownership

### Requirement: Installed shortcuts are ordinary launches
Start Menu and desktop shortcuts SHALL invoke `SuperExplorer.exe` without
diagnostic, fixture, auto-close, or plugin-development arguments.

#### Scenario: Test installer creates shortcuts
- **WHEN** a test installer is built with finish-page diagnostics enabled
- **THEN** its installed shortcuts still contain an empty argument string and participate in repeated-launch detection

### Requirement: In-place upgrade replaces the running installed application
The installer SHALL close only SuperExplorer processes executing from its
selected installation directory before replacing application files, SHALL use
a bounded graceful-then-force sequence, and SHALL abort when final absence
cannot be proven.

The uninstaller SHALL apply the same rule before deleting application files,
and silent install or uninstall failures SHALL not wait on an interactive dialog.

#### Scenario: Upgrade while two installed windows are open
- **WHEN** two SuperExplorer processes execute from the selected install directory and an upgrade begins
- **THEN** both receive graceful close, any bounded-time survivors are terminated, and file replacement begins only after neither process remains

#### Scenario: Development copy is also running
- **WHEN** another `SuperExplorer.exe` executes outside the selected install directory
- **THEN** installer quiescence leaves that process untouched

#### Scenario: Quiescence cannot be verified
- **WHEN** process query or final absence verification fails
- **THEN** the installer returns a controlled failure and does not report a successful upgrade

#### Scenario: Uninstall while an installed window is open
- **WHEN** SuperExplorer executes from the selected install directory and uninstall begins
- **THEN** the exact process is quiesced before file deletion, while silent failure remains non-interactive
