## ADDED Requirements

### Requirement: Light-theme file-row hover matches Explorer
An unselected file row in the light theme SHALL use `#E5F3FF` as its hover fill and SHALL retain its existing text, icon, geometry, and input behavior.

#### Scenario: Pointer hovers an unselected file row
- **WHEN** the pointer enters an unselected file row in the light theme
- **THEN** the row background becomes `#E5F3FF` without adding a focus outline

#### Scenario: Pointer leaves an unselected file row
- **WHEN** the pointer leaves an unselected file row
- **THEN** the row returns to its normal surface color

### Requirement: Light-theme focused selection matches Explorer
An actively selected file row in the light theme SHALL use `#CCE8FF` as its fill and an opaque black one-logical-pixel outline.

#### Scenario: File row is actively selected
- **WHEN** a file row is selected while its selection is active
- **THEN** the row renders the Explorer selection fill and one-logical-pixel black outline

#### Scenario: Pointer hovers a selected file row
- **WHEN** the pointer enters an actively or inactively selected file row
- **THEN** no unselected-hover overlay replaces its selection fill

### Requirement: Inactive and accessible themes remain distinct
Inactive selection, dark theme, and Windows high-contrast mode SHALL use theme-owned file-row roles with readable foreground/background contrast instead of fixed light-theme RGB values.

#### Scenario: Selected row becomes inactive
- **WHEN** an actively selected row loses active selection ownership
- **THEN** it uses the theme's inactive file-row selection fill and subdued outline

#### Scenario: Windows high contrast is active
- **WHEN** file-row interaction visuals are resolved in Windows high-contrast mode
- **THEN** fill, outline, and selected text derive from Windows system color roles

#### Scenario: Dark theme is active
- **WHEN** a file row is hovered or selected in the dark theme
- **THEN** it uses dark-theme file-row colors with distinguishable fill, outline, and text

### Requirement: File-row calibration is isolated
The calibrated colors SHALL apply only to file-surface rows and SHALL NOT change interaction state, persisted settings, or unrelated UI surfaces.

#### Scenario: Theme tokens are consumed
- **WHEN** navigation, menus, tabs, or generic controls render
- **THEN** their existing generic hover, selection, and focus semantic roles remain unchanged

### Requirement: Details row visuals stop at the last visible column
In Details view, a file row's hover fill, selection fill, focus outline, and row hit boundary SHALL end at the visible center line of the final header-column resize grip. The boundary SHALL include the header's visible-divider inset plus the combined width of visible data columns and SHALL NOT extend across unused viewport space.

#### Scenario: Visible columns are narrower than the viewport
- **WHEN** the combined visible-column width is less than the Details viewport width
- **THEN** the row visual ends exactly at the final header-column divider and the trailing area retains the ordinary file-view background

#### Scenario: Header exposes a centered resize grip
- **WHEN** the Details header renders its final separator centered inside the resize grip
- **THEN** the row boundary uses the horizontal control padding minus half the grip width so its right edge is pixel-aligned with the visible separator

#### Scenario: Visible columns exceed the viewport
- **WHEN** the combined visible-column width exceeds the Details viewport width
- **THEN** row visuals, cells, and the Details header share the existing horizontally scrollable extent

#### Scenario: Column visibility or width changes
- **WHEN** a visible column is resized, hidden, shown, or supplied by the runtime column registry
- **THEN** the row visual boundary follows the recomputed visible-column total without separate stored state

#### Scenario: Another view mode renders
- **WHEN** List, Content, Tiles, or an icon view renders file items
- **THEN** its existing item-width behavior remains unchanged
