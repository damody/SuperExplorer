## MODIFIED Requirements

### Requirement: Lock-owner column example
`rust-lock-owner-column` SHALL use `LockOwnerQueryServiceV1` in a background batch provider, display one/multiple process names and details from genuine file-lock and directory-current-owner sources, provide manual refresh, use short TTL and update/clear through F5 generation without process-control capability. Its blocking Windows UTIT SHALL use the production plugin and host service with a real held file plus real native and WOW64 processes whose current directories are nested fixture folders. Its English and Traditional Chinese READMEs SHALL document directory ancestry, privacy-safe output, inaccessible-process false negatives, shared cancellation/deadline behavior, TTL/F5 refresh and an offline reproduction command.

#### Scenario: Lock appears and disappears
- **WHEN** a helper holds a file across one F5 and releases it before the next
- **THEN** the owner name appears then clears, and a late old-generation result cannot restore it

#### Scenario: Nested console owner projects to parent and clears
- **WHEN** the headful fixture launches `cmd.exe` in a nested directory while its parent row is visible
- **THEN** the production Lock owners column displays `cmd.exe` for the nested directory and the parent, captures required evidence, and clears both after the console exits or leaves the subtree and F5 completes

#### Scenario: Feature is disabled during delayed discovery
- **WHEN** the Lock owners feature is disabled while an older file-lock or current-directory query is pending
- **THEN** the column and value remain absent and the delayed result cannot restore either one

#### Scenario: Author reproduces directory ownership offline
- **WHEN** an author follows either maintained README in an offline clean environment
- **THEN** the documented locked build/package/test commands reproduce the example and explain the observable native/WOW64 ancestry and refresh limitations without claiming process-control capability
