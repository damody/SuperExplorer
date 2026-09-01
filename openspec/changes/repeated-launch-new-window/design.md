## Context

Installed verification exposed two integration constraints that were not part
of the initial implementation evidence. Windows rejected the second extension
host's open of `.sepack-staging` because the long-lived directory handle allowed
only read sharing, and test installers copied their finish-page diagnostics
argument into persistent shortcuts. These are B-level corrections within the
approved independent-process design.

`explorer-app` already creates an independent top-level GPUI window for every
process invocation. The missing File Explorer behavior is location selection:
every process currently applies the same session restore unless an explicit
initial path overrides it. The large window-construction closure does not need
to change to meet the user-visible requirement.

## Goals / Non-Goals

**Goals:**

- Detect whether an ordinary SuperExplorer process already exists in the current
  Windows login session.
- Make every later ordinary invocation open its independent window at `C:\`.
- Preserve first-launch session restoration and special-launch isolation.
- Preserve independent window/process close behavior.

**Non-Goals:**

- Consolidating windows into one process or adding IPC.
- Arbitrary command-line paths, cross-session coordination, or multi-window
  session restoration.
- Changes to plugin APIs, persistence schemas, or existing window composition.

## Decisions

### Share the verified staging root, not import candidates

The staging-root directory handle allows `FILE_SHARE_READ` and
`FILE_SHARE_WRITE`, but continues to deny delete sharing so the held handle
prevents adversarial parent replacement. Candidate subdirectories remain unique and create-new;
identity checks, reparse rejection, active-owner scavenging, and cleanup bounds
remain unchanged. This permits multiple process-owned extension hosts without
weakening candidate ownership.

### Separate finish-page and persistent shortcut arguments

`--diagnostics-console` may be used by a test installer's immediate finish-page
launch. Start Menu and desktop shortcuts always have empty arguments so they are
classified as ordinary launches.

### Quiesce exact installed application processes before replacement

NSIS extracts an installer-owned PowerShell helper and invokes it with the
selected install directory before service shutdown or file replacement. The
helper gracefully closes, then boundedly force-stops only processes whose
normalized executable path equals the target `SuperExplorer.exe`; it returns
nonzero unless final absence is proven. NSIS aborts on nonzero so stale binary
replacement can no longer be reported as a successful upgrade.

### Login-session named mutex as a lifetime marker

Every ordinary process calls `CreateMutexW` for a versioned name in the Windows
`Local\` namespace and retains the returned handle until shutdown. Windows
atomically reports `ERROR_ALREADY_EXISTS` to later processes. Because every
process retains a handle, repeated-launch detection continues to work if the
oldest window closes while another remains. The namespace scopes independent
interactive sessions without parsing or persisting user identity.

This is preferred to named-pipe forwarding because the application already
supports independent top-level processes and no cross-thread GPUI dispatch is
needed. It is preferred to process enumeration because it is race-free and does
not depend on executable paths or localized window titles.

### Explicit startup override, not environment mutation

`ApplicationLifecycle` accepts an optional initial filesystem path. A repeated
launch passes `C:\`; an initial or isolated launch passes no override. Existing
`configured_initial_location` validation turns the path into a `HistoryEntry`,
and existing session logic suppresses restored tabs when an explicit location is
present. This avoids unsafe process-environment mutation after diagnostics may
have started threads.

### Isolated modes bypass the marker

Diagnostic-console launches, explicit plugin DLL launches,
`EXPLORER_VISUAL_FIXTURE`, `EXPLORER_AUTO_CLOSE_MS`, and the explicit test bypass
environment variable do not acquire or observe the marker. Automated and plugin
development runs therefore retain deterministic process-local behavior.

### Failure and observability

If `CreateMutexW` fails, startup records the controlled error through the
existing top-level diagnostic path and does not corrupt persistent state.
Ordinary startup records a `repeated_launch` boolean. No user path or private
identity is logged.

### Plan correction policy

The shift from resident IPC to independent-process detection is a category B
design/spec correction within the approved visible scope. It removes unnecessary
attack surface and refactoring without weakening any visible requirement.
Category C changes to visible behavior, platforms, or validation gates still
require user approval.

## Risks / Trade-offs

- **[First-process crash]** → Windows closes its handle; any remaining process
  retains its own handle, so future launches still detect an existing window.
- **[Marker-name collision]** → Use a product-specific versioned `Local\` name.
- **[Special test interference]** → Classify and bypass special launches before
  marker acquisition.
- **[External `EXPLORER_INITIAL_PATH`]** → The in-process repeated-launch
  override has explicit precedence and deterministically selects `C:\`.

## Migration Plan

No data migration is required. Build and unit-test launch classification, then
run a two-process Windows smoke test with the first path forced to `D:\`; the
second must display `C:\`. Rollback is a code revert.

## Open Questions

None.
