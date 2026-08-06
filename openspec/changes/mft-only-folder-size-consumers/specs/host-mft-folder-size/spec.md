## ADDED Requirements

### Requirement: Host cache and MFT Service are the only folder-size sources
The system SHALL obtain recursive folder-size data only from the persistent Host cache or SuperExplorer MFT Windows Service. It MUST NOT use Everything or recursive file-system traversal as a fallback.

#### Scenario: Host cache hit
- **WHEN** an eligible folder has a valid Host cache entry for its canonical identity, modification timestamp, and cache schema
- **THEN** every active consumer receives that cached recursive byte count without querying another backend

#### Scenario: Host cache miss with MFT result
- **WHEN** an eligible folder has no valid Host cache entry and MFT Service returns a complete aggregate
- **THEN** the Host admits the result to its persistent cache and publishes the same byte count to every active consumer

#### Scenario: MFT unavailable
- **WHEN** cache lookup misses and MFT Service is stopped, unavailable, stale, malformed, unsupported, or cannot provide a complete result
- **THEN** the folder-size value remains unavailable and neither Everything nor recursive traversal starts

#### Scenario: Service recovers
- **WHEN** a prior lookup was unavailable and a refresh occurs after MFT Service provides a valid current index
- **THEN** the Host retries the MFT path and publishes a successful value without requiring application restart

### Requirement: Folder-size cache is owned and invalidated by the Host
The system SHALL keep persistent folder-size validity policy in the Host and SHALL share it across built-in Size, Folder size, and Size Map consumers.

#### Scenario: Unchanged folder reuses cache
- **WHEN** multiple consumers request the same canonical folder identity with an unchanged modification timestamp and schema
- **THEN** they reuse one Host cache value without duplicate measurement

#### Scenario: Modification timestamp changes
- **WHEN** a folder's modification timestamp differs from the timestamp in its cache admission record
- **THEN** the Host treats the entry as stale and requests a replacement only from MFT Service

#### Scenario: Cache schema changes
- **WHEN** the Host folder-size cache schema changes
- **THEN** entries written under the prior schema are not used

### Requirement: Built-in Size displays recursive folder bytes
The system SHALL display an available Host folder-size value in the built-in Size column for eligible folders independently of whether the Folder size extension is enabled. Files SHALL retain their ordinary file length.

#### Scenario: Extension disabled
- **WHEN** the Folder size extension is disabled and MFT Service or Host cache supplies a folder total
- **THEN** the built-in Size cell displays that recursive folder total

#### Scenario: File row
- **WHEN** a row is an ordinary file with a known file length
- **THEN** the built-in Size cell displays the ordinary file length and does not replace it with folder-size data

#### Scenario: ZIP Shell container
- **WHEN** a ZIP or other Shell namespace item reports container semantics but also has ordinary file bytes
- **THEN** it is not requested as a folder and its Folder size cell remains blank

#### Scenario: Folder value unavailable
- **WHEN** an eligible folder has neither a valid Host cache value nor a complete MFT result
- **THEN** its built-in Size and Folder size values remain blank rather than displaying zero or a continuing calculation

### Requirement: Built-in Size sorting uses the displayed byte source
The system SHALL use ordinary file bytes for files and recursive Host folder bytes for eligible folders when sorting by built-in Size.

#### Scenario: Mixed known values
- **WHEN** Size sorting includes files and folders with known displayed byte values
- **THEN** rows are ordered numerically using those respective values

#### Scenario: Missing folder values
- **WHEN** Size sorting includes folders without an available recursive value
- **THEN** those rows follow the existing missing-value ordering and are not coerced to zero

### Requirement: Size Map uses the shared MFT-only service
The system SHALL source Size Map folder hierarchy and totals through the same Host service and MFT index contract used by data columns.

#### Scenario: Size Map after column lookup
- **WHEN** Size Map requests a folder already represented by a valid Host/MFT snapshot
- **THEN** it reuses shared Host data rather than starting a recursive scan

#### Scenario: Size Map MFT failure
- **WHEN** Size Map has no valid shared data and MFT Service cannot provide a complete snapshot
- **THEN** Size Map reports unavailable and does not invoke Everything or recursive traversal

### Requirement: Folder-size backend status is observable
The system SHALL expose the latest relevant terminal or active folder-size backend state in the status bar.

#### Scenario: Cache backend
- **WHEN** displayed values are served from Host cache
- **THEN** the status bar displays `Folder size: Host cache`

#### Scenario: MFT backend
- **WHEN** displayed values are being obtained from or were most recently supplied by MFT Service
- **THEN** the status bar displays `Folder size: MFT service`, with an ellipsis only while work is active

#### Scenario: Unavailable backend
- **WHEN** folder-size data is required but MFT Service cannot provide it
- **THEN** the status bar displays `Folder size: MFT unavailable` without indicating an active slow calculation

### Requirement: MFT aggregation remains bounded and privileged
The system SHALL keep raw NTFS access in the installed LocalSystem MFT service and SHALL use no more than eight aggregation workers.

#### Scenario: Large volume index
- **WHEN** MFT Service aggregates a volume containing more work partitions than the configured maximum
- **THEN** no more than eight aggregation workers execute concurrently and the resulting totals remain exact

#### Scenario: Installed service identity
- **WHEN** SuperExplorer is installed normally
- **THEN** `SuperExplorerMft` is configured to run automatically as LocalSystem and the unprivileged UI does not open the raw volume directly
