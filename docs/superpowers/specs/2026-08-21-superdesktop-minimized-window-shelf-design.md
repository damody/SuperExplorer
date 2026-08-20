# SuperDesktop Minimized Window Shelf Design

## Goal

When SuperDesktop owns the Windows shell, a minimized application remains represented by its taskbar button but its legacy iconic title tile must not appear at the lower-left desktop edge. Restoring, maximizing, taskbar activation, Alt+Tab, and application-owned restore must retain the original window placement.

## Native contract

Windows represents a minimized top-level window with `WS_MINIMIZE`. Without Explorer's taskbar hosting, the system can arrange that minimized representation as a visible iconic tile in the workspace. Win32 exposes minimized placement separately through `WINDOWPLACEMENT.ptMinPosition`; the restored rectangle remains in `rcNormalPosition`.

SuperDesktop will therefore keep the real minimized state and alter only the minimized position through `GetWindowPlacement` and `SetWindowPlacement` with `WPF_SETMINPOSITION | WPF_ASYNCWINDOWPLACEMENT`.

## Platform boundary

`platform-win::taskbar` adds a `MinimizedWindowShelf` reconciler. It accepts owned snapshot values, not raw caller assumptions. Before changing a window it revalidates the live HWND, PID, stable window identity, visibility, minimized state, and task eligibility. Tool windows, cloaked windows, owned transient windows, invisible windows, retired handles, reused handles, and SuperDesktop surfaces are rejected or ignored.

The shelf coordinate is the conventional off-screen iconic point `(-32000, -32000)`. The adapter never changes `rcNormalPosition`, normal size, maximized position, window styles, ownership, or visibility.

## Runtime flow

The existing 50 ms task snapshot is the authoritative reconciliation input:

1. collect task windows;
2. in owned-shell mode, reconcile eligible minimized windows into the off-screen shelf;
3. retain their normal taskbar models because they stay visible and iconic;
4. prune restored, destroyed, hidden, or identity-changed entries from the shelf cache;
5. report one contextual console error per continuous failing identity, retrying only after its state changes.

SuperDesktop-originated minimize commands call the same validated shelf adapter immediately after `ShowWindow(SW_MINIMIZE)` to avoid a visible interval. Application-originated minimize actions are repaired by the next snapshot. Preview mode performs observation only and never changes host Explorer window placement.

Restore, restore-and-activate, maximize, and close continue through existing actions. Because only `ptMinPosition` changes, Windows restores from the untouched `rcNormalPosition`; the next non-minimized snapshot removes the identity from the shelf cache.

## Failure handling

All Win32 calls return typed errors to the runtime and are printed through the existing contextual console reporter. No production `unwrap`, panic, simulated input, `SW_HIDE`, style mutation, or Explorer fallback is introduced. Cross-thread placement uses `WPF_ASYNCWINDOWPLACEMENT` to avoid blocking the GPUI refresh loop.

## Verification

Unit and fixture tests cover eligibility, idempotence, identity reuse, restored pruning, one-shot error reporting, placement flags, off-screen coordinates, unchanged normal placement, and taskbar retention.

A headful UTIT fixture creates a visible ordinary window with a known restored rectangle, minimizes it through its real system command, and verifies:

- `IsIconic` remains true;
- no minimized window rectangle intersects any monitor or work area;
- the taskbar model still contains the window;
- taskbar activation restores the exact normal rectangle within DPI rounding;
- application-owned minimize receives the same treatment;
- SuperDesktop survives and Explorer/Winlogon Shell state is restored in `finally` cleanup.

The final candidate must pass twice, followed by workspace tests, Clippy with warnings denied, release build, installer build, embedded-binary hash equality, strict OpenSpec validation, and tracked-clean parent/nested repositories.
