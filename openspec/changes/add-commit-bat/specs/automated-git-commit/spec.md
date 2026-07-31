## ADDED Requirements

### Requirement: Non-interactive Codex launch
The batch entry point SHALL launch Codex CLI in non-interactive execution mode from the repository root using `gpt-5.3-codex-spark` with low reasoning effort.

#### Scenario: Launch from another directory
- **WHEN** a user invokes `commit.bat` while the shell current directory differs from the repository root
- **THEN** Codex executes with the directory containing `commit.bat` as its working root

### Requirement: Preserve existing project content
The embedded instruction MUST prohibit Codex from modifying existing project files while preparing commits.

#### Scenario: Existing unstaged changes are analyzed
- **WHEN** Codex reviews the current working tree
- **THEN** it only stages and commits eligible existing changes without editing their contents

### Requirement: Functional Chinese commits
The embedded instruction SHALL require changes to be grouped by functional relationship into separate commits with Chinese subjects and detailed commit bodies.

#### Scenario: Unrelated changes exist
- **WHEN** the working tree contains changes serving different functions
- **THEN** Codex creates separate Chinese commits whose bodies explain each functional group

### Requirement: Generated artifacts are excluded
The embedded instruction MUST exclude temporary files and outputs created by compilation, builds, tests, or development tools from commits.

#### Scenario: Build artifacts coexist with source changes
- **WHEN** generated artifacts and valid source changes are both present
- **THEN** Codex leaves the generated artifacts uncommitted and commits only valid project changes

### Requirement: Submodule-aware commit and push
The embedded instruction SHALL require Codex to inspect and commit eligible submodule changes, record resulting submodule pointer changes in the parent repository, and push every affected branch in a valid dependency order.

#### Scenario: A submodule contains changes
- **WHEN** an initialized submodule has eligible uncommitted changes
- **THEN** Codex commits and pushes the submodule before committing and pushing the updated pointer in the parent repository

### Requirement: Observable execution result
The batch entry point SHALL return the Codex CLI exit code and display whether execution completed or failed.

#### Scenario: Codex CLI fails
- **WHEN** Codex exits with a non-zero status
- **THEN** the batch file displays a failure message and exits with the same non-zero status
