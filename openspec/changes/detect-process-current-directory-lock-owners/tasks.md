## 1. Native discovery foundation

### 1.1 Path matching and bounded process model

**目的：** Define the pure, independently testable semantics and limits used by native discovery.
**輸入：** Approved design; modified `lock-owner-host-service` requirements; existing `LockOwner` identity and `RoadmapLimits`.
**產出：** Component-aware directory ancestry matcher, explicit discovery bounds, focused tests.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1; owned paths root `Cargo.toml`, `crates/explorer-shell-win/src/**`, relevant limits/tests; forbidden paths public extension ABI and unrelated shared-manifest entries.
**Gate／Evidence：** G1; `evidence/index.json` records `1.1.*` with focused test commands and results.
**完成門檻：** Every matching boundary and bound has a named passing test, with no string-prefix or file-row false positive.

- [ ] 1.1.1 Implement a pure Windows path-component matcher for normalized absolute local, UNC and extended-length directory paths.
- [ ] 1.1.2 Add focused matcher tests for exact/nested ancestry, roots, case, repeated/trailing separators, `D:\AI` versus `D:\AI_Picture`, sibling shares, unresolved traversal and file/metadata-race bypass.
- [ ] 1.1.3 Define the 4,096-candidate bound, 32,768 UTF-16-code-unit remote string limit and skip outcome, absolute elapsed deadline and deterministic owner-result limit using existing project limit patterns.
- [ ] 1.1.4 Enable the exact Toolhelp Windows bindings in the owned workspace manifest without adding an unlocked dependency or helper executable.
- [ ] 1.1.5 Run locked/offline metadata resolution for the changed Windows feature manifest.
- [ ] 1.1.6 Record Wave 1 matching/manifest commands, expected/actual results, exit codes, timestamp and source hashes in the evidence index.

### 1.2 Audited native and WOW64 current-directory reader

**目的：** Discover candidate process current directories without leaking data or acquiring mutation authority.
**輸入：** 1.1 matcher/limits; Windows process snapshot and process-query APIs; existing RAII Win32 patterns.
**產出：** Discover-only current-directory probe, native/WOW64 readers, safe owner projection, adversarial tests.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / Wave 1; owned path `crates/explorer-shell-win/src/**`; forbidden paths extension ABI and process-control code.
**Gate／Evidence：** G1 and G3; `evidence/index.json` records `1.2.*` plus native test logs.
**完成門檻：** A real accessible process can be attributed; malformed, denied, exiting, self, cancelled and over-bound candidates cannot leak handles, panic, or fail unrelated results.

- [ ] 1.2.1 Add one-per-batch RAII process-snapshot enumeration with current-process exclusion, minimum query/read access and the 4,096-candidate fail-closed bound.
- [ ] 1.2.2 Implement checked native-width remote process-parameter reading for only the current-directory field.
- [ ] 1.2.3 Implement checked WOW64 remote process-parameter reading and reject unknown layouts without guessing fields.
- [ ] 1.2.4 Project PID, creation time, safe executable basename, and application type into existing `LockOwner` records while discarding discovered paths after matching.
- [ ] 1.2.5 Add reader seam tests for malformed remote pointers, lengths, alignment and checked arithmetic.
- [ ] 1.2.6 Add reader seam tests proving access-denied and process-exit races skip only the affected candidate.
- [ ] 1.2.7 Add reader seam tests proving cancellation interrupts native reads and returns Cancelled.
- [ ] 1.2.8 Add reader seam tests proving the absolute deadline interrupts native reads and returns DeadlineElapsed.
- [ ] 1.2.9 Add a process-enumeration test proving current-process exclusion.
- [ ] 1.2.10 Add a process-enumeration test proving 4,096-candidate overflow returns Unavailable without partial scanning.
- [ ] 1.2.11 Add a process-enumeration test proving candidate-order independence.
- [ ] 1.2.12 Add RAII instrumentation tests proving snapshot/process-handle cleanup on success and typed error.
- [ ] 1.2.13 Add RAII instrumentation tests proving snapshot/process-handle cleanup on cancellation and deadline.
- [ ] 1.2.14 Add RAII instrumentation tests proving snapshot/process-handle cleanup after an injected panic.
- [ ] 1.2.15 Add a Windows real native-process integration test proving exact and parent attribution through the production probe.
- [ ] 1.2.16 Add a Windows real `%SystemRoot%\SysWOW64\cmd.exe` integration test, verified with `IsWow64Process2`, proving exact and parent attribution on supported x64 Windows.
- [ ] 1.2.17 Run the focused locked/offline `explorer-shell-win` test command.
- [ ] 1.2.18 Run the native unsafe-boundary architecture/security scan.
- [ ] 1.2.19 Record G1/G3 native test and review evidence with unique task records and hashes.

