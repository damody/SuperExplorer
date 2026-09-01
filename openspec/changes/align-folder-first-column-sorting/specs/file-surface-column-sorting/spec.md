## ADDED Requirements

### Requirement: Folders precede files for every column sort
The file surface SHALL present real folders before files regardless of selected column or sort direction. A browsable local filesystem archive SHALL remain in the file group even when its navigation model reports it as a container.

#### Scenario: Ascending sort contains folders and files
- **WHEN** a directory containing folders and files is sorted ascending by a supported column
- **THEN** every folder appears before the first file

#### Scenario: Descending sort contains folders and files
- **WHEN** a directory containing folders and files is sorted descending by a supported column
- **THEN** every folder still appears before the first file

#### Scenario: Runtime extension column sorts mixed entries
- **WHEN** mixed folders and files are sorted by a runtime extension column with available sort values
- **THEN** the file surface applies the same folder-before-file boundary used for built-in columns

#### Scenario: Browsable ZIP is mixed with real folders
- **WHEN** a local ZIP file is navigable as a Shell container and is sorted with real folders and files
- **THEN** every real folder appears before the ZIP and the ZIP remains in the file group

#### Scenario: Non-filesystem provider folder has no Windows directory bit
- **WHEN** a remote, virtual, or Shell namespace entry has no local filesystem directory metadata and its provider classifies it as a container
- **THEN** the entry remains in the folder group

### Requirement: Each classification group sorts independently
Within the folder group and within the file group, the file surface SHALL order entries by the selected column and direction without reversing the classification-group boundary.

#### Scenario: Direction reverses values within both groups
- **WHEN** the user changes a supported column from ascending to descending
- **THEN** comparable values reverse independently among folders and among files while folders remain first

#### Scenario: Equal primary values are deterministic
- **WHEN** two entries in the same classification group have equal selected-column values
- **THEN** their order is determined by the existing name and stable provider-identity tie-breakers

### Requirement: Missing values remain bounded by classification
An entry lacking an optional sort value SHALL remain after entries with present values in its own sorting classification group for both directions and SHALL NOT cross the folder-before-file boundary.

#### Scenario: Folder lacks a value while files have values
- **WHEN** a folder has no value for the selected optional column and one or more files have values
- **THEN** the folder remains in the folder group before every file

#### Scenario: Mixed extension values are absent
- **WHEN** runtime extension sort values are absent for some folders and files
- **THEN** missing values sort after present values within the corresponding group and the folder group remains first

### Requirement: Sorting does not add visible grouping controls
Folder-first column sorting SHALL remain a contiguous ordering behavior and SHALL NOT add section headings, separators, preferences, or persisted grouping state.

#### Scenario: File surface renders a sorted directory
- **WHEN** the file surface renders a folder-first sorted directory
- **THEN** it renders the existing item rows or tiles without additional folder/file group chrome
