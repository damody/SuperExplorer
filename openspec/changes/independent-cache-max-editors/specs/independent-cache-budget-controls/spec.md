## ADDED Requirements

### Requirement: Independent cache budget contract
The system SHALL persist and normalize independent budgets for all 14 approved memory, GPU, disk, extension-column, and MFT rows using the defaults, minima, and maxima in the approved design.

#### Scenario: Existing session has no aggregate budget object
- **WHEN** a legacy session is loaded without one or more new budget fields
- **THEN** the missing fields SHALL use their approved defaults and existing compatible values SHALL be normalized and preserved

#### Scenario: Value is outside its row bounds
- **WHEN** a budget is loaded or committed below its minimum or above its maximum
- **THEN** the effective value SHALL be clamped to that row's minimum or maximum without overflow

### Requirement: Synchronized numeric and slider controls
Each configurable telemetry row SHALL render a whole-number MB editor and a 400 px horizontal logarithmic progress-slider with stable automation identifiers.

#### Scenario: User adjusts a low budget
- **WHEN** the user moves a slider through its valid low-range stops
- **THEN** the sequence SHALL include `8, 16, 24, 32, 48, 64, 72, 84, 96, 128, 192, 256` where those values fall within the row bounds

#### Scenario: User types an intermediate value
- **WHEN** the user types a valid integer that is not a slider stop
- **THEN** the textbox and logarithmically interpolated slider position SHALL show that value and a subsequent slider gesture SHALL snap to the nearest valid stop

#### Scenario: User operates the slider from the keyboard
- **WHEN** the slider has focus and receives arrows, Home, or End
- **THEN** arrows SHALL move one stop and Home/End SHALL select the row minimum/maximum

#### Scenario: Available width is narrow
- **WHEN** the row cannot fit its label, usage, editor, and 400 px slider on one line
- **THEN** controls SHALL wrap without horizontal clipping and remain reachable by vertical scrolling

### Requirement: Transactional Folder Options commit
Folder Options SHALL parse and normalize all budget editors before Apply or OK commits one aggregate settings snapshot; Cancel SHALL discard the complete draft.

#### Scenario: MFT LRU changes from 512 to 2048
- **WHEN** the user enters `2048` and presses Apply or OK
- **THEN** the committed and persisted MFT LRU budget SHALL be 2048 MB and SHALL NOT reuse the previous 512 MB draft

#### Scenario: One editor is invalid
- **WHEN** an editor is empty or non-numeric at commit time
- **THEN** that editor SHALL restore its last valid committed value while other valid editors SHALL commit atomically

#### Scenario: User cancels changes
- **WHEN** the user changes multiple editors and presses Cancel
- **THEN** no runtime owner or persisted setting SHALL receive those draft values

### Requirement: Immediate owner propagation and telemetry
After a successful commit, each runtime owner SHALL receive and enforce its effective limit, and telemetry SHALL report the effective limit on the next sample.

#### Scenario: In-process limit is reduced
- **WHEN** a committed UI, Host, renderer, or disk-cache limit is below current usage
- **THEN** its owner SHALL begin bounded enforcement immediately and telemetry SHALL distinguish current usage from the new effective maximum

#### Scenario: Application restarts
- **WHEN** SuperExplorer restarts after budget settings were committed
- **THEN** every owner SHALL be configured from the persisted values before accepting new cache work

### Requirement: Derived telemetry remains read-only
Section headings, subtotals, BC7 availability, and service entry/hit/miss counters SHALL remain read-only.

#### Scenario: Folder Options renders telemetry
- **WHEN** the cache usage section is shown
- **THEN** only the 14 approved budget rows SHALL expose editors and sliders
