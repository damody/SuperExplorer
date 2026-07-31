## ADDED Requirements

### Requirement: Invisible persistent supervisor session
The application SHALL start at most one compatible broker supervisor generation without blocking the UI, SHALL reuse it across independent broker requests, and SHALL NOT display a console or helper window. The app MAY warm the session in the background, while the shared client SHALL retain lazy initialization as a race-safe fallback.

#### Scenario: Warm context-menu request
- **WHEN** a second context-menu request is issued after a successful first request
- **THEN** the application SHALL reuse the same broker process, SHALL create only a fresh disposable worker, and SHALL show no console window

### Requirement: Generation-scoped authenticated handshake
Each broker generation MUST complete one authenticated version/build/architecture/role handshake before accepting extension work, and normal requests MUST NOT launch a separate version-probe process.

#### Scenario: Compatible startup
- **WHEN** the adjacent broker starts with the expected protocol, build, architecture, role, and session nonce
- **THEN** one Hello/HelloAck exchange SHALL authorize subsequent bounded Start frames on that session

#### Scenario: Incompatible startup
- **WHEN** any handshake identity or authentication field is invalid
- **THEN** the application SHALL terminate and reap that broker without activating a worker and SHALL report typed version or protocol unavailability

### Requirement: Disposable worker supervision
Every dangerous Shell operation SHALL run in a fresh restricted worker owned by a kill-on-close Job Object while the broker supervisor remains reusable.

#### Scenario: Sequential successful requests
- **WHEN** two operations complete through one broker generation
- **THEN** they SHALL use distinct worker processes and the first worker SHALL be reaped before its result is accepted

#### Scenario: Worker hangs or crashes
- **WHEN** a worker exceeds its deadline or exits abnormally
- **THEN** the broker SHALL terminate and reap that worker, emit one correlated terminal, and remain able to accept a later request

### Requirement: Deterministic recovery and shutdown
Transport failure SHALL invalidate and reap the affected broker generation, and application shutdown SHALL close the session without leaving broker or worker processes.

#### Scenario: Broker disconnect
- **WHEN** the active broker exits or returns malformed or unauthenticated data
- **THEN** the client SHALL invalidate and reap it, SHALL NOT replay a possibly effectful request, and a later independent request SHALL be able to start a new generation

#### Scenario: Application shutdown
- **WHEN** the final application lifecycle shuts down
- **THEN** it SHALL send bounded Shutdown when possible and otherwise kill and reap the broker and all Job-owned workers

### Requirement: Non-blocking bounded context-menu latency
Broker process management and Shell extension work MUST remain off the GPUI thread, and warm context-menu requests SHALL avoid version-probe and supervisor cold-start latency.

#### Scenario: Cold and warm measurement
- **WHEN** the controlled context-menu fixture is invoked twice
- **THEN** evidence SHALL record cold and warm latency, broker/worker process identities, zero visible consoles, and warm execution with no additional broker launch

### Requirement: Truthful lifecycle evidence
Completion evidence MUST include protocol tests, process-boundary reuse and recovery tests, console-window inspection, process census, context-menu behavior, quality gates, and rollback documentation.

#### Scenario: Focused capability gate
- **WHEN** the persistent broker lifecycle is claimed complete
- **THEN** all required evidence SHALL pass without treating one-shot stdin/stdout execution or hidden-window assumptions as persistent lifecycle coverage
