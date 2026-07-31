## ADDED Requirements

### Requirement: Authenticated versioned broker protocol
The application and broker SHALL communicate through a versioned, authenticated, bounded local IPC protocol with explicit request, progress, cancellation, and exactly-one terminal messages.

#### Scenario: Invalid protocol peer
- **WHEN** a peer supplies an invalid secret, unsupported version, unknown message type, oversized frame, or illegal state transition
- **THEN** the endpoint SHALL reject and close that session without executing an extension or destabilizing other sessions

### Requirement: Disposable worker isolation
Potentially blocking or untrusted extension, codec, preview, and provider activation SHALL run in disposable worker processes supervised separately from the GPUI process and primary Shell STA.

#### Scenario: Extension permanently hangs
- **WHEN** a worker fails to finish before its deadline and cancellation grace period
- **THEN** the supervisor SHALL terminate the worker Job Object, emit one timeout terminal, and accept later work in a fresh worker

#### Scenario: Extension crashes
- **WHEN** an extension crashes its worker process
- **THEN** the main application SHALL remain running, receive one correlated crash terminal, release session UI, and keep unrelated broker jobs usable

### Requirement: Least-authority execution
Broker workers SHALL run with only the token privileges, handles, item descriptors, filesystem access, UI ownership, memory, CPU, and child-process authority required by the requested operation.

#### Scenario: Worker requests undeclared authority
- **WHEN** a worker attempts an operation outside its assigned capability or Job Object policy
- **THEN** Windows or the broker SHALL deny it and record redacted correlated diagnostics

### Requirement: Correlation, cancellation, and stale isolation
Every broker request SHALL carry request, tab, generation, handler, deadline, and cancellation identity, and late or duplicate messages SHALL never mutate current UI/model state.

#### Scenario: Tab closes during broker work
- **WHEN** a tab closes while a worker is producing a thumbnail, namespace batch, context menu, or preview
- **THEN** the request SHALL cancel, its terminal SHALL be idempotent, and all later messages from that generation SHALL be ignored

### Requirement: Handler quarantine and recovery
The broker SHALL maintain bounded failure/backoff state by handler identity and operation class, automatically quarantine repeated crash/hang offenders, and expose a user-controlled retry/reset path.

#### Scenario: Repeatedly crashing handler
- **WHEN** the same handler crosses the configured crash threshold
- **THEN** subsequent automatic activations SHALL use safe fallback until quarantine expires or the user explicitly retries

### Requirement: Broker lifecycle and packaging
The finalized application and installer SHALL include compatible signed/versioned broker binaries, verify their architecture/version at startup, supervise clean shutdown, and fail safely when the broker is missing or incompatible.

#### Scenario: Broker binary is missing or wrong version
- **WHEN** a broker-required feature is invoked without a compatible executable
- **THEN** the feature SHALL show a recoverable unavailable state, diagnostics SHALL identify the packaging problem, and filesystem navigation SHALL continue

### Requirement: Brokered Shell interaction
Context menus, untrusted thumbnail/preview codecs, slow namespace extensions, and other designated Shell providers SHALL migrate to broker routes without losing owner-window, message, command, progress, or result semantics supported by public APIs.

#### Scenario: Brokered context menu invocation
- **WHEN** a compatible third-party context menu is queried and invoked
- **THEN** submenu, owner-draw/message forwarding, command identity, terminal status, and resulting filesystem/Shell effects SHALL match the existing safe behavior

### Requirement: Resource budgets and observability
The broker SHALL enforce and report bounded message size, queue depth, worker count, wall time, CPU, memory, handles, restarts, crashes, timeouts, quarantines, and terminal balance.

#### Scenario: Broker soak with mixed failures
- **WHEN** normal, slow, reentrant, oversized, hung, crashing, and unload-failing fixtures run repeatedly
- **THEN** the app SHALL remain responsive, budgets SHALL hold, workers SHALL be reclaimed, and every accepted request SHALL have exactly one terminal outcome

### Requirement: Privacy-safe diagnostics
Broker logs and crash reports SHALL contain sufficient protocol, handler, version, and correlation data for diagnosis while excluding credentials, file contents, and unnecessary full sensitive paths.

#### Scenario: Broker failure on sensitive path
- **WHEN** a controlled handler crashes while processing an item beneath a configured sensitive root
- **THEN** exported diagnostics SHALL identify the handler and failure class without exposing the sensitive root or content
