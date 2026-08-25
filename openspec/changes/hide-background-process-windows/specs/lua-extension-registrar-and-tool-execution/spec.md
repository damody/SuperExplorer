## ADDED Requirements

### Requirement: Invisible Windows extension tool processes
On Windows, the extension host, broker, worker, and worker-owned child processes SHALL use `CREATE_NO_WINDOW` for every internal tool launch and diagnostic probe while preserving the existing shell-free request, ProcessLease, Job Object, output-limit, cancellation, timeout, and terminal-result contracts.

#### Scenario: Lua tool completes successfully
- **WHEN** an authorized Lua callback runs a packaged console-subsystem tool
- **THEN** the tool creates no visible console and the callback receives its bounded output and successful terminal result

#### Scenario: Lua tool is cancelled with a child process
- **WHEN** a running packaged tool owns a child process and the Lua callback is cancelled
- **THEN** neither process shows a console, the Job Object terminates and reaps the full tree, and the callback receives the cancelled terminal result

#### Scenario: Extension diagnostic probe fails
- **WHEN** a broker or worker diagnostic probe cannot spawn or returns malformed output
- **THEN** no console becomes visible and the existing typed spawn or protocol failure is returned without an interactive fallback
