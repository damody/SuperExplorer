## 1. Launch coordination foundation

### 1.1 Versioned IPC contract and launch classification

**目的：** Define a bounded internal contract and determine which invocations participate.
**輸入：** Approved proposal, design, and repeated-launch specification.
**產出：** `explorer-app` coordination module and focused unit tests.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** G1 protocol/classification tests; `evidence/index.jsonl`.
**完成門檻：** Valid commands round-trip; invalid inputs reject; special launches bypass.

- [ ] 1.1.1 Add the versioned bounded `OpenWindowV1` command codec and validation.
- [ ] 1.1.2 Add ordinary-versus-isolated launch classification for arguments and test environment.
- [ ] 1.1.3 Run focused protocol and classification tests and record G1 evidence.

### 1.2 Per-user resident election and transport

**目的：** Elect one resident and reliably deliver acknowledged launch requests.
**輸入：** Work package 1.1 contract and current Win32 dependency surface.
**產出：** SID-scoped mutex/pipe client-server lifecycle with bounded waits.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** G2 transport tests; `evidence/index.jsonl`.
**完成門檻：** Election races have one owner, valid delivery is acknowledged, failures fall back.

- [ ] 1.2.1 Implement current-user endpoint identity and restrictive pipe security.
- [ ] 1.2.2 Implement resident election, pipe listener, bounded client retry, acknowledgment, and shutdown.
- [ ] 1.2.3 Test election, delivery, malformed input recovery, timeout fallback, and clean shutdown; record G2 evidence.

## 2. GPUI multi-window integration

### 2.1 Reusable explorer-window construction

**目的：** Make initial and relaunch explorer windows share one composition path.
**輸入：** Existing `ApplicationLifecycle::run_gpui` window closure and 1.2 command receiver.
**產出：** Reusable main-window factory/context in `application.rs`.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** G3 application/UI tests; `evidence/index.jsonl`.
**完成門檻：** Initial restoration is unchanged and fresh windows can be constructed independently.

- [ ] 2.1.1 Extract shared explorer-window dependencies and auxiliary-window observers without changing initial behavior.
- [ ] 2.1.2 Add a fresh-window input that creates one independent root at `C:\` with normal placement.
- [ ] 2.1.3 Run existing startup, session, title, and render tests and record G3 evidence.

### 2.2 Foreground dispatch and lifecycle

**目的：** Convert accepted relaunch commands into exactly one activated GPUI window.
**輸入：** 1.2 receiver and 2.1 window factory.
**產出：** Foreground command pump, diagnostics, and listener ownership in shutdown resources.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** G4 dispatch/lifetime tests; `evidence/index.jsonl`.
**完成門檻：** Each command opens one `C:\` window; closing non-final windows does not quit.

- [ ] 2.2.1 Wire launch-role selection before heavyweight application composition.
- [ ] 2.2.2 Drain resident commands on the GPUI foreground executor and activate each created window.
- [ ] 2.2.3 Integrate listener shutdown, multi-window close semantics, and structured diagnostics.
- [ ] 2.2.4 Test one-command/one-window dispatch and final-window shutdown; record G4 evidence.

## 3. System verification and handoff

### 3.1 Windows repeated-launch smoke coverage

**目的：** Prove installed-style behavior against real Windows processes and windows.
**輸入：** Completed implementation and a debug/release executable.
**產出：** Repeatable smoke script and captured report.
**依賴：** 2.2.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** G5 headful smoke; `evidence/repeated-launch/report.json` and `evidence/index.jsonl`.
**完成門檻：** Two launches yield one resident process and two windows; newest window reports `C:\`; independent close passes.

- [ ] 3.1.1 Add a repeatable Windows smoke script that isolates profile state and launches twice.
- [ ] 3.1.2 Run the smoke test and capture process, window-count, address, and close-lifetime results.
- [ ] 3.1.3 Record hashed G5 evidence for the smoke report and any screenshots.

### 3.2 Final quality and traceability

**目的：** Close all requirements with reproducible evidence and repository-wide checks.
**輸入：** G1-G5 outputs and all changed source/spec/task files.
**產出：** Formatted code, passing checks, complete evidence index, final review record.
**依賴：** 3.1.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** G6 final quality; `evidence/final-review.md` and `evidence/index.jsonl`.
**完成門檻：** Formatting, focused tests, clippy/check, strict OpenSpec validation, and traceability all pass with no unresolved P0/P1 issue.

- [ ] 3.2.1 Run Rust formatting and focused workspace build/test/clippy gates; record G6 command evidence.
- [ ] 3.2.2 Map every normative scenario to passing G1-G6 evidence with immutable hashes.
- [ ] 3.2.3 Review security, lifecycle, fallback, regression scope, and task atomicity; resolve all P0/P1 findings.
- [ ] 3.2.4 Run strict OpenSpec validation and write the final review record.
