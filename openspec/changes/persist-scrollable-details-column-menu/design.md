## Context

The chooser popup is rendered by the details file view and its state is explicitly preserved for visibility and auto-size actions. However, `ToggleDetailsColumn` and related menu actions report `FocusSurface::CommandBar`, transferring focus away from the popup owner during the click-triggered rerender. Headful evidence also showed that every inactive column separator emits `EndDetailsColumnResize` on an ordinary release; because that terminal action was unconditionally available, the first false terminal closed the chooser before the row toggle ran. The popup also lays out all registered rows without a maximum height or vertical overflow policy. The approved design requires File Explorer-style persistent multi-selection and access to arbitrary extension columns.

## Goals / Non-Goals

**Goals:**

- Keep the chooser open and interactive across repeated immediate row toggles.
- Retain popup and scroll identity even if the originating header is hidden.
- Bound the popup inside the usable explorer menu area and expose every row through vertical scrolling.
- Preserve existing per-tab settings persistence and the fixed `Name` rule.
- Verify installed behavior with genuine pointer and wheel input.

**Non-Goals:**

- Changing column order, drag/drop, resize, sorting, filter menus, or Folder Options.
- Changing extension registration or settings serialization.
- Introducing a separate chooser dialog.

## Decisions

### Keep chooser actions on the file-view focus surface

`ToggleDetailsColumn`, current/all auto-size, and chooser-specific display toggles will remain associated with `FocusSurface::FileView`, the surface that renders and owns the popup. The existing action-preservation policy remains authoritative for whether the chooser closes.

Alternative rejected: reopen the popup after each update. Reopening resets transient interaction state and produces visible flicker.

### Reject inactive resize terminal actions

`EndDetailsColumnResize` is available only while a resize session is active. Ordinary row clicks can still produce separator release callbacks, but those false terminals become disabled no-ops and cannot dismiss the chooser. A genuine active resize retains the existing terminal transition.

### Use the existing popup identity with bounded GPUI overflow

The existing `details-column-menu` element keeps a stable ID and gains the standard menu maximum-height token plus `overflow_y_scroll`. GPUI's keyed element identity retains scroll state across ordinary rerenders, matching the repository's existing new-menu overflow pattern. If verification shows the framework does not retain the offset for this keyed popup, a dedicated `ScrollHandle` owned by the explorer view is the in-scope fallback; this is a design/spec correction only if it changes no observable requirement.

### Keep one list for built-in and extension columns

The registry-backed ordered row generation remains unchanged. Bounded overflow wraps the complete menu, including auto-size actions, separator, built-in rows, extension rows, and applicable column-specific display actions. There is no alternate paging or truncation path.

### Preserve established dismissal boundaries

The existing dismiss layer, `Esc` handling, navigation/tab transitions, and popup replacement continue to close the chooser. Row clicks stop propagation and therefore do not reach the dismiss layer.

## Risks / Trade-offs

- **Scroll position resets after a rerender** → Verify with UTIT after a bottom-row toggle; add a dedicated view-owned `ScrollHandle` if the stable ID is insufficient.
- **Menu height token exceeds a very short viewport** → Exercise a deliberately short window and verify the bottom edge remains usable; derive a smaller viewport-aware maximum if the fixed token fails the gate.
- **Changing focus ownership affects keyboard behavior** → Run existing chooser `Esc` and action-dispatch tests and add assertions that file-view ownership still closes on explicit dismissal.
- **An originating header is hidden** → Keep popup state independent of visible header enumeration and verify the chooser remains present after toggling that column off.

## Migration Plan

No data migration is required. Build the application and test installer, run Rust tests plus the targeted installed-app UTIT, and capture top/bottom screenshots and a report. Rollback is a source revert; persisted visibility remains compatible.

## Open Questions

None. The `ScrollHandle` fallback is an implementation refinement constrained by the same approved behavior and tests.
