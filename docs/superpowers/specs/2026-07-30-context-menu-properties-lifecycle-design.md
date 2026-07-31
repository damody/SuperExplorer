# Context Menu Selection and Properties Lifecycle Design

## Scope

Fix two regressions without replacing or duplicating the existing native context-menu broker:

1. A selected file row uses a focus outline instead of an opaque full-row highlight, including while a native popup owns foreground focus.
2. Selecting Properties opens the real Windows property sheet for the immutable popup target, and closing it leaves the next genuine mouse right-click fully functional.

The implementation must preserve complete third-party Shell menus, exact non-first-row targeting, right-click replacement, Escape/outside dismissal, and one persistent broker process.

## Current Failure Mechanism

File rows currently apply `row_hover`, then paint `selected_active` or `selected_inactive` across the full item before adding a border. Removing only the selected fill is insufficient because the hover pseudo-state can still repaint a selected row.

Properties is delegated out of the disposable menu worker, but `start_host_owned` creates a new short-lived OLE thread for each request. That thread re-resolves an `IContextMenu`, invokes a numeric offset with a temporary hidden owner, then releases the apartment, owner window, and COM interfaces when `InvokeCommand` returns. Windows Shell handlers may retain asynchronous work after that return. The reverted `SHObjectProperties` shortcut kept the same lifetime defect and bypassed the target-complete `IContextMenu` path.

## Design

### Selection outline

Centralize file-row visual state in one helper used by Details, List, and icon views. In normal and high-contrast themes, selected rows keep the surface interior and use an active or inactive semantic border. Hover fill applies only to unselected rows. A context-menu-pending selection retains its active outline even if the app temporarily loses foreground focus. Text keeps the normal surface foreground because there is no selected fill behind it.

### Persistent host Shell STA

Replace per-request `start_host_owned` threads with one bounded, application-owned Shell STA executor initialized once and shut down with the application. It owns its OLE apartment, message pump, and command queue for Properties, Share, and Pin to Start. This is not another broker process; the visible native popup remains in the existing persistent broker and isolated worker.

### Properties invocation

The disposable popup worker continues to classify Properties and return a typed delegated terminal containing the immutable popup target. The host Shell STA resolves that target, obtains one host-side `IContextMenu`, queries it, locates the canonical `properties` command, and invokes that command on the same interface instance.

Invocation uses `CMINVOKECOMMANDINFOEX`, the validated real SuperExplorer HWND as UI owner, Unicode fields, and the command offset obtained from the same queried menu. The hidden message window remains responsible only for `IContextMenu2/3` forwarding. The persistent STA prevents Shell-owned asynchronous property-page work from losing its parent apartment or owner immediately after invocation.

`SHObjectProperties` and synthesized `IDataObject`/`SHMultiFileProperties` are explicitly excluded because they either bypass target-complete Shell semantics or previously produced a generic unavailable dialog.

## Error and Lifecycle Rules

- A failed Properties invocation terminates only its correlated request and cannot poison the next popup.
- A successful HRESULT is not accepted as proof of a working property sheet; headful UTIT must observe the real target sheet.
- The UI thread never performs Shell COM work or waits for the property sheet.
- App shutdown closes the host queue, drains/cancels pending work, destroys its owner window, uninitializes OLE, and joins the executor.
- No localized menu label is the primary command identity; structural label fallback remains only for identifying the original built-in Properties entry when a handler omits its canonical verb.

## Test Design

Use genuine Win32 mouse down/up injection at DPI-correct physical coordinates. Bind every popup and dialog oracle to the exact app/broker/worker PIDs created by the test.

The focused test sequence is:

1. Right-click a non-first file and activate Properties by physically clicking its native menu row.
2. Require a real target property sheet and reject Desktop/generic unavailable dialogs.
3. Close the sheet with Escape.
4. Right-click a different non-first item and require a complete replacement native popup.
5. Physically click a safe built-in command and verify its observable result.
6. Repeat the sequence ten times and assert bounded broker, worker, host-thread, window, handle, and menu counts.

Properties coverage includes file, folder, executable, script, and compatible multi-selection targets. Selection visual coverage samples the row interior and all four border edges for selected idle, selected hover, native-popup focus loss, inactive window, high contrast, Details view, and an icon view.

## Acceptance Gates

- No selected or selected-hover state paints an opaque full-row band.
- The selected target remains obvious under a native popup.
- Every supported target opens its actual Windows property sheet.
- Closing Properties never prevents the next real right-click from opening and invoking a complete menu.
- Ten lifecycle repetitions leave one broker and no accumulated worker, host-thread, owner-window, or native-menu resources.
- Debug, release, and installed binaries pass the same focused flow.
- Focused context-menu suites, full UTIT, release build, installed-path smoke, and strict OpenSpec validation pass.

## Delivery and Rollback

Deliver selection styling and Properties lifecycle as separate commits so either can be reverted independently. Avoid broker protocol changes unless a failing target proves that the existing immutable descriptor is insufficient. Do not touch untracked workspace content such as `SteamLibrary/`.
