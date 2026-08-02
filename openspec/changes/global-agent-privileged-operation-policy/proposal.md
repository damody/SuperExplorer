## Why

Subagents can currently treat permission-sensitive work or an incomplete progress report as a stopping point, leaving the user to restart coordination manually. Global guidance is needed so the primary agent owns privileged decisions, direct execution, integration, and verified completion across every project.

## What Changes

- Add global personal agent guidance that makes the primary agent the decision and execution point for privileged operations.
- Require subagents to escalate sensitive operations with exact targets, commands, risks, and safer alternatives instead of executing them.
- Make direct primary-agent execution the default when the action is in scope, targets are verified, and current authorization is sufficient.
- Require the primary agent to continue unrelated work while a genuine decision remains blocked.
- Define progress reports and incomplete delegated work as non-terminal states.
- Preserve runtime authorization boundaries and existing built-in agent definitions.

## Capabilities

### New Capabilities

- `global-agent-coordination-policy`: Defines primary-agent ownership, privileged-operation escalation, direct execution, and completion behavior for all projects.

### Modified Capabilities

None.

## Impact

- Updates the personal Codex guidance file at `C:\Users\Damody\.codex\AGENTS.md`.
- Adds no code dependencies and changes no application APIs, models, concurrency limits, or sandbox settings.
- Affects coordination behavior in newly started Codex tasks that load the global guidance.
