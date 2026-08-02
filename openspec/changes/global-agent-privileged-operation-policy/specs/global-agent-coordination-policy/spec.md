## ADDED Requirements

### Requirement: Primary agent owns privileged-operation decisions
The primary agent SHALL independently validate privileged-operation requests from subagents and SHALL own the decision and execution path.

#### Scenario: Authorized in-scope operation
- **WHEN** a subagent proposes a permission-sensitive operation whose targets are verified, whose action is within the user's requested scope, and whose current authorization is sufficient
- **THEN** the primary agent executes the operation directly without redundant user confirmation

#### Scenario: Genuine user decision required
- **WHEN** an operation lacks required authority, has unresolved target choices, or requires a material user decision
- **THEN** only the primary agent asks the user for the missing decision or authority

### Requirement: Subagents escalate privileged operations
Subagents MUST NOT execute destructive, credentialed, externally visible, or permission-sensitive operations and SHALL return a structured request to the primary agent.

#### Scenario: Subagent identifies a privileged operation
- **WHEN** a subagent determines that a privileged operation is necessary
- **THEN** it reports the necessity, exact command or action, exact affected targets, risks, expected impact, and a safer or reversible alternative when available

### Requirement: Authorization boundaries remain intact
The coordination policy MUST NOT grant new permissions, approve on the user's behalf, or infer authorization for unclear destructive targets.

#### Scenario: Existing authorization is insufficient
- **WHEN** the runtime or user has not granted the authority required for an operation
- **THEN** the primary agent does not execute the operation until the required authority is obtained

### Requirement: Primary agent maintains task liveness
The primary agent SHALL treat delegated results as inputs rather than completion and SHALL continue coordinating until completion is verified or a genuine blocker requires user input.

#### Scenario: Subagent returns incomplete progress
- **WHEN** a subagent reports that tests, configuration removal, integration, or another required item remains unfinished
- **THEN** the primary agent updates the remaining checklist and immediately continues by assigning or performing the next actionable work

#### Scenario: One operation is blocked
- **WHEN** one privileged operation is waiting for a genuine user decision
- **THEN** the primary agent continues all unrelated work that is not blocked

### Requirement: Built-in agent capabilities remain unchanged
The policy SHALL apply through global guidance without redefining built-in agent roles or changing model, concurrency, sandbox, or approval configuration.

#### Scenario: Global policy is installed
- **WHEN** the coordination policy is added for all projects
- **THEN** existing built-in agent definitions and runtime configuration remain unchanged
