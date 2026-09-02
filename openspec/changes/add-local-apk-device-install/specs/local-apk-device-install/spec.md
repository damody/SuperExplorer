## ADDED Requirements

### Requirement: Local single APK selection exposes installation
The system SHALL add an `Install` submenu only when the context target is exactly one regular file in a Local filesystem location and its extension equals `.apk` case-insensitively.

#### Scenario: One local APK is selected
- **WHEN** the user opens the context menu for exactly one Local regular file ending in `.apk` with any extension casing
- **THEN** the first context-menu item is the `Install` submenu, followed by a separator, without removing or reordering the remaining Shell commands

#### Scenario: Input is not an eligible local APK
- **WHEN** the selection is empty, multiple, a directory, a non-APK file, or belongs to ADB, SFTP, or another virtual provider
- **THEN** the APK `Install` submenu is absent

### Requirement: Device submenu is complete and exact
The system SHALL derive an immutable device snapshot from bounded `adb devices -l` output. Each row MUST retain the stable serial, display a human-friendly model/device name with serial fallback, and expose whether its state is installable. Visible names MUST NOT be used as command identifiers.

#### Scenario: Multiple usable devices are connected
- **WHEN** discovery returns multiple devices in the `device` state, including devices with duplicate display names
- **THEN** the submenu shows every device name as a selectable row and each row targets its own captured serial

#### Scenario: Device is unavailable
- **WHEN** discovery reports `offline`, `unauthorized`, or another non-installable state
- **THEN** the submenu shows that device and state as a disabled row

#### Scenario: No devices are connected
- **WHEN** a usable ADB returns no device rows
- **THEN** the submenu shows a disabled `No devices detected` row and an enabled refresh command

#### Scenario: Discovery is loading or fails
- **WHEN** no current snapshot exists or bounded discovery fails
- **THEN** the submenu presents a non-blocking loading or actionable error state and permits refresh without freezing the native menu loop

#### Scenario: Late discovery result is stale
- **WHEN** a tool change, refresh, or menu-session replacement advances the generation before an earlier discovery completes
- **THEN** the earlier result cannot replace the current snapshot or create an install request

### Requirement: APK installs through the existing operation lifecycle
Selecting an installable device SHALL submit a background operation that revalidates the canonical Local regular APK and ADB identity and invokes separate process arguments equivalent to `adb -s <serial> install -r <absolute-apk-path>`. The UI thread and native menu message loop MUST NOT wait for command completion.

#### Scenario: APK update succeeds
- **WHEN** the selected file and tool remain valid and ADB exits successfully with an accepted install-success result
- **THEN** the operation reports pending/running and one successful terminal state for the selected serial

#### Scenario: Path or serial contains shell-sensitive text
- **WHEN** the APK path contains spaces, Unicode, or shell metacharacters or the serial contains accepted ADB serial punctuation
- **THEN** the runner passes separate arguments without shell interpolation and targets exactly the captured path and serial

#### Scenario: APK or tool becomes stale before spawn
- **WHEN** the file is no longer the selected Local regular APK or the resolved tool identity no longer matches before process creation
- **THEN** the operation fails before invoking ADB and offers a refreshable bounded diagnostic

#### Scenario: ADB rejects or loses the device
- **WHEN** ADB reports signature mismatch, downgrade rejection, authorization loss, disconnect, non-zero exit, or missing success confirmation
- **THEN** the operation reports one failed terminal state with bounded relevant diagnostics and does not add `-d`, uninstall, or retry another device

#### Scenario: Installation is cancelled or times out
- **WHEN** the user cancels the operation or its deadline expires
- **THEN** the child process is terminated through the existing bounded runner and the operation reports one cancelled or timed-out terminal state rather than success

### Requirement: Download recovery refreshes the install workflow
After a managed ADB installation succeeds, the system SHALL invalidate resolver and device snapshots, resolve the newly available tool, and refresh device state for the APK install workflow.

#### Scenario: Managed download completes from the submenu
- **WHEN** no existing ADB was usable and the user-requested managed installation succeeds
- **THEN** the menu workflow can transition to current device rows without application restart

#### Scenario: Managed download fails
- **WHEN** the user-requested managed installation fails or is cancelled
- **THEN** the submenu remains recoverable, shows the bounded failure, and permits a later retry without advertising a partial installation