## 2. Host composition and refresh

### 2.1 Batched cancellable host-service and panic boundary

**目的：** Replace per-item uncancellable calls with one bounded internal batch call and contain every ABI-facing panic.
**輸入：** 1.2 probe; existing host-only service seam, runtime job lifecycle and public ABI adapter.
**產出：** Batch request/result seam with absolute deadline/live cancellation and generation-preserving panic containment.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator / Wave 2; owned paths `crates/explorer-extension-host/src/extension_job_runtime.rs`, `crates/explorer-app/src/application.rs` and focused tests; forbidden paths public extension ABI and unrelated UI.
**Gate／Evidence：** G2 and G3; `evidence/index.json` records `2.1.*`.
**完成門檻：** A 128-item request performs one current-directory snapshot, observes live cancellation/shared deadline, contains injected panics and preserves request generations without changing public ABI.

- [ ] 2.1.1 Replace the internal per-path host closure with one resolved item/path batch request carrying one absolute deadline and live job-cancellation predicate.
- [ ] 2.1.2 Change the ABI adapter to authorize/resolve the whole bounded item list and invoke the internal batch service exactly once.
- [ ] 2.1.3 Thread the live cancellation predicate and remaining absolute deadline through application composition, Restart Manager work and current-directory discovery.
- [ ] 2.1.4 Split ABI adapter work into caught `query_inner` and generation-preserving HostError fallback with an empty owner payload.
- [ ] 2.1.5 Add a maximum-128-item test proving exactly one current-directory snapshot/service invocation.
- [ ] 2.1.6 Add a host-adapter test proving job cancellation becomes observable while the internal service is running.
- [ ] 2.1.7 Add a host-adapter test proving one absolute deadline decreases across items and both sources.
- [ ] 2.1.8 Add an injected native-reader panic test proving HostError, preserved generations and resource cleanup.
- [ ] 2.1.9 Add an injected composition panic test proving HostError, preserved generations and resource cleanup.
- [ ] 2.1.10 Add an injected host-callback panic test proving HostError, preserved generations and subsequent host usability.
- [ ] 2.1.11 Record G2/G3 batch, cancellation, deadline and panic evidence with unique records and hashes.

### 2.2 Mixed-source terminal and deterministic owner composition

**目的：** Present one bounded privacy-safe result using the frozen terminal truth table and stable owner ordering.
**輸入：** 2.1 batch seam; Restart Manager/current-directory terminal values; existing owner model and public statuses.
**產出：** Typed DeadlineElapsed, truth-table composition, stable identity merge and focused tests.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / Wave 2; owned paths lock-recovery model, application composition and focused tests; forbidden paths unrelated models/UI.
**Gate／Evidence：** G2/G3; `evidence/index.json` records `2.2.*`.
**完成門檻：** Every truth-table precedence class and deterministic truncation rule has a named passing test; no ABI or sensitive-data surface changes.

- [ ] 2.2.1 Add the internal DeadlineElapsed discovery terminal and map it to the existing public DEADLINE_ELAPSED status.
- [ ] 2.2.2 Implement per-item cancellation dominance and deadline dominance from the approved truth table.
- [ ] 2.2.3 Implement Ready partial-source precedence and no-Ready HostError/Unavailable/Empty precedence from the approved truth table.
- [ ] 2.2.4 Implement PID-plus-creation-time deduplication with Restart Manager metadata precedence.
- [ ] 2.2.5 Implement the frozen process-ID, creation-time, case-folded-name and application-type ordering before truncation.
- [ ] 2.2.6 Add focused tests for cancellation precedence across both source positions.
- [ ] 2.2.7 Add focused tests for deadline precedence across both source positions.
- [ ] 2.2.8 Add a focused test for Ready paired with Ready.
- [ ] 2.2.9 Add a focused test for Ready paired with Empty.
- [ ] 2.2.10 Add a focused test for Ready paired with Unavailable.
- [ ] 2.2.11 Add a focused test for Ready paired with HostError.
- [ ] 2.2.12 Add a focused test for no-Ready HostError precedence.
- [ ] 2.2.13 Add a focused test for no-Ready Unavailable precedence.
- [ ] 2.2.14 Add a focused test for both-source Empty.
- [ ] 2.2.15 Add a mixed three-item batch test for Ready owners plus ownerless Empty and Unavailable items using the conservative global fallback status.
- [ ] 2.2.16 Add a source/input order-invariance test before truncation.
- [ ] 2.2.17 Add a deterministic owner-overflow/truncation test.
- [ ] 2.2.18 Run public API/host surface scans proving no ABI, process-control, command-line, environment, handle or path disclosure.
- [ ] 2.2.19 Record G2/G3 truth-table, merge and contract evidence with unique task records and hashes.

