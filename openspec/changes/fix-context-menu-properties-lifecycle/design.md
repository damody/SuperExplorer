## Context

The visible classic Shell popup is correctly isolated in the existing persistent broker and disposable worker. Built-in Properties is delegated with an immutable popup target, but host execution currently creates a fresh OLE thread, re-resolves the menu, invokes through a temporary hidden owner, and releases the apartment immediately after `InvokeCommand` returns. The reverted `SHObjectProperties` shortcut preserved that lifetime defect and caused later right-click regression. File-row rendering separately composes hover and selected full-row fills before its border.

The fix must preserve complete provider menus, exact pointer targeting, popup replacement, no console flash, and one broker process. Shell COM stays off the UI thread, and COM interfaces/handles do not cross process or apartment boundaries.

## Goals / Non-Goals

**Goals:**

- Show selection as a visible outline without an opaque selected or selected-hover band.
- Open the real target-correct Windows property sheet and keep subsequent genuine right-clicks usable.
- Give host-owned asynchronous Shell work a persistent STA, message pump, and valid app owner HWND.
- Prove behavior with PID-bound physical mouse input and observable result oracles across debug, release, and installed binaries.

**Non-Goals:**

- Replacing the native menu with a GPUI menu tree.
- Adding another broker process or loading third-party handlers on the UI thread.
- Reintroducing `SHObjectProperties`, synthesized `IDataObject`, or `SHMultiFileProperties` as the primary Properties path.
- Broad visual refactoring outside file-row selection.

## Decisions

### Use an outline-only row state machine

One helper computes selected fill, hover eligibility, border color, and text role. Selected rows use the surface interior and active/inactive focus border; hover fill is allowed only for unselected rows. Context-menu pending preserves the active outline across foreground transfer. High contrast uses system Window/WindowText for the interior and Highlight for the border.

Removing only the selected background was rejected because GPUI hover styling can still repaint the whole selected row. Keeping a low-alpha fill was rejected because the requested behavior is a frame rather than a band.

### Use one persistent application-owned Shell STA executor

Host-owned Shell commands move from per-request threads to one bounded queue and one executor initialized with OLE for the application lifetime. It owns a message pump and shuts down deterministically. The existing broker/worker still owns visible popup query, presentation, provider invocation, timeout, and crash isolation.

Per-command threads were rejected because `InvokeCommand` may start asynchronous Shell work that outlives its return. Running on the UI thread was rejected because extensions may block or re-enter.

### Invoke Properties on one host-side native menu instance

The worker delegates the immutable target. On the host STA, the target is resolved into one `IContextMenu`; that interface is queried, its canonical Properties command is located, and the offset from that same query is invoked with `CMINVOKECOMMANDINFOEX`. The command UI owner is the validated real SuperExplorer HWND; the hidden owner is limited to `IContextMenu2/3` message forwarding.

The result remains asynchronous relative to UI dispatch. An invocation HRESULT is only a transport terminal; headful evidence must observe a real property sheet and later usable context menu.

### Test the manual sequence, not isolated endpoints

UTIT injects real Win32 mouse down/up at DPI-correct coordinates, physically clicks the native Properties row, closes the property sheet, right-clicks another non-first item, then physically invokes a safe command. Popup/dialog discovery is restricted to the launched process tree. The sequence repeats ten times with resource snapshots.

Keyboard activation and UIA Invoke were rejected as primary evidence because they bypass the pointer path reported by the user.

## Risks / Trade-offs

- [Some handlers return no canonical Properties verb] → preserve the existing structural classification for the original popup and use a bounded host-side fallback only on the exact target/menu instance.
- [A host Shell command blocks its STA] → isolate it from the UI and visible-popup broker; use bounded queue observability and shutdown handling.
- [A property sheet is hosted outside the app process] → bind evidence by owner/process relationship and target-specific UI fields, not only process ID.
- [Pixel tests vary with DPI/theme] → compare semantic regions and tolerances, and require both empty interior and border contrast rather than fixed RGB alone.
- [Concurrent agents modify chrome or Shell lifecycle] → rebase each focused commit on current HEAD and stop on overlapping tracked edits rather than overwriting them.

## Migration Plan

1. Land failing PID-bound pointer and post-Properties right-click tests.
2. Land selection styling independently.
3. Introduce the persistent host STA and migrate host-owned commands without changing broker IPC.
4. Change Properties invocation and run focused lifecycle tests for ten repetitions.
5. Run context-menu/broker regression suites, full UTIT, release build, installer, installed-path smoke, and strict validation.

Rollback reverts selection and Properties commits independently. The previous broker and immutable target protocol remain compatible.

## Open Questions

None. The reviewed policy is outline-only selection plus a lifecycle-correct native Properties path, verified primarily through genuine mouse input.
