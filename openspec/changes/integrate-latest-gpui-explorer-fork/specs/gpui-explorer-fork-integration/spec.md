## ADDED Requirements

### Requirement: Fork is based on the latest observed upstream main
The integration SHALL record the upstream `main` object ID observed immediately before integration and SHALL build the fork candidate from that commit without copying or rewriting unrelated upstream source history.

#### Scenario: Upstream refresh starts
- **WHEN** the integration begins
- **THEN** the process SHALL fetch `gpui-ce/gpui-ce` and record the fetched `main` commit
- **AND** the candidate branch SHALL contain that commit as an ancestor

### Requirement: Explorer extensions remain isolated and functional
The fork SHALL preserve the Explorer-required editable-text pointer selection, selected-glyph clipping, single-line selection height, accessibility interfaces, and native Windows external-drop negotiation used by SuperExplorer.

#### Scenario: Explorer commits are replayed
- **WHEN** the custom commits are applied to the new upstream base
- **THEN** each concern SHALL remain represented by an auditable fork commit
- **AND** conflicts SHALL be resolved without deleting the public APIs consumed by SuperExplorer

### Requirement: Fork candidate passes host compatibility gates
The fork SHALL NOT be published until its relevant crates compile and SuperExplorer resolves exclusively to the candidate path crates and passes its locked explorer-ui tests and application build under Rust 1.97.1 for `x86_64-pc-windows-msvc`.

#### Scenario: Candidate is compatible
- **WHEN** all fork and host validation commands complete successfully
- **THEN** the candidate SHALL be eligible for publication

#### Scenario: Candidate breaks the host
- **WHEN** any required fork check, explorer-ui test, or explorer-app build fails
- **THEN** the candidate SHALL remain unpublished until corrected

### Requirement: Publication never rewrites remote history
The integration SHALL publish to `damody/gpui-ce-explorer.git` only as a normal fast-forward update from the fetched remote `main` tip.

#### Scenario: Remote main is unchanged
- **WHEN** the validated candidate descends from the current remote main
- **THEN** the process SHALL push the candidate to remote main without force

#### Scenario: Remote main advanced concurrently
- **WHEN** the final remote fetch reveals commits not contained by the candidate
- **THEN** the process SHALL stop publication, integrate the new tip, and repeat validation

### Requirement: Parent repository records the validated fork revision
The SuperExplorer repository SHALL reference the exact published fork commit through its `vendor/gpui-ce` submodule and SHALL use matching path dependencies and a regenerated lockfile.

#### Scenario: Fork push succeeds
- **WHEN** the fork candidate is published successfully
- **THEN** the parent submodule pointer SHALL equal the published commit
- **AND** the parent repository SHALL build without resolving the incompatible crates.io GPUI package