### 2.3 Cache, F5, stale state and lifecycle

**目的：** Ensure current-directory ownership changes are observable through existing refresh behavior and cannot reappear from stale work.
**輸入：** 2.2 merged terminal/owner composition; existing canonical cache key, TTL, refresh generation and feature revocation.
**產出：** Correct combined cache invalidation/rescheduling and lifecycle tests.
**依賴：** 2.2.
**Owner／Wave：** Primary integrator / Wave 2; owned paths application cache/refresh tests; forbidden paths unrelated cache systems.
**Gate／Evidence：** G2; `evidence/index.json` records `2.3.*`.
**完成門檻：** F5/manual refresh clears exited or moved owners, rapid F5/folder/tab/feature transitions reject every older-source result, and unchanged short-TTL behavior remains bounded.

- [ ] 2.3.1 Route merged results through the existing canonical resource identity, short TTL and refresh-generation cache without adding polling.
- [ ] 2.3.2 Add a test proving F5 clears an occupied result after process exit.
- [ ] 2.3.3 Add a controlled-process test proving F5 clears an occupied result after the process changes outside the subtree.
- [ ] 2.3.4 Add a test proving rapid F5 rejects delayed results from either source.
- [ ] 2.3.5 Add a test proving folder change rejects delayed results from either source.
- [ ] 2.3.6 Add a test proving tab change rejects delayed results from either source.
- [ ] 2.3.7 Add a test proving feature disable rejects delayed results from either source.
- [ ] 2.3.8 Run focused cache/generation tests.
- [ ] 2.3.9 Record G2 cache evidence with expected and actual generation transitions.

## 3. Production plugin and headful regression

### 3.1 Real `cmd.exe` UTIT fixture and assertions

**目的：** Prove the production plugin/host/UI path displays nested console ownership on a visible parent and clears it through F5.
**輸入：** 2.3 production behavior; existing Rust Lock owner fixture; `smoke_tokei_plugin_headful.ps1`; UTIT manifest case.
**產出：** Extended real-process fixture, required screenshots/report fields, passing manifest case.
**依賴：** 2.3.
**Owner／Wave：** Primary integrator / Wave 3; owned paths `scripts/smoke_tokei_plugin_headful.ps1`, `uitest/manifest.json`, lock-owner fixture only if required; shared manifest integrated only by primary.
**Gate／Evidence：** G4; UTIT evidence directory plus `evidence/index.json` records `3.1.*`.
**完成門檻：** The local runner proves native and WOW64 nested/parent values before refresh, then proves clearing after required process exit plus F5, while docs reproduce and all existing lock, stale-generation and feature-disable checks pass.

