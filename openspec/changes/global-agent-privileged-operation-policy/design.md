## Context

Codex loads `C:\Users\Damody\.codex\AGENTS.md` as personal guidance across projects. The file is currently empty. Project configuration already enables multi-agent work, but it does not define ownership when subagents encounter privileged operations or return incomplete progress.

Subagents normally inherit the parent turn's live permission mode, so permission separation alone cannot reliably make the primary agent the sole privileged executor. A role-aware coordination policy in global guidance is therefore the smallest durable solution.

## Goals / Non-Goals

**Goals:**

- Make the primary agent own privileged-operation decisions and execution.
- Make direct execution the default when targets, scope, and authorization are clear.
- Require subagents to return enough evidence for the primary agent to decide safely.
- Prevent incomplete delegated work or progress summaries from ending coordination.
- Apply the behavior across all projects.

**Non-Goals:**

- Grant new runtime permissions or pre-authorize unclear destructive operations.
- Override built-in agent definitions or models.
- Change concurrency, sandbox, approval, plugin, or MCP configuration.
- Guarantee enforcement beyond instruction precedence.

## Decisions

### Use global personal guidance

Write the policy to `C:\Users\Damody\.codex\AGENTS.md`. This is preferred over a project-local file because the requested behavior applies to all projects. It is preferred over duplicating the policy in every repository because a single source is easier to audit and maintain.

### Do not redefine `core_implementer`

Do not create a custom agent with the same name as a built-in role. A same-name custom agent would take precedence and could replace specialized built-in instructions. Global role-aware guidance preserves existing capabilities.

### Use a structured escalation contract

A subagent escalation contains necessity, exact action, exact targets, risks, and a safer alternative. This gives the primary agent enough information to validate and execute without another discovery round.

### Prefer direct primary-agent execution

When targets are verified, the action is within the user's requested scope, and current authorization is sufficient, the primary agent executes directly. It does not request redundant confirmation merely because an action is sensitive. It asks the user only for missing authority, unresolved target choices, or material decisions.

### Preserve completion ownership

The primary agent treats subagent results as inputs, not final completion. It inspects results, updates remaining work, continues actionable tasks, and verifies completion. While one operation is blocked, unrelated work continues.

## Risks / Trade-offs

- [Instruction-only enforcement can be superseded by higher-priority instructions] → State the precedence boundary explicitly and verify the guidance is loaded in a fresh task.
- [Broad direct-execution wording could be misread as blanket destructive authorization] → Require verified targets, in-scope action, and sufficient existing authorization before direct execution.
- [Global guidance may affect workflows that intentionally give workers autonomy] → Limit the restriction to destructive, credentialed, externally visible, or permission-sensitive operations.
- [A long global file consumes context in every task] → Keep the policy concise and operational.

## Migration Plan

1. Preserve any existing global guidance and append or merge the new policy without deleting unrelated instructions.
2. Read the resulting file back and scan for conflicting requirements or placeholders.
3. Validate instruction discovery in a fresh Codex invocation.
4. Roll back by removing only the added policy section if the behavior is undesirable.

## Open Questions

None.
