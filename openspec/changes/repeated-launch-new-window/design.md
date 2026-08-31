## Context

`explorer-app` currently performs all composition in one process and opens one
GPUI main window in `ApplicationLifecycle::run_gpui`. A later executable launch
creates another process, including session persistence, extension hosts, broker,
and shell services. The first window may restore a session or honor
`EXPLORER_INITIAL_PATH`. Main-window construction is currently embedded in the
startup closure and is not reusable.

This Windows-only change must preserve GPUI thread affinity, avoid accepting
cross-user commands, keep automated fixture launches isolated, and coexist with
the repository's current Rust and Win32 dependency policy.

## Goals / Non-Goals

**Goals:**

- Elect one resident ordinary SuperExplorer process per interactive user.
- Deliver one authenticated-by-user-boundary, bounded relaunch request to it.
- Create exactly one independent top-level explorer window at `C:\` per request.
- Keep first-launch restoration and final-window shutdown behavior.
- Fail open to normal startup when coordination is unavailable.

**Non-Goals:**

- Arbitrary command-line paths or cross-user launch delivery.
- Persisting or restoring every open window.
- A public automation, plugin, or SDK API for window creation.
- Redirecting diagnostics-console, visual-fixture, auto-close, or plugin
  development launches.

## Decisions

### Resident process with a per-user named-pipe endpoint

Ordinary startup first attempts a bounded `OpenWindowV1` request. If no resident
accepts it, the process claims a per-user named mutex and starts the pipe server.
The endpoint identity incorporates the current Windows user SID rather than a
display name. The pipe's security descriptor admits the current user and local
system only. The fixed request is versioned, length bounded, and acknowledged.

This is preferred to independent processes because session files and heavyweight
services keep one owner. It is preferred to window-message discovery because a
named pipe has explicit framing, user scoping, and acknowledgment semantics.

### Explicit launch-role boundary

Argument/environment classification happens before heavyweight composition.
Only an ordinary production launch participates. Diagnostic console,
`EXPLORER_VISUAL_FIXTURE`, `EXPLORER_AUTO_CLOSE_MS`, and explicit `--plugin-dll`
launches remain independent so tests and plugin development are deterministic.
If connection or acknowledgment fails, startup continues independently and logs
the reason.

### Foreground command delivery and reusable main-window factory

The listener thread sends validated commands through a bounded in-process
channel. A GPUI foreground task drains it and owns every `open_window` call.
Main-window assembly is refactored into a reusable factory/context that retains
shared shell, remote, extension, bookmark, settings, diagnostics, and auxiliary
window controllers. The initial invocation supplies restored tabs and placement;
a relaunch supplies a fresh filesystem `HistoryEntry` for `C:\` and normal
placement. Each window receives independent `ExplorerRoot` state.

### Lifetime and persistence

The resident listener lives in `ShutdownResources` and stops during normal app
shutdown. Existing `on_window_closed` logic continues to quit only after the
last top-level/owned window closes. Durable session observation remains attached
to explorer roots, while startup restore is consumed only by the initial window.

### Diagnostics and evidence

Structured events record role selection, accepted/rejected requests, window
dispatch, fallback startup, and listener shutdown without recording paths beyond
the fixed public `C:\` contract. Automated evidence uses task-index JSON records
under `openspec/changes/repeated-launch-new-window/evidence/`.

### Plan correction policy

- **A — task refinement:** leaf split/order/command/evidence-path changes that do
  not alter requirements or gates may be recorded in tasks and evidence.
- **B — design/spec correction:** an in-scope technical correction pauses
  affected work and updates design, spec, tasks, and stale evidence together.
- **C — material change:** changing public behavior, platform, security boundary,
  blocking gates, or required evidence requires user approval.

## Risks / Trade-offs

- **[Race between simultaneous first launches]** → The named mutex is the single
  election primitive; losers retry the pipe for a bounded interval.
- **[Hung or stale resident endpoint]** → Timeouts are bounded and failure starts
  an independent process rather than blocking launch.
- **[Untrusted local client input]** → SID-scoped ACL, fixed command vocabulary,
  strict version/framing limits, and no caller-supplied path.
- **[Large startup closure refactor causes regressions]** → Extract the smallest
  reusable factory and preserve existing observers; run focused application and
  UI tests before headful validation.
- **[Foreground activation restrictions]** → The secondary process grants
  foreground permission to the resident when possible; the resident activates
  the created window after opening it.

## Migration Plan

Ship as an internal behavioral change with no persisted-data migration. Build,
run focused tests, then perform a two-launch Windows smoke test. Rollback is a
code revert; protocol versioning ensures mismatched binaries reject and fall
back to independent startup.

## Open Questions

None. Evidence may refine implementation mechanics under category A but cannot
weaken the user-visible or security requirements.
