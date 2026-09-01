## 1. Launch detection foundation

### 1.1 Launch classification contract

**目的：** Determine which invocations participate without mutating global state.
**輸入：** Approved proposal, corrected design, and specification.
**產出：** Launch classification module and unit tests.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** G1 classification tests; `evidence/index.jsonl`.
**完成門檻：** Ordinary launches participate; diagnostics, fixtures, auto-close, plugin, and bypass modes do not.

- [x] 1.1.1 Add the ordinary-versus-isolated launch-kind contract.
- [x] 1.1.2 Implement diagnostic, plugin, fixture, auto-close, and explicit test-bypass classification.
- [x] 1.1.3 Run focused classification tests and record G1 evidence.

### 1.2 Login-session marker lifecycle

**目的：** Atomically distinguish initial and repeated ordinary invocations.
**輸入：** Work package 1.1 and the workspace Win32 dependency surface.
**產出：** Versioned `Local\` named-mutex guard retained for process lifetime.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** G2 marker tests; `evidence/index.jsonl`.
**完成門檻：** First acquisition is initial, later acquisition is repeated, and handle release is owned.

- [x] 1.2.1 Implement the product-specific login-session marker name and atomic acquisition.
- [x] 1.2.2 Retain and close each process marker handle through an RAII guard.
- [x] 1.2.3 Test repeated acquisition and record G2 evidence.

## 2. Startup-location integration

### 2.1 Explicit initial-path API

**目的：** Override saved tabs without unsafe environment mutation.
**輸入：** Existing `ApplicationLifecycle::run_gpui` and initial-location validation.
**產出：** Optional `PathBuf` startup override threaded into GPUI startup.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** G3 application tests; `evidence/index.jsonl`.
**完成門檻：** No override preserves restoration; a valid override suppresses restored tabs.

- [x] 2.1.1 Add `run_gpui_with_initial_path` while preserving the existing `run_gpui` API.
- [x] 2.1.2 Give the in-process startup override precedence over `EXPLORER_INITIAL_PATH`.
- [x] 2.1.3 Run startup/session tests and record G3 evidence.

### 2.2 Repeated-launch composition

**目的：** Make later ordinary processes open one independent `C:\` window.
**輸入：** 1.2 marker result and 2.1 initial-path API.
**產出：** Main-entry composition, diagnostics, and lifetime behavior.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** G4 build/unit tests; `evidence/index.jsonl`.
**完成門檻：** Repeated launches pass `C:\`; initial/special launches do not; all processes retain their marker until shutdown.

- [x] 2.2.1 Acquire the marker before heavyweight application composition for ordinary launches only.
- [x] 2.2.2 Pass `C:\` only for repeated launches and record the classification diagnostic.
- [x] 2.2.3 Keep the marker guard alive through GPUI and application shutdown.
- [x] 2.2.4 Run focused binary build and unit tests; record G4 evidence.

## 3. System verification and handoff

### 3.1 Windows repeated-launch smoke coverage

**目的：** Prove real Windows process/window behavior and location precedence.
**輸入：** Completed debug executable.
**產出：** Repeatable smoke script and JSON report.
**依賴：** 2.2.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** G5 headful smoke; `evidence/repeated-launch/report.json` and `evidence/index.jsonl`.
**完成門檻：** First forced `D:\` launch displays `D:\`; second displays `C:\`; both remain responsive and independently closable.

- [x] 3.1.1 Add a repeatable Windows smoke script that isolates profile state and tracks only its child PIDs.
- [x] 3.1.2 Run the smoke test and capture PID, window title/address, responsiveness, and close results.
- [x] 3.1.3 Record hashed G5 evidence for the smoke report.

### 3.2 Final quality and traceability

**目的：** Close all requirements with reproducible evidence and repository checks.
**輸入：** G1-G5 outputs and all changed source/spec/task files.
**產出：** Passing gates, traceability index, and final review record.
**依賴：** 3.1.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** G6 final quality; `evidence/final-review.md` and `evidence/index.jsonl`.
**完成門檻：** Formatting, focused tests, clippy/check, strict OpenSpec validation, and traceability pass with no unresolved P0/P1 issue.

- [x] 3.2.1 Run Rust formatting and focused build/test/clippy gates, distinguish unrelated baseline lint failures, and record G6 evidence.
- [x] 3.2.2 Map every normative scenario to passing G1-G6 evidence with immutable hashes.
- [x] 3.2.3 Review security, lifetime, regression scope, and task atomicity; resolve all P0/P1 findings.
- [x] 3.2.4 Run strict OpenSpec validation and write the final review record.

## 4. Installed multi-process correction

### 4.1 Concurrent extension-host startup

**目的：** Remove the Windows staging-root sharing violation without weakening package isolation.
**輸入：** Existing verified staging root and unique candidate allocation.
**產出：** Share-compatible root handles and focused concurrent-open coverage.
**依賴：** 2.2.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** G7 extension-host tests; `evidence/index.jsonl`.
**完成門檻：** Two live importers can open the same root while unsafe-root checks remain passing.

- [x] 4.1.1 Allow read and write sharing, while denying delete sharing, on the long-lived Windows staging-root handle.
- [x] 4.1.2 Add and pass a Windows regression test with two simultaneous importer instances.

### 4.2 Installed shortcut and end-to-end verification

**目的：** Ensure persistent shortcuts participate in ordinary repeated-launch detection.
**輸入：** Test and production NSIS argument configuration plus the corrected executable.
**產出：** Argument-free shortcuts, rebuilt installer, and installed double-launch evidence.
**依賴：** 4.1.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** G8 installer/headful smoke; `evidence/repeated-launch/installed-report.json`.
**完成門檻：** Installed shortcut launches twice, leaves two responsive windows, and records no staging sharing violation.

- [x] 4.2.1 Separate finish-page diagnostic arguments from persistent shortcut arguments.
- [x] 4.2.2 Run focused format, unit, and strict OpenSpec validation gates.
- [x] 4.2.3 Rebuild and install the test package, then verify two shortcut launches and record evidence.

## 5. Running-application upgrade correction

### 5.1 Exact-path process quiescence

**目的：** Prevent stale application binaries while avoiding unrelated process termination.
**輸入：** Selected NSIS install directory and running-process executable paths.
**產出：** Bounded installer-owned quiescence helper and fail-closed NSIS integration.
**依賴：** 4.2.
**Owner／Wave：** Primary integrator / Wave 5.
**Gate／Evidence：** G9 helper and source-contract tests; `evidence/index.jsonl`.
**完成門檻：** Exact target processes exit, outside processes survive, and NSIS cannot replace files before verified quiescence.

- [x] 5.1.1 Add an exact-path graceful-then-force PowerShell quiescence helper with bounded verification.
- [x] 5.1.2 Extract and invoke the helper before install/uninstall file mutation, abort on nonzero result, and keep silent failure non-interactive.
- [x] 5.1.3 Add controlled helper behavior and NSIS ordering/failure contract tests.

### 5.2 Final installed upgrade loop

**目的：** Prove the installer itself deploys the tested candidate and repeated launch remains correct afterward.
**輸入：** Two running installed windows and the rebuilt test installer.
**產出：** Hash-matched installed binary, argument-free shortcut, double-launch evidence, and final review.
**依賴：** 5.1.
**Owner／Wave：** Primary integrator / Wave 5.
**Gate／Evidence：** G10 installed upgrade; `evidence/repeated-launch/installed-upgrade-report.json`.
**完成門檻：** Installer exit zero, old PIDs exit, hashes match without manual copying, double launch passes, and all automated/strict gates pass.

- [x] 5.2.1 Run formatting, helper tests, extension-host tests, launch tests, and strict OpenSpec validation.
- [x] 5.2.2 Rebuild the release candidate and test installer.
- [x] 5.2.3 Upgrade with two installed windows open and verify old-PID exit plus installed/release hash equality without manual file replacement.
- [x] 5.2.4 Relaunch twice, verify responsiveness and `C:\`, record final evidence, and repeat any failed gate until passing.
