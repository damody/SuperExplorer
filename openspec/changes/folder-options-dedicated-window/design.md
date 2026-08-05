## Context

Folder Options is currently rendered by `folder_options_dialog` as an absolute
`folder-options-overlay` inside each `ExplorerRoot`. The draft and page selection live
in `ExplorerState`, and the same typed `ExplorerAction` reducer already owns changes,
Apply, OK, Cancel, reset, and extension toggles. The page shell fixes both width and
height and hides overflow; only selected long sections use nested scrolling. This
couples settings lifetime to one Explorer render tree, blocks the underlying window,
and provides no dedicated right-side scrollbar for the complete page.

`explorer-app` is the GPUI composition root and already creates native GPUI windows.
`explorer-ui` owns reusable controls, semantic tokens, typed actions, scroll geometry,
and the existing Explorer-style scrollbar implementation. The worktree contains
concurrent extension-platform changes, so this change must preserve existing edits
and avoid broad state refactors.

The approved source design is
`docs/superpowers/specs/2026-08-05-folder-options-window-design.md`.

## Goals / Non-Goals

**Goals:**

- Open exactly one application-scoped, modeless Folder Options GPUI window.
- Leave every Explorer window responsive while settings are edited.
- Reuse existing pages and typed reducers without duplicating setting semantics.
- Keep window chrome, page tabs, and action buttons fixed while the current page
  scrolls through a visible right-side scrollbar.
- Preserve one scroll offset per page and isolate all options-window input.
- Implement deterministic draft, Apply-baseline, OK, Cancel, Escape, title-close,
  stale-handle, creation-failure, and shutdown behavior.
- Verify behavior in Rust tests and registered headful UITEST at representative DPI.

**Non-Goals:**

- Replacing GPUI controls with Win32 common controls.
- Changing persisted setting schemas, plugin ABI, page inventory, or extension
  enablement rules.
- Creating more than one options draft or window.
- Redesigning unrelated overlays, menus, About, or file-view scrollbars.

## Decisions

### Application-scoped single-instance controller

`explorer-app` will own a `FolderOptionsWindowController` containing the live GPUI
window handle/entity identity and enough state to clear stale handles. An open request
will update/activate the live window or create one if absent. Creation state is only
published after `open_window` succeeds, so a failure remains retryable. Window close
and application shutdown clear the controller idempotently.

Application scope is chosen over one controller per `ExplorerRoot` because settings
apply user-wide and concurrent drafts would have ambiguous ordering. A Win32 dialog
is rejected because it would duplicate all existing GPUI controls and extension UI.

### Dedicated UI entity with reused page composition

`explorer-ui` will expose a dedicated Folder Options view entity and extract the
current page builders from the overlay shell. The entity owns its draft, Apply
baseline, active page, validation/persistence error, focus handle, and three
`ScrollHandle`s. It emits typed setting intents to an application bridge; it does not
mutate another `ExplorerRoot` directly.

The old absolute overlay and backdrop are removed once open routing is connected.
The existing reducers remain the source of truth for option mutation and reset
semantics. Any minimum extraction needed to make reducers reusable is scoped to
Folder Options and must not create a second setting implementation.

### Versioned setting synchronization

The application snapshots applied settings and a monotonic settings revision when a
new draft opens. Apply validates the draft, commits it through the existing settings
path, persists it, broadcasts the new applied snapshot/revision to every Explorer
window, and replaces the options entity's cancellation baseline. OK performs Apply
and closes only after success. Cancel/Escape/title-close restore no global values;
they simply discard draft changes made after the latest successful Apply.

An external applied-settings update does not overwrite a dirty draft. Apply remains
an explicit last-user-choice commit through the typed path. A clean draft may adopt a
new application snapshot. This preserves predictable user intent without adding a
merge UI.

### Fixed shell and page-local scrolling

The native window has a normal Windows title bar, resizable initial bounds, and a
minimum logical size. Inside it, a fixed header/page-tab region and fixed action
footer surround one `min_h_0` page viewport. The viewport reserves a fixed logical
strip for the vertical track and attaches the current page's `ScrollHandle`.

The existing Explorer scrollbar geometry and pointer-capture behavior will be reused
or factored into a window-agnostic UI helper. Each page keeps its own offset. Track,
thumb, wheel/touchpad, Page Up/Down, Home, and End clamp to current content extent.
When scrolling is unnecessary, the track remains visible with disabled semantic
colors and a full-height thumb. Page content never renders under the reserved strip.

GPUI logical coordinates remain authoritative; native physical pointer coordinates
are converted by the window scale factor exactly once before drag calculations.

### Input, focus, and accessibility isolation

The options entity owns keyboard focus and consumes pointer, wheel, and keyboard
events within its native window. No event route targets Explorer navigation or file
view scroll handles. Tab/Shift+Tab stay within the settings controls. Escape invokes
Cancel exactly once. Page changes focus the first interactive control while retaining
all page offsets. Controls and scrollbar expose stable accessibility names and
disabled/active state.

### Diagnostics and bounded work

Window creation/activation failures and apply/persistence failures emit structured
diagnostics without panicking or blocking the Explorer. Render and scroll operations
perform no filesystem or extension work. Opening an existing window is bounded to
handle validation, state update, and activation.

### Testing and evidence

Rust tests cover controller state transitions, stale handles, draft/baseline rules,
page-local scrolling, offset clamping, DPI conversion, focus/Escape, and source/render
contracts. A registered headful UITEST uses owned fixtures, records native HWND/count
and scroll offsets, and captures screenshots/reports for distinct-window,
single-instance, modeless navigation, visible scrollbar, input isolation, page offset
restoration, actions, resize, and DPI cases.

No test may rely only on source text when a behavior can be exercised through the
typed reducer or headful UI. UITEST failures must retain screenshots and JSON evidence.

## Risks / Trade-offs

- [Cross-window state can diverge] → Apply only through one application-owned typed
  synchronization path with a revision and broadcast; test two Explorer windows.
- [GPUI handle can outlive a closed native window] → Validate/update handles
  fallibly, clear close callbacks idempotently, and retry creation after stale state.
- [Scrollbar events can leak or double-scale at DPI] → Keep a window-local handle,
  stop propagation, reuse shared geometry, and test physical-to-logical conversion at
  100%, 125%, 150%, and 200%.
- [Existing dirty worktree changes overlap large UI files] → Make focused patches,
  never revert unrelated hunks, and review the final diff by path/hunk.
- [Always-visible disabled scrollbar differs from file-list auto-hide] → This is an
  intentional approved requirement that preserves layout and makes the scroll affordance
  explicit.

## Migration Plan

1. Add reusable controller/view seams and tests while the overlay remains available.
2. Connect Open Folder Options to the application controller and synchronize Apply.
3. Replace the overlay shell with the dedicated window and remove overlay-only state.
4. Register and pass headful UITEST, then run focused workspace checks.

Rollback restores the prior open routing and overlay shell; persisted settings require
no migration or rollback because their schema does not change.

## Adaptive implementation governance

- **A — task refinement:** task split/order, file placement, test command, or evidence
  path may change without altering behavior, gates, or public contracts.
- **B — design/spec correction:** an implementation-discovered correction within this
  approved scope pauses affected work and updates design, spec, tasks, and stale
  evidence before continuing.
- **C — material change:** modal ownership, multiple windows/drafts, setting semantics,
  required DPI coverage, platform/framework, external writes, or weakened gates require
  user approval.

No blocking validation or evidence requirement may be silently reduced.

## Open Questions

None. The approved design fixes the modeless, application-singleton, always-visible
scrollbar, and Apply-baseline decisions.
