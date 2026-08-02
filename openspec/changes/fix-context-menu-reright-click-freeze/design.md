## Context

`TrackPopupMenuEx` owns a native modal loop in a disposable Shell menu worker. A scoped `WH_MOUSE_LL` hook captures a second right-click over the originating SuperExplorer window, but currently calls `EndMenu` synchronously from the hook callback. The implementation already defers tagged `SendInput` replay until popup teardown, yet synchronous teardown inside the hook can still re-enter the native menu loop and freeze replacement. The approved source design is `docs/superpowers/specs/2026-08-02-context-menu-reright-click-freeze-design.md`.

## Goals / Non-Goals

**Goals:**

- End the old popup without blocking or re-entering the low-level mouse hook.
- Open exactly one replacement popup through the existing real-input hit-test path.
- Keep only the latest complete replacement request and reject stale completion.
- Prove responsiveness, exact target, and bounded resources with deterministic and headful tests.

**Non-Goals:**

- Reimplementing native Shell menus in GPUI.
- Reusing an `IContextMenu` or `HMENU` for a different target.
- Changing extension enumeration, broker IPC, selection semantics, or menu commands.
- Replaying clicks that land on the popup, another process, or obscured content.

## Decisions

### Post cancellation to the popup owner

The hook receives the active hidden popup-owner HWND as scoped state. On a matched second right-button release it records the point, suppresses the physical event, and uses `PostMessageW(..., WM_CANCELMODE, ...)` once. The post is non-blocking and lets the owning native message loop close `TrackPopupMenuEx` normally. Synchronous `EndMenu` in the hook is removed because it permits re-entrant modal teardown.

### Preserve deferred tagged input replay

After `TrackPopupMenuEx` returns, the hook and old popup/menu resources are dropped before scheduling replay. The replay validates the real app owner and point, waits within a bound for physical button release, restores foreground, and submits one tagged `SendInput` down/up batch. Direct semantic dispatch is rejected because it bypasses GPUI hit testing and stable row selection.

### Mouse replacement supersedes stale pending state

The worker publishes the old popup terminal before replaying the captured mouse gesture. Because that replay cannot occur until the old `TrackPopupMenuEx` has returned and released its resources, the UI reducer treats a mouse replacement as the new authoritative pending session immediately. A late old terminal is rejected by request correlation. Keyboard and programmatic replacement remain serialized through the existing single queued request. This avoids relying on terminal-lane wake timing while preventing overlapping native popups.

### Result-based UTIT

The focused headful test uses genuine mouse input: open Alpha's popup, right-click Beta without separately dismissing Alpha, prove the window remains responsive, require exactly one popup, invoke Copy, and verify the clipboard contains Beta. Repeated alternating replacements assert bounded process, hook, window, menu, thread, and handle counts.

## Risks / Trade-offs

- **[Risk] `WM_CANCELMODE` is posted to a stale or wrong HWND.** → Store only the session's owned HWND, verify it is live, and scope the post to the matched owner gesture.
- **[Risk] Posted cancellation is delayed by a broken Shell handler.** → Retain the existing bounded worker deadline and isolation; never wait on the UI thread.
- **[Risk] A replay recursively cancels its own replacement.** → Keep the private `dwExtraInfo` tag and ignore tagged input in the hook state machine.
- **[Risk] A rapid third click supersedes the second.** → Coalesce to the latest complete request and require correlated terminal promotion.

## Migration Plan

1. Add a testable asynchronous-cancel decision and replace hook-time `EndMenu`.
2. Strengthen reducer replacement tests and focused headful UTIT.
3. Register exact OpenSpec coverage and run context-menu regression suites.
4. Roll back the focused cancellation change if native-menu compatibility regresses; no persisted data or protocol migration is involved.

## Open Questions

None.
