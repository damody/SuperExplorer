## 1. Tooling contracts and fixtures

### 1.1 Resolver and device model

**目的：** Establish one validated system-first ADB resolution contract and immutable serial-keyed device snapshots.
**輸入：** Approved design, existing `explorer-remote` runner/provider, environment path helpers.
**產出：** Resolver/device types, parsers, deterministic fake-runner fixtures, focused tests.
**依賴：** None.
**Owner／Wave：** primary／1.
**Gate／Evidence：** G1, G2; `evidence/index.jsonl`.
**完成門檻：** Precedence, probe rejection/fallback, device states/names, duplicate names, malformed/stale results all pass offline tests.

- [x] 1.1.1 Inventory existing ADB construction, context-menu, operation-status, localization, dependency, and test seams and record the implementation map.
- [x] 1.1.2 Define validated ADB resolution provenance, rejected-candidate diagnostics, and immutable device snapshot types.
- [x] 1.1.3 Implement system `PATH`, configured/recognized Android SDK, then managed-install candidate enumeration without environment mutation.
- [x] 1.1.4 Implement bounded `adb version` validation with invalid-candidate fallback and cache invalidation.
- [x] 1.1.5 Implement bounded `adb devices -l` parsing, display-name normalization, installability states, and snapshot generations.
- [x] 1.1.6 Add resolver and discovery tests for precedence, invalid candidates, later system appearance, duplicate names, fallback names, device states, malformed output, and stale generations.

### 1.2 APK install executor

**目的：** Provide exact-device, argument-safe APK updates through the bounded ADB runner.
**輸入：** 1.1 contracts, existing cancellation/timeout/output capture and operation lifecycle.
**產出：** Install request/result API, runner integration, focused tests.
**依賴：** 1.1.
**Owner／Wave：** primary／2.
**Gate／Evidence：** G1, G2; `evidence/index.jsonl`.
**完成門檻：** Only canonical Local regular APKs reach `install -r`; exact serial/arguments and every terminal path are verified.

- [x] 1.2.1 Define install request validation and bounded result/error mapping without adding a public extension ABI.
- [x] 1.2.2 Implement canonical Local regular `.apk` revalidation and stale tool/snapshot rejection before spawn.
- [x] 1.2.3 Implement separate-argument `-s <serial> install -r <path>` execution and accepted success confirmation.
- [x] 1.2.4 Add tests for spaces, Unicode, shell metacharacters, exact serial, signature/downgrade rejection, missing success, disconnect, cancellation, timeout, and no retry/uninstall flags.

## 2. Managed Google Platform-Tools

### 2.1 Download and archive transaction

**目的：** Install an official managed ADB fallback safely without touching system state.
**輸入：** 1.1 shared probe, approved Google-only policy, workspace dependency inventory.
**產出：** Injectable transport, safe extractor, atomic managed-version activation, dependency/license record.
**依賴：** 1.1.
**Owner／Wave：** primary／3.
**Gate／Evidence：** G1, G2; `evidence/index.jsonl`.
**完成門檻：** Valid fixture activates atomically; policy, traversal, type, count, size, cancellation, probe, and promotion failures preserve the prior active version.

- [x] 2.1.1 Select or add minimal HTTP/ZIP dependencies and record source, license, feature, and binary-impact rationale.
- [x] 2.1.2 Centralize the official Google Windows Platform-Tools source and enforce HTTPS host/path plus redirect allowlisting.
- [x] 2.1.3 Implement injectable streaming download with cancellation, connect/read/total deadlines, compressed-byte limit, and progress events.
- [x] 2.1.4 Implement transaction-root creation and ZIP path/type/entry-count/expanded-byte validation with destination-containment checks.
- [x] 2.1.5 Implement expected-layout validation, shared ADB probe, atomic activation, cache invalidation, rollback, and verified temporary cleanup.
- [x] 2.1.6 Add offline valid/malicious archive tests for traversal, rooted paths, links, entry/size limits, missing ADB, failed probe, cancellation, promotion failure, and prior-version preservation.

### 2.2 Application service composition

**目的：** Make resolver, discovery, downloader, and install execution share one lifecycle and observable state.
**輸入：** 1.1, 1.2, 2.1 services and existing app/remote service composition.
**產出：** Application-owned service wiring, refresh/invalidation events, status mapping tests.
**依賴：** 1.2, 2.1.
**Owner／Wave：** primary／4.
**Gate／Evidence：** G1, G3; `evidence/index.jsonl`.
**完成門檻：** App services expose non-blocking snapshots/actions, reuse system ADB for remote provider behavior, and deliver one terminal event per operation.

- [x] 2.2.1 Compose one application-owned ADB tooling service and adapt existing provider creation to validated resolution.
- [x] 2.2.2 Implement asynchronous device refresh, generation invalidation, download-complete refresh, and late-result rejection.
- [x] 2.2.3 Route download and APK install lifecycle/progress/errors through existing operation-status infrastructure.
- [x] 2.2.4 Add app integration tests for cache invalidation, terminal uniqueness, failure recovery, and existing remote-provider compatibility.

## 3. Local APK context-menu experience

### 3.1 Menu contribution and localization

