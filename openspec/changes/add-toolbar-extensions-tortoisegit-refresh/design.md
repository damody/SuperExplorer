## Context

The command bar already owns mutually exclusive Sort, View, and More popups through typed actions and `AppViewState`. Shell icons already use independent association and overlay epochs, with a live Shell query path for filesystem items. The UI crate deliberately has no Windows Shell dependency, while the application crate composes `explorer-shell-win` and `explorer-ui`.

The implementation must preserve the current More menu, TortoiseGit/OneDrive overlay ownership by Windows Shell, file-view virtualization, and overlay-aware cache keys. Refreshing Git badges must not enumerate Git status itself or mutate the folder.

## Goals / Non-Goals

**Goals:**

- Give the existing More button the visible label `其它`.
- Add a stable `擴充功能` popup to its right.
- Discover a valid TortoiseGit executable without making startup fallible.
- Re-query the active folder's overlay icons through the existing Shell pipeline.
- Preserve navigation, selection, view mode, and scroll state.
- Provide keyboard, accessibility, unit, render-contract, and headful coverage.

**Non-Goals:**

- Running TortoiseGit commands or rebuilding the Windows icon cache.
- Implementing Git status parsing or drawing TortoiseGit badges.
- Building a general plugin SDK or extension marketplace.
- Guaranteeing an overlay when Windows has excluded TortoiseGit from active overlay slots.

## Decisions

### Discover capability at the Windows adapter boundary

`explorer-shell-win` will expose a bounded detector that checks owned candidate paths derived from `ProgramW6432`, `ProgramFiles`, and `ProgramFiles(x86)`. The application composition root injects the resulting boolean into `ExplorerRoot`, which stores it in `AppViewState` for rendering and action enablement.

This keeps `explorer-ui` independent from Win32 and makes tests deterministic. Registry-only discovery was rejected because stale registration does not prove the executable still exists; process invocation was rejected because discovery must have no visible side effect.

### Add a separate typed extension-menu state machine

The UI will add `ToggleExtensionsMenu`, `CloseExtensionsMenu`, and `RefreshTortoiseGitStatus`. The extension popup is mutually exclusive with Sort, View, and More, uses the same direct-child/deferred anchoring pattern, and provides a disabled placeholder when no extension is available.

Keeping a separate menu avoids changing the existing eleven-item More keyboard index contract and makes future extension commands additive.

### Refresh by advancing only overlay identity

The root handler for `RefreshTortoiseGitStatus` will advance the global overlay epoch beyond every per-item epoch, clear per-item overlay generations, and invalidate overlay-dependent visible caches, negative results, pending icon consumers, and thumbnail presentations. It will preserve the association epoch and base-icon cache, then re-submit active snapshot icon requests and navigation icons.

This guarantees new request keys while retaining extension-wide shared base images. A directory refresh alone was rejected because rows can remain identity-equal and continue to reuse cached overlay pixels.

### Keep Shell as the only badge authority

The app will not call Git or synthesize status. New icon requests still pass through `LoadShellIcon`, including the current live-overlay refresh behavior and stale-key rejection.

## Risks / Trade-offs

- **[Custom TortoiseGit installation path is not under Program Files]** → Treat it as unavailable rather than scanning disks or PATH; candidate discovery remains deterministic and side-effect free.
- **[TortoiseGit's own status cache has not converged]** → The command guarantees a fresh Shell query, not an external cache rebuild; watcher changes and repeated manual refresh remain available.
- **[An old async icon result completes after refresh]** → Advance the epoch before resubmission and reject/ignore results whose keys no longer match current presentation generations.
- **[Clearing the visible texture cache briefly exposes base icons]** → Re-submit only bounded active/visible requests through the existing virtualization cap.
- **[Compact command bar width]** → Keep both textual buttons in the same command-strip overflow behavior already used by Sort/View/More; no new responsive layout policy is introduced.

## Migration Plan

No persisted schema or external API migration is required. The new capability defaults to unavailable, so restored sessions and tests remain compatible. Rollback consists of removing the injected capability, typed actions/menu state, and overlay refresh handler; existing icon epochs remain unchanged.

## Open Questions

None. Product choices for the unavailable placeholder, menu position, and refresh scope are fixed by the approved design.
