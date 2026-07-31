## ADDED Requirements

### Requirement: OpenSpec requirements are the coverage source of truth
Runner SHALL scan every non-archived change spec and assign a stable identity to every `### Requirement:` block. A manifest with any uncovered requirement, zero-match selector, duplicate case identity, unknown suite, or invalid timeout MUST fail validation.

#### Scenario: Newly added requirement has no test mapping
- **WHEN** a developer adds an OpenSpec requirement without updating the test manifest
- **THEN** the coverage gate fails and names the exact uncovered requirement

#### Scenario: Every requirement is covered
- **WHEN** all discovered requirements match one or more enabled test cases
- **THEN** coverage output lists every requirement and all mapped case identities

### Requirement: Test cases are manifest-driven and reproducible
Each case SHALL define stable id, description, suites, command, timeout, prerequisites, exclusive resources, coverage selectors and evidence behavior. The runner SHALL expose list, suite filter, case filter and failed-case rerun commands without changing the manifest.

#### Scenario: List cases without executing
- **WHEN** the user invokes the runner with `--list`
- **THEN** it prints selected cases, suites, prerequisites, timeout and expanded requirement count without starting subprocesses

#### Scenario: Rerun one failure
- **WHEN** a report contains a failed case
- **THEN** the report includes an executable command that selects only that case

### Requirement: Layered regression suites cover product risk
Runner SHALL provide quick, full, interop, visual and soak suites. Full SHALL exercise real GPUI headful behavior; interop SHALL exercise Clipboard, OLE drag-and-drop, Shell context menus, real file operations and search; visual SHALL keep baselines read-only; soak SHALL exercise large datasets and resource-leak oracles.

#### Scenario: Quick developer gate
- **WHEN** no suite is specified
- **THEN** runner executes the quick suite without requiring interactive desktop or destructive external state

#### Scenario: Complete release gate
- **WHEN** full, interop, visual and soak are selected with `--fail-on-skip`
- **THEN** every selected case either passes or makes the run fail

### Requirement: Windows UI and interop cases are isolated
Headful, cursor, Clipboard, OLE, context-menu and Explorer cases SHALL execute serially with declared exclusive resources. Every mutating case MUST use a runner-owned fixture root and verified containment cleanup. Timeout SHALL terminate the full child process tree and report the terminal reason.

#### Scenario: Headful case times out
- **WHEN** a UI case exceeds its declared timeout
- **THEN** runner terminates its process tree, marks TIMEOUT, preserves logs and continues according to fail-fast policy

#### Scenario: Fixture cleanup target escapes owned root
- **WHEN** a case resolves a cleanup target outside the run fixture root
- **THEN** cleanup is refused and the case fails without deleting the target

### Requirement: Missing prerequisites are truthful
Runner SHALL evaluate Windows version, interactive desktop, commands, paths/drives, environment opt-ins and profile requirements before execution. Missing prerequisites SHALL produce SKIP with a concrete reason and MUST NOT be counted as PASS. `--fail-on-skip` SHALL make the overall run fail.

#### Scenario: D drive is unavailable
- **WHEN** a cross-drive case requires `D:\` but the drive does not exist
- **THEN** the case is SKIP with `missing path D:\` unless fail-on-skip makes the run fail

### Requirement: Reports are complete and machine-readable
Every run SHALL atomically produce versioned JSON, JUnit XML, Markdown summary and requirement coverage JSON. Each case result SHALL include status, duration, command, exit code or terminal reason, stdout/stderr paths, artifacts, mapped requirements and rerun command.

#### Scenario: Mixed pass fail and skip results
- **WHEN** selected cases include PASS, FAIL and SKIP
- **THEN** all formats report the same counts and identities and the process exits nonzero

### Requirement: The runner verifies itself
The runner SHALL include unit and integration tests for manifest parsing, path normalization, OpenSpec extraction, selector expansion, coverage failure, prerequisite evaluation, timeout cleanup, XML escaping and report consistency. Production crates SHALL NOT depend on the runner.

#### Scenario: Architecture audit
- **WHEN** dependency architecture is checked
- **THEN** explorer-app and production libraries have no dependency path to explorer-uitest
