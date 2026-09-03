## Context

APK installation is selected in the native context-menu worker and currently converted to `Invoked` on success or `Failed` on error. No event announces that ADB began, the success result has no APK-specific meaning, and UI state cannot isolate concurrent installs. Existing file-transfer progress is byte/percentage oriented and would misrepresent ADB installation.

## Goals / Non-Goals

**Goals:**

- Display an immediate correlated in-app installing notice and one truthful terminal result.
- Support concurrent installs without overwriting or stale completion.
- Keep the UI and native context menu non-blocking and preserve ADB resolution/security rules.
- Reuse the operation-notice visual surface and fade scheduler without reusing invalid transfer metrics.

**Non-Goals:**

- Percentage progress, Windows notifications, remote/split APKs, system PATH changes, or real-device mutation beyond explicitly authorized scope.

## Decisions

### Typed APK lifecycle rather than transfer progress

Add a model event containing request context, safe APK base name, friendly device name, serial identity, and `Started` or terminal status. The model validates transitions and bounds visible strings. This avoids fake bytes/percentages while keeping lifecycle semantics testable.

### Started-before-spawn delivery gate

The worker publishes `Started` before resolving/spawning ADB. If the event cannot be accepted, it rejects the install before spawn; users never get an invisible long-running install. Exactly one terminal is attempted after an accepted start.

### First terminal wins

UI state keys records by request ID, accepts only `Started → terminal`, ignores duplicates/late/stale events, and never evicts an active record to retain terminal history. Terminal classification is succeeded, failed, cancelled, or timed out.

### Existing visual surface, APK-specific view model

The operation notice area renders APK records with an indeterminate activity indicator and localized APK-specific wording. It does not manufacture `OperationProgress`. Success uses the short existing notice lifetime; failure/cancel/timeout use the longer error lifetime.

### Friendly label and execution identity

The menu's device snapshot supplies the friendly display name alongside serial. The worker executes only the serial and revalidates APK/tool/device inputs. UI text uses the friendly label; bounded diagnostics may include serial.

### Evidence corrections

- **A:** refine commands/task mechanics/evidence names without changing requirements or gates.
- **B:** correct an in-scope design/spec assumption, pause affected work, update artifacts, and mark dependent evidence stale.
- **C:** changing user-visible lifecycle, system-first ADB, required terminals, real-device authority, or blocking evidence requires user approval.

## Data flow

1. Native menu returns selected serial and friendly device label with the exact APK target.
2. App allocates a fresh request context and sends `Started` through the existing service event channel.
3. Background worker revalidates and executes `adb -s <serial> install -r <apk>`.
4. Worker maps the outcome to one typed terminal and sends it with the same request ID.
5. UI reducer updates the bounded APK notice record and schedules repaint/fade.

## Risks / Trade-offs

- [Terminal delivery fails] → Log bounded request-correlated diagnostics; never synthesize success.
- [Cancellation/timeout errors are text-only] → Prefer typed cancellation/deadline signals; otherwise conservatively classify as failure.
- [Concurrent notices crowd UI] → Bound history, retain all active records, and evict oldest terminal records first.
- [Friendly device label becomes stale] → Treat it as immutable presentation captured at selection; serial remains execution identity.

## Migration Plan

No persisted data migration. Land model, worker, UI, and tests together. Rollback removes typed notices without changing APK or ADB state.

## Blocking gates

- **G1 Lifecycle:** accepted selection produces Started before spawn and exactly one correlated terminal.
- **G2 Truthfulness:** running UI is indeterminate and all terminal wording matches actual outcome.
- **G3 Isolation:** concurrent, duplicate, late, and stale events cannot overwrite unrelated records.
- **G4 Compatibility:** system-first/managed ADB, argument safety, and non-blocking menu behavior remain intact.
- **G5 User journey:** controlled headful run visibly proves installing then success/failure; supplied APK eligibility is checked without unauthorized mutation.

Evidence lives under `openspec/changes/add-apk-install-status-notices/evidence/`.

## Open Questions

None.
