## Context

The operation center currently stacks a summary, a semantic button that expands to the full surface width, and an absolute progress bar spanning the whole surface. Remote byte events can be published more frequently than a human-visible refresh, while cancellation is request-scoped but does not expose a distinct immediate UI state. Transfers can involve a local Shell operation, an ADB subprocess, an SFTP stream, or two provider stages joined by a local staging file.

The approved source design is `docs/superpowers/specs/2026-09-02-transfer-progress-layout-cancellation-design.md`. The implementation must preserve existing operation correlation, exactly-one terminal behavior, 8-second terminal fade, credential secrecy, and move-source safety.

## Goals / Non-Goals

**Goals:**

- Give active operations a compact, stable 250px cancel region and a remaining-width progress region.
- Keep ordinary visible progress updates on a 200ms cadence without delaying state transitions.
- Make cancellation visible immediately and stop provider work at the earliest safe interrupt boundary.
- Keep byte/item reporting truthful and prevent cancelled moves from cleaning up their source.
- Verify the installed build from the same user interactions that exposed the defect.

**Non-Goals:**

- ETA, throughput charts, pause/resume, public extension ABI changes, or credential format changes.
- Artificial delay for small transfers.
- Changes to the existing terminal fade duration.

## Decisions

### Active row owns two explicit layout regions

The active operation surface will render a horizontal row containing a fixed-width 250px cancel slot and a `flex-grow` progress slot. The compact semantic Cancel button remains keyboard accessible inside the left slot. Summary text and the determinate/indeterminate bar share the right slot so their layout and state cannot diverge. Terminal surfaces omit the cancel slot and use the full width.

This is preferred over fixed button dimensions alone because it gives progress a predictable start edge, and over an overlay because overlays can intercept drag/context-menu input and obscure text.

### Throttle at progress producers and retain forced boundaries

Remote and Windows Shell byte producers will coalesce ordinary updates to the latest value and publish at most once per 200ms. Preparing, total discovery, item boundaries, finalizing, and terminal events bypass throttling. There is no timer-induced sleep in the transfer path; the next producer callback publishes the latest eligible value, and terminal publication flushes final state.

This is preferred over globally delaying operation-center events because global delay would also postpone failures and cancellation feedback. It also avoids queue/render pressure from discarding events only after they reached the UI.

### Cancellation is acknowledged in UI before provider completion

Dispatching Cancel marks that request as cancelling in UI state before the background command completes. The operation summary changes to `正在取消` and disables repeated cancellation. A correlated terminal event clears the marker. Submission failure also clears it and exposes the concrete error.

### Each provider owns its interrupt boundary

- Local streaming checks the token at read/write and recursive-item boundaries.
- ADB owns the spawned process and kills it when cancellation wins, then drains/joins the process without publishing late progress.
- SFTP checks cancellation around chunk operations and recursive entries; a cancelled request cannot enqueue another chunk or item.
- The cross-provider engine checks the token before and after every staging stage and never starts a later provider call after cancellation.
- Move cleanup remains success-only.

Killing the whole application worker or detaching a provider task was rejected because either can corrupt shared state or allow remote mutation after the UI claims cancellation.

### Evidence may refine tasks, not requirements

Implementation evidence may split or reorder tasks and adjust commands without changing requirements. A discovered framework constraint can correct the design within this approved scope only after the affected artifacts are updated and revalidated. Public behavior, the 200ms threshold, safety gates, or required endpoint evidence cannot be weakened without user approval.

## Risks / Trade-offs

- **A provider call may be temporarily non-interruptible** → UI acknowledges immediately, and the provider checks the token before any subsequent chunk, item, stage, or cleanup.
- **Throttling can omit intermediate percentages** → retain the latest byte value and force lifecycle/terminal publication; correctness is based on monotonic latest state rather than every percentage.
- **A cancel/complete race can produce competing terminal results** → retain request correlation and the operation center's first-terminal-wins rule.
- **ADB process termination can leave unread pipe output** → terminate through the owned runner and join/drain it before returning.
- **Layout can regress under narrow windows** → the right region uses minimum-width-zero and clipping/wrapping rules while preserving the fixed cancel slot.

## Migration Plan

No data migration is required. Land UI/state, producer cadence, and provider cancellation changes together, run focused tests, build/install using `build_test_install.bat`, then verify Local→ADB, Local→SFTP, ADB/SFTP large-transfer cancellation, progress cadence, and final state in the installed application. Rollback is a code revert; persistent formats are unchanged.

## Open Questions

None.
