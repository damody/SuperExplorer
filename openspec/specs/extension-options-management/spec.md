# extension-options-management Specification

## Purpose
TBD - created by archiving change build-extensible-plugin-platform. Update Purpose after archive.
## Requirements
### Requirement: Extensions tab and global scope
Folder Options SHALL add an `Extensions` tab beside General and View. Its settings SHALL apply to the current Windows user's SuperExplorer windows/folders, not only the folder where the dialog opened.

#### Scenario: User opens Folder Options
- **WHEN** the dialog is shown
- **THEN** the third tab is keyboard/UIA accessible and presents the current immutable extension catalog

### Requirement: Searchable package catalog
The tab SHALL provide global enable, search, type/status filters and virtualized expandable package rows showing identity, version, source/signature status, content types, author contacts, capabilities, bundled tools/licenses, compatibility, diagnostics and restart impact.

#### Scenario: User filters incompatible Rust packages
- **WHEN** type `Rust` and status `Incompatible` are selected
- **THEN** only matching rows remain without blocking the GPUI thread

### Requirement: Package and feature controls
Each package SHALL have a package switch and expanded stable feature rows showing localized description, category, dependencies, capabilities and immediate/restart behavior. Global disable SHALL stop all non-core extension contributions without deleting packages, settings or cache.

#### Scenario: Global extensions are disabled
- **WHEN** the user applies the global off state
- **THEN** non-core Rust/Lua/Skin contributions stop according to lifecycle rules while core navigation, file operations and Safe Mode remain available

### Requirement: Separate options snapshot and draft
Dynamic catalog/state SHALL use `ExtensionOptionsSnapshot` and `ExtensionOptionsDraft`, not the existing fixed Copy folder-options draft. Draft SHALL track global/package/feature desired states, filters and unsaved changes.

#### Scenario: Catalog changes while dialog is open
- **WHEN** host catalog generation changes
- **THEN** the dialog reconciles or asks to refresh without corrupting the user's draft or copying dynamic data into the fixed draft

### Requirement: Effective-state presentation
The UI SHALL display enabled, disabled, disabling, pending-restart, blocked and faulted with dependency, capability, compatibility and diagnostic reasons. Child desired states SHALL remain visible/preserved when a parent is off.

#### Scenario: Required tool is quarantined
- **WHEN** a bundled tool disappears after package validation
- **THEN** dependent feature shows blocked with repair information and does not silently use a system tool

### Requirement: Transactional Apply, OK, Cancel and close
Apply SHALL validate/persist and activate possible changes while keeping the dialog open; OK SHALL do the same then close; Cancel SHALL discard only changes since the last Apply; close with unsaved changes SHALL offer Apply/Discard/Return.

#### Scenario: User applies then changes another switch and cancels
- **WHEN** the first changes were successfully applied before the second draft change
- **THEN** Cancel discards only the second change and does not roll back the applied state

### Requirement: Impact preview and feature drain
Before applying changes that disable dependents, close panels, remove columns/views or leave virtual locations, the transaction SHALL show concrete impact. `FeatureDrainCoordinator` SHALL stop dispatch, cancel jobs, close contributions, handle virtual tabs and determine restart state.

#### Scenario: 7z provider is disabled while tab is inside archive
- **WHEN** the user applies disable
- **THEN** the UI offers navigation back to the container's normal folder; cancelling that navigation cancels the disable transaction

### Requirement: Type-specific switching semantics
Lua SHALL stop/re-register immediately after cancellation/drain; Skin SHALL fall back to default when active; loaded Rust SHALL gate/drain without unload; unloaded/new/updated Rust SHALL require restart. Faulted drains SHALL not force unload.

#### Scenario: Unloaded Rust feature is enabled
- **WHEN** the DLL was not loaded at startup
- **THEN** desired state is saved as pending-restart and no runtime hot-load occurs

### Requirement: Accessible and localized management UI
The Extensions tab SHALL support keyboard navigation, UI Automation, high DPI, high contrast, localization and long-list virtualization. Contact links SHALL be displayed but SHALL NOT be automatically joined, opened or messaged by validation.

#### Scenario: High contrast is active
- **WHEN** the package catalog is rendered under high contrast
- **THEN** state and controls remain distinguishable without relying solely on custom colors
