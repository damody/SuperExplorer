## ADDED Requirements

### Requirement: Versioned crash-safe state store
The application SHALL persist session and view settings in a versioned application-owned state envelope using atomic replacement, validation, and a last-known-good recovery copy.

#### Scenario: Interrupted state write
- **WHEN** the process terminates after writing a temporary snapshot but before atomic replacement completes
- **THEN** the next launch SHALL restore the last complete valid snapshot or defaults and SHALL NOT fail startup

#### Scenario: Unsupported or corrupt schema
- **WHEN** the current state file has an unsupported version, invalid checksum, invalid encoding, or violates model invariants
- **THEN** the application SHALL preserve diagnostic evidence, ignore the invalid state, and start with safe defaults or a valid backup

### Requirement: Explorer session restoration
When session restoration is enabled, the application SHALL restore the ordered tabs, active tab, reconstructible current locations, and bounded back/forward history from the last accepted snapshot.

#### Scenario: Restore mixed location tabs
- **WHEN** the saved session contains filesystem, Known Folder, and serializable Shell namespace tabs
- **THEN** each valid location SHALL reopen in the original order and the saved active tab SHALL receive focus

#### Scenario: Partially stale session
- **WHEN** one saved location no longer resolves but other tabs remain valid
- **THEN** the invalid tab SHALL fall back to the nearest valid parent or configured start location without discarding valid tabs

### Requirement: Per-tab view settings persistence
The application SHALL persist and restore each tab's view mode, sort/group descriptors, Details column order and widths, pane visibility, compact mode, hidden-item and extension visibility, and other durable `ViewSettings` independently.

#### Scenario: Tabs restore independent settings
- **WHEN** two tabs are saved with different view, sort, column, and pane settings
- **THEN** restarting SHALL restore each tab's settings without copying the active tab's values over the other tab

### Requirement: Window placement recovery
The application SHALL restore the main window's normal bounds and maximized state using monitor work areas and current DPI, while ensuring the title bar remains reachable.

#### Scenario: Saved monitor is unavailable
- **WHEN** a session was saved on a monitor that is no longer connected
- **THEN** the window SHALL be clamped and scaled onto an active monitor work area rather than opening off-screen

### Requirement: Bounded and private persisted state
The application SHALL bound tab and history counts and SHALL NOT persist selections, clipboard ownership, inline edits, preview instances/content, credentials, file contents, in-flight operations, or search result snapshots.

#### Scenario: Snapshot contains transient activity
- **WHEN** a save occurs during selection, rename, search, preview, or file operation activity
- **THEN** only durable reconstructible navigation and settings state SHALL be written

### Requirement: Debounced lifecycle integration
The application SHALL schedule persistence after accepted durable model transitions, flush a final snapshot during orderly shutdown, and avoid synchronous disk writes in GPUI input/render callbacks.

#### Scenario: Rapid tab and column changes
- **WHEN** many durable state changes occur within the configured debounce window
- **THEN** they SHALL coalesce into a bounded number of background writes and the final accepted state SHALL be recoverable

### Requirement: User control and reset
The application SHALL provide discoverable controls to enable or disable session restoration and to reset saved session/view state without deleting unrelated user data.

#### Scenario: Session restoration disabled
- **WHEN** restoration is disabled and the application starts
- **THEN** it SHALL open the configured default location while preserving non-session preferences allowed by the selected reset scope

### Requirement: Persistence evidence and migration
Every supported state schema SHALL have deterministic round-trip, prior-version migration, corrupt-input, partial-recovery, and real restart tests.

#### Scenario: Upgrade from prior schema
- **WHEN** a valid snapshot from a supported prior schema is loaded
- **THEN** it SHALL migrate deterministically and the next save SHALL use the current schema without losing recognized settings

#### Scenario: Headful multi-tab restart
- **WHEN** a visible session with multiple ordered filesystem and Shell namespace tabs, independent history and view settings, a non-first active tab, known focus, and non-default window bounds is closed or forcibly terminated
- **THEN** the recovery process SHALL match before/after UIA and durable-state evidence for every field, and a restore-ready diagnostic marker by itself SHALL NOT count as PASS
