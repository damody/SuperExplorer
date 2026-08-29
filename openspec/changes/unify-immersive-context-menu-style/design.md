## Context

SuperExplorer obtains Local context commands through `IContextMenu`, populates an `HMENU`, forwards `IContextMenu3` owner messages through a hidden STA window, and displays the result with `TrackPopupMenuEx`. ExplorerPatcher demonstrates that a legacy menu can retain this command model while receiving Windows immersive owner-draw styling. Its process-wide hooks and GPLv2 implementation are unsuitable for direct reuse; SuperExplorer owns the popup call site and can use a smaller independently written scoped adapter.

ADB/SFTP has no Shell menu and uses a GPUI renderer. It can match the accepted Local appearance only by consuming measured visual tokens, not by using the native HMENU renderer.

Constraints include third-party owner-draw extensions, dynamic submenus, per-monitor DPI, light/dark/high-contrast modes, exactly-once native cleanup, and a dirty repository with unrelated user work that must remain untouched.

## Goals / Non-Goals

**Goals:**

- Give supported Local file menus the same immersive treatment as accepted folder/background menus without changing command identity or execution.
- Preserve native extensions, nested menus, owner messages, keyboard behavior, cancellation, replacement, and replay.
- Fail safely to the existing native menu on unsupported or incompatible sessions.
- Make ADB/SFTP visually equivalent through one typed token projection.
- Produce deterministic diagnostics, automated tests, and headful evidence across theme/DPI/extension matrices.

**Non-Goals:**

- Replacing Local commands with GPUI snapshots.
- Injecting or globally hooking another process.
- Requiring ExplorerPatcher at runtime.
- Copying ExplorerPatcher code, signatures, or binary assets.
- Calling or reimplementing private Windows immersive-menu helpers.
- Adding Local-only Shell extension commands to ADB/SFTP.

## Decisions

### 1. Independently written public-API popup host (B-003)

Create a Windows-only `immersive_popup` component in `explorer-shell-win`. It enumerates an authoritative HMENU into a non-owning presentation model, renders it in a documented Win32/GDI `WS_POPUP`, forwards dynamic submenu initialization to the existing owner, and returns the original command ID. It never writes `MFT_OWNERDRAW` or `dwItemData`.

This is preferred over process-wide hooks because SuperExplorer controls the call site, and over a GPUI Local menu because `IContextMenu` and HMENU remain the command authority.

### 2. Materialization result, not OS-version assumption

The host returns a selected command or a structured unsupported reason. Availability requires a valid owner/HMENU and supported row forms; high contrast and extension-owned owner-draw rows use the native fallback. An OS version number alone never authorizes custom presentation.

The renderer implementation must be independent and reviewable. ExplorerPatcher source is behavioral research only; no GPL code, pattern table, private ABI declaration, or derived implementation block enters SuperExplorer.

### 3. One popup call owns lifecycle and cleanup

`present` owns row metadata, font, popup HWND, capture, and shadow HWNDs. RAII releases GDI/window resources after selection, cancellation, replay, message-loop failure, or unwind. The HMENU remains caller-owned and unchanged.

### 4. Compatibility gate before presentation

Materialization reads item count, command IDs, submenu handles, type/state flags, strings, and bitmap handles. Extension-owned owner-draw content that cannot be represented falls back before creating the custom window. Production never rewrites extension-owned data.

### 5. Explicit Shell message ownership

The custom host owns its paint/input messages. `WM_INITMENUPOPUP` for nested native submenus continues through the existing STA owner and `IContextMenu3` path. The fallback `TrackPopupMenuEx` path retains the original owner-message routing unchanged.

### 6. Per-session fallback and diagnostics

Enumeration or window-creation failure falls back to `TrackPopupMenuEx` for that session. Diagnostics contain renderer strategy, phase, DPI, and fallback category, but never paths, labels, user names, PIDLs, or raw extension data. A failed session cannot disable all later menus.

### 7. High contrast and rollout setting

High contrast always uses `TrackPopupMenuEx`. A typed runtime setting controls new sessions only; disabling it is the data-free rollback mechanism. B-002 records the user-approved enabled default for this feature branch.

### 8. Shared remote visual projection

