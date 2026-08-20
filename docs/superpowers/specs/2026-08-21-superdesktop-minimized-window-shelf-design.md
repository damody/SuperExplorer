# SuperDesktop Minimized Window Shelf Design

## Goal

When SuperDesktop owns the Windows shell, a minimized application remains represented by its taskbar button but its legacy iconic title tile must not appear at the lower-left desktop edge. Restoring, maximizing, taskbar activation, Alt+Tab, and application-owned restore must retain the original window placement.

## Native contract

Windows represents a minimized top-level window with `WS_MINIMIZE`. Without Explorer's taskbar hosting, the system can arrange that minimized representation as a visible iconic tile in the workspace. Win32 exposes minimized and restored placement separately through `WINDOWPLACEMENT`; the restored rectangle remains in `rcNormalPosition`. Live Windows 11 build 26200 evidence also shows that `SetWindowPlacement` clamps a requested `ptMinPosition=(-32000,-32000)` back to the workspace edge `(-2,-2)`, so that API alone cannot hide an Explorer-free iconic tile.

SuperDesktop will therefore keep the real minimized state and use `ShowWindowAsync(SW_HIDE)` only after exact validation proves the window is iconic. A bounded shelf cache retains its taskbar model while Windows preserves the untouched normal placement; `SW_RESTORE` or application-owned restore makes the same window visible again.

## Platform boundary

`platform-win::taskbar` adds a `MinimizedWindowShelf` reconciler. It accepts owned snapshot values, not raw caller assumptions. Before changing a window it revalidates the live HWND, PID, stable window identity, visibility, minimized state, and task eligibility. Tool windows, cloaked windows, owned transient windows, invisible windows, retired handles, reused handles, and SuperDesktop surfaces are rejected or ignored.

The shelf never changes coordinates, size, `rcNormalPosition`, maximized position, styles, ownership, or minimized state. It changes visibility only for the already-iconic representation and retains a copied task snapshot until that exact live identity restores or retires.

## Runtime flow

The existing 50 ms task snapshot is the authoritative reconciliation input:

1. collect task windows;
2. in owned-shell mode, reconcile eligible minimized windows into the hidden shelf;
3. retain their normal taskbar models because they stay visible and iconic;
4. prune restored, destroyed, hidden, or identity-changed entries from the shelf cache;
5. report one contextual console error per continuous failing identity, retrying only after its state changes.

SuperDesktop-originated minimize commands call the same validated shelf adapter immediately after `ShowWindow(SW_MINIMIZE)` to avoid a visible interval. Application-originated minimize actions are repaired by the next snapshot. Preview mode performs observation only and never changes host Explorer window placement.

Restore, restore-and-activate, maximize, and close continue through existing actions. `SW_RESTORE` both restores and shows the hidden iconic window from its untouched normal placement; the next non-minimized snapshot removes the identity from the shelf cache.

## Failure handling

All fallible Win32 observations return typed errors to the runtime and are printed through the existing contextual console reporter. No production `unwrap`, panic, simulated input, placement mutation, style mutation, or Explorer fallback is introduced. Cross-thread hiding uses `ShowWindowAsync` to avoid blocking the GPUI refresh loop.

## Verification

Unit and fixture tests cover eligibility, idempotence, identity reuse, restored pruning, one-shot error reporting, asynchronous hide, cached task retention, and unchanged normal placement.

A headful UTIT fixture creates a visible ordinary window with a known restored rectangle, minimizes it through its real system command, and verifies:

- `IsIconic` remains true;
- the iconic window is not visible on the desktop;
- the taskbar model still contains the window;
- taskbar activation restores the exact normal rectangle within DPI rounding;
- application-owned minimize receives the same treatment;
- SuperDesktop survives and Explorer/Winlogon Shell state is restored in `finally` cleanup.

The final candidate must pass twice, followed by workspace tests, Clippy with warnings denied, release build, installer build, embedded-binary hash equality, strict OpenSpec validation, and tracked-clean parent/nested repositories.
