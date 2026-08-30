## Context

Remote item menus already expose Properties through a provider metadata route and an owned GPUI
window. Remote background menus use the same custom Windows-style renderer but currently expose
only folder creation and context-dependent paste. `RemoteProvider` has bounded list and mutation
operations executed away from GPUI; it does not yet expose single-location metadata or symbolic
link creation. ADB already parses link-aware stat output and SFTP already resolves links while
listing, so both backends have relevant primitives but no provider-neutral mutation contract.

The approved source design is
`docs/superpowers/specs/2026-08-30-remote-background-symlink-properties-design.md`.

## Goals / Non-Goals

**Goals:**

- Add real Create Shortcut and current-directory Properties commands to ADB/SFTP background menus.
- Create relative, absolute, and dangling Linux symbolic links without blocking GPUI.
- Use one dedicated owned editor window with recoverable validation/provider errors.
- Obtain authoritative current-directory metadata and reuse the remote Properties window.
- Preserve accepted menu visuals, item commands, accessibility, dismissal, and stale-state rules.
- Add a folder-item `新增捷徑` command that directly creates a uniquely named sibling symlink to
  the clicked ADB/SFTP folder.

**Non-Goals:**

- Windows `.lnk`, Linux `.desktop`, Local filesystem symlinks, target browsing, post-create link
  editing, recursive target validation, or following a link before creation.
- Adding dependency crates, persistence migrations, or shell-terminal UI.

## Decisions

### 1. Extend `RemoteProvider` with narrow typed operations

Add provider-neutral `create_symlink(destination, target, cancellation)` and
`metadata(location, cancellation)` operations. The target remains an opaque string because Linux
link semantics deliberately allow relative, absolute, and nonexistent targets. The destination is
a validated child `VirtualLocationDescriptor`, not a concatenated command string.

Alternative: issue ADB/SFTP-specific commands directly from UI state. Rejected because it leaks
provider details into GPUI, bypasses the worker/cancellation boundary, and duplicates error rules.

### 2. Keep provider-native execution

ADB uses its existing argument-safe script-input pattern to call `ln -s --` without interpolating
untrusted text into shell source. SFTP calls the protocol symlink operation. Both metadata paths
reuse the same bounded, cancellable parsing/protocol primitives as listing.

Alternative: upload a `.desktop` file or call an SSH terminal. Rejected because neither creates a
filesystem symlink and ADB has no SSH assumption.

### 3. Use a dedicated owned editor window

Add a `RemoteSymlinkWindow` opened through an observer registered beside the existing remote
Properties observer. The window owns two text inputs, focus, validation, progress, and error text.
It submits an immutable request to `ExplorerRoot`; provider I/O stays in the remote worker path.
One editor window is reused/replaced so repeated commands cannot accumulate hidden modal state.

Alternative: inline rename followed by a second prompt. Rejected because two dependent prompts
lose input on failure and do not present link name and target as one atomic intent.

### 4. Treat background Properties as current-location metadata

The action captures tab ID, generation, and current `LocationDescriptor`, requests metadata, then
builds the existing `RemotePropertiesWindowSnapshotV1`. Completion is ignored when the tab,
generation, or location changed. No synthetic selected row is inserted into the listing.

Alternative: derive data from breadcrumb/list cache. Rejected because cached listings describe
children, may omit the current directory, and cannot authoritatively provide permissions/time.

### 5. Validate only the link child name locally

The link name MUST be one component and rejects empty/whitespace-only, `.`, `..`, `/`, `\\`, and
NUL. Provider descriptor validation remains authoritative for additional restrictions. The target
MUST be nonempty but is otherwise forwarded exactly, including whitespace and dangling targets.

### 6. Adjustment and evidence policy

- A-level: split/reorder task mechanics without changing requirements, gates, or contracts.
- B-level: correct design/spec/task detail inside this approved scope; reopen dependent work and
  mark evidence stale.
- C-level: expand providers, add destructive behavior, weaken gates, change dependencies, or alter
  public commitments; requires user approval.

### 7. Folder-item shortcuts are direct and collision-safe

For an ADB/SFTP folder item, `新增捷徑` creates a sibling link without opening the editor. The
stored target is the clicked folder's display name, preserving relative Linux symlink semantics.
The destination starts at `原名稱 - 捷徑` and advances through numbered suffixes until it finds a
name absent from the current listing. Provider-side duplicate detection remains authoritative if
the directory changes concurrently; SuperExplorer never overwrites an existing entry.

## Data flow

1. Background menu action closes the menu and captures the current remote context.
2. Create Shortcut opens/replaces the editor snapshot. Create validates and submits one immutable
   request through the remote worker boundary.
3. Provider success triggers a location-scoped refresh and selection request; failure returns an
   editor error while preserving values.
4. Properties submits one metadata request and opens/replaces the existing Properties window only
   if its captured tab/generation/location remains current.
5. A folder-item shortcut captures the clicked folder and current parent, derives a collision-safe
   sibling name, and submits through the same worker/completion path without an editor session.

## Risks / Trade-offs

- **ADB shell portability** → Use only `ln`, bounded stat primitives already required by listing,
  test emulator behavior, and surface unsupported-tool errors.
- **SFTP servers may reject symlink operations** → Preserve inputs, show protocol error, and do not
  mutate UI optimistically.
- **Argument injection or path escape** → Pass values as positional data to a fixed script and
  construct the destination from a validated child descriptor.
- **Stale async completion** → Apply only against captured tab/generation/location.
- **Dangling links may be displayed as unresolved** → Creation still succeeds; refresh represents
  the provider's truthful unresolved-link state.
- **Size for a directory can be unavailable or non-recursive** → Properties labels it unavailable
  instead of starting an implicit recursive scan.

## Blocking gates and evidence

- **G1 Provider safety:** unit tests prove exact ADB arguments/script separation, SFTP protocol
  routing, cancellation, and no target preexistence requirement.
- **G2 UI lifecycle:** state/window tests prove validation, preserved failure inputs, single
  dispatch, stale completion rejection, refresh/selection, and current-directory metadata routing.
- **G3 Menu regression:** remote command membership/order plus existing visual, dismissal,
  accessibility, keyboard, and edge-clamp suites pass.
- **G4 Headful providers:** disposable ADB and SFTP links are created, read back, removed, and each
  current-directory Properties window is observed. Missing provider access is blocking, not a pass.
- **G5 Final quality:** format, focused/full affected tests, app check, strict OpenSpec validation,
  diff check, and scoped review pass.

Evidence is stored under `evidence/tasks/<task-id>.json` with command/manual procedure, expected and
actual result, exit status, hashes, related gates, adjustment ID when applicable, and timestamp.

## Migration Plan

No stored data or settings change. Deploy provider contract, implementations, UI observer/window,
and commands in one binary. Rollback reverts those source additions; existing bookmarks, profiles,
and remote files require no migration. Test-created links use unique disposable names and are
removed during headful cleanup.

## Open Questions

None. The approved design fixes both fields, dangling-target behavior, validation boundary,
provider set, owned-window behavior, and Properties metadata semantics.
