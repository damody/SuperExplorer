# Session Restore Headful Validation Design

## Problem

The existing roadmap session harness starts the application at its default `C:\` location with one tab. It treats a valid `session.json` and a `session_restore_ready` log entry as proof of restoration. Those signals prove that the loader ran, but they do not prove that the visible window restored multiple tabs, mixed locations, the active tab, per-tab settings, history, focus, or bounds.

## Decision

Use a real, two-process, UI Automation-driven validation. The first process creates and observes a non-default session through the same keyboard and typed action entry points a user uses. After a clean close, the second process must expose an equivalent visible state. The durable JSON snapshot is a second oracle, not the sole oracle.

The fixture contains three ordered tabs:

1. An owned fixture directory on the system drive.
2. An owned fixture directory under the workspace drive when it is a distinct volume; otherwise a second distinct filesystem location with the limitation recorded.
3. The Windows `This PC` Shell namespace (`shell:MyComputerFolder`).

The filesystem tabs receive different navigation histories and view settings. The harness selects a non-first active tab, moves focus to a known surface, and sets non-default reachable window bounds.

## Oracles

Before shutdown and after restart, the harness saves:

- raw UIA tab names, ordering, selection state, active location text, and focused element;
- window bounds and a screenshot;
- the validated session envelope, including ordered locations, active tab, history, and per-tab view settings;
- application diagnostics and process resource snapshots.

The test fails if tab count/order, active tab, reconstructible location, view settings, focus surface, or reachable bounds differ. A `session_restore_ready` log entry alone can never produce PASS.

## Restart Soak

Run ten restart cycles total: five orderly closes and five forced exits followed by recovery launches. Every recovery launch repeats the complete UIA and durable-state comparison. The forced-exit cases must recover the last atomically published snapshot. Process, thread, handle, and working-set measurements are retained for each cycle.

## Failure and Environment Handling

- A missing interactive desktop is a truthful prerequisite SKIP in UTIT.
- If no second volume exists, cross-volume coverage is marked unavailable while mixed filesystem/Shell-namespace coverage still runs.
- The harness creates and removes only explicitly owned fixture and isolated `LOCALAPPDATA` directories.
- Before typing an address, the harness requests English (US) input on the explorer UI thread and verifies LANGID `0x0409`, so the machine's current IME cannot corrupt ASCII paths or Shell parsing names.
- Timeouts, missing UIA elements, mismatched state, inaccessible locations, and off-screen bounds are failures with before/after artifacts.

## Rejected Alternatives

- Pre-seeding JSON alone: deterministic, but it does not prove user-visible restoration.
- A production snapshot test API: reliable, but adds a test-only surface to the application and bypasses UI behavior.