**目的：** Deliver the complete Local APK submenu without blocking or disrupting native Shell commands.
**輸入：** 2.2 application actions/snapshots, owned context-menu architecture, localization catalog.
**產出：** Eligibility policy, submenu states/actions, localized strings, UI tests.
**依賴：** 2.2.
**Owner／Wave：** primary／5.
**Gate／Evidence：** G3, G4; `evidence/index.jsonl`.
**完成門檻：** Eligible APKs show correct named/disabled/download/empty/loading/error/refresh states; ineligible selections never show the submenu; native menu remains responsive.

- [x] 3.1.1 Implement exact single Local regular case-insensitive `.apk` context eligibility and exclude remote/multi/directory/non-APK inputs.
- [x] 3.1.2 Insert the `Install` submenu as the first item followed by a separator while preserving the order of existing Shell/owned commands and menu-session replacement behavior.
- [x] 3.1.3 Render usable device names, duplicate-name rows, disabled state reasons, loading, empty, error, refresh, and missing-ADB download actions from immutable snapshots.
- [x] 3.1.4 Bind rows to captured serial/generation and dispatch download/install asynchronously without using labels as identifiers.
- [x] 3.1.5 Add complete Traditional Chinese, Simplified Chinese, and English strings following the existing localization fallback contract.
- [x] 3.1.6 Add model/UI tests for eligibility, all submenu states, exact dispatch payload, stale session/result rejection, and non-blocking behavior.

## 4. Integration and completion gates

### 4.1 Automated integration and security gates

**目的：** Prove contracts, security controls, regressions, formatting, and packaging readiness together.
**輸入：** All implementation packages and offline fixtures.
**產出：** Raw command outputs, security review, dependency record, evidence index.
**依賴：** 3.1.
**Owner／Wave：** primary／6.
**Gate／Evidence：** G1, G2, G3; `evidence/index.jsonl` and `evidence/final-validation.md`.
**完成門檻：** Every blocking focused/build/security command passes; failures reopen their implementation leaf and are fixed before completion.

- [x] 4.1.1 Run formatter and focused resolver/device/install/downloader/archive tests and retain raw results.
- [x] 4.1.2 Run relevant `explorer-remote`, `explorer-app`, and `explorer-ui` tests plus existing ADB provider regressions.
- [x] 4.1.3 Run workspace checks required by affected crates, dependency/license checks, `git diff --check`, and a credential/secret scan.
- [x] 4.1.4 Review archive containment, redirect policy, process arguments, cancellation/timeout cleanup, stale generations, and bounded diagnostics for security defects and fix all P0/P1 findings.
- [x] 4.1.5 Build the user-installable/test application path and confirm managed-tool directories and new resources are packaged or runtime-created correctly.

### 4.2 User-perspective headful validation

**目的：** Verify the feature as a user across the exact requested workflow and recovery states.
**輸入：** 4.1 passing build, controlled fake ADB, Local APK fixture, optional authorized real device.
**產出：** Screenshots/logs/report for menus, dispatch, status, recovery, and responsiveness.
**依賴：** 4.1.
**Owner／Wave：** primary／7.
**Gate／Evidence：** G4; `evidence/headful/`, `evidence/index.jsonl`.
**完成門檻：** Controlled installed/headful flow passes every requested state; real-device branch is passed or evidence-backed not-applicable; any UX failure is fixed and rerun.

- [x] 4.2.1 Verify one Local APK shows Install while non-APK, directory, multi-select, ADB, and SFTP targets do not.
- [x] 4.2.2 Verify multiple fake devices display their names, retain duplicate-name rows, disable offline/unauthorized rows, and invoke the exact selected serial.
- [x] 4.2.3 Verify no-ADB state offers the explicit Google download, successful fixture installation refreshes devices without restart, and failed/cancelled download remains retryable.
- [x] 4.2.4 Verify `install -r` pending/running/success and failure/cancel/timeout terminals with a Unicode/spaced APK path and no UI/menu freeze.
- [x] 4.2.5 Run one real authorized-device install/update when hardware is available, otherwise record an evidence-backed not-applicable disposition without weakening the fake-device gate.

### 4.3 Final traceability and dual review

**目的：** Close every requirement/task with auditable evidence and repeat both technical and user-perspective review after fixes.
**輸入：** 4.1 and 4.2 evidence, proposal/design/spec/tasks, working diff.
**產出：** Complete evidence index, traceability report, strict OpenSpec validation, final reviews.
**依賴：** 4.1, 4.2.
**Owner／Wave：** primary／8.
**Gate／Evidence：** G5; `evidence/index.jsonl`, `evidence/final-review.md`.
**完成門檻：** Every leaf has a unique passing/not-applicable record, all requirements trace to passing evidence, strict validation passes, and neither review has unresolved P0/P1 findings.

- [x] 4.3.1 Write one unique evidence-index record for every resolved leaf with commands/procedures, expected/actual results, status, hashes, gates, and timestamps.
- [x] 4.3.2 Audit proposal-to-design-to-requirement-to-task-to-evidence traceability and repair every missing or contradictory link.
- [x] 4.3.3 Run the detailed-task validator, placeholder/contradiction scans, and `openspec validate add-local-apk-device-install --strict`.
- [x] 4.3.4 Perform a final technical diff/test review, fix every P0/P1 defect, and rerun affected gates.
- [x] 4.3.5 Perform a fresh user-perspective review of discoverability, wording, device targeting, progress, errors, recovery, and responsiveness; fix every P0/P1 issue and rerun affected headful gates.