- [ ] 3.1.1 Extend the headful setup to start native `cmd.exe` directly with an explicit nested fixture working directory and retain its process handle.
- [ ] 3.1.2 Start `%SystemRoot%\SysWOW64\cmd.exe` with an explicit nested fixture working directory, retain its handle and verify WOW64 identity with `IsWow64Process2`.
- [ ] 3.1.3 Navigate/render the fixture so the native/WOW64 nested directories and their visible parent rows use the production plugin.
- [ ] 3.1.4 Assert and capture required native owner evidence on both the nested directory and parent row.
- [ ] 3.1.5 Assert and capture required WOW64 owner evidence on both the nested directory and parent row.
- [ ] 3.1.6 Exit both controlled processes, invoke the production F5 control, and assert every current-directory owner value clears.
- [ ] 3.1.7 Preserve the existing real file-lock appearance and clearing assertions in the blocking case.
- [ ] 3.1.8 Preserve the existing rapid-refresh stale-generation assertion in the blocking case.
- [ ] 3.1.9 Preserve the existing folder-generation rejection assertion in the blocking case.
- [ ] 3.1.10 Preserve the existing tab-generation rejection assertion in the blocking case.
- [ ] 3.1.11 Preserve the existing feature-disable delayed-result rejection assertion in the blocking case.
- [ ] 3.1.12 Update the English README for ancestry, privacy/false negatives, cancellation/deadline, TTL/F5 and offline reproduction.
- [ ] 3.1.13 Update the Traditional Chinese README with the same normative reproduction and limitation content.
- [ ] 3.1.14 Reproduce the documented example build command offline and record its unique gate result.
- [ ] 3.1.15 Reproduce the documented example test/validation command offline and record its unique gate result.
- [ ] 3.1.16 Reproduce the documented example package command offline and record its unique gate result.
- [ ] 3.1.17 Update `uitest/manifest.json` required native/WOW64/clearing artifacts and report fields.
- [ ] 3.1.18 Validate UTIT manifest capability mappings.
- [ ] 3.1.19 Build the application offline for the blocking run.
- [ ] 3.1.20 Build the Lock owner plugin offline for the blocking run; native/WOW64 console fixtures come from the verified operating-system paths.
- [ ] 3.1.21 Run `rust-lock-owner-headful` through `explorer-uitest` and require a passing runner exit code.
- [ ] 3.1.22 Record G4 screenshots, report, command output, hashes and documentation reproduction evidence.

## 4. Integration, review and completion

### 4.1 Affected regression and security review

**目的：** Establish that the complete change is safe, formatted, reproducible, and limited to approved scope.
**輸入：** Completed Waves 1–3; all generated evidence; repository dirty-state inventory.
**產出：** Passing affected regression matrix, reviewed diff, completed evidence ledger, strict-valid OpenSpec change.
**依賴：** 1.1 through 3.1.
**Owner／Wave：** Primary integrator / Wave 4; owned paths change artifacts/evidence and integration diff; unrelated user/agent changes remain untouched.
**Gate／Evidence：** G5 plus closure of G1 through G4; `evidence/index.json` records `4.1.*`.
**完成門檻：** Every requirement/scenario maps to a passing task/evidence record; format, affected tests, offline compile, UTIT, manifest and strict validation pass; no unresolved P0/P1 review finding remains.

- [ ] 4.1.1 Run the repository formatting check and record its exact locked toolchain result.
- [ ] 4.1.2 Run affected `explorer-shell-win` tests with the exact locked/offline command.
- [ ] 4.1.3 Run affected `explorer-app` tests with the exact locked/offline command.
- [ ] 4.1.4 Run affected `explorer-extension-host` tests with the exact locked/offline command.
- [ ] 4.1.5 Run affected `explorer-extension-api` contract tests with the exact locked/offline command.
- [ ] 4.1.6 Run affected `explorer-ui` tests with the exact locked/offline command.
- [ ] 4.1.7 Run affected workspace compile checks.
- [ ] 4.1.8 Classify any unrelated pre-existing failure with reproducible evidence without weakening G1 through G5.
- [ ] 4.1.9 Review checked remote addresses/lengths and pointer-width layout isolation at the final native unsafe boundary.
- [ ] 4.1.10 Review minimum access, cancellation/deadline checks and snapshot/process-handle lifetime at the final native unsafe boundary.
- [ ] 4.1.11 Review privacy projection, self exclusion, panic containment and lack of process control at the final native unsafe boundary.
- [ ] 4.1.12 Review proposal → design → requirement/scenario → gate → task traceability.
- [ ] 4.1.13 Verify each resolved leaf has one unique evidence record or immutable subcheck.
- [ ] 4.1.14 Run `openspec validate detect-process-current-directory-lock-owners --strict`.
- [ ] 4.1.15 Scan artifacts for unresolved template markers and contradictions.
- [ ] 4.1.16 Resolve every P0/P1 independent-review finding and record the reviewer disposition.
- [ ] 4.1.17 Mark tasks complete only after every required evidence record is current.
- [ ] 4.1.18 Report the final test matrix and any remaining unrelated failures, leaving the valid change ready for archive.
