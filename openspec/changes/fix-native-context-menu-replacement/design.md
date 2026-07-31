## Context

The native Shell popup runs in `TrackPopupMenuEx`, which consumes outside pointer gestures while it owns the modal loop. SuperExplorer currently observes an outside `WM_RBUTTONUP`, calls `EndMenu`, and synchronously emits `mouse_event` down/up. Windows can process that replay before the original physical release and foreground transition settle, so the first popup disappears without a clean GPUI gesture for the second target.

The solution must retain native `IContextMenu` behavior, worker isolation, stable row hit-testing, multi-selection semantics, DPI-correct physical coordinates, and one broker process. The approved source design is `docs/superpowers/specs/2026-07-30-native-context-menu-replacement-design.md`.

## Goals / Non-Goals

**Goals:**

- Replace an open native popup when a complete second right-button gesture lands on visible SuperExplorer content.
- Route the replacement through normal Win32/GPUI pointer hit-testing rather than a direct semantic command.
- Prove the replacement command targets the second item and remains bounded for ten cycles.
- Preserve ordinary native-menu commands, Escape/outside dismissal, background menus, selection behavior, and broker/worker isolation.

**Non-Goals:**

- Reusing the original `HMENU` or `IContextMenu` for another target.
- Opening content hidden beneath the physical bounds of the existing popup.
- Changing the broker protocol, extension discovery, command enumeration, or third-party handler policy.
- Adding another broker or long-lived menu process.

## Decisions

### Capture and suppress the complete second right-button gesture

The scoped `WH_MOUSE_LL` hook will track untagged `WM_RBUTTONDOWN` and its matching `WM_RBUTTONUP` only when both points resolve to the validated originating app root. Those physical events are suppressed while the first popup is active. The completed release calls `EndMenu`; incomplete, moved-off-owner, tagged, or stale gestures clear state without replay.

Capturing both halves avoids delivering an orphaned up/down to GPUI and lets the replacement begin from a clean pointer state. Observing only button-up is rejected because it retains the current ordering race.

### Replay only after native-menu teardown

After `TrackPopupMenuEx` returns, the old hook is removed and popup-owned resources are released before replay. The code restores the real SuperExplorer foreground window, verifies the window and screen point, waits within a small bound until `GetAsyncKeyState(VK_RBUTTON)` reports release, and moves the cursor back to the captured physical point.

A single `SendInput` batch emits tagged right-down/right-up events. `SendInput` is selected over `mouse_event` because it preserves ordered input batching and explicit `dwExtraInfo`; the marker makes the replacement menu ignore its own replay terminal and prevents recursion.

### Keep target resolution in the existing UI path

The replay enters GPUI's existing row/background handlers. `BeginContextItemGesture` selects the exact hit item on down, and `ShowContextMenu` validates the same item identity on up. No item ID crosses from the worker back to the UI, so sorting, virtualization, background targeting, and multi-selection retain their current contracts.

### Strengthen result-based UTIT

The focused headful case will bind popup discovery to the launched process tree, capture the first popup identity, right-click an unobscured second row with genuine physical input, and require one replacement popup plus exact second-row UIA selection. It will physically invoke Copy and require the clipboard file-drop list to contain the second file rather than the first. Ten alternating cycles will assert bounded process, thread, handle, popup, and broker counts.

Separate Escape and outside-left-click paths remain covered so replacement cannot turn normal cancellation into replay.

## Risks / Trade-offs

- **[Risk] Low-level hook suppression could swallow an unrelated right-click.** → Scope capture to untagged events whose points resolve to the exact originating top-level HWND, require a matched down/up pair, and remove the hook with the popup session.
- **[Risk] Windows foreground restrictions could reject the replay.** → Restore the already validated owner immediately after its own menu loop, validate `SetForegroundWindow`/owner liveness, and treat failed `SendInput` as cancellation rather than retrying indefinitely.
- **[Risk] Physical button state can remain down briefly after the hook callback.** → Use a short bounded release wait; never replay while `VK_RBUTTON` remains pressed.
- **[Risk] A new popup can reuse a native HWND.** → UTIT does not rely on HWND inequality alone; it requires old-session disappearance, exact second-row selection, one process-bound popup, and a clipboard result naming the second target.
- **[Risk] Synthetic input could recurse.** → Tag both `SendInput` records and ignore the tag in every replacement hook state transition.

## Migration Plan

1. Add hook-state and input-batch helpers with unit coverage.
2. Replace synchronous `mouse_event` replay without changing public commands or broker protocol.
3. Upgrade the focused UTIT and manifest requirement mapping.
4. Run focused debug/release/installed tests and existing context-menu/broker/resource regressions.
5. Roll back the single implementation commit if native replacement regressions appear; the prior ordinary cancellation behavior remains the fallback.

## Open Questions

None. The approved behavior and failure policy are fully specified.
