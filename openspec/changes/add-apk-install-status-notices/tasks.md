## 1. Typed APK lifecycle

### 1.1 Define request and terminal contracts

**目的：** Model one safe, correlated APK install lifecycle without transfer percentages.
**輸入：** Approved design, current context-menu outcomes, RequestContext and cancellation contracts.
**產出：** Typed started/terminal payloads, bounded presentation fields, transition helpers.
**依賴：** None.
**Owner／Wave：** Primary／Wave 1.
**Gate／Evidence：** G1–G3; `evidence/model-lifecycle.json`.
**完成門檻：** Started and four terminals are representable; fields are bounded; request identity and first-terminal semantics are testable.

- [ ] 1.1.1 Add typed APK install started and succeeded/failed/cancelled/timed-out event payloads and record `evidence/model-lifecycle.json#1.1.1`.
- [ ] 1.1.2 Add bounded APK/device/error presentation normalization without exposing full paths or output and record `evidence/model-lifecycle.json#1.1.2`.
- [ ] 1.1.3 Add model tests for valid transitions, first terminal, duplicate/late/unmatched events, and independent request IDs and record `evidence/model-lifecycle.json#1.1.3`.

### 1.2 Preserve selected device presentation identity

**目的：** Carry friendly device name from menu selection while retaining exact serial execution identity.
**輸入：** Existing ADB device snapshot and `InstallApk` context outcome.
**產出：** Friendly label in the selected outcome/request and serialization/tests where applicable.
**依賴：** 1.1.
**Owner／Wave：** Primary／Wave 1.
**Gate／Evidence：** G1, G4; `evidence/device-identity.json`.
**完成門檻：** Menu label and serial remain correlated through broker/app boundaries; malformed or stale identity is rejected.

- [ ] 1.2.1 Extend APK menu outcome wiring with immutable friendly device label alongside serial and record `evidence/device-identity.json#1.2.1`.
- [ ] 1.2.2 Update broker encoding/decoding and compatibility tests for the extended APK selection and record `evidence/device-identity.json#1.2.2`.
- [ ] 1.2.3 Add stale/malformed serial-label mapping tests and verify serial remains the only execution selector and record `evidence/device-identity.json#1.2.3`.

## 2. Worker sequencing and outcome mapping

### 2.1 Publish started before ADB spawn

**目的：** Guarantee visible running status before any long-running install can begin.
**輸入：** 1.x contracts, brokered context-menu worker, service event channel.
**產出：** Fresh request allocation, pre-spawn started gate, non-blocking worker flow.
**依賴：** 1.2.
**Owner／Wave：** Primary／Wave 2.
**Gate／Evidence：** G1, G4; `evidence/worker-start.json`.
**完成門檻：** Accepted installs publish Started before resolve/spawn; failed delivery prevents spawn; menu returns promptly.

- [ ] 2.1.1 Allocate an APK install request context and publish Started before resolver/process work and record `evidence/worker-start.json#2.1.1`.
- [ ] 2.1.2 Reject installation before spawn when Started delivery is unavailable and emit bounded diagnostics and record `evidence/worker-start.json#2.1.2`.
- [ ] 2.1.3 Add ordering and responsiveness tests proving menu completion, Started, and ADB spawn sequence and record `evidence/worker-start.json#2.1.3`.

### 2.2 Map every worker terminal exactly once

**目的：** Convert ADB/validation/cancellation/deadline outcomes into one correlated terminal without losing existing resolver guarantees.
**輸入：** 2.1 accepted request, ADB resolver/runner errors and cancellation token.
**產出：** Terminal classifier, exactly-once publisher, system-first compatibility tests.
**依賴：** 2.1.
**Owner／Wave：** Primary／Wave 2.
**Gate／Evidence：** G1, G2, G4; `evidence/worker-terminal.json`.
**完成門檻：** Success/failure/cancel/timeout map distinctly; exactly one terminal is attempted; system-first and managed fallback stay unchanged.

- [ ] 2.2.1 Map successful and non-zero/validation failures to correlated terminals with bounded summaries and record `evidence/worker-terminal.json#2.2.1`.
- [ ] 2.2.2 Map cancellation and configured deadline expiry distinctly and prevent later success and record `evidence/worker-terminal.json#2.2.2`.
- [ ] 2.2.3 Add fake-runner tests for missing APK/ADB, unauthorized/offline device, success, failure, cancellation, timeout, and duplicate callback and record `evidence/worker-terminal.json#2.2.3`.
- [ ] 2.2.4 Verify existing system ADB precedence, managed fallback, canonical APK, exact serial, and argument-safe `install -r` remain intact and record `evidence/worker-terminal.json#2.2.4`.

## 3. In-app notice state and rendering

### 3.1 Build bounded concurrent notice state

