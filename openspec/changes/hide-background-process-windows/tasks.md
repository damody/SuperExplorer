## 1. Freeze Inventory and Evidence Contract

### 1.1 Production process-launch inventory

**目的：** Classify every current process launch before changing behavior so background, explicit-visible, test-only, and build-time sites cannot be confused.
**輸入：** Approved proposal/design/specs; `crates/**/src/**/*.rs`; build scripts and integration tests.
**產出：** `process-launch-inventory.json` with source path, line anchor, owner crate, classification, required configurator, and rationale.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** `INV-1`; `evidence/index.json` records task IDs `1.1.1`–`1.1.3` and hashes the inventory.
**完成門檻：** Every current `Command::new`-style site has exactly one classification; all production background and explicit-visible sites are identified; exclusions name their compile-time boundary.

- [ ] 1.1.1 Enumerate all repository process-launch sites and write their source anchors to `process-launch-inventory.json`.
- [ ] 1.1.2 Classify production runtime sites as hidden-background or explicit-visible and record a rationale for every visible exception.
- [ ] 1.1.3 Classify test-only and build-time sites with their `cfg`, target, or script boundary so they cannot satisfy production coverage.

### 1.2 Evidence schema and baseline

**目的：** Establish auditable, append-only results for every task and blocking gate.
**輸入：** Work package 1.1 inventory and the change correction rules.
**產出：** `evidence/README.md`, `evidence/index.json`, and baseline source/build metadata.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** `EVID-1`; task IDs `1.2.1`–`1.2.3` in `evidence/index.json`.
**完成門檻：** Schema records task ID, artifact/command, expected/actual, status or reviewer, exit status, hashes, gates, adjustment ID, and timestamp; every completed leaf has a unique record or immutable subcheck.

- [ ] 1.2.1 Define the evidence record fields and valid `passed`, `not-applicable`, and `superseded` terminal dispositions.
- [ ] 1.2.2 Create the evidence index with source revision and approved design/spec hashes.
- [ ] 1.2.3 Record the pre-change debug/release subsystem and background-launch baseline without claiming compliant runtime behavior.

## 2. Implement the Shared Policy and Parent Console

### 2.1 Shared background-command configurator

**目的：** Provide one dependency-safe API that applies `CREATE_NO_WINDOW` on Windows and is a no-op elsewhere.
**輸入：** Inventory classifications; `explorer-common` architecture and manifests.
**產出：** Common process module, public export, and focused unit/contract tests.
**依賴：** 1.1 and 1.2.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** `POLICY-1`; task IDs `2.1.1`–`2.1.4`.
**完成門檻：** The helper compiles on Windows/non-Windows cfg paths, applies the named flag exactly once, preserves caller configuration, and passes focused tests.

- [ ] 2.1.1 Add the cfg-aware `explorer-common` background-command configurator using `CommandExt::creation_flags(CREATE_NO_WINDOW)` on Windows.
- [ ] 2.1.2 Export the helper without introducing a Win32 crate dependency or dependency cycle.
- [ ] 2.1.3 Add a focused Windows test proving a configured console-subsystem fixture produces captured output without a visible console.
- [ ] 2.1.4 Add a non-Windows compile/behavior contract proving the configurator remains a no-op.

### 2.2 SuperExplorer parent and explicit-visible exception

**目的：** Keep the parent diagnostics console in both profiles while preserving the user-requested visible terminal action.
**輸入：** `explorer-app/src/main.rs`, `explorer-shell-win::launch_command_prompt`, and window-policy spec.
**產出：** Updated application subsystem configuration and source contracts for the sole visible exception.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** `PARENT-1`, `VISIBLE-1`; task IDs `2.2.1`–`2.2.3`.
**完成門檻：** Debug and release app artifacts use the console subsystem; Open Command Prompt still uses `CREATE_NEW_CONSOLE`; no other production site is authorized as visible.

- [ ] 2.2.1 Remove the release-only Windows GUI-subsystem attribute from the SuperExplorer application entry point.
- [ ] 2.2.2 Add a build/source contract that verifies debug and release application subsystem policy.
- [ ] 2.2.3 Add a focused contract proving `launch_command_prompt` remains the sole `CREATE_NEW_CONSOLE` production exception.

## 3. Migrate Production Background Launchers

### 3.1 ADB remote-process runner

**目的：** Hide startup and operation-time ADB processes without changing remote-provider results.
**輸入：** Common configurator; `explorer-remote/src/adb.rs`; existing ADB cancellation/output tests.
**產出：** Updated dependency manifest, configured ADB runner, and regression tests.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** `ADB-1`; task IDs `3.1.1`–`3.1.4`.
**完成門檻：** Every SystemAdbCommandRunner spawn is hidden on Windows and existing devices/output/timeout/cancellation behavior passes.

- [ ] 3.1.1 Add the internal `explorer-common` dependency to `explorer-remote` and configure `SystemAdbCommandRunner` before spawn.
- [ ] 3.1.2 Add a controlled console-subsystem fixture test for successful ADB-style output capture with no visible child console.
- [ ] 3.1.3 Run and record the ADB cancellation and timeout regression result.
- [ ] 3.1.4 Record real ADB startup discovery as passed when available or evidence-backed `not-applicable` with fixture substitution when unavailable.

### 3.2 Automation process hosts

**目的：** Hide Windows automation children while retaining policy validation, output bounds, cancellation, timeout, and Job Object containment.
**輸入：** Common configurator; native and Windows-contained automation hosts.
**產出：** Updated process hosts/dependencies and focused lifecycle regression results.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** `AUTO-1`, `AUTO-LIFECYCLE-1`; task IDs `3.2.1`–`3.2.5`.
**完成門檻：** Both Windows-compiled production hosts configure children before spawn; output and lifecycle tests pass; no shell host is introduced.

