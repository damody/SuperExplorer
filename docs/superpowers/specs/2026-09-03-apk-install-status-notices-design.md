# APK Install Status Notices Design

## Goal

Give every APK context-menu installation an immediate, truthful in-app status while ADB is running and a clear success, failure, cancellation, or timeout result when it ends. Do not invent percentage progress that ADB cannot reliably provide.

## User experience

- Selecting a device closes the native context menu and immediately creates an in-app notice reading `正在將 <APK 名稱> 安裝到 <裝置名稱>…`.
- The running notice uses an indeterminate activity indicator and never displays a fabricated percentage or byte count.
- Success replaces the running notice with `<APK 名稱> 已成功安裝到 <裝置名稱>`.
- Failure replaces it with `安裝失敗` plus a bounded, user-actionable summary. Cancellation and timeout have distinct wording.
- Success notices fade after a short bounded interval. Failure, cancellation, and timeout notices remain longer so the user can read them.
- Concurrent installations have independent request IDs and records. One completion never overwrites another installation's state.
- The notice includes the APK base name and friendly device display name; the ADB serial remains the execution identity and may be included in diagnostics without replacing the friendly label.

## Architecture

The context-menu selection path will stop collapsing APK installation into an ordinary `ContextMenuOutcome::Invoked`. It will create a typed APK installation request/session and publish a started event before invoking ADB on the existing background worker. Exactly one terminal event follows: succeeded, failed, cancelled, or timed out.

The model owns the status payload and transition rules. The UI stores a bounded set of current/recent APK install notices, keyed by request ID, and renders them through the existing operation-notice surface with APK-specific wording and indeterminate-running presentation. This reuses the visual placement and fade scheduling without pretending the install is a file transfer or adding invalid byte/percentage fields.

The application worker resolves ADB system-first, retains the managed Google Platform-Tools fallback, revalidates the canonical Local APK and exact serial before spawn, and maps cancellation/timeout separately from general failure. The native context-menu thread remains non-blocking from the UI user's perspective.

## State contract

- `Started` creates one running record only when the request ID is new.
- The first valid terminal event wins. Duplicate or late terminal events are ignored.
- Terminal events without a matching started session are rejected from user-visible state but remain diagnosable.
- Stale events from a closed window or replaced generation do not mutate the current notice list.
- Notice capacity and text lengths are bounded. Old terminal notices may be evicted; active notices are not evicted merely to add terminal history.
- No APK path contents, environment values, or unbounded ADB output are displayed or logged.

## Failure handling

- Missing or changed APK, missing ADB, disconnected/unauthorized device, non-zero ADB exit, cancellation, and timeout produce distinct terminal classification where determinable.
- If the started event cannot be delivered, the worker still performs no UI access; the install is rejected before spawn so an invisible long-running operation is not created.
- If terminal delivery fails after spawn, diagnostics retain the request ID and bounded error, while later UI sessions do not synthesize success.
- Existing system ADB resolution remains first. Managed ADB is used only through the existing resolver rules.

## Alternatives

- Reusing byte-oriented file-transfer progress was rejected because APK install does not expose reliable total bytes or percentage semantics and would create misleading UI.
- A single global status string was rejected because concurrent installations would overwrite each other and stale completions could replace newer state.
- Windows system notifications were rejected by user choice; notices remain inside Super Explorer.

## Verification

- Unit tests cover started and every terminal transition, first-terminal-wins, stale/duplicate rejection, capacity, wording, and fade timing.
- Worker tests cover event order, exact request correlation, success, non-zero exit, missing APK/ADB, unauthorized device, cancellation, timeout, and delivery failure before spawn.
- Integration tests prove the menu returns immediately while installation continues in the background and system-first/managed ADB behavior is unchanged.
- Headful tests use a controlled fake ADB to hold an install open long enough to observe `安裝中`, then release success and failure terminals and inspect the in-app result.
- The supplied `qq9.3.55.apk` is used for the final local eligibility and user-journey check without reinstalling it on an unapproved real device; exact real-device mutation remains limited to previously authorized test scope.
- Final verification runs formatting, focused tests, application build/check, OpenSpec validation, diff checks, and a user-perspective review. Any failure reopens the owning task and is repaired before completion.