**目的：** Retain active/recent APK installs independently with stale-event and eviction safety.
**輸入：** 1.1 lifecycle events, current AppViewState notice timing.
**產出：** APK notice records/reducer, capacity, deadlines, and state tests.
**依賴：** 1.1.
**Owner／Wave：** Primary／Wave 2.
**Gate／Evidence：** G2, G3; `evidence/ui-state.json`.
**完成門檻：** Concurrent records remain isolated; first terminal wins; active records survive capacity pressure; success/error deadlines differ.

- [ ] 3.1.1 Add bounded APK notice records keyed by request ID and apply Started plus first-terminal-wins transitions and record `evidence/ui-state.json#3.1.1`.
- [ ] 3.1.2 Implement oldest-terminal-first eviction, stale generation rejection, and independent fade deadlines and record `evidence/ui-state.json#3.1.2`.
- [ ] 3.1.3 Add reducer tests for concurrency, reversed completion, duplicates, unmatched/stale events, capacity, and time-based removal and record `evidence/ui-state.json#3.1.3`.

### 3.2 Render truthful localized notices

**目的：** Show immediate installing activity and clear terminal wording on the existing in-app notice surface.
**輸入：** 3.1 view state, operation notice layout/theme primitives.
**產出：** APK notice view model/render, indeterminate animation, accessibility labels, visual tests.
**依賴：** 3.1.
**Owner／Wave：** Primary／Wave 3.
**Gate／Evidence：** G2, G5; `evidence/ui-render.json`.
**完成門檻：** Running/success/failure/cancel/timeout text is correct, no percent/bytes appear, multiple rows render, and repaint/fade scheduling is active.

- [ ] 3.2.1 Add localized running and terminal formatters using APK base name and friendly device label and record `evidence/ui-render.json#3.2.1`.
- [ ] 3.2.2 Render indeterminate running activity and multiple bounded APK notices through the in-app operation notice area and record `evidence/ui-render.json#3.2.2`.
- [ ] 3.2.3 Add accessibility/render tests proving truthful text, absent percentage/bytes, theme behavior, and success/error retention and record `evidence/ui-render.json#3.2.3`.

## 4. Integration, user journey, and closure

### 4.1 Prove controlled and supplied-APK workflows

**目的：** Verify visible Started and terminal states without relying on an unauthorized real-device mutation.
**輸入：** Completed 2.x/3.x product, controlled fake ADB, `qq9.3.55.apk`, headful harness.
**產出：** Headful script/reports/screenshots and supplied-APK eligibility evidence.
**依賴：** 2.2, 3.2.
**Owner／Wave：** Primary／Wave 4.
**Gate／Evidence：** G5; `evidence/user-perspective/`.
**完成門檻：** Held fake install visibly shows running then success/failure; menu remains responsive; supplied APK eligibility is proven without silent real install.

- [ ] 4.1.1 Add controlled fake ADB gates that hold, succeed, fail, cancel, and time out with deterministic markers and record `evidence/user-perspective/headful.json#4.1.1`.
- [ ] 4.1.2 Run headful install to capture immediate `安裝中` then success and failure terminal notices and record `evidence/user-perspective/headful.json#4.1.2`.
- [ ] 4.1.3 Verify concurrent notice isolation, fade timing, menu responsiveness, and absence of fabricated progress from a user perspective and record `evidence/user-perspective/headful.json#4.1.3`.
- [ ] 4.1.4 Use `qq9.3.55.apk` for final Local menu/status eligibility and document real-device mutation as not-authorized/not-performed and record `evidence/user-perspective/qq-apk.json#4.1.4`.

### 4.2 Run final gates and independent review

**目的：** Close every contract with automated, OpenSpec, code-review, and user-review evidence; repair rather than waive failures.
**輸入：** All implementation and 4.1 evidence.
**產出：** Final automated report, evidence index, final review, checked tasks.
**依賴：** 4.1.
**Owner／Wave：** Primary／Wave 5.
**Gate／Evidence：** G1–G5; `evidence/final-automated.json`, `evidence/index.json`, `evidence/final-review.md`.
**完成門檻：** All commands pass, every L3 maps to evidence, no unresolved finding remains, and failed/stale gates are rerun after repair.

- [ ] 4.2.1 Run formatting, model/app/UI/ADB focused tests, explorer-app check/build, and context-menu regressions and record `evidence/final-automated.json#4.2.1`.
- [ ] 4.2.2 Run headful user-perspective checks repeatedly and repair every failure before recording `evidence/final-automated.json#4.2.2`.
- [ ] 4.2.3 Run strict OpenSpec/task validation, placeholder/traceability scan, and git diff check and record `evidence/final-automated.json#4.2.3`.
- [ ] 4.2.4 Review thread ownership, terminal races, eviction/fade, text safety, ADB compatibility, and unrelated-work preservation and record `evidence/final-review.md#4.2.4`.
- [ ] 4.2.5 Review the complete workflow as a user, reconcile every checkbox with unique evidence metadata, and write the zero-open-findings disposition in `evidence/index.json` and `evidence/final-review.md#4.2.5`.
