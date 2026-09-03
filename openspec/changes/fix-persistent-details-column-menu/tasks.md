## 1. Persistent native popup contract

### 1.1 Define activation metadata and ordered events

**目的：** Give each application-owned popup command an explicit terminal or persistent-toggle contract with stable resulting-state events.
**輸入：** Approved design, current `OwnedPopupMenuEntry`, immutable menu ordering, G1/G3 requirements.
**產出：** Typed activation metadata, persistent event/session identifiers, and contract tests.
**依賴：** None.
**Owner／Wave：** Primary／Wave 1.
**Gate／Evidence：** G1, G3; `evidence/native-contract.json`.
**完成門檻：** Separators consume no command index; persistent rows publish ordered resulting states; terminal and disabled rows retain specified semantics.

- [ ] 1.1.1 Add explicit terminal and persistent-toggle activation metadata to application-owned popup entries and record `evidence/native-contract.json#1.1.1`.
- [ ] 1.1.2 Define the bounded persistent-event payload and stable popup-session identity contract and record `evidence/native-contract.json#1.1.2`.
- [ ] 1.1.3 Add contract tests for separator indexing, disabled rows, terminal rows, and ordered checked-state events and record `evidence/native-contract.json#1.1.3`.

### 1.2 Keep immersive popup sessions alive for toggles

**目的：** Mutate and repaint check state without ending the native popup message loop or replacing session context.
**輸入：** 1.1 metadata/events, immersive popup row model, HMENU ownership.
**產出：** Persistent activation path, check mutation/repaint, terminal fallback, and lifecycle tests.
**依賴：** 1.1.
**Owner／Wave：** Primary／Wave 2.
**Gate／Evidence：** G1, G4; `evidence/immersive-persistence.json`.
**完成門檻：** Repeated toggles preserve HWND/position/scroll and repaint checks; terminal/dismissal paths still release every resource exactly once.

- [ ] 1.2.1 Implement persistent activation that updates HMENU plus materialized row state and invalidates the existing popup and record `evidence/immersive-persistence.json#1.2.1`.
- [ ] 1.2.2 Keep mouse and keyboard persistent activation inside the same message loop while retaining terminal submenu/command behavior and record `evidence/immersive-persistence.json#1.2.2`.
- [ ] 1.2.3 Terminate the popup on event publication failure and preserve Escape, outside-click, deactivation, and replacement dismissal and record `evidence/immersive-persistence.json#1.2.3`.
- [ ] 1.2.4 Add repeated-toggle, stable-session, scroll preservation, failure cleanup, and resource lifecycle tests and record `evidence/immersive-persistence.json#1.2.4`.

## 2. Foreground Details reconciliation

### 2.1 Add idempotent requested visibility actions

**目的：** Apply the exact native resulting state without rapid-toggle inversion and preserve the required Name column.
**輸入：** Persistent event payload, current Details reducer and per-tab layout.
**產出：** Requested-state action/reducer, validation, durable-state behavior, and tests.
**依賴：** 1.1.
**Owner／Wave：** Primary／Wave 2.
**Gate／Evidence：** G3; `evidence/details-reconciliation.json`.
**完成門檻：** FIFO requested states converge exactly, duplicates are idempotent, stale tab/session data is rejected, and Name remains visible.

- [ ] 2.1.1 Add `SetDetailsColumnVisibility` with column, requested visibility, and session correlation and record `evidence/details-reconciliation.json#2.1.1`.
- [ ] 2.1.2 Reconcile visibility idempotently in the active owning tab while rejecting Name hiding, missing columns, and stale sessions and record `evidence/details-reconciliation.json#2.1.2`.
- [ ] 2.1.3 Add repeated on/off, duplicate, stale-session, fixed-Name, and durable-layout tests and record `evidence/details-reconciliation.json#2.1.3`.

### 2.2 Bridge background popup events to GPUI

**目的：** Deliver each accepted persistent state to `ExplorerRoot` on the foreground context without UI-thread re-entry.
**輸入：** 1.2 publisher, 2.1 requested-state action, current app background popup integration.
**產出：** Bounded session bridge, foreground pump, completion drain, and isolation tests.
**依賴：** 1.2, 2.1.
**Owner／Wave：** Primary／Wave 3.
**Gate／Evidence：** G2, G3, G4; `evidence/foreground-bridge.json`.
**完成門檻：** Events arrive FIFO during one visible session, accepted events drain before completion, late events cannot mutate state, and no popup thread touches GPUI.

- [ ] 2.2.1 Create the bounded per-popup event bridge and pass its publisher into the native popup worker and record `evidence/foreground-bridge.json#2.2.1`.
- [ ] 2.2.2 Pump requested-state events through the owning `ExplorerRoot` foreground context while the popup future remains active and record `evidence/foreground-bridge.json#2.2.2`.
- [ ] 2.2.3 Drain accepted events before terminal completion and reject disconnected, invalid-index, stale-session, or closed-owner delivery and record `evidence/foreground-bridge.json#2.2.3`.
- [ ] 2.2.4 Add concurrency/isolation tests reproducing rapid clicks and proving the prior GPUI re-entry panic does not recur and record `evidence/foreground-bridge.json#2.2.4`.

