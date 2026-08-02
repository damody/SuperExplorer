## ADDED Requirements

### Requirement: Left-button drag starts from the selected filesystem items
The application SHALL start exactly one native OLE drag after a left-button gesture crosses the Windows drag threshold, including when Shift was held during mouse-down selection.

#### Scenario: Plain left drag crosses the threshold
- **WHEN** the user presses a selected filesystem item with the left button and moves beyond the Windows drag threshold
- **THEN** one drag request SHALL contain the current selection and advertise Copy and Move

#### Scenario: Shift selection remains draggable
- **WHEN** Shift extends the selection and the same left-button gesture crosses the drag threshold
- **THEN** the resulting drag SHALL contain the extended selection and remain eligible for Move

### Requirement: Modifier and default effects match Windows Explorer
The application MUST make Ctrl force Copy, Shift force Move, and an unmodified drag choose Move for same-volume destinations or Copy for cross-volume destinations when the respective effect is allowed.

#### Scenario: Ctrl forces copy
- **WHEN** Ctrl is held while a left drag is over a writable destination that accepts Copy
- **THEN** the negotiated and performed effect SHALL be Copy

#### Scenario: Shift forces move
- **WHEN** Shift is held while a left drag is over a writable destination that accepts Move
- **THEN** the negotiated and performed effect SHALL be Move

#### Scenario: Unmodified same-volume drag moves
- **WHEN** an unmodified left drag has filesystem sources and destination on the same volume
- **THEN** the negotiated effect SHALL be Move

#### Scenario: Unmodified cross-volume drag copies
- **WHEN** an unmodified left drag has any filesystem source on a different volume from the destination
- **THEN** the negotiated effect SHALL be Copy

### Requirement: Folder and background drop targets are safe and typed
The application SHALL accept left-button drops on writable folder rows and writable current-folder backgrounds, queue a typed Copy or Move request with conflict prompting, and reject invalid destinations without mutating the filesystem.

#### Scenario: Drop on a folder row
- **WHEN** a valid left drag is released over a writable folder row
- **THEN** that folder SHALL be the immutable destination of one typed transfer request

#### Scenario: Drop on the file-view background
- **WHEN** a valid left drag is released over the writable file-view background
- **THEN** the current folder SHALL be the immutable destination of one typed transfer request

#### Scenario: Unsafe recursive target is rejected
- **WHEN** a selected folder is dragged onto itself or one of its descendants
- **THEN** the target SHALL advertise no effect and no transfer request SHALL be queued

### Requirement: Drag lifecycle is bounded and recoverable
The application SHALL keep drag work off the UI thread and MUST clear transient drag state on success, cancellation, invalid release, focus loss, navigation, tab change, and shutdown.

#### Scenario: Cancelled drag performs no mutation
- **WHEN** the user presses Escape or releases outside a valid target
- **THEN** the drag SHALL terminate once, clear its cues and pending command, and perform no Copy or Move

#### Scenario: Transfer does not block browsing
- **WHEN** a valid Copy or Move continues in the background
- **THEN** navigation and tab interaction SHALL remain available while operation progress is reported

