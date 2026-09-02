## ADDED Requirements

### Requirement: Existing usable ADB takes precedence
The system SHALL resolve a usable ADB executable in the order process `PATH`, recognized or configured Android SDK locations, then the active SuperExplorer-managed installation. Every candidate MUST be a regular executable file that passes a bounded version probe, and a rejected candidate MUST NOT prevent evaluation of later candidates.

#### Scenario: System ADB is usable
- **WHEN** `PATH` supplies an ADB executable that passes the version probe
- **THEN** the system uses that executable without downloading, replacing, or modifying it

#### Scenario: Earlier candidate is invalid
- **WHEN** a `PATH` candidate is missing, non-regular, non-executable, times out, or fails its version probe and a later SDK or managed candidate is valid
- **THEN** the system selects the first later valid candidate and retains bounded diagnostic provenance for the rejected candidate

#### Scenario: System ADB appears after managed installation
- **WHEN** a later resolution finds a valid system ADB while a managed copy exists
- **THEN** the system selects the system ADB and leaves the managed copy intact

### Requirement: Managed ADB installation is explicit and official
The system SHALL offer managed ADB installation only after no usable existing candidate is found and only after the user selects the download action. Production download requests MUST originate from the centralized allowlisted Google HTTPS Platform-Tools for Windows source and MUST validate every redirect destination.

#### Scenario: No usable ADB exists
- **WHEN** resolution exhausts all existing candidates while the user opens the Local APK Install submenu
- **THEN** the submenu offers `Download and install Google ADB...` and no network request starts until the user selects it

#### Scenario: Download source violates policy
- **WHEN** a production request or redirect uses a non-HTTPS scheme, non-allowlisted host, or non-Platform-Tools path
- **THEN** the system rejects it before consuming an archive and reports a bounded policy error

### Requirement: Managed installation is bounded and transactional
The installer SHALL bound transfer time and bytes, archive entries and expanded bytes, and extracted paths. It MUST reject rooted paths, parent traversal, links or reparse-like entries, destination escape, and archives without the expected `platform-tools/adb.exe`. It SHALL activate a version atomically only after the shared version probe succeeds.

#### Scenario: Valid official archive installs
- **WHEN** the user-approved download completes within all limits, safe extraction produces the expected layout, and the extracted ADB passes its version probe
- **THEN** the installer atomically activates that managed version, invalidates tool/device caches, and preserves no partial transaction as active

#### Scenario: Malicious or oversized archive is received
- **WHEN** an archive violates a path, type, entry-count, compressed-byte, or expanded-byte limit
- **THEN** the installer fails closed, removes only its verified transaction directory, and does not change the active managed version

#### Scenario: Cancellation or validation failure occurs
- **WHEN** download, extraction, probing, or promotion is cancelled, times out, or fails
- **THEN** the operation reaches one non-success terminal state and any previously active managed version remains usable

### Requirement: Tool diagnostics are bounded and private
ADB resolution and installation SHALL expose actionable stage and provenance diagnostics while excluding full environment dumps, credentials, unbounded subprocess output, and unrelated user data.

#### Scenario: Multiple candidates and installation stages fail
- **WHEN** resolution or managed installation cannot produce a usable ADB
- **THEN** the user receives bounded candidate/stage diagnostics sufficient to retry without disclosure of unrelated environment values
