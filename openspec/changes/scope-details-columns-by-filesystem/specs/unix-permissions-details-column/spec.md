## ADDED Requirements

### Requirement: Remote entries carry optional Unix mode metadata
The shared file-entry metadata SHALL carry an optional Unix mode containing file-type, special, and permission bits. SFTP SHALL map the server attributes' mode and ADB SHALL collect mode data through a bounded directory-level operation without launching one subprocess per entry.

#### Scenario: SFTP supplies permission attributes
- **WHEN** an SFTP directory entry contains a valid Unix mode
- **THEN** the corresponding file entry carries the same relevant type and permission bits

#### Scenario: ADB directory contains many entries
- **WHEN** ADB lists a directory and collects Unix modes
- **THEN** it uses bounded directory-level metadata collection and does not issue one metadata subprocess per row

#### Scenario: Mode is absent or malformed
- **WHEN** ADB or SFTP does not provide a valid mode for an entry
- **THEN** the entry mode is absent, the directory listing remains successful, and no per-entry error log is emitted

### Requirement: Permissions renders symbolic and octal mode
The built-in Permissions column SHALL render a known Unix mode as the standard file-type character plus nine symbolic permission characters, followed by a space and four-digit octal permissions in parentheses.

#### Scenario: Common Unix entry types render
- **WHEN** modes represent a directory `0755`, regular file `0644`, and symbolic link `0777`
- **THEN** they render respectively as `drwxr-xr-x (0755)`, `-rw-r--r-- (0644)`, and `lrwxrwxrwx (0777)`

#### Scenario: Special permission bits render
- **WHEN** setuid, setgid, or sticky bits occur with or without their corresponding execute bit
- **THEN** the symbolic form uses the standard `s`, `S`, `t`, or `T` character and the octal form retains the special-bit digit

#### Scenario: File type is unknown
- **WHEN** permission bits are valid but file-type bits are unknown
- **THEN** the symbolic form begins with `?` and retains the permission and octal representation

#### Scenario: Mode is unavailable
- **WHEN** an ADB or SFTP entry has no Unix mode
- **THEN** Permissions renders an em dash without reporting a provider or plugin fault

### Requirement: Permissions sorts by numeric mode
The Permissions column SHALL sort known entries by numeric Unix mode and SHALL place missing modes after known modes for ascending order with deterministic inverse behavior for descending order.

#### Scenario: Display text order differs from mode order
- **WHEN** entries with different modes are sorted by Permissions
- **THEN** their numeric mode values, not formatted strings, determine order

#### Scenario: Known and missing modes are mixed
- **WHEN** an ascending Permissions sort includes known and missing modes
- **THEN** all known modes precede missing modes with stable tie behavior
