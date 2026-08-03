# extension-skin-customization Specification

## Purpose
TBD - created by archiving change build-extensible-plugin-platform. Update Purpose after archive.
## Requirements
### Requirement: Data-only skin schema
Skin packages SHALL use a versioned declarative schema for images/backgrounds, file/folder/toolbar icons, fonts/weights, button states, nine-slice/vector assets, colors, spacing, radius, shadows, density, transparency, blur/acrylic and hit-test masks. Skin SHALL NOT execute Rust, Lua, JavaScript, native shader or other code.

#### Scenario: Skin contains executable content
- **WHEN** a skin manifest or payload attempts to declare executable script/native content
- **THEN** package validation rejects the skin feature before assets are applied

### Requirement: Complete button state customization
The schema SHALL allow distinct normal, hover, pressed, focused and disabled button assets/styles while preserving host command, focus and accessibility semantics.

#### Scenario: Custom close button is applied
- **WHEN** a valid skin replaces all visual states of a core window button
- **THEN** the button retains its host-owned command, keyboard focus and UI Automation role

### Requirement: Irregular visual frame on rectangular OS window
Skins MAY create an irregular visual outline using transparent regions and host-validated hit-test masks, but the native window geometry SHALL remain a resizable rectangle so Windows Snap, maximize, resize, DPI and multi-monitor behavior continue to work.

#### Scenario: Transparent corner is clicked
- **WHEN** a point lies in a declared pass-through region that does not overlap a required resize/command area
- **THEN** host hit testing applies pass-through while preserving required resize and window controls

### Requirement: Host-owned safety and accessibility
The host SHALL retain title-bar drag/window commands, resize handles, keyboard focus/shortcuts, UI Automation semantics, high-contrast overrides and safe core control fallbacks regardless of skin data.

#### Scenario: High contrast is enabled
- **WHEN** an active skin would make a required control indistinguishable under high contrast
- **THEN** the host applies accessible fallback semantics/styles without disabling the whole skin

### Requirement: Per-asset validation and fallback
The loader SHALL validate asset type, dimensions, size, path containment and references. Missing, malformed or over-budget assets SHALL individually fall back to default Skin rather than making the application unusable.

#### Scenario: One button image is corrupt
- **WHEN** all other skin assets are valid but a pressed-state image cannot decode
- **THEN** only that state uses the default asset and navigation/settings remain operable

### Requirement: Runtime skin switching
Enabling a skin SHALL make it selectable without automatically replacing the active skin. Disabling the active skin SHALL immediately restore the default skin and SHALL NOT require native DLL unload/restart.

#### Scenario: Active skin is disabled
- **WHEN** the user applies off for the current skin feature
- **THEN** default visuals are restored immediately while package settings remain persisted

### Requirement: Skin quality gate
Skin implementation SHALL be tested at supported DPI scales, high contrast, transparency/hit-test, Snap, maximize, resize, multi-monitor, keyboard and UIA, and malformed/oversized asset fallback.

#### Scenario: Skin passes release validation
- **WHEN** the P1 skin suite runs across declared DPI and accessibility fixtures
- **THEN** all host window behavior and safe fallbacks remain functional before first-stage completion
