## Context

Address editing and details-column dragging share the explorer window's root pointer event stream. The column-drag implementation currently attaches an unconditional root `on_mouse_up` that dispatches `CancelDetailsColumnDrag`. The action dispatcher evaluates editor termination before applying the action, so the release from a click that entered address editing immediately cancels that editor. Existing address parsing, per-tab state, focus, and editable-text behavior are otherwise intact.

## Goals / Non-Goals

**Goals:**

- Restore pointer, `Ctrl+L`, and `Alt+D` entry into a persistent address editor.
- Ensure details-column drag lifecycle actions are focus-neutral.
- Retain click-outside editor termination and outside-release drag cleanup.
- Lock the interaction contract with unit/structural tests and real-input UTIT evidence.

**Non-Goals:**

- Change address parsing, history, error recovery, typography, or breadcrumb layout.
- Change details-column ordering, persistence, or drag preview semantics.
- Refactor unrelated focus or pointer interactions.

## Decisions

### Keep the root drag terminal dispatch capture-safe

The root release handler remains attached before a drag begins so GPUI pointer capture can deliver a release outside the window. The cancel reducer already clears state only when a drag exists; with passive lifecycle classification, an inactive cancellation becomes a focus-neutral no-op and cannot close an editor.

The draggable header and its sort child also own `mouse_up_out` terminal handlers guarded by GPUI's active-drag state. This binds cleanup to the elements that establish drag capture, so release over another surface or beyond the valid drop target cancels preview state even when root bubbling is retargeted, without emitting cancellation from ordinary clicks.

Alternative: attach the handler only after drag state becomes active. Rejected by real-input evidence because changing the handler after pointer-down misses the established capture and leaves preview order active after an outside release.

### Treat column-drag lifecycle actions as passive pointer actions

Update, commit, and cancel actions will be added to the centralized passive pointer classifier. Like resize and scrollbar lifecycle actions, they continue or terminate an established interaction rather than expressing a new focus target. This prevents valid terminal actions from independently closing address or rename editors.

Alternative: stop mouse-up propagation only inside the address control. Rejected because it couples a text editor to column dragging and leaves search, rename, and future editors vulnerable.

### Verify public behavior through genuine input

Unit tests will cover action classification and conditional root dispatch. UTIT will use native pointer and keyboard input against the installed/rendered application, assert accessibility state/text changes, and preserve screenshot/report evidence. The scenario will cover address entry and survival as well as genuine drag cleanup to prevent fixing one path by disabling another.

## Risks / Trade-offs

- **Risk: inactive root cancellation adds an internal no-op action** → Passive classification prevents focus effects and the existing reducer returns false without drag state; focused tests and UTIT verify no public effect.
- **Risk: passive classification hides a genuine focus transition** → Only the three internal column-drag lifecycle actions are added; drag start and ordinary click actions retain existing editor termination behavior.
- **Risk: UTIT pointer coordinates vary with DPI** → Resolve targets from UI Automation bounding rectangles and use physical screen coordinates, following existing UTIT patterns.

## Migration Plan

No data migration is required. Ship the UI and UTIT changes together. Rollback consists of reverting the scoped UI changes; no persisted state or ABI is affected.

## Open Questions

None. The approved design fixes the confirmed event-order regression without changing public navigation contracts.
