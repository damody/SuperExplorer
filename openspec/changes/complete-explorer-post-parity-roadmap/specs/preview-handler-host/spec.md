## ADDED Requirements

### Requirement: Explorer-like preview pane
The application SHALL provide a per-tab Preview Pane that can be toggled through View commands and the standard shortcut, persists as a view setting, and previews the single focused/selected eligible item.

#### Scenario: Toggle preview pane
- **WHEN** the user invokes the Preview Pane command or shortcut
- **THEN** the pane SHALL open or close, command state and accessibility state SHALL update, and the file view SHALL preserve selection and a usable viewport

### Requirement: Broker-only handler activation
Third-party Windows Preview Handlers SHALL be resolved and activated only in disposable broker workers; the GPUI process and primary Shell STA SHALL never instantiate them.

#### Scenario: Eligible file is selected
- **WHEN** Windows registration identifies a Preview Handler for the selected item
- **THEN** the application SHALL request a brokered preview using the item's generation-scoped descriptor and SHALL retain a safe loading/fallback UI

### Requirement: Complete Preview Handler lifecycle
The broker SHALL support public initialization by file, stream, or Shell item as advertised, native host-window assignment, initial/updated rectangles, preview start, focus, accelerator translation, and idempotent unload.

#### Scenario: Resize and focus preview
- **WHEN** the user resizes the pane, changes DPI, tabs into the preview, and invokes supported accelerators
- **THEN** bounds, DPI, focus, and accelerator messages SHALL reach the active handler in order without stealing unrelated application shortcuts

#### Scenario: Selection changes during load
- **WHEN** selection changes before the current handler finishes loading
- **THEN** the old generation SHALL unload or terminate exactly once and only the newest eligible item SHALL become visible

### Requirement: Preview eligibility and fallback
Folders, multiple selections, unsupported items, offline placeholders, unsafe cross-process HWND configurations, and failed handlers SHALL display an Explorer-like icon/properties/unavailable state without blocking navigation.

#### Scenario: Multiple items selected
- **WHEN** more than one file-view item is selected
- **THEN** no Preview Handler SHALL activate and the pane SHALL present a localized multiple-selection summary or neutral state

#### Scenario: Offline cloud file selected
- **WHEN** an offline placeholder has no locally available preview data
- **THEN** automatic preview SHALL not force hydration and the pane SHALL show a truthful fallback with any provider-owned availability action

### Requirement: Preview timeout, crash, and quarantine recovery
Preview load, input, resize, and unload SHALL have bounded deadlines and SHALL integrate with broker crash recovery and handler quarantine.

#### Scenario: Handler hangs during unload
- **WHEN** a Preview Handler does not complete unload within its deadline
- **THEN** its disposable worker SHALL be terminated, the pane SHALL return to a safe state, and a later non-quarantined handler SHALL still load

### Requirement: Preview visual and input integration
The preview boundary SHALL clip correctly, respect pane resizing, minimum widths, light/dark/high-contrast fallback chrome, 100/125/150/175/200% DPI, window activation, and keyboard/mouse focus transitions.

#### Scenario: Move window across DPI monitors
- **WHEN** an active preview moves between monitors with different DPI and then maximizes/restores
- **THEN** the host and handler rectangles SHALL renegotiate without overflow, stale scaling, lost focus, or interaction outside the preview pane

### Requirement: Preview accessibility
The preview pane, loading/error/fallback surfaces, resize splitter, close/toggle command, and handler boundary SHALL expose stable UIA names, roles, state, focus order, and available actions.

#### Scenario: Keyboard and screen-reader traversal
- **WHEN** a user opens the pane and traverses it without a mouse
- **THEN** focus SHALL move predictably among file view, splitter, preview content/fallback, and chrome while accessible status changes are announced

### Requirement: Preview resource and privacy boundaries
The application SHALL not persist preview content, handler COM objects, or rendered surfaces, and SHALL release preview HWNDs, streams, mapped buffers, files, processes, and IPC sessions after unload or failure.

#### Scenario: Repeated preview soak
- **WHEN** supported, unsupported, large, malformed, slow, and crashing fixtures are selected repeatedly across tabs
- **THEN** process/thread/GDI/User handles, working set, workers, and outstanding requests SHALL return within configured steady-state bounds

### Requirement: Real-handler compatibility evidence
The project SHALL maintain controlled fake-handler coverage and real-Windows evidence for representative image, text/code, PDF/document, media/property fallback, and installed third-party Preview Handlers available in the test environment.

#### Scenario: Representative handler matrix
- **WHEN** the compatibility suite runs on a documented Windows build
- **THEN** each available handler class SHALL record initialization mode, focus/resize/unload result, timeout/crash behavior, fallback, and any public-API limitation without claiming unavailable handlers passed
