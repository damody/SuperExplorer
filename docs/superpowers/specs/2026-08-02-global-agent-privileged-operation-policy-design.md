# Global Agent Privileged-Operation Policy

## Goal

Add durable personal guidance so every Codex project uses the primary agent as
the decision and execution point for permission-sensitive or high-risk work.
Subagents remain responsible for bounded implementation and analysis, but they
must escalate privileged operations to the primary agent instead of executing
them or ending the task while waiting for the user.

## Scope

The policy will be written to `C:\Users\Damody\.codex\AGENTS.md`, which is the
global personal guidance file. It applies across repositories unless a higher
priority instruction explicitly conflicts with it.

This change will not:

- replace or redefine built-in agent roles such as `core_implementer`;
- change model selection, concurrency, sandbox, or approval settings;
- grant authority that the user or runtime has not already granted;
- make destructive operations implicitly authorized.

## Policy Design

### Primary-agent ownership

The primary agent owns the final outcome. After receiving a subagent result, it
must inspect the result, update the remaining work, and continue with the next
actionable step. A progress report or an incomplete checklist is not a valid
completion state.

### Subagent escalation contract

Subagents must not execute destructive, credentialed, externally visible, or
permission-sensitive operations. When such an operation appears necessary, the
subagent returns a structured request containing:

1. why the operation is necessary;
2. the exact command or action;
3. the exact affected targets;
4. risks and expected impact;
5. a safer or reversible alternative when one exists.

Representative operations include recursive deletion, force operations,
publishing, pushing, modifying external services, installing system software,
changing security settings, and using credentials.

### Primary-agent decision flow

The primary agent independently validates the request and prefers a
non-destructive or reversible alternative. If the operation is safe and within
existing authorization, the primary agent executes it directly. If additional
user authority or a material user decision is genuinely required, only the
primary agent asks the user.

While an operation is awaiting a decision, the primary agent continues all
unrelated work that is not blocked. It must not delegate the privileged action
back to a subagent.

### Authorization boundary

This is a coordination policy, not a permission escalation mechanism. The
primary agent cannot approve on the user's behalf, broaden runtime permissions,
or infer authorization for unclear destructive targets. Runtime, system,
developer, and explicit user instructions retain precedence.

## Verification

After updating the global guidance:

1. Read the file back and confirm the policy is present without placeholders or
   conflicting requirements.
2. Start a fresh Codex task or run an instruction-summary command and confirm the
   global guidance is loaded.
3. In a harmless test prompt, ask a subagent to propose a simulated privileged
   operation and confirm that the primary agent receives the structured request
   and owns the decision.

## Expected Result

Across all projects, subagents escalate privileged operations to the primary
agent, and the primary agent keeps coordinating until the task is verified or a
genuine user decision is required.
