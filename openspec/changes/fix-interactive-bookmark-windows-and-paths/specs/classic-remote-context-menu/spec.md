# Classic Remote Context Menu

## ADDED Requirements

### Requirement: Custom remote commands use a classic vertical menu

The custom Local/ADB/SFTP context menu SHALL render every applicable command as a full-width vertical row and SHALL NOT render a horizontal command strip.

#### Scenario: Item menu opens

- **WHEN** a remote item context menu is requested
- **THEN** Open, Cut, Copy, Rename, and Permanent Delete appear in their existing order and behavior
- **AND** Open is separated from the editing commands by a thin divider

#### Scenario: Context changes command membership

- **WHEN** Paste is available or the menu targets background space
- **THEN** the same existing contextual command-membership rules are retained

### Requirement: Menu typography and geometry match the classic reference style

The menu SHALL use the configured application UI font at 12 logical pixels, 22 logical-pixel rows, an 18px icon slot, 10px gap, 6px inset, 236px width, full-row feedback, a one-pixel border, and square menu and row corners. Its light-theme surface SHALL be `#F7F7F7` with a directional soft shadow that has no top or left reach, retains 18px bottom reach, and has a shorter 10px right reach.

#### Scenario: Menu is rendered

- **WHEN** any custom remote context menu is visible
- **THEN** no rounded-corner style or card-sized command button is present
- **AND** no command from the visual reference is added unless it already belongs to the existing command model
