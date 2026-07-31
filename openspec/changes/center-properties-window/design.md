## Context

Properties is a host-owned canonical context-menu verb executed on SuperExplorer's persistent Shell
STA with the real app HWND. The Shell creates the native property sheet, but its default placement
currently resolves to the desktop origin. Placement must occur before activation without polling,
replacing the property sheet, or weakening the persistent STA lifecycle.

## Goals / Non-Goals

**Goals:**

- Center native Properties sheets relative to the validated active SuperExplorer window.
- Use a deterministic monitor-work-area fallback and keep the whole usable sheet on screen.
- Preserve native handler content, size, focus, ownership, Z-order, and cancellation behavior.
- Prove placement and post-close menu usability with physical-pointer UTIT.

**Non-Goals:**

- Reimplementing Properties UI or modifying property pages.
- Repositioning unrelated extension dialogs or ordinary context-menu commands.
- Adding a broker, worker, protocol, or dependency.

## Decisions

### Scope a process WinEvent hook to the Properties invocation

Install an in-context, process-scoped `EVENT_OBJECT_SHOW` WinEvent hook immediately around the
Properties `IContextMenu::InvokeCommand` call on the persistent Shell STA. Shell can create the
sheet on a helper thread, which makes a thread-specific CBT hook insufficient. The first eligible
top-level dialog is positioned and marked complete. RAII removal clears the hook and synchronized
one-shot state on every return path. Since some handlers return before their helper thread shows
the sheet, successful invocation transfers the hook to a bounded two-second asynchronous lease so
the persistent STA immediately resumes its message loop. A 2 ms same-process dialog enumeration
fallback handles missing show events, with the same synchronized claim preventing duplicate
placement. The lease exits immediately after success.

### Compute placement from real rectangles

The callback reads the final dialog size. A pure helper centers that size over the validated owner
rectangle and clamps the result to the selected monitor work area. If the owner cannot be queried,
the invocation point selects the nearest monitor and its work area becomes both anchor and clamp.
An oversized dimension aligns to the corresponding work-area origin.

### Preserve Shell window semantics

Use `SetWindowPos` with `SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE`. The Shell continues to own
activation, sizing, DPI, modality, property pages, and destruction. Hook or positioning failure is
non-fatal and the Properties invocation proceeds normally with a bounded diagnostic.

### Extend the existing result-based UTIT

The built-in context-command case already opens real file, folder, multi-selection, executable,
and script sheets. It will capture app/dialog/work-area rectangles, assert center tolerance and
full work-area containment for every sheet, then close via Escape and prove a later genuine menu
command still works.

## Risks / Trade-offs

- **A handler shows a helper dialog first** → Restrict eligibility to top-level dialog-class
  windows created on the scoped STA and stop after one placement; retain target/title UTIT across
  representative in-box handlers.
- **Per-monitor DPI rounding shifts the exact center** → Use physical Win32 rectangles and a small
  explicit pixel tolerance in UTIT rather than equality.
- **Hook installation or monitor lookup fails** → Treat positioning as best-effort and continue the
  native Properties invocation unchanged.
- **A property sheet exceeds the work area** → Align oversized dimensions to the work-area origin
  so the title bar remains reachable without resizing provider-owned UI.

## Migration Plan

No persisted data or protocol migration is required. Ship the hook with the application binaries.
Rollback is the independent code/UTIT commit that removes the scoped hook and placement evidence.

## Open Questions

None. Owner-relative placement with pointer-monitor fallback was approved by the user.