Add `ContextMenuVisualTokens` in the UI theme boundary. Its fields cover surface, border, divider, primary/danger text, hover, pressed, font, row height, icon gutter, inset, width policy, and shadow. A theme/DPI projection feeds only the ADB/SFTP renderer. Local listing rows and Local native HMENU colors remain outside this projection.

Accepted values come from indexed headful Local measurements, not ad-hoc screenshot guesses. When native immersive capability is unavailable, remote tokens continue to use the last approved contract rather than copying the fallback classic menu.

### 9. Evidence model

Every atomic task writes or references a unique record under `openspec/changes/unify-immersive-context-menu-style/evidence/`. Each record includes `task_id`, command/manual procedure, expected and actual result, exit status or reviewer, hashes, gate IDs, adjustment lineage, and timestamp. Screenshot indexes store content hashes and environment metadata.

### 10. Adjustment governance

- **A — task refinement:** task split/order/owner/command changes without changing scope, requirements, gates, or public contracts.
- **B — design/spec correction:** discoveries within approved scope; pause the branch, update design/spec/tasks, mark dependent evidence stale, and revalidate.
- **C — material change:** copying GPL code, injection/global hooks, platform or public-contract changes, removal/weakened fallback, or lowered blocking evidence; requires user approval.

### 11. SuperExplorer-owned popup presentation host (B-003)

Source and headful research proved that ExplorerPatcher obtains its result by running in
the Explorer process and resolving three private `twinui.pcshell.dll` implementations:
the immersive apply/remove helpers and their owner-window procedure. A documented
`TrackPopupMenuEx` popup does not acquire that presentation through `SetWindowTheme`,
UxTheme row drawing, or DWM attributes. SuperExplorer MUST NOT copy those patterns,
declare that private ABI, inject into Explorer, or depend on ExplorerPatcher.

The clean-room implementation seam is therefore a SuperExplorer-owned popup presentation
host. HMENU and `IContextMenu/IContextMenu3` remain authoritative for command IDs,
canonical verbs, states, dynamic submenu initialization, and invocation. The host owns
surface composition, typography, geometry, input, placement, and shadow; it materializes
a non-owning presentation model after `QueryContextMenu`, forwards extension messages in
the existing STA, and returns the selected native command ID. Unsupported extension-owned
owner-draw content falls back without mutation until it has a proven compatibility
adapter. Fallback evidence MUST NOT close the visual-parity gate.

## Risks / Trade-offs

- **UxTheme metrics change across Windows builds** → runtime theme measurement, capability isolation, circuit breaker, permanent fallback, and build-matrix evidence.
- **Third-party owner-draw corruption** → pre-apply compatibility gate, invariant snapshots, representative extension matrix, and no forced rewriting.
- **Double handling between immersive and `IContextMenu3`** → explicit claimed-message semantics and controlled fake-handler tests.
- **Cleanup leak or use-after-free** → session state machine, explicit finish, unwind fallback, repeated-cycle resource tests, and STA ownership.
- **GPL contamination** → behavioral-reference record, independent implementation review, and license/provenance gate before enablement.
- **Remote UI only approximates native rendering** → measured token evidence at every supported DPI/theme and explicit visual tolerances.
- **Pixel-perfect comparison is display-dependent** → record OS build, theme, monitor DPI, font, scaling, and crop geometry with every screenshot.

## Migration Plan

1. Land capability types, unsupported fallback, diagnostics schema, and fake tests with the feature disabled.
2. Implement the documented Win32/GDI popup host, native fallback, and dynamic submenu routing.
3. Run Local headful evidence with the setting opt-in.
4. Freeze approved visual measurements and migrate ADB/SFTP tokens.
5. Run combined regression/headful gates and enable by default only if every blocking gate passes.
6. Roll back by disabling the typed setting or circuit breaker; no persisted data conversion is needed.

## Open Questions

- Exact UxTheme parts/metrics and supported HMENU item forms are evidence outputs of the capability spike; unsupported outcomes are valid and do not authorize overwriting extension-owned data.
- Default enablement is conditional on the full Local compatibility and headful matrix. If the gate fails, the adapter remains opt-in while ADB/SFTP token work may still complete against the approved reference.
