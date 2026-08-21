# Owned-shell system interactions design

## Goal

Restore six Explorer-compatible interaction paths while SuperDesktop owns the shell: Win+D minimize/restore, smooth volume dragging, Win+Shift+S screen snipping, notification-area icon interaction, input-profile switching, and Windows notification events. The implementation must use documented Windows contracts, keep Explorer stopped, preserve exact native identities, and surface failures in the console without crashing the UI.

## Chosen approach

Use a bounded state machine at each native boundary and synchronize provider generations before issuing identity-sensitive commands. This preserves the existing provider-host architecture while fixing the root causes instead of adding synthetic input or unbounded polling.

Two alternatives were rejected:

- Delegating these actions to Explorer would reintroduce the shell process, duplicate taskbars, and revive the restart race SuperDesktop is designed to prevent.
- Replaying keyboard/mouse input or polling until the UI happens to match would be timing-sensitive, recursive under hooks, and unable to prove exact-window or exact-icon behavior.

## Keyboard and Show Desktop

The low-level keyboard hook will maintain explicit left/right Windows-key and Shift-key state. Chords are recognized from this tracked state, with `GetAsyncKeyState` retained only as a recovery signal. A Windows-key release opens Start only when no chord consumed the gesture.

Win+D starts a session containing the exact HWND, process ID, and stable window identity of each eligible visible window successfully minimized by SuperDesktop. The second Win+D restores only that set. Because minimized windows are intentionally hidden off the desktop, the restore snapshot is merged with `MinimizedWindowShelf` observations before planning. Stale or reused HWNDs remain excluded.

Win+Shift+S invokes only the Windows `ms-screenclip` overlay. In owned-shell mode a short-lived, verified Explorer broker may be used solely to activate the system protocol and must be cleaned up after the overlay is observed; it must never become the persistent shell.

## Volume and input profiles

The volume slider updates its visible value synchronously with pointer motion. Native writes are submitted through a latest-value-wins coalescer with at most one command in flight; pointer release always commits the final value. This removes provider round-trip latency from rendering while keeping authoritative reconciliation and mute behavior.

Input-profile commands first refresh stale provider generations. Activation uses the documented TSF/input-locale path, then observes the active profile with a bounded retry that tolerates delayed focus propagation. A successful native activation is not downgraded merely because one intermediate snapshot retained the old generation; a fresh snapshot is required before a terminal result is returned. Real failures remain visible in the console and UI state is resynchronized.

## Notification area and Windows notifications

Notification icons retain their version, owner HWND, GUID/ID, callback message, and registration generation. Left click, keyboard select, right click, and context-menu requests map to the version-correct `NIN_SELECT`, `NIN_KEYSELECT`, `WM_CONTEXTMENU`, or legacy mouse message. The host establishes foreground ownership for context menus, validates the icon immediately before dispatch, and removes stale registrations rather than sending to a reused HWND.

The Windows notification host keeps a WinRT apartment alive for the lifetime of `UserNotificationListener`, subscribes to `NotificationChanged`, and marks its snapshot dirty from a panic-contained callback. Refreshes are event-driven with a bounded fallback cadence. Remove, clear, and action requests validate the native notification identity, refresh after completion, and report access-denied or unsupported actions without panicking.

## Error handling and lifecycle

Every command is generation-bound and has one bounded resynchronization path. No callback holds a GPUI `RefCell` borrow across an asynchronous update. Native callbacks only write atomic or mutex-protected state and schedule UI work afterward. Stale identities, timeouts, access denial, and unavailable brokers are logged to the console with subsystem context; the shell continues running.

## Verification

Unit and integration tests cover tracked chord state, two-cycle Win+D with hidden minimized windows, volume command coalescing and final commit, exact tray callback payloads, delayed input-profile observation, and notification-event dirty/refresh behavior. Physical GUI UTIT cases exercise Win+D twice, Win+Shift+S, continuous volume dragging, left/right tray interaction, Win+Space plus mouse input-profile switching, and add/remove/clear Windows notifications. Final verification includes workspace tests, Clippy, release build, installer build, installed-artifact hash checks, and strict OpenSpec validation.
