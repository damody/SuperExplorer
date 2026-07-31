## ADDED Requirements

### Requirement: Filesystem-backed Shell locations publish canonical paths
The system SHALL publish a `FileSystem` location containing the complete Windows-resolved path when a
successfully resolved Shell folder provides a non-empty filesystem path.

#### Scenario: Known folder resolves to a local path
- **WHEN** Documents, Downloads, Desktop, Pictures, Music, Videos, or another Shell folder resolves to
  a drive-qualified filesystem path
- **THEN** committed navigation history and the editable address use that complete path

#### Scenario: Known folder is redirected to UNC
- **WHEN** Windows resolves a filesystem-backed known folder to a UNC location
- **THEN** the editable address exposes the complete redirected UNC path

### Requirement: Editable address is portable
The system MUST select the complete canonical filesystem text when the address bar enters edit mode.

#### Scenario: User copies a known-folder address
- **WHEN** the user clicks the address bar after navigating through a filesystem-backed Shell shortcut
- **THEN** the selected value has no `shell:` prefix and can be submitted to navigate to the same folder

### Requirement: Pure namespaces preserve Shell identity
The system MUST preserve the original descriptor when Windows does not provide a non-empty filesystem
path.

#### Scenario: User edits a pure namespace address
- **WHEN** Home, This PC, Recycle Bin, Network, Libraries, or another non-filesystem namespace resolves
- **THEN** navigation remains committed to its Shell descriptor without a fabricated filesystem path

### Requirement: Friendly browsing labels remain unchanged
The system SHALL keep Shell display names for breadcrumbs and navigation labels while using canonical
paths only for committed filesystem location identity and editable address text.

#### Scenario: Documents is shown in browsing mode
- **WHEN** the user navigates to Documents through the navigation pane
- **THEN** the breadcrumb remains a friendly localized label and edit mode exposes the complete path
