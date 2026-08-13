## 1. State and command lifecycle

### 1.1 Idle restored-tab eligibility

**目的：** Provide one state-owned decision for starting an unloaded active tab without retrying loading, ready, or failed tabs.
**輸入：** Approved design; `DirectoryState`; active-tab history and request-generation APIs.
**產出：** Idle-only active-location load operation and focused state tests.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1; owned path `crates/explorer-ui/src/state.rs`; forbidden paths session schema and directory service API.
**Gate／Evidence：** G1; `evidence/index.json` records `1.1.*` with focused test commands and source hashes.
**完成門檻：** Every directory-state class has a named test and only `Idle` creates one correlated navigation command.

- [x] 1.1.1 Add an active-tab load operation that returns a normal navigation command only when the active directory state is `Idle`.
- [x] 1.1.2 Add a state test proving an idle restored tab creates one command with its current resolved location.
- [x] 1.1.3 Add state tests proving `Loading`, `Ready`, and `Error` tabs create no automatic activation command.
- [x] 1.1.4 Record G1 focused-test results, exit status, timestamp, and changed-source hashes in the evidence index.

### 1.2 Shared post-activation submission

**目的：** Route every action that can reveal an idle restored tab through one service-submission path.
**輸入：** 1.1 idle-only operation; root dispatcher; existing `submit_command` failure synthesis.
**產出：** Shared post-action helper and exact-submission-count regression tests.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / Wave 1; owned paths `crates/explorer-ui/src/lib.rs` and focused tests; forbidden paths service implementation and persisted schema.
**Gate／Evidence：** G1 and G2; `evidence/index.json` records `1.2.*`.
**完成門檻：** Pointer activation, keyboard cycling, and active-tab closure load an idle tab exactly once; new-tab and non-idle paths do not duplicate work.

- [x] 1.2.1 Add one post-action helper that submits the idle active-tab command through the existing service boundary.
- [x] 1.2.2 Invoke the helper after pointer activation, next/previous cycling, close-active, and close-tab actions while excluding the existing new-tab pending-command path.
- [x] 1.2.3 Add root-level tests proving each active-tab-changing action submits exactly one command for an idle restored destination.
- [x] 1.2.4 Add root-level tests proving repeated activation while loading and revisiting ready/error tabs submit no duplicate command.
- [x] 1.2.5 Add a service-admission test proving rejection becomes the existing retryable directory error rather than a persistent idle/disconnected state.
- [x] 1.2.6 Record G1/G2 root and failure-path evidence with unique task records and hashes.

## 2. Restart regression coverage

### 2.1 Two-process restored-tab UTIT

**目的：** Prove the installed production path restores and automatically loads active and background filesystem tabs across a real restart.
**輸入：** Wave 1 behavior; existing isolated session restore headful harness and UI Automation helpers.
**產出：** Restart automation, screenshots, logs, machine-readable assertions, and manifest registration.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator / Wave 2; owned paths the selected restart smoke script and `uitest/manifest.json`; shared manifest edits remain primary-owned.
**Gate／Evidence：** G3; formal UTIT output plus `evidence/index.json` records `2.1.*`.
**完成門檻：** A clean two-tab session restart displays the active fixture immediately and the background fixture after UIA activation without F5 or a persistent disconnected message.

- [x] 2.1.1 Extend or add an isolated-profile headful fixture that persists two distinct filesystem tabs and closes the first process cleanly.
- [x] 2.1.2 Start a second SuperExplorer process against that profile and assert the restored active tab displays its expected fixture entry without refresh.
- [x] 2.1.3 Activate the restored background tab with UI Automation and assert its expected fixture entry appears without refresh.
- [x] 2.1.4 Fail the case if `Directory service is not connected` remains visible after either restored tab becomes active.
- [x] 2.1.5 Emit required active-tab and background-tab screenshots, process logs, and structured report fields.
- [x] 2.1.6 Register the case, timeout, requirements, cleanup, and required artifacts in `uitest/manifest.json`.
- [x] 2.1.7 Attempt the formal UTIT runner and, when unrelated global manifest validation blocks preflight, execute the registered case script directly with fail-on-error and index both the preflight failure and G3 PASS evidence.

## 3. Integration and release evidence

### 3.1 Final validation and audit

**目的：** Demonstrate the behavior integrates without formatting, build, session, or unrelated test regressions.
**輸入：** Waves 1-2 implementation and evidence.
**產出：** Passing locked/offline checks, strict OpenSpec validation, reviewed diff, and complete evidence index.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / Wave 3; repository-wide read access, edits limited to this change's evidence/task disposition and necessary owned implementation files.
**Gate／Evidence：** G4; `evidence/index.json` records `3.1.*` and links immutable shared logs by subcheck.
**完成門檻：** Formatting, focused tests, UI/app build, formal UTIT, and strict OpenSpec validation pass; no unresolved P0/P1 finding or temporary diagnostic hook remains.

- [x] 3.1.1 Run `cargo fmt --all -- --check` and record the result.
- [x] 3.1.2 Run the focused explorer-ui restored-tab tests locked and offline and record the result.
- [x] 3.1.3 Build the SuperExplorer binary locked and offline and record the result.
- [x] 3.1.4 Run the relevant existing session-restore tests and record the result.
- [x] 3.1.5 Run `openspec validate autoload-restored-tab-directories --strict` and record the result.
- [x] 3.1.6 Scan the changed paths for temporary diagnostics, placeholders, unrelated edits, and public/session-schema drift.
- [x] 3.1.7 Review proposal-to-design-to-spec-to-task-to-evidence traceability and resolve every P0/P1 finding.
- [x] 3.1.8 Finalize `evidence/index.json` with one terminal record or immutable subcheck for every resolved L3 task.
