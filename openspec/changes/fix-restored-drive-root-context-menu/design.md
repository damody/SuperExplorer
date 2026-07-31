## Context

Windows distinguishes `D:` (the current directory associated with drive D) from `D:\` (the absolute drive root). SuperExplorer currently preserves every explicit filesystem descriptor during Shell resolution. A restored `D:` tab can therefore display entries resolved by the process environment while later passing `D:` as the context-menu parent beside absolute child PIDLs. The Shell rejects that mismatched parent/child relationship. Existing UTIT starts from an absolute fixture path and forces the app topmost, so it cannot expose this state.

## Goals / Non-Goals

**Goals:**

- Convert only a two-character ASCII drive designator to its absolute root after successful Shell resolution.
- Allow restored tabs with legacy `X:` values to repair themselves through the normal resolved-location commit and persistence path.
- Reproduce the user path with multiple restored tabs, high-DPI placement, a physical right click, and no topmost window flag.

**Non-Goals:**

- Canonicalize case, junctions, UNC aliases, extended-length prefixes, or arbitrary explicit filesystem paths.
- Replace the native Shell menu or change broker/worker process architecture.
- Rewrite the user's session file out of band before the location has resolved successfully.

## Decisions

1. Detect a bare drive root structurally (`[A-Za-z]:`) and append the Windows separator. This is narrower than applying `canonicalize`, which touches the filesystem, resolves links, and can change user-visible path spelling.
2. Apply the repair in Shell location resolution, where a successful absolute filesystem path is already available. This keeps persistence and live navigation on the same canonical descriptor without adding session-schema migration logic.
3. Preserve ordinary explicit paths and opaque namespace descriptors exactly as before. The existing identity contract remains intact.
4. Add one focused restored-session headful case that clones a fixture session into isolated `LOCALAPPDATA`, launches normally, activates the window without topmost forcing, physically right-clicks a non-first row, and verifies a process-bound native popup.

## Risks / Trade-offs

- [A malformed legacy drive designator could be repaired before a drive is available] → Repair only occurs after Shell resolution succeeds.
- [Physical headful input can be affected by unrelated desktop focus] → Bind UIA and popup discovery to the launched process tree and explicitly activate once, without keeping the window topmost.
- [Over-broad canonicalization could change valid explicit path identity] → Unit-test that ordinary explicit paths and namespaces remain unchanged.

## Migration Plan

Legacy `X:` session entries migrate lazily on first successful restore/navigation and are saved as `X:\` by the existing session writer. Rollback is code-only; the canonical root remains a valid descriptor for older builds.

## Open Questions

None.
