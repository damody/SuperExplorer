# Native Context Menu Replacement Design

## Scope

Match Windows File Explorer when the user right-clicks a second visible file or folder while a
native Shell context menu is still open. The first popup must close, the second pointer target
must become selected, and a new complete native Shell menu for that exact target must open.

Clicks on the existing popup remain menu interactions. Replacement applies only when the second
right-button gesture lands on visible SuperExplorer content outside the popup.

## Current Failure

The menu worker installs a low-level mouse hook and observes only `WM_RBUTTONUP`. It calls
`EndMenu`, immediately restores foreground, and synchronously injects `mouse_event` down/up from
the same native-menu completion path. Depending on Windows input ordering, the synthetic gesture
can run before the original physical button-up and foreground transition have fully settled. The
old popup closes, but GPUI never receives a clean second right-button gesture, so no replacement
menu appears.

The current UTIT checks selection and the presence of some worker popup, but it does not prove
that the old popup was replaced or that a command targets the second file. This permits false
passes when an unrelated or stale popup remains observable.

## Considered Approaches

### Deferred physical input replay — selected

Capture and suppress the complete second physical right-button gesture while the old menu owns
the modal loop. End the old menu after the captured button-up, restore the real app window, wait
until Windows reports the physical right button released, then inject a tagged `SendInput`
right-down/right-up pair at the captured physical screen coordinate.

This retains the real GPUI hit-testing, selection, pointer-session proof, background-versus-item
routing, DPI behavior, and normal `ShowContextMenu` path. It is the closest automation analogue
to a user releasing and pressing the mouse again.

### Direct semantic `ShowContextMenu` dispatch — rejected

The worker could send coordinates or a stable item identity directly to the UI. This avoids input
timing, but bypasses pointer hit-testing and could hide the same class of first-row and
background-target regressions that the user has observed.

### Reuse and reposition the existing `HMENU` — rejected

An `IContextMenu` and its command offsets belong to the original immutable target. Reusing it for
another row would execute commands against the wrong file and violate Shell extension lifetime
rules.

## Input and Lifecycle Design

The scoped low-level hook tracks a small right-button replacement state:

1. On an untagged `WM_RBUTTONDOWN`, resolve the screen point with `WindowFromPoint`. If its root
   is the originating SuperExplorer HWND, store the point and suppress the event so the app cannot
   receive an orphaned button-down while the popup is active.
2. On the matching untagged `WM_RBUTTONUP`, require that the point still belongs to the same app,
   store the completed gesture, call `EndMenu`, and suppress the event.
3. Let `TrackPopupMenuEx` return and fully destroy the old popup/menu owner before replay.
4. Restore foreground to the validated SuperExplorer HWND. Confirm the HWND and replay point are
   still valid, and wait for the physical right-button state to be released.
5. Move the cursor to the captured physical point and use one tagged `SendInput` batch containing
   right-down and right-up. The tag prevents the replacement menu's hook from recursively treating
   injected input as another replacement request.

If capture is incomplete, the target window changes, the owner is destroyed, or input injection
fails, the operation remains an ordinary cancellation. No stale gesture is retained across menu
sessions.

## Compatibility Rules

- Clicking an entry or submenu in the current native popup is unchanged.
- Escape and left-click outside dismissal remain unchanged.
- Right-clicking visible file-view background replaces the item menu with the normal background
  menu through existing hit-testing.
- An already selected multi-selection remains intact when the second target belongs to it;
  right-clicking an unselected row selects only that row, matching Explorer.
- Visible popup isolation remains in the existing worker and broker; this change creates neither
  another broker process nor a parallel context-menu protocol.
- The replay path is bounded and cannot recursively replay its own tagged input.

## UTIT Design

Strengthen `context-menu-selection-replacement-headful` with genuine DPI-correct input:

1. Launch one isolated SuperExplorer process with `Alpha.txt`, `Beta.txt`, and a first-row sentinel.
2. Physically right-click Alpha and capture the exact popup HWND and process identity.
3. Without dismissing it, physically right-click a visible portion of Beta.
4. Require Alpha's popup to disappear, Beta to be selected, and exactly one popup belonging to the
   launched process tree to remain.
5. Physically click Copy in the replacement popup and require the clipboard file-drop list to
   contain Beta and not Alpha.
6. Repeat the replacement flow ten times, alternating targets, and require bounded broker, worker,
   popup, thread, and handle counts.
7. Keep separate Escape and outside-click dismissal assertions so replacement cannot weaken normal
   cancellation.

Unit tests cover hook state transitions, tagged-input rejection, incomplete gestures, wrong-owner
points, and replay failure cleanup. Existing broker, worker, complete-menu, Properties lifecycle,
focus, and resource-soak suites remain mandatory regressions.

## Acceptance Criteria

- A right-click on a second visible item while the first popup is open always opens a complete
  replacement native menu for the second item.
- The first popup cannot remain actionable after replacement.
- The second item is visibly selected and a safe command demonstrably targets it.
- No first-row fallback, background-menu substitution, duplicate popup, recursive replay, extra
  broker, or accumulating worker/thread/handle remains after ten cycles.
- Debug, release, and installed artifacts pass the same focused behavior.

## Delivery

Deliver the native input/lifecycle change and its UTIT coverage in an independently revertible
commit. Preserve unrelated workspace content, including the untracked `SteamLibrary/` directory.