## 3. Details popup composition and headful workflow

### 3.1 Classify and compose Details commands

**目的：** Mark only enabled column visibility rows persistent while keeping all other commands terminal and preserving ordered native/UI mapping.
**輸入：** 1.x native contract, 2.x UI bridge, current Details popup model.
**產出：** Details entry classification, correlated action mapping, and composition tests.
**依賴：** 1.2, 2.2.
**Owner／Wave：** Primary／Wave 3.
**Gate／Evidence：** G1, G3; `evidence/details-composition.json`.
**完成門檻：** Every displayed row maps to the intended action and policy; Name is disabled; auto-size/target rows terminate; dynamic columns toggle persistently.

- [ ] 3.1.1 Classify built-in and dynamic visibility rows as persistent and auto-size/target-specific rows as terminal and record `evidence/details-composition.json#3.1.1`.
- [ ] 3.1.2 Preserve one command-row index mapping across separators and dynamic registry columns and record `evidence/details-composition.json#3.1.2`.
- [ ] 3.1.3 Add composition tests for built-in, count, folder-size, code-lines, disabled Name, and terminal commands and record `evidence/details-composition.json#3.1.3`.

### 3.2 Extend the real user workflow smoke test

**目的：** Prove persistent native checks and live Details layout changes in one small-window popup session.
**輸入：** Completed 3.1 product, `smoke_details_column_popup.ps1`, native popup test messages, UIA Details headers.
**產出：** Repeatable headful script, screenshots, popup/session report.
**依賴：** 3.1.
**Owner／Wave：** Primary／Wave 4.
**Gate／Evidence：** G5; `evidence/user-perspective/`.
**完成門檻：** One unchanged HWND toggles a column on/off with immediate native and UI proof, remains unclipped, and dismisses correctly with Escape and outside click.

- [ ] 3.2.1 Extend the headful helper to click a native row without assuming popup dismissal and read its checked state and record `evidence/user-perspective/headful.json#3.2.1`.
- [ ] 3.2.2 Verify unchecked→checked→unchecked for one column with unchanged HWND, position, scroll, and corresponding visible/hidden Details header and record `evidence/user-perspective/headful.json#3.2.2`.
- [ ] 3.2.3 Verify disabled Name, terminal auto-size dismissal, Escape dismissal, outside-click dismissal, and small-window non-clipping and record `evidence/user-perspective/headful.json#3.2.3`.
- [ ] 3.2.4 Save user-perspective screenshots plus machine-readable popup geometry, labels, states, and session identity and record `evidence/user-perspective/headful.json#3.2.4`.

## 4. Final gates and evidence closure

### 4.1 Run automated quality and regression gates

**目的：** Verify formatting, compilation, focused behavior, generic context-menu isolation, and artifact traceability after implementation.
**輸入：** All 1.x through 3.x outputs.
**產出：** Final automated result and immutable evidence index.
**依賴：** 3.2.
**Owner／Wave：** Primary／Wave 5.
**Gate／Evidence：** G1–G5; `evidence/final-automated.json`, `evidence/index.json`.
**完成門檻：** Every command exits zero, every L3 has unique evidence, no required scenario is missing, and failures are fixed and rerun rather than waived.

- [ ] 4.1.1 Run cargo fmt check, explorer-shell-win immersive popup tests, explorer-ui Details tests, and explorer-app check and record `evidence/final-automated.json#4.1.1`.
- [ ] 4.1.2 Run generic filesystem context-menu regression/resource tests and verify terminal command semantics remain unchanged and record `evidence/final-automated.json#4.1.2`.
- [ ] 4.1.3 Run the headful persistent workflow repeatedly from a user perspective and fix every failure before recording `evidence/final-automated.json#4.1.3`.
- [ ] 4.1.4 Run strict OpenSpec validation, detailed-task validation, placeholder/contradiction/traceability scan, and git diff check and record `evidence/final-automated.json#4.1.4`.
- [ ] 4.1.5 Write `evidence/index.json` with each task ID, subcheck, procedure, expected/actual result, exit status or reviewer, artifact hash, gates, adjustment lineage, and timestamp.

### 4.2 Perform final implementation and user-perspective review

**目的：** Independently reread the delivered behavior against the approved request and ensure no incomplete, stale, or accidental changes remain.
**輸入：** Passing 4.1 gates and complete evidence index.
**產出：** Final review report and completed task checklist.
**依賴：** 4.1.
**Owner／Wave：** Primary／Wave 6.
**Gate／Evidence：** G1–G5; `evidence/final-review.md`.
**完成門檻：** Code review and user workflow review find no unresolved defect; any finding reopens and repairs its owning leaf before final completion.

- [ ] 4.2.1 Review implementation for thread ownership, session races, idempotency, resource cleanup, and unrelated-work preservation and record `evidence/final-review.md#4.2.1`.
- [ ] 4.2.2 Review the workflow as a user against the supplied screenshot/request, including repeated visible check feedback and no forced reopen, and record `evidence/final-review.md#4.2.2`.
- [ ] 4.2.3 Reconcile task checkboxes with evidence, rerun stale dependent gates, and record the final zero-open-findings disposition in `evidence/final-review.md#4.2.3`.
