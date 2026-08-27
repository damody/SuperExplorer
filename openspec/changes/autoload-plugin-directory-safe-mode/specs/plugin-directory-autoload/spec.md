## ADDED Requirements

### Requirement: Executable-relative SePack discovery
SuperExplorer SHALL discover only direct `.sepack` files beneath the executable-relative `plugins` directory, SHALL inspect at most 1,024 entries in deterministic order, and SHALL ignore symlinks and non-files.

#### Scenario: Package copied before startup
- **WHEN** a valid `.sepack` archive is added directly beneath `plugins` while SuperExplorer is closed
- **THEN** the next startup imports, validates, seals, resolves, and admits it without requiring a shortcut edit

#### Scenario: Loose DLL or nested content exists
- **WHEN** `plugins` contains a loose DLL, nested directory, symlink, or unrelated file
- **THEN** production discovery does not import or execute that entry

### Requirement: Manifest identity and desired state
Package identity SHALL come only from the validated `.sepack` manifest. A newly discovered package and its features SHALL default enabled when no explicit desired-state entry exists, while explicit global or package disabled state SHALL prevent native admission.

#### Scenario: Archive filename disagrees with manifest
- **WHEN** a valid archive filename differs from its manifest package ID
- **THEN** catalog, settings, diagnostics, and admission use the validated manifest ID

#### Scenario: Package was disabled
- **WHEN** the desired-state store explicitly disables a validated package ID
- **THEN** startup does not admit its native code even though the archive remains installed

### Requirement: Installer and development override
The installer SHALL deploy complete `.sepack` archives and create shortcuts without fixed Plugin arguments. Explicit `--plugin-dll` SHALL remain available only as a development/test override and loose DLLs SHALL NOT be auto-discovered.

#### Scenario: Installed shortcut launches
- **WHEN** SuperExplorer is launched from a newly installed shortcut
- **THEN** bundled `.sepack` archives are discovered without command-line Plugin arguments

#### Scenario: Developer supplies an explicit DLL
- **WHEN** a developer launches with `--plugin-dll` and an absolute DLL path
- **THEN** the explicit development loader remains available independently of production `.sepack` discovery