- [ ] 3.2.1 Configure the platform-neutral `NativeProcessHost` background child when compiled for Windows.
- [ ] 3.2.2 Add the common dependency to `explorer-automation-win` and configure its contained child before spawn.
- [ ] 3.2.3 Run focused successful-output and spawn-failure tests for both automation hosts.
- [ ] 3.2.4 Run the Windows timeout and cancellation regressions and record independent results.
- [ ] 3.2.5 Run the Windows Job Object process-tree cleanup regression and record its result.

### 3.3 Extension broker, worker, and helper audit

**目的：** Close remaining internal extension launch gaps and replace duplicated numeric policy with the common configurator where safe.
**輸入：** Inventory; broker main/library/worker sources; existing process-boundary tests.
**產出：** Normalized broker/worker creation paths and extension lifecycle evidence.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** `EXT-1`, `EXT-LIFECYCLE-1`; task IDs `3.3.1`–`3.3.4`.
**完成門檻：** Every production broker, worker, probe, and worker-owned child is hidden; helper release subsystem attributes remain intact; typed failures and cleanup tests pass.

- [ ] 3.3.1 Replace broker-client background creation flags with the common configurator without changing IPC or stdio.
- [ ] 3.3.2 Configure broker-owned worker/probe launches and the worker-owned child-process path as hidden background processes.
- [ ] 3.3.3 Run extension process-boundary success, malformed-probe, and spawn-failure tests as independent evidence subchecks.
- [ ] 3.3.4 Run extension cancellation, timeout, crash recovery, and orphan-process checks as independent evidence subchecks.

## 4. Prevent Regression and Verify Runtime Behavior

### 4.1 Blocking launch-inventory gate

**目的：** Fail future changes that add an unclassified production process launch or visible-console exception.
**輸入：** Completed inventory and migrated production sources.
**產出：** Checked-in inventory validator and deterministic gate output.
**依賴：** 3.1, 3.2, and 3.3.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** `INV-2`; task IDs `4.1.1`–`4.1.3`.
**完成門檻：** Validator passes the repository, fails injected unknown production launches, and distinguishes test/build exclusions.

- [ ] 4.1.1 Implement the deterministic production process-launch inventory validator.
- [ ] 4.1.2 Add a self-test proving an unclassified production `Command::new` site fails with a source location.
- [ ] 4.1.3 Add a self-test proving test-only/build-time commands are excluded without being counted as production compliance.

### 4.2 Debug and release Windows window gate

**目的：** Prove the compiled application has one visible parent diagnostics console and no visible background child consoles.
**輸入：** Migrated binaries, controlled console fixture, optional ADB, Win32 window/process inspection harness.
**產出：** Raw process/window observations and hashed debug/release reports.
**依賴：** 2.2, 3.1, 3.2, 3.3, and 4.1.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** `WINDOW-DEBUG-1`, `WINDOW-RELEASE-1`; task IDs `4.2.1`–`4.2.5`.
**完成門檻：** Both profiles show the parent console, representative background children show no visible top-level window, output is captured, and hanging children terminate invisibly.

- [ ] 4.2.1 Build the debug SuperExplorer and representative helper artifacts used by the window gate.
- [ ] 4.2.2 Run the debug parent/background visibility inspection and save raw observations.
- [ ] 4.2.3 Build the release SuperExplorer and representative helper artifacts used by the window gate.
- [ ] 4.2.4 Run the release parent/background visibility inspection and save raw observations.
- [ ] 4.2.5 Run the hidden hanging-child timeout/cancellation inspection and save its independent result.

## 5. Integration and Final Review

### 5.1 Focused and architecture validation

**目的：** Confirm the process-window change composes with crate tests, dependency rules, formatting, and lint/build gates.
**輸入：** All implementation packages and their evidence.
**產出：** Command logs and indexed validation results.
**依賴：** 4.1 and 4.2.
**Owner／Wave：** Primary integrator / Wave 5.
**Gate／Evidence：** `BUILD-1`, `TEST-1`, `ARCH-1`; task IDs `5.1.1`–`5.1.4`.
**完成門檻：** Formatting, focused tests, architecture checks, and required debug/release builds pass with no unclassified launch sites.

- [ ] 5.1.1 Run formatting verification and record the result.
- [ ] 5.1.2 Run focused tests for `explorer-common`, `explorer-remote`, `explorer-automation`, and `explorer-automation-win`.
- [ ] 5.1.3 Run focused extension-broker process and lifecycle tests.
- [ ] 5.1.4 Run repository architecture checks and required debug/release build checks.

### 5.2 Traceability and completion audit

**目的：** Close only work supported by immutable evidence and confirm every normative scenario is implemented and verified.
**輸入：** Proposal, design, specs, task results, inventory, and all gate reports.
**產出：** Final traceability matrix, reviewed evidence index, and completion summary.
**依賴：** 5.1.
**Owner／Wave：** Primary integrator / Wave 5.
**Gate／Evidence：** `TRACE-1`, `FINAL-1`; task IDs `5.2.1`–`5.2.4`.
**完成門檻：** Every requirement/scenario maps to implementation and a passing or valid not-applicable record; no stale/failed/unexecuted leaf is checked; strict OpenSpec validation passes.

- [ ] 5.2.1 Map every proposal outcome and normative scenario to implementation files, task IDs, gates, and evidence records.
- [ ] 5.2.2 Reconcile evidence hashes and mark superseded records without deleting lineage.
- [ ] 5.2.3 Review all leaves for atomic completion and reopen any leaf whose required evidence is stale, failed, blocked, or missing.
- [ ] 5.2.4 Run strict OpenSpec validation and publish the final completion summary.
