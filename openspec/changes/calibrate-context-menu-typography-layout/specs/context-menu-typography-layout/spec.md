## ADDED Requirements

### Requirement: Remote context menus use the Windows menu typography profile
The system SHALL render every ADB and SFTP context-menu command with Microsoft JhengHei UI as the primary family, 12 logical-pixel text, 16 logical-pixel line height, and weight 400 through the active menu typography tokens.

#### Scenario: Remote menu opens at any monitor DPI
- **WHEN** an ADB or SFTP context menu is rendered on a monitor at any supported DPI scale
- **THEN** the family, size, line height, and weight are applied as logical values and scale together without physical-pixel overrides

#### Scenario: Primary font is unavailable
- **WHEN** Microsoft JhengHei UI cannot be resolved
- **THEN** the existing Segoe UI Variable Text, Segoe UI, and sans-serif fallback order remains available

### Requirement: Menu typography fits the established row geometry
The system MUST vertically center the 16px menu line box within the existing 23px command row and MUST preserve the 42px icon gutter, 16px icon, and 13px icon-left offset.

#### Scenario: Text and icon command share a row
- **WHEN** a remote command includes both an icon and a label
- **THEN** the icon remains in the established icon column and the label uses the calibrated line box without clipping or changing the text origin

#### Scenario: Theme changes
- **WHEN** the menu renders in light, dark, or high-contrast mode
- **THEN** typography and row geometry remain identical while only semantic colors change

### Requirement: Windows owner-draw fallback uses the calibrated menu size
The system SHALL use a 12 logical-pixel fallback font size only when the Windows owner-draw popup cannot create `NONCLIENTMETRICS.lfMenuFont`.

#### Scenario: System menu font is available
- **WHEN** Windows returns a valid `lfMenuFont`
- **THEN** the owner-draw popup uses that system font instead of constructing the fallback font

#### Scenario: System menu font creation fails
- **WHEN** the system menu font cannot be created
- **THEN** the owner-draw popup creates its existing fallback family at the calibrated 12 logical-pixel size

### Requirement: Context-menu behavior remains unchanged
The system MUST preserve all existing remote commands, popup positioning, dismissal, clipboard, provider actions, colors, icons, separators, and shadows.

#### Scenario: Typography calibration is applied
- **WHEN** the updated remote menu is used
- **THEN** only typography metrics change and existing command actions and popup lifecycle remain observable as before
