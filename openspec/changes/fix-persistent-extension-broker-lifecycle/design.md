## Context

The app currently verifies the helper by launching `explorer-extension-broker.exe --version-json` and then starts another one-shot broker for every operation. That supervisor processes one frame, spawns one disposable worker, returns one frame, and exits. The version launch is visible and the three cold starts delay context menus. The active umbrella design already requires authenticated local IPC, a supervised broker lifecycle, disposable workers, typed failures, and no orphan processes, so this change corrects implementation drift rather than weakening isolation.

## Goals / Non-Goals

**Goals:**

- Reuse one invisible authenticated broker process across requests.
- Validate protocol/build/architecture once per broker generation through Hello/HelloAck.
- Preserve one restricted Job-owned worker per dangerous Shell operation.
- Terminate, reap, restart, and shut down deterministically without blocking GPUI.
- Prove no visible console, no redundant broker launch, bounded latency, and no orphan processes.

**Non-Goals:**

- Loading third-party Shell extensions in the app process.
- Keeping a handler-loaded worker alive after its request.
- Changing public context-menu, thumbnail, namespace, or preview commands.
- Creating a machine-wide broker service or globally discoverable IPC endpoint.

## Decisions

### 1. Persist an app-owned broker child over inherited anonymous pipes

All `BrokerClient` clones share one synchronized runtime holding the child, stdin/stdout, nonce, generation, and health. Inherited pipes are private local IPC handles with no global name or access-control surface. Existing length-delimited frames remain the transport contract. A named pipe remains a future interchangeable transport if cross-process attachment beyond the parent/child lifetime is required.

Alternatives rejected: a one-shot broker retains the current latency; a machine-wide named endpoint adds ACL, discovery, and stale-service complexity without current benefit.

### 2. Serialize app requests in the first persistent implementation

The shared runtime accepts one request at a time, assigns a unique request ID, writes one frame, and reads its correlated terminal. GPUI already dispatches broker work on background threads, so UI responsiveness is preserved. Serialization avoids response multiplexing bugs and is adequate for the bounded in-flight policy while measurements determine whether supervisor concurrency is necessary.

Alternative rejected: immediate multiplexing adds pending maps, reader lifetime races, cancellation routing, and interleaved output before the core lifecycle is proven.

The app starts the session on a named background warmup thread after process composition. The
handshake also prepares one clean restricted worker that has not loaded any handler. Each request
consumes that worker while the broker prepares its clean replacement concurrently. This preserves
handler disposal while removing broker and worker creation from the usual right-click critical path.
App shutdown joins the bounded warmup before shutting down the shared broker generation.

### 3. Handshake once per broker generation

Startup sends Hello containing expected build/role information under the session nonce. The broker returns HelloAck. The client rejects wrong protocol, build, architecture, role, nonce, or request ID before any Start request. `--version-json` remains diagnostic-only and is never called by normal request flow.

### 4. Keep workers disposable

For every Start, the persistent broker starts a new restricted worker with `CREATE_NO_WINDOW`, assigns it to a kill-on-close Job Object, enforces its deadline, emits one terminal, and drops all handler state. A worker crash or hang returns a typed terminal but leaves the broker loop alive for later requests.

### 5. Treat transport failure as generation failure

Timeout, EOF, malformed response, failed authentication, or child exit invalidates the whole session. The client kills and reaps the broker before a later request starts a fresh generation. Requests are not blindly replayed after their worker might have produced an external effect. Explicit Retry and a later independent operation can recover.

### 6. Hide helpers defensively

Every Windows `Command` path uses `CREATE_NO_WINDOW`, including diagnostic probes and test helpers. Non-debug helper binaries also declare the Windows subsystem while retaining redirected standard handles. Tests inspect top-level windows rather than assuming spawn flags are sufficient.

## Risks / Trade-offs

- [Serialized thumbnails wait behind a context menu] → retain bounded fallback/deadlines and add measured concurrency only after lifecycle correctness.
- [Broker dies with an active worker] → broker owns a kill-on-close Job Object and the app kills/reaps the failed broker generation.
- [Windows-subsystem helpers lose diagnostics] → diagnostics use redirected stdout/stderr and structured files; no interactive console is required.
- [A request times out after an external verb ran] → never auto-replay the same request; return one typed terminal and restart only for later work.
- [Other agents edit the same files] → reread touched regions before patches and preserve unrelated changes.

## Migration Plan

1. Add persistent-loop and handshake behavior while retaining existing protocol encoding.
2. Replace one-shot `BrokerClient` state with a shared runtime and explicit shutdown.
3. Wire app shutdown and health/retry to the session lifecycle.
4. Add process, console, latency, recovery, and regression tests.
5. Reopen incorrect umbrella task claims until the focused gate passes.

Rollback disables broker-backed routes and keeps filesystem navigation available; it does not restore unsafe in-process extension activation.

## Open Questions

None block implementation. Concurrent request multiplexing remains measurement-driven follow-up rather than part of this correctness fix.
