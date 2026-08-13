## ADDED Requirements

### Requirement: Recursive MFT directory facts
SuperExplorer SHALL obtain exact recursive folder facts exclusively from MFT Service. `File Count` SHALL count regular-file descendants, and `Folder Count` SHALL count directory entries while excluding the queried root. Reparse-point, junction, and symbolic-link directory entries SHALL count once, while their targets SHALL NOT be traversed.

#### Scenario: Nested NTFS folder is complete
- **WHEN** MFT Service returns a complete aggregate for a folder containing nested regular files and real directories
- **THEN** File Count equals all regular-file descendants and Folder Count equals all real directory descendants without counting the queried root

#### Scenario: Reparse directory is present
- **WHEN** a subtree contains a reparse-point directory that targets another location
- **THEN** the reparse directory entry contributes one Folder Count and its target descendants contribute neither File Count nor Folder Count

#### Scenario: MFT facts are not exact
- **WHEN** the location is virtual, unsupported, unavailable, partial, cancelled, or stale
- **THEN** SuperExplorer publishes no exact File Count or Folder Count and performs no filesystem fallback scan

### Requirement: Optional built-in count columns
SuperExplorer SHALL register `File Count` and `Folder Count` as default-hidden built-in Details columns with stable IDs `builtin:file_count` and `builtin:folder_count`, container applicability, integer values, and background aggregate cost. Each column SHALL support independent visibility, width, order, persistence, and exact numeric sorting.

#### Scenario: User enables both count columns
- **WHEN** the user selects File Count and Folder Count in the Details column chooser
- **THEN** eligible folder rows display exact recursive values, file rows remain blank, and the chosen visibility/order/widths survive session restore

#### Scenario: Count is unavailable
- **WHEN** an eligible folder has no exact MFT facts
- **THEN** the visible count cell displays `—` and is excluded from the exact integer sort domain rather than being treated as zero

#### Scenario: Legacy session is restored
- **WHEN** a session written before the count-column IDs existed is decoded
- **THEN** all prior layout preferences are preserved, File Count and Folder Count are appended exactly once with default widths and hidden visibility, and both appear as unchecked rows in the Details column chooser

#### Scenario: Current session already contains count columns
- **WHEN** a current extensible layout containing File Count and Folder Count is decoded
- **THEN** reconciliation does not duplicate, reorder, resize, or change the visibility of either saved entry

### Requirement: Shared deduplicated directory facts
The Host SHALL share one generation-tagged directory-facts result among File Count, Folder Count, and eligible dependent extension contributions. Directory-facts demand SHALL exist only while File Count or Folder Count is visible. An enabled contribution's admission policy alone SHALL NOT create demand.

#### Scenario: Multiple consumers request one folder
- **WHEN** both count columns and multiple dependent contributions need the same folder in one MFT generation
- **THEN** the coordinator performs at most one MFT aggregate query and fans the result out to all consumers

#### Scenario: A count column becomes visible
- **WHEN** File Count or Folder Count transitions from hidden to visible for an already loaded directory
- **THEN** the Host immediately submits deduplicated MFT requests for its eligible folder rows without requiring refresh or navigation and repaints exact results as they arrive

#### Scenario: Both count columns are hidden
- **WHEN** File Count and Folder Count are both hidden, including when an enabled contribution declares count limits
- **THEN** the Host submits no directory-facts query for count presentation or admission

#### Scenario: Last visible count column is hidden
- **WHEN** the user hides the last visible count column
- **THEN** obsolete count-only presentation work is cancelled and no new directory-facts request is submitted until a count column becomes visible again

### Requirement: Directory-fact invalidation
Directory facts and admission decisions SHALL carry request and MFT generations. Navigation, refresh, watcher invalidation, cancellation, or a newer MFT generation SHALL prevent obsolete facts and derived results from updating current presentation.

#### Scenario: MFT generation changes before dispatch
- **WHEN** exact facts were obtained but their MFT generation becomes obsolete before an extension job is dispatched
- **THEN** the Host does not dispatch from the obsolete decision and returns the item to dependency acquisition for the current generation

#### Scenario: User navigates away
- **WHEN** a directory-facts or dependent extension result completes for an old tab generation
- **THEN** the old result is discarded and cannot populate the current columns
