## MODIFIED Requirements

### Requirement: Package and feature controls
Each package SHALL have a package switch and expanded stable feature rows showing localized description, category, dependencies, capabilities and immediate/restart behavior. Global disable SHALL stop all non-core extension contributions without deleting packages, settings or cache. Apply/OK SHALL atomically persist global, package, and feature desired states; entries absent from the store SHALL default to enabled.

#### Scenario: Global extensions are disabled
- **WHEN** the user applies the global off state
- **THEN** non-core Rust/Lua/Skin contributions stop according to lifecycle rules while core navigation, file operations and Safe Mode remain available

#### Scenario: Package switch is applied
- **WHEN** the user changes a package switch and selects Apply or OK
- **THEN** the desired state is atomically persisted and governs the next startup

#### Scenario: Draft is cancelled
- **WHEN** the user changes switches after the last Apply and selects Cancel
- **THEN** those draft-only changes do not alter the settings file

### Requirement: Effective-state presentation
The UI SHALL display enabled, disabled, disabling, pending-restart, blocked and faulted with dependency, capability, compatibility and diagnostic reasons. Child desired states SHALL remain visible/preserved when a parent is off. When global Safe Mode is latched, the Extensions tab SHALL display its non-path incident reason and an accessible explicit **Re-enable all plugins** action whose successful result requires restart and preserves individual desired states.

#### Scenario: Required tool is quarantined
- **WHEN** a bundled tool disappears after package validation
- **THEN** dependent feature shows blocked with repair information and does not silently use a system tool

#### Scenario: Global Safe Mode is latched
- **WHEN** the user opens Extensions options during global Safe Mode
- **THEN** every plugin is shown blocked by Safe Mode and recovery is available without hiding package/feature switches
