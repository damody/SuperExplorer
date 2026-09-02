## Context

The approved source design is `docs/superpowers/specs/2026-09-02-remote-cancel-fluent-progress-design.md`. Remote workers receive request tokens, but Cancel currently falls through to the inner Shell registry which cannot find them.

## Goals / Non-Goals

**Goals:** cancel the actual remote token, guarantee cleanup and source safety, and align progress visuals with Windows Fluent.

**Non-Goals:** pause/resume, ETA, speed reporting, ABI changes.

## Decisions

- `RemoteExplorerService` owns an active remote request map because it owns the workers and their tokens.
- Registration happens before spawn and a guard removes entries on every exit; unknown Cancel delegates to Shell.
- Fluent progress is a contained rounded track with rounded determinate fill or a shorter indeterminate segment.
- Terminal/cancellation races remain first-terminal-wins; cancelled ratios use real bytes.

## Risks / Trade-offs

- **Stale registry entry** → RAII terminal cleanup plus tests for success, failure, and cancellation.
- **Cancel arrives before worker starts** → register before spawn; worker observes the already-cancelled token.
- **Provider call is temporarily non-interruptible** → SFTP races await against the token and ADB kills its owned child.
- **Narrow layout clips text** → right region remains `min_w_0` and the track stays inside it.
