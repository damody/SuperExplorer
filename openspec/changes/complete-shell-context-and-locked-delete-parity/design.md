## Context

SuperExplorer already resolves and displays a native `IContextMenu3` inside a disposable broker worker, but it always queries `CMF_NORMAL | CMF_CANRENAME`. That profile omits Explorer/item context and modifier semantics and therefore exposes fewer built-in and third-party commands. File deletion uses `IFileOperation`; sharing and lock failures currently collapse into a generic terminal error with no owner discovery or safe recovery flow.

The change crosses model contracts, broker IPC, the disposable worker, Windows Shell/Restart Manager adapters, the GPUI reducer and modal UI, and UTIT. Shell COM and Restart Manager calls must remain off the UI thread, native handles/interfaces must not cross process or apartment boundaries, and closing an unrelated process requires explicit user authority and strict safety checks.

## Goals / Non-Goals

**Goals:**

- Match File Explorer's complete classic Shell menu for file, folder, multi-item, and background targets, including third-party and owner-drawn submenus.
- Preserve normal versus Shift-extended verb behavior and native keyboard/focus/cancellation semantics.
- Identify processes that lock delete targets through Restart Manager and present a bounded, accessible recovery dialog.
- Support explicit graceful close followed by exactly-once delete retry while excluding unsafe processes and stale identities.
- Finish with deterministic, real-Windows, destructive-fixture, accessibility, interop, and complete UTIT evidence.

**Non-Goals:**

- Reimplementing provider-owned menus in GPUI or serializing arbitrary menu trees over IPC.
- Force termination, privilege elevation, system-wide handle enumeration, credential collection, or closing external processes without a visible confirmation.
- Claiming third-party extension coverage when the extension is not installed.

## Decisions

### Keep the native popup in the disposable worker

`ContextMenuRequest` gains a bounded invocation profile including `extended_verbs`. The worker chooses target-appropriate public flags: item menus include Explorer/item/rename/synchronous-cascade semantics; background menus omit item-only flags; Shift adds extended verbs. `IContextMenu2/3` messages continue to be forwarded through the owner window and only the terminal command offset/cancel/failure crosses IPC.

This preserves lazy and owner-drawn provider behavior. A GPUI menu-tree copy was rejected because it cannot reproduce arbitrary third-party native state, accessibility, or invocation identity. Always enabling extended verbs was rejected because it differs from Explorer and exposes maintenance verbs during ordinary right-click.

### Use structured lock failures and Restart Manager

Windows errors are classified before localization. Only sharing/lock violations create a lock-recovery request. The Windows adapter owns `RmStartSession`, bounded resource registration, `RmGetList`, graceful shutdown, and `RmEndSession`, returning owned typed records rather than handles.

The result is correlated with the failed request, tab generation, delete target identities, and discovery generation. Empty, late, cancelled, oversized, duplicate, or stale results cannot reopen the dialog or retry deletion.

Restart Manager was chosen over system-wide handle scanning because it is the supported Windows resource-owner API, avoids invasive privileges, and matches Explorer-style recovery. Parsing localized `IFileOperation` text was rejected because it is unstable and cannot safely identify processes.

### Graceful close only, with identity revalidation

“Close programs and retry” revalidates PID plus creation time and denies SuperExplorer/broker/worker, system/critical/protected, PID 0/4, and elevated-inaccessible targets. Eligible owners receive only a graceful Restart Manager shutdown request within a fixed deadline. There is no `TerminateProcess` fallback. The original delete is resubmitted once only after a safe terminal close outcome; partial or failed close remains visible and retryable.

This trades guaranteed deletion for protection against data loss and privilege mistakes, matching Explorer's user-safety priority.

### Reducer-owned modal and focus lifecycle

The reducer owns discovery/loading/ready/closing/partial/error state. The dialog provides Close programs and retry, Retry, and Cancel actions; traps modal focus; exposes UIA dialog/list/status/action metadata; announces state changes; and restores the originating file-view focus. Navigation, tab close, window close, or shutdown cancels discovery and dismisses stale state.

### Final UTIT is an acceptance gate

New unit, controlled Restart Manager, native Shell differential, headful UIA/focus, cancellation, and destructive-fixture cases are mapped into UITEST. After targeted cases pass, the complete UTIT suite runs once. A product failure is fixed and the relevant targeted case rerun before the complete gate is accepted; an absent provider/hardware prerequisite is recorded as a truthful skip.

### Make the pointer-hit identity authoritative and audit commands by ownership

The pointer test converts UI Automation rectangles into the native HWND coordinate space before injecting a genuine secondary-button gesture. This avoids DPI virtualization in the PowerShell test host and proves the second, middle, and last visible rows rather than relying on accessibility focus plus Shift+F10.

