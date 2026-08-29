## ADDED Requirements

### Requirement: Runtime-gated popup capability
The system SHALL use application-owned presentation only after documented Win32/GDI services and per-menu materialization establish that the HMENU can be represented without mutation.

#### Scenario: Complete capability is available
- **WHEN** the owner, HMENU, row forms, DPI, and accessibility policy are supported
- **THEN** the system presents a custom popup without modifying the HMENU

#### Scenario: Capability is incomplete or unsupported
- **WHEN** the owner, HMENU, row form, DPI, or accessibility policy cannot be supported
- **THEN** the system returns a structured unsupported reason and uses `TrackPopupMenuEx`

#### Scenario: ExplorerPatcher is absent
- **WHEN** ExplorerPatcher is not installed or injected
- **THEN** capability discovery and fallback remain functional without loading ExplorerPatcher or private Windows menu helpers

### Requirement: Scoped presentation and cleanup lifecycle
The system SHALL scope popup HWNDs, capture, fonts, row metadata, and shadows to one presentation call and SHALL release them exactly once after selection, cancellation, error, replay, or unwind.

#### Scenario: Command selection completes
- **WHEN** the application-owned popup returns a native command ID
- **THEN** all presentation resources are released before command invocation continues

#### Scenario: Menu is cancelled or replaced
- **WHEN** the popup returns no command or schedules a right-click replay
- **THEN** capture, windows, fonts, and row metadata are released before the replacement gesture opens another menu

#### Scenario: Presentation cannot start
- **WHEN** materialization or custom window creation fails
- **THEN** the system displays the same unchanged HMENU through `TrackPopupMenuEx`

#### Scenario: A presentation fails
- **WHEN** the custom message loop or window lifecycle fails
- **THEN** that session terminates safely and a later context menu remains available

### Requirement: Native menu identity preservation
The system MUST treat command IDs, submenu handles, type/state flags, bitmap handles, canonical verbs, and extension-owned `dwItemData` as non-owning input and MUST NOT rewrite them for presentation.

#### Scenario: Standard and nested commands are styled
- **WHEN** a menu contains normal commands, separators, bitmaps, and nested submenus
- **THEN** materialization preserves their identity and selecting a row returns its original command ID

#### Scenario: Existing owner-draw styling is detected
- **WHEN** the target contains extension-owned owner-draw state that cannot be represented safely
- **THEN** the system tracks the original HMENU unchanged

#### Scenario: Incompatible owner-draw entry is detected
- **WHEN** an extension-owned owner-draw item cannot be isolated with deterministic single-owner routing
- **THEN** the complete session falls back before mutation and the original handler remains authoritative

### Requirement: Application-owned popup presentation
The system SHALL use a SuperExplorer-owned presentation host, rather than a private
Windows or ExplorerPatcher helper, when exact Local visual control is required.

#### Scenario: Compatible native menu is presented
- **WHEN** HMENU materialization yields supported rows after `QueryContextMenu`
- **THEN** the host renders and operates those rows while returning the original native command ID

#### Scenario: Private immersive helper is unavailable
- **WHEN** ExplorerPatcher is absent or private `twinui.pcshell.dll` symbols are unavailable
- **THEN** the application-owned host remains functional without loading or resolving them

### Requirement: Owner-window message routing
The system SHALL keep Shell extension messages on the existing STA owner while the popup host owns only its own paint and input messages.

#### Scenario: Shell extension owns a message
- **WHEN** the fallback native menu requires an `IContextMenu3` message
- **THEN** the message reaches the existing `IContextMenu3` forwarding path exactly once

#### Scenario: Dynamic submenu initializes
- **WHEN** a nested Shell submenu is opened by the custom host
- **THEN** the owner receives `WM_INITMENUPOPUP`, the current extension handler can populate it, and the child is rematerialized before display

### Requirement: Theme, DPI, and accessibility safety
The system SHALL preserve usable native behavior across supported per-monitor DPI, light/dark themes, keyboard invocation, and high contrast.

#### Scenario: Supported theme and DPI
- **WHEN** a capable session opens at 100%, 125%, 150%, or 200% scaling in light or dark mode
- **THEN** the popup uses the active monitor/theme metrics and remains within the monitor work area

#### Scenario: High contrast is active
- **WHEN** high contrast is enabled and no verified system-theme-safe immersive path exists
- **THEN** the system uses the existing native rendering without forced custom colors

#### Scenario: Keyboard invocation
- **WHEN** the menu is opened from the keyboard and navigated with accelerators, arrows, Enter, or Escape
- **THEN** focus, selection, invocation, and cancellation match the existing native behavior

### Requirement: Privacy-bounded diagnostics and rollout
The system SHALL expose a typed rollout setting and bounded diagnostics without recording target paths, menu labels, user names, or raw extension data.

#### Scenario: Capability or session fallback is recorded
- **WHEN** materialization, presentation, routing, or cleanup falls back
- **THEN** diagnostics record strategy, phase, result category, theme, and DPI only

#### Scenario: Feature is disabled
- **WHEN** the rollout setting is off
- **THEN** no custom popup is created and the existing native path remains unchanged

#### Scenario: Default enablement gate fails
- **WHEN** any blocking compatibility or headful gate remains failed, blocked, stale, or unexecuted
- **THEN** the rollout setting does not become enabled by default

### Requirement: Independent implementation provenance
The system MUST implement the adapter independently with documented Windows APIs and MUST NOT copy or ship ExplorerPatcher GPLv2 source, binary code, signature tables, private ABI declarations, or assets.

#### Scenario: Provenance review
- **WHEN** the adapter is ready for integration
- **THEN** a reviewer can trace each implementation source and confirms ExplorerPatcher was used only as behavioral reference
