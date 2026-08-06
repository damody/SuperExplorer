## ADDED Requirements

### Requirement: Built-in Size uses fixed one-decimal precision
The system SHALL format every nonzero built-in Size value in KB, MB, GB, or TB with exactly one decimal place while preserving binary unit scaling.

#### Scenario: Large fractional folder size
- **WHEN** the built-in Size value represents 250.5 GiB
- **THEN** the displayed label is `250.5 GB`

#### Scenario: Exact unit
- **WHEN** the built-in Size value is exactly one MiB
- **THEN** the displayed label is `1.0 MB`

#### Scenario: Fractional kilobytes
- **WHEN** the built-in Size value is 1,536 bytes
- **THEN** the displayed label is `1.5 KB`

#### Scenario: Zero bytes
- **WHEN** the built-in Size value is zero
- **THEN** the displayed label remains `0 KB`

#### Scenario: Nonzero sub-kilobyte value
- **WHEN** the built-in Size value is between one and 1,023 bytes
- **THEN** the displayed label is `1.0 KB`

### Requirement: Formatting does not change size semantics
The system MUST limit this change to presentation and SHALL preserve the underlying bytes, sorting values, Host cache, MFT Service, and Folder size plugin behavior.

#### Scenario: Same bytes in both columns
- **WHEN** Size and Folder size render the same available recursive byte count
- **THEN** both labels show the same scaled value to one decimal place