Each actionable menu ID is also classified from the live `IContextMenu`: application-owned verbs are delegated with the immutable popup target, while all remaining provider-owned IDs are invoked on the same worker-owned COM object. Open, Copy path, Share, and Quick access pin join the existing edit/file-operation commands because their observable result belongs to SuperExplorer navigation, clipboard, window ownership, or durable navigation state. Commands that launch arbitrary installed software, upload data, authenticate, or mutate repositories are inventory-tested and lifecycle-tested but are not blindly activated by unattended UTIT.

### Preserve native ownership for Properties and Pin to Start

Properties and `PinToStartScreen` are resolved and invoked from the original item `IContextMenu` on the long-lived Shell STA. A synthesized `IDataObject` passed to `SHMultiFileProperties` is rejected because virtual or incomplete Shell data can produce the Windows “properties unavailable” error instead of the real property sheet. Pin to Start is delegated before the disposable worker exits so StartMenuExperienceHost activation is not constrained by worker lifetime or process quotas.

The inline rename editor owns pointer events inside its bounds. Mouse down/up used to position or select text stop at the editor container and cannot bubble into the file row’s selection/drag handlers; clicking outside retains the existing Explorer-like blur commit.

### Keep history menus and cloned tabs reducer-owned

Back and Forward secondary-click menus are projections of the active tab's committed `NavigationHistory`, ordered from the nearest destination outward. Selecting an entry begins one correlated navigation request carrying its direction and step count; the stacks are changed only after that exact destination resolves, so a failed multi-step jump leaves the current location and both stacks intact.

The `+` button and Ctrl+T use the same typed `NewTab` action. A new tab receives a value clone of the active tab's current, Back, and Forward entries plus its durable view settings, then receives a new tab identity, request generation, selection, directory snapshot, and cancellation scopes. The two tabs therefore start with equal history but diverge independently. Tab shortcut listeners live on the root action scope that owns keyboard focus rather than an inner child scope.

### Require real Properties results and use inner text bounds for pointer hit-testing

Properties acceptance is target- and result-based: genuine-pointer tests exercise an ordinary file, a filesystem folder, and compatible multi-selection and require the actual Windows property sheet for that immutable popup target. A generic unavailable dialog, wrong target, missing property-page controls, or a success terminal without a property sheet is a failure. Canonical verb discovery may occur in the disposable worker, but the long-lived Shell STA retains target identity and property-sheet ownership.

Address and search inputs share the vendored editable-text pointer transform. Window coordinates subtract the padded inner text origin and scroll offset exactly once before glyph hit-testing. Entering address edit mode creates and selects the input once; a later pointer press while already editing reuses the entity so the text control can place its caret or extend its selection. Focused selection uses opaque semantic Highlight with HighlightText glyphs, including Windows system colors in high contrast, rather than a low-alpha overlay painted over the original foreground.

### Replay native-popup replacement through the real input path

`TrackPopupMenuEx` owns a modal loop and consumes a secondary click outside the popup. The worker observes a completed right-button gesture over the originating application window, ends the old menu, restores foreground ownership, and replays the click through the Win32 mouse input path. Posting synthetic window messages was rejected because GPUI pointer state and hit testing are driven by the real input stream. The replay carries a private `dwExtraInfo` marker so the replacement popup's hook ignores only its own synthesized release instead of recursively opening another menu; hardware input and UTIT injection remain eligible. The replay is restricted to an unobscured point whose root window is the originating SuperExplorer window; clicks on the menu, desktop, or another application remain ordinary cancellation.

## Risks / Trade-offs

- [Some Shell handlers ignore or mishandle the richer query profile] → retain worker deadlines, quarantine, crash isolation, bounded command ranges, and safe cancellation fallback.
- [Third-party menus differ by machine, Windows build, target, and install bitness] → compare brokered and direct native queries on the same host and record provider inventory/prerequisite skips.
- [Restart Manager reports stale or incomplete owners] → revalidate PID creation identity immediately before shutdown and never infer that an empty list means no owner.
- [A process refuses graceful shutdown] → report the per-process failure and preserve Retry/Cancel; never escalate to force termination.
- [Closing an application can still lose unsaved work] → require an explicit modal action, list affected applications, and never close protected/ineligible owners.
- [A lock disappears during discovery] → permit plain Retry and treat a no-longer-running owner as a benign stale result without closing a replacement PID.

## Migration Plan

1. Add backward-compatible model defaults for ordinary context invocation and typed lock recovery.
2. Version the broker payload and deploy app, broker, and worker binaries as one verified set.
3. Enable complete target profiles and differential tests before enabling locked-delete UI.
4. Enable Restart Manager discovery, then graceful shutdown/retry behind the typed reducer.
5. Run targeted and complete UTIT plus installed-path validation.

Rollback can restore the prior normal context query profile. Lock recovery can be disabled independently, leaving the existing generic delete failure and safe navigation intact. Neither rollback modifies user files or persistent state.

## Open Questions

None. The approved policy is complete native menus and graceful process close without force termination.
