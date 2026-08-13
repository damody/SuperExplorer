## MODIFIED Requirements

### Requirement: Rust tokei column example
`rust-tokei-code-lines-column` SHALL use a locked Rust tokei library in its DLL to return language, code, comment, blank and total counts in bounded batches, with a numeric selected sort metric and no OS process per file. Its folder-applicable data-column contribution SHALL declare `max_file_count = 999`; the Host SHALL keep folder Code Lines undispatched while the File Count column is hidden or its fact is pending, unavailable, stale, partial, or at least 1000, while ordinary file analysis remains unchanged.

#### Scenario: Mixed language fixture is analyzed
- **WHEN** Rust, C/C++, Python, Lua, JavaScript, empty, invalid-text and unknown files are processed
- **THEN** supported files receive typed counts, unsupported files are not reported as zero and the test observes no per-file process creation

#### Scenario: Folder contains fewer than 1000 files
- **WHEN** exact current-generation File Count for a folder is between 0 and 999 inclusive
- **THEN** the Host admits the Rust Code Lines folder job and the provider may calculate it

#### Scenario: Folder count dependency cannot admit work
- **WHEN** File Count is pending, unavailable, stale, partial, or at least 1000
- **THEN** the Rust provider receives no folder callback and the cell displays the matching Host-owned waiting, dependency, or over-limit state

#### Scenario: File Count column is hidden
- **WHEN** Rust Code Lines is enabled for a folder row while the built-in File Count column is hidden
- **THEN** the Host performs no count query for Code Lines, sends no folder callback, displays a red `Limit`, and exposes `依賴 File Count，因此未啟動` through hover tooltip and the accessible cell name

#### Scenario: Folder exceeds the Code Lines File Count limit
- **WHEN** Rust or Lua Code Lines receives an exact current-generation File Count of at least 1000 for a folder row
- **THEN** the Host sends no folder callback, displays a red `Limit`, and exposes `File Count 超過限制，因此未啟動` through hover tooltip and the accessible cell name

#### Scenario: File Count is still pending
- **WHEN** a visible File Count query has not yet produced an exact current-generation fact
- **THEN** the Code Lines cell continues to display `等待 File Count…` using its normal pending presentation and does not display `Limit`

### Requirement: Lua tokei column example
`lua-tokei-code-lines-column` SHALL package its exact `windows-x64` `tokei.exe`, license and hash and invoke it only through `tools.execute_bundled`/ToolHandle with shell-free bounded batches and JSON mapping. Its folder-applicable data-column contribution SHALL declare `max_file_count = 999` and SHALL use the same Host-enforced folder admission and presentation states as the Rust example.

#### Scenario: Tool payload is tampered
- **WHEN** the packaged tokei hash differs or the executable is removed while another tokei exists on PATH
- **THEN** the feature is blocked before callback and no fallback executable is used

#### Scenario: Lua folder reaches the boundary
- **WHEN** exact current-generation File Count is 999 and then 1000 in successive valid generations
- **THEN** the Host admits the 999-file folder, rejects the 1000-file folder, and never launches the bundled tool for the rejected folder
