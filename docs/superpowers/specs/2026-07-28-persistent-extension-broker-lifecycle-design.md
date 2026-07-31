# Persistent Extension Broker Lifecycle Design

## Problem

The current client performs `--version-json`, starts a one-shot broker, and lets that broker start a disposable worker for every request. The version probe is a console process without `CREATE_NO_WINDOW`, so a visible console flashes before a context menu. Three cold process launches plus COM activation delay the menu. The implementation therefore does not satisfy the roadmap's persistent supervisor and lifecycle claims.

## Decision

Keep the security boundary but separate supervisor and worker lifetimes:

```text
explorer-app
  -> one shared authenticated broker session
       -> one disposable restricted worker per dangerous operation
```

The app starts the adjacent broker on a background warmup thread and owns its stdin/stdout handles. These inherited anonymous pipes are the persistent bounded local IPC transport. They expose no globally addressable endpoint, preserve the existing framed protocol, and are replaceable by named pipes without changing request types. A per-session nonce authenticates every frame. A Hello/HelloAck handshake validates protocol, build, architecture, and role exactly once per broker generation. The handshake also prepares one clean restricted worker; each request consumes it while the broker prepares a clean replacement concurrently, so no handler-loaded worker is reused.

## Client Lifecycle

All `BrokerClient` clones share one synchronized runtime. The runtime owns the child, pipe handles, nonce, generation, and health. Requests use unique IDs and framed writes. The background warmup normally starts and handshakes before the first request; a request can safely perform the same initialization if warmup has not finished. Later requests reuse the same broker. Transport failure, protocol failure, or timeout terminates and reaps that generation. One bounded retry may start a fresh generation only when no extension outcome could have been accepted. Explicit shutdown sends `Shutdown`, waits briefly, then kills and reaps if necessary. App shutdown first joins the bounded warmup and then closes the broker, so Drop and app shutdown cannot leave an orphan.

The first implementation serializes broker requests at the shared session boundary. Context-menu work is already asynchronous relative to GPUI, and serialization prevents response confusion while removing all redundant broker launches. Worker concurrency can be added behind the same protocol after measured need; it is not required to fix the visible regression.

## Supervisor and Worker Lifecycle

The broker reads multiple frames until authenticated Shutdown or pipe EOF. Hello is answered without extension activation. Each Start request launches a fresh restricted worker in a kill-on-close Job Object, waits within the worker deadline, returns exactly one terminal frame, and discards the worker. Worker crash or hang does not end the supervisor. Unknown, replayed, oversized, or unauthenticated frames end the session without activation.

Every broker and worker spawn path uses `CREATE_NO_WINDOW`. Release broker binaries also use the Windows subsystem while retaining redirected standard handles. Direct diagnostic/version output remains available when stdout is explicitly redirected.

## Context Menu Behavior

Right-click dispatch remains off the GPUI thread. A warm request performs only one IPC exchange plus one disposable worker launch and COM query. The visible menu remains owned by the worker for the complete `IContextMenu2/3` session. Cancellation, timeout, owner HWND, submenu, owner-draw, command invocation, and resulting Shell effects retain their existing contracts.

## Verification

- Unit tests cover lifecycle state transitions, handshake validation, request IDs, timeout invalidation, and idempotent shutdown.
- Process-boundary tests assert two requests reuse one broker PID while workers are replaced.
- Crash/hang tests assert the broker or session recovers and subsequent requests succeed.
- Windows tests inspect top-level windows and process census to prove no console is visible and no broker/worker remains after shutdown.
- A context-menu cold/warm benchmark records time to menu availability; warm execution must avoid version and broker process launches.
- Existing protocol, context-menu differential, Clippy, and UITEST gates remain required.

## Rollback

If persistent startup or handshake fails, broker-backed features report typed unavailable state while filesystem navigation continues. Rollback disables broker routes; it never restores unsafe third-party in-process activation.
