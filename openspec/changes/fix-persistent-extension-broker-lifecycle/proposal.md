## Why

Every brokered context-menu request currently starts a visible version-probe console, a one-shot broker, and a disposable worker before the menu can appear. This causes visible flashing and avoidable latency, while contradicting the roadmap's completed claims for persistent local IPC and app-owned broker lifecycle.

## What Changes

- Replace per-request broker/version processes with one non-blocking background-warmed, shared, authenticated broker session, retaining lazy initialization as the client fallback.
- Reuse bounded framed local IPC across requests and perform protocol/build/architecture handshake once per broker generation.
- Keep untrusted Shell activation inside disposable restricted workers supervised by Job Objects.
- Hide every broker and worker launch and build release helper binaries as Windows-subsystem executables.
- Add deterministic shutdown, timeout invalidation, restart, process reaping, health, and no-orphan behavior.
- Add cold/warm latency, process reuse, console-window, crash/hang recovery, and context-menu regression evidence.
- Correct the umbrella roadmap evidence so one-shot stdin/stdout execution is no longer accepted as completed lifecycle coverage.

## Capabilities

### New Capabilities

- `persistent-extension-broker-lifecycle`: Covers invisible persistent broker startup, authenticated handshake, session reuse, disposable worker supervision, recovery, shutdown, and performance/process evidence.

### Modified Capabilities

None. The umbrella change is still active; this focused capability tightens and verifies its existing broker requirements without changing public application commands.

## Impact

Affected areas include `explorer-extension-protocol`, `explorer-extension-broker`, `explorer-app` lifecycle/service routing, helper binary subsystem configuration, process-boundary tests, UITEST broker validation, packaging diagnostics, and the umbrella roadmap task/evidence status. No user-facing command or persisted-state schema is intentionally broken.
