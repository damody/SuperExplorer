## 1. Correct Contracts and Baseline

- [x] 1.1 Record the one-shot version/broker/worker launch baseline and reopen inaccurate umbrella lifecycle/IPC task claims.
- [x] 1.2 Add lifecycle configuration and test diagnostics for broker generation, PID, worker PID, launches, handshakes, requests, restarts, and shutdowns.
- [x] 1.3 Add protocol fixtures for persistent multi-frame Hello, Start, Terminal, and Shutdown exchanges.

## 2. Persistent Broker Supervisor

- [x] 2.1 Change the supervisor from one-frame EOF processing to a bounded authenticated multi-frame loop.
- [x] 2.2 Implement Hello/HelloAck compatibility data and reject Start before a valid handshake.
- [x] 2.3 Preserve a fresh restricted Job-owned worker per Start and keep the supervisor alive after worker success, crash, or timeout.
- [x] 2.4 Implement authenticated Shutdown, pipe-EOF cleanup, and deterministic worker/process reaping.
- [x] 2.5 Apply `CREATE_NO_WINDOW` to every helper spawn and Windows subsystem configuration to release broker/worker binaries.

## 3. Shared App Client Lifecycle

- [x] 3.1 Replace clone-by-value one-shot client state with one shared synchronized broker runtime.
- [x] 3.2 Implement lazy client initialization/background warmup, one handshake per generation, unique request IDs, bounded framed request/response reuse, and correlation checks.
- [x] 3.3 Implement timeout/disconnect/protocol invalidation, kill/reap, later-generation recovery, and no automatic replay of effectful requests.
- [x] 3.4 Implement explicit idempotent client shutdown and connect it to final app lifecycle ordering.
- [x] 3.5 Update broker health/retry UI wiring to reflect the live shared session without spawning diagnostic version processes per request.

## 4. Context Menu and Regression Coverage

- [x] 4.1 Add process-boundary tests proving multiple requests reuse one broker PID and use distinct reaped workers.
- [x] 4.2 Add crash, hang, malformed response, incompatible handshake, restart, repeated shutdown, and orphan-census tests.
- [x] 4.3 Add Windows top-level-window inspection proving version, broker, and worker paths create no visible console.
- [x] 4.4 Add controlled cold/warm context-menu latency evidence proving warm requests launch no new broker and preserve menu effects/cancel behavior.
- [x] 4.5 Update UITEST broker validation, required artifacts, diagnostics, rollback notes, and Explorer behavior evidence.

## 5. Capability Gate

- [x] 5.1 Run formatting, locked checks, Clippy warnings denied, protocol/broker/app tests, process-boundary tests, and relevant doc tests.
- [x] 5.2 Run focused quick/interop/soak validation and inspect process census, console evidence, latency, terminal balance, and artifacts.
- [x] 5.3 Mark focused and umbrella tasks complete only when persistent reuse, recovery, shutdown, no-console, and no-orphan evidence all pass.
