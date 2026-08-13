## ADDED Requirements

### Requirement: Host-prepared official Code Lines directory snapshots
The Host SHALL prepare Rust and Lua Code Lines directory snapshots only from regular, non-symlink files that the workspace-locked tokei path classifier recognizes, SHALL keep the complete `SECLDIR1` snapshot within `MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1`, and SHALL preserve the sealed Host-attested stream boundary.

#### Scenario: Binary-heavy repository contains supported source
- **WHEN** an admitted folder contains supported source files together with repository metadata, images, executables, archives, or other unrecognized files
- **THEN** the Host packs only recognized source files and both official Code Lines providers receive a dispatchable snapshot whose counts exclude unsupported payloads

#### Scenario: All-language Code Lines uses the Host classifier
- **WHEN** the Lua Code Lines provider receives a file or directory snapshot containing any language recognized by the workspace-locked tokei classifier
- **THEN** it uses tokei parsing for that language and includes its exact code, comment, and blank counts in the all-language total

#### Scenario: Supported source exceeds the single-stream limit
- **WHEN** the complete framed snapshot of recognized source would exceed `MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1`
- **THEN** that folder is reported as `Unsupported source` and no oversized stream is submitted

#### Scenario: Directory contains no recognized source
- **WHEN** an admitted folder contains no regular file recognized by the locked tokei classifier
- **THEN** the folder is reported as `Unsupported source` rather than zero or a batch preparation failure

#### Scenario: One row cannot be prepared
- **WHEN** canonicalization, filename extraction, or Host stream construction fails for one row in a batch containing other valid rows
- **THEN** only that row receives a terminal preparation error and every valid row remains eligible for provider dispatch

#### Scenario: Directory snapshot remains within authority boundaries
- **WHEN** either official provider analyzes an admitted folder
- **THEN** it receives only the existing Host-owned `SECLDIR1` input stream and receives no new path, filesystem, process, or network authority
