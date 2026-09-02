## Context

SuperExplorer already has a bounded ADB command runner and remote provider, but its executable resolution prefers an application-relative candidate and then `PATH`, does not validate candidates, and exposes no managed installation workflow. Local file context menus are owned/native-compatible UI sessions and operation status is already centralized. The approved source design is `docs/superpowers/specs/2026-09-02-local-apk-device-install-and-managed-adb-design.md`.

This change crosses remote tooling, application composition, UI menus, localization, network/archive security, packaging dependencies, and headful verification. It therefore uses a detailed implementation plan and append-only task evidence under `openspec/changes/add-local-apk-device-install/evidence/`.

## Goals / Non-Goals

**Goals:**

- Offer exact-device `adb install -r` from a single Local APK context menu.
- Prefer validated system/SDK ADB and retain a private, user-initiated managed fallback.
- Keep discovery, download, extraction, and install work off UI callbacks and bounded.
- Make stale state, device authorization, failures, cancellation, and rollback observable.
- Verify the full user path with deterministic fake ADB and, conditionally, real hardware.

**Non-Goals:**

- System `PATH` mutation, administrator installation, SDK replacement, silent download, split APK support, remote APK staging, downgrade, auto-uninstall, or public extension ABI changes.

## Decisions

### Central ADB tool service with explicit provenance

Add a reusable resolver/tool service in `explorer-remote` and compose it once in the app. Candidates are probed in this order: process `PATH`, configured/recognized Android SDK roots, active managed install. Each candidate must be a regular file and pass a bounded `adb version` command. Resolution returns path, provenance, and bounded rejected-candidate diagnostics.

This corrects the present application-relative-first behavior and ensures a later system ADB becomes authoritative without deleting the managed copy. Scattered UI path checks were rejected because they would diverge from the provider and bypass command bounds.

### Immutable device snapshots keyed by serial

Parse `adb devices -l` into immutable records containing serial, display name, state, and installability. The display name prefers `model`, then `device`, then serial, with ADB underscore escapes presented as spaces. Serial is the only targeting key. Snapshot generations allow late discovery results to be rejected after refresh or tool changes.

The submenu can render disabled offline/unauthorized rows while never using presentation text as command input. Live discovery inside native menu message handling was rejected because it can freeze the popup.

### Owned APK submenu feeding the existing operation lifecycle

Eligibility is exactly one regular Local file with case-insensitive `.apk`. The owned context-menu layer inserts an `Install` submenu as the first item, followed by a separator, without replacing or reordering unrelated Shell verbs. It renders loading, missing-tool, empty, error, and device states from the latest snapshot and offers refresh. Selecting an installable device submits a background operation containing the snapshot generation, resolved executable identity, canonical APK path, and serial.

Before spawn, the operation revalidates the local regular file and tool identity. It invokes the process with separate arguments `-s`, serial, `install`, `-r`, and path. No shell command string is constructed. Existing operation status receives pending, running, success, cancelled, timed-out, and failed terminals.

### User-initiated managed Platform-Tools transaction

The production source is the centralized Google HTTPS Windows Platform-Tools archive URL. Tests inject bytes/local fixtures into the installer service; production UI cannot supply an arbitrary URL. The downloader enforces HTTPS host/path allowlisting, redirect destination validation, connect/read/total timeout, cancellation, and a conservative compressed-byte maximum.

Bytes land in an operation-specific directory beneath the managed-tool root. ZIP processing rejects absolute/rooted paths, `..`, alternate roots, links/reparse-like entries, excessive entry count, excessive expanded total, and any canonical destination escape. Only the expected `platform-tools/adb.exe` layout is accepted. The extracted candidate must pass the shared version probe before an atomic directory rename activates it. A failed attempt removes only its verified transaction directory and leaves the active version untouched.

Changing system `PATH` or installing globally was rejected because it broadens authority and conflicts with SDK management. Bundling ADB was rejected due to installer size and update coupling.

### Dependency and platform boundary

