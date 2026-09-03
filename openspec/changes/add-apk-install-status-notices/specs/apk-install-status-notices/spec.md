## ADDED Requirements

### Requirement: Immediate truthful installation status
After an installable APK device is selected, Super Explorer SHALL show an in-app installing notice before ADB process creation and SHALL represent running work without a percentage or byte count.

#### Scenario: Installation starts
- **WHEN** the APK install selection is accepted
- **THEN** the UI displays `正在將 <APK 名稱> 安裝到 <裝置名稱>…` with indeterminate activity before ADB is spawned

#### Scenario: Started delivery unavailable
- **WHEN** the started event cannot be accepted by the UI event boundary
- **THEN** the system rejects the install before ADB spawn and records a bounded diagnostic

### Requirement: Correlated terminal notice
Every accepted APK install start SHALL produce at most one user-visible terminal status with the same request identity and SHALL distinguish success, failure, cancellation, and timeout.

#### Scenario: Successful installation
- **WHEN** ADB reports successful completion
- **THEN** the running notice becomes `<APK 名稱> 已成功安裝到 <裝置名稱>`

#### Scenario: Failed installation
- **WHEN** validation fails or ADB exits unsuccessfully without cancellation or timeout
- **THEN** the running notice becomes `安裝失敗` with a bounded actionable summary

#### Scenario: Cancelled installation
- **WHEN** the correlated cancellation token stops the install
- **THEN** the running notice becomes an explicit cancellation result and never later becomes success

#### Scenario: Timed-out installation
- **WHEN** the install exceeds its configured deadline
- **THEN** the running notice becomes an explicit timeout result and never later becomes success

### Requirement: Concurrent and stale-state isolation
APK install notices SHALL be keyed by request ID, SHALL apply first-terminal-wins, and SHALL prevent duplicate, late, stale, or unmatched events from mutating unrelated current state.

#### Scenario: Concurrent installs
- **WHEN** two APK installs overlap and complete in either order
- **THEN** each retains its own APK, device, running state, and terminal result

#### Scenario: Duplicate terminal
- **WHEN** a second terminal arrives for an already terminal request
- **THEN** it is ignored and the first terminal remains visible

#### Scenario: Terminal without start
- **WHEN** a terminal arrives without an accepted matching start
- **THEN** it is excluded from user-visible notice state and remains diagnosable

#### Scenario: Closed or replaced UI generation
- **WHEN** an event targets a closed window or stale generation
- **THEN** it does not mutate the current notice list

### Requirement: Bounded in-app presentation
The UI SHALL bound APK notice count and text, retain active notices over terminal history, fade successful results after a short interval, and retain unsuccessful results for a longer readable interval.

#### Scenario: Notice capacity reached
- **WHEN** adding a record reaches capacity
- **THEN** the oldest terminal history is evicted before any active install and all displayed text remains bounded

#### Scenario: Success fade
- **WHEN** a successful notice reaches its success retention deadline
- **THEN** it fades and is removed without affecting another install

#### Scenario: Error retention
- **WHEN** failure, cancellation, or timeout reaches the short success deadline
- **THEN** it remains visible until the longer error retention deadline

### Requirement: Existing APK execution guarantees remain intact
Status publication MUST preserve non-blocking native-menu behavior, canonical Local APK validation, exact serial execution, argument-safe `install -r`, existing system ADB precedence, and managed Google Platform-Tools fallback.

#### Scenario: Context menu returns
- **WHEN** the user selects an installable device
- **THEN** the native menu closes promptly while status and ADB work continue in the background

#### Scenario: Existing system ADB
- **WHEN** a valid system ADB is available
- **THEN** it is used directly and no managed download is triggered

#### Scenario: Supplied APK user check
- **WHEN** `qq9.3.55.apk` is used for final eligibility and status verification without newly authorized real-device mutation
- **THEN** the workflow proves menu eligibility and controlled installing/terminal notices without silently installing on an unapproved device
