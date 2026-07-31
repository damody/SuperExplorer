## ADDED Requirements

### Requirement: Script manager controls
The UI SHALL list discovered scripts and provide enable, disable, reload, external-editor launch, activation mode, current state, and source diagnostics.

#### Scenario: User opens script externally
- **WHEN** the user invokes edit for a discovered script
- **THEN** the configured external editor opens the exact script path without an embedded editor being required

### Requirement: Non-destructive UI overrides
The manager SHALL persist overrides for watch roots/filters, dispatch, queue/concurrency limits, timeout, schedules, and summary mode without rewriting Lua source.

#### Scenario: Watch root is overridden
- **WHEN** the user changes a script watch root in the manager
- **THEN** the enabled runtime uses the override and the original Lua file remains byte-for-byte unchanged

### Requirement: Task and error visibility
The manager SHALL expose bounded current/recent task state, duration, safe output summaries, reload errors, overload, timeout, cancellation, and structured failure kinds.

#### Scenario: Handler fails
- **WHEN** a task raises a Lua or host error
- **THEN** the manager identifies the script, handler, correlation ID, safe message, and source location while other tasks continue

### Requirement: AI-oriented documentation package
The system SHALL ship `AI_LUA_CONTEXT.md`, `AI_PROMPT_TEMPLATE.md`, `API_REFERENCE.md`, `EVENT_CATALOG.json`, EmmyLua type stubs, and runnable examples declaring `explorer-automation/v1`.

#### Scenario: External AI receives the context file
- **WHEN** the single AI context document is provided to an external model
- **THEN** it contains the complete supported API surface, event/task rules, forbidden APIs, errors, and representative valid scripts without requiring hidden application context

### Requirement: Documentation-runtime consistency
CI SHALL parse/register every example and SHALL generate or verify documented signatures and event schemas against runtime typed definitions.

#### Scenario: API changes without documentation
- **WHEN** a runtime signature or event schema changes without the generated documentation/type artifacts changing
- **THEN** the documentation consistency gate fails

### Requirement: Trusted-script warnings
The manager SHALL explain that global input/clipboard subscriptions and CLI/AI calls can expose sensitive data and that arbitrary executable deletion behavior cannot be proven, without adding permission prompts beyond deletion confirmation.

#### Scenario: Sensitive capabilities first detected
- **WHEN** a script first registers global input/clipboard or invokes CLI/AI
- **THEN** the manager records and displays the applicable trust warning while allowing execution under the approved trusted-script policy