Prefer existing workspace HTTP/archive facilities if they satisfy streaming bounds and ZIP metadata validation. Otherwise add narrowly configured Rust dependencies with default features disabled where practical and record license/source impact. Windows is the production target. Test seams cover transport, clock/cancellation, filesystem promotion, process runner, and device output without network or hardware.

### Evidence and adaptive correction

Every completed atomic task writes or references one evidence-index JSONL record containing `task_id`, artifact/command/manual procedure, expected/actual result, exit status or reviewer, SHA-256 hashes where applicable, gate IDs, timestamp, and optional adjustment ID.

- **A — task refinement:** task ordering, command, split, or owner mechanics may change without altering scope, requirements, gates, or contracts.
- **B — design/spec correction:** an implementation-discovered correction within approved scope pauses the affected branch; design/spec/tasks are updated, dependent evidence is marked stale, and affected gates rerun.
- **C — material change:** scope, public behavior, platform, permission, external-write authority, blocking gate, threshold, or required evidence cannot change without user approval.

The user's instruction authorizes independent decisions within the approved design but does not authorize category C expansion.

## Data and Control Flow

1. App startup composes the shared tool resolver, installer, discovery service, and install executor.
2. Local APK menu eligibility requests a cached snapshot and schedules bounded refresh work when absent/stale.
3. Missing tool yields only the managed-download action; usable tool yields device-state rows and refresh.
4. Download runs transactionally, activates only after validation, invalidates resolution/discovery caches, and requests refresh.
5. Device selection submits a background install request with exact serial and canonical path.
6. Runner output is bounded and mapped to one terminal operation event; successful process exit must also contain an accepted ADB success result.

## Failure Handling and Observability

- All subprocesses have cancellation, timeout, bounded stdout/stderr, and one terminal outcome.
- Resolver diagnostics name candidate provenance/path but never dump the environment.
- Download errors distinguish policy rejection, transport, size, archive validation, probe, and promotion stages.
- Stale menu generations cannot install to a device after tool/snapshot replacement; the user receives a refreshable error.
- Unauthorized/offline devices are visible but disabled; disconnect during install is a bounded failure.
- Logs and evidence exclude APK contents, credentials, full environment values, and unbounded device output.

## Risks / Trade-offs

- [Google changes the stable archive or redirect layout] → centralize policy, validate redirects, cover production URL with a network-policy test, and surface a repairable download failure.
- [System ADB is broken or incompatible] → probe every candidate and continue to SDK/managed candidates; retain rejected-candidate diagnostics.
- [ZIP bomb or traversal] → enforce path, type, count, compressed-size, and expanded-size limits before activation.
- [Native menu lifetime races asynchronous refresh] → immutable generation snapshots and late-result rejection; never mutate a destroyed session.
- [A device changes state after discovery] → target the serial once and report ADB's terminal error; never fall through to another device.
- [Real hardware unavailable in CI] → deterministic fake-ADB headful flow is blocking; real-device evidence is conditional and must be recorded as passed or evidence-backed not-applicable.

## Migration Plan

No data migration is required. Ship the new services and menu contribution disabled only by natural eligibility. Existing ADB remote browsing reuses the validated resolver but retains its public behavior. Rollback removes the menu/service wiring and leaves any managed tools directory inert; it does not delete user files or alter system state. A later launch of the changed version can reuse a previously validated managed install.

## Blocking Gates

- **G1 Contract:** resolver/device/install/archive unit and integration tests pass with no network.
- **G2 Security:** malicious ZIP, redirect, bounds, stale-path, and argument-injection cases fail closed; dependency/license review has no unresolved high finding.
- **G3 Integration:** targeted workspace build/tests and existing ADB provider regressions pass.
- **G4 User journey:** installed/headful build shows correct Local APK submenu, multiple named devices, disabled bad states, download recovery, exact serial invocation, and terminal status without UI freeze.
- **G5 Final review:** technical diff review and independent user-perspective review have no unresolved P0/P1 issue; any failure reopens affected tasks and gates.

## Open Questions

None. Exact crate/file placement and dependency selection are implementation refinements governed by the decisions and gates above.
