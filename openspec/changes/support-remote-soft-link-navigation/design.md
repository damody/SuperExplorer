## Context

The remote provider boundary currently reduces every listed item to a name, location, directory
boolean, and optional size. ADB derives the boolean from `ls -1Ap`; SFTP derives it from the
directory entry's own metadata. Neither follows a symbolic link to determine the target type, so the
application adapter emits `FileEntry.is_container = false` for links to directories. The file view,
navigation pane, and breadcrumb child menus all correctly trust that flag and consequently cannot
enter those links.

The implementation must preserve the existing remote virtual location, cancellation, argument-array
execution, credential isolation, and UI navigation contracts. Device permissions and SFTP server
capabilities vary, so target resolution must have bounded, testable terminal states.

## Goals / Non-Goals

**Goals:**

- Classify ordinary remote entries and links to files or directories consistently across ADB and
  SFTP.
- Expose directory links as containers through every existing navigation surface.
- Distinguish broken links from circular links before navigation.
- Keep the selected link path in history, breadcrumbs, and the address bar.
- Bound link traversal and retain request cancellation.

**Non-Goals:**

- Create, edit, or retarget symbolic links.
- Show link targets in the initial UI or canonicalize navigation to target paths.
- Change local Windows reparse-point behavior or public extension ABI.
- Treat a general provider/session failure as a broken individual link.

## Decisions

### Provider-neutral item kind is the source of truth

`RemoteEntry` will carry a `RemoteEntryKind` enum with `File`, `Directory`, `FileSymlink`,
`DirectorySymlink`, `BrokenSymlink`, and `CircularSymlink`. Methods on the enum provide the
container decision and stable Type label. The application adapter maps the container decision into
the existing `FileEntry.is_container` field.

This was selected over UI-side probing because both navigation surfaces already share the adapter's
entry model and because per-row UI requests would introduce latency and inconsistent intermediate
states. It was selected over a pair of booleans because booleans cannot make broken and circular
states mutually exclusive or exhaustive.

### Resolution is bounded and path-preserving

Providers resolve only enough metadata to classify a direct child. They track normalized visited
paths and stop after 40 symbolic-link hops, matching the conventional Linux traversal ceiling.
Revisiting a path or reaching the ceiling is classified as circular. A definite missing or
inaccessible target is classified as broken. Directory request, transport, or cancellation failures
remain request failures.

The returned `LocationDescriptor` continues to contain the link-side child components. This makes
existing navigation enter the selected URI and lets the provider follow the link when listing,
without unexpectedly replacing the user's address with a target path.

### ADB uses fixed device-side code and structured records

ADB replaces display-oriented `ls -1Ap` parsing with a fixed shell probe that emits one structured
record per direct child. Because `adb shell` joins host arguments into one remote command, the
validated parent path is base64-encoded into a safe data-only assignment prefix and decoded by the
fixed probe; raw path bytes never enter device-side shell syntax. The record contains an encoded
name and item-kind token so whitespace and delimiter characters cannot change record boundaries.
Unsupported probe behavior fails the listing explicitly rather than silently reverting to incorrect
classification.

### SFTP resolves link metadata within the existing session

SFTP uses directory-entry metadata for ordinary items. Link entries are resolved using read-link and
metadata operations on normalized absolute remote paths within the same session. Relative targets
are joined to the link parent. The resolver shares the hop bound and visited-path policy with ADB's
classification semantics.

### Invalid links remain selectable and distinct

Broken and circular links have `is_container = false`, stay present in directory results, and receive
`Broken remote link` and `Circular remote link` Type labels respectively. They therefore remain
selectable but do not enter directory navigation. Directory and file links use `Remote folder link`
and `Remote file link`.

## Risks / Trade-offs

- [Per-entry link metadata increases remote round trips] → Resolve only link entries, reuse the
  active SFTP session, keep traversal bounded, and test ordinary entries remain single-pass.
- [Android shell tools vary] → Use broadly available Android shell primitives, validate structured
  output strictly, and surface unsupported execution as a directory error instead of mislabeling.
- [Permissions can resemble missing targets] → Classify a per-target inability to stat as broken for
  navigation safety while preserving whole-list transport and cancellation failures as request errors.
- [A long acyclic chain can reach the hop ceiling] → Deliberately classify it as circular so the UI
  remains non-navigable and explicitly communicates the traversal safety condition.
- [Existing tests construct `RemoteEntry` directly] → Update all constructors atomically with the
  enum contract and add exhaustive enum behavior tests.

## Migration Plan

No persisted data migration is required. Land the enum contract, provider implementations, adapter,
and tests together. Rollback restores the old provider contract; saved remote locations and profiles
remain compatible because descriptor serialization is unchanged.

## Implementation Evidence Adjustments

- **A — task refinement:** Tasks, order, owners, or commands may be refined without changing scope,
  requirements, gates, or public contracts; record the refinement in task evidence.
- **B — design/spec correction:** If provider-library behavior disproves an implementation detail,
  pause affected work, update design/spec/tasks within approved scope, reopen dependent evidence,
  and revalidate OpenSpec.
- **C — material change:** Any scope expansion, public contract change, weaker blocking gate,
  dependency addition, external write, credential use, or destructive operation requires user
  approval.

## Open Questions

None. Provider implementation may choose equivalent structured primitives when verified tests retain
the normative classification, safety, and navigation behavior.
