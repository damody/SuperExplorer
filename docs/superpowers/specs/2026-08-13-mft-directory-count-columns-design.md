# MFT Directory Count Columns and Extension Admission Design

## Goal

Add optional built-in `File Count` and `Folder Count` Details columns backed exclusively by MFT Service, then let any extension package's data-column contributions declare folder-count limits that the Host enforces before invoking extension code. The Code Lines contribution must calculate a folder only when its recursive file count is less than 1000.

## User-visible behavior

`File Count` and `Folder Count` are built-in Details columns. They appear in the existing column chooser, are hidden by default, and can be shown, hidden, resized, reordered, persisted, restored, and sorted like the other built-in columns.

- `File Count` is the number of regular-file descendants in the complete folder subtree.
- `Folder Count` is the number of real directory descendants in the complete folder subtree. It excludes the queried folder itself.
- Reparse points, junctions, and symbolic links are neither counted nor traversed.
- Both columns apply only to filesystem folder rows. File rows are blank.
- Exact values use unsigned-integer sorting. Unavailable or incomplete values display `—` and do not enter the integer sort domain.

The columns use stable IDs `builtin:file_count` and `builtin:folder_count`. Existing sessions migrate with both columns hidden. This change does not add localization resources; the specified status strings are rendered directly.

## Directory facts architecture

Introduce a Host-owned `DirectoryFactsV1` projection containing the recursive file count, descendant folder count, MFT generation, and an exact/unavailable state. A single directory-facts coordinator owns request deduplication, cancellation, cache lookup, MFT generation validation, and result fan-out.

The existing MFT aggregate already carries `file_count` and `directory_count`. The MFT aggregate counts its root directory, so the projection converts it to the user-visible descendant count with `directory_count.saturating_sub(1)`. No UI-thread directory walk or filesystem fallback may produce either count.

The cache key includes canonical folder identity, volume identity, and MFT generation. `File Count`, `Folder Count`, and extension admission consume the same result. Showing both columns or enabling several dependent extensions must issue no duplicate MFT query for the same key. Navigation and refresh cancel obsolete presentation work. A newer MFT generation, watcher invalidation, or explicit refresh makes an older admission result ineligible for reuse.

When MFT Service is unavailable, the location is not backed by a supported NTFS volume, the item is virtual, or the aggregate is partial, directory facts are unavailable. Partial lower bounds are never presented or used as exact counts.

## Built-in column integration

Extend the built-in `ColumnId` set, stable-ID parser, registry descriptors, ordered layout, session persistence, rendering, auto-sizing, filtering behavior where applicable, and integer sorting for the two columns. Both descriptors use container applicability and background aggregate cost.

The directory-facts coordinator starts work when either count column is visible or an enabled extension contribution requires the facts. Therefore, hiding `File Count` affects only presentation; it does not disable a dependency required by Code Lines or another extension.

## Extension admission policy

Any extension package's data-column contribution that applies to folders may declare an optional folder admission policy in its validated manifest metadata:

- `max_file_count`: inclusive recursive file-count maximum;
- `max_folder_count`: inclusive descendant folder-count maximum.

An omitted field is unlimited. When both fields are present, both conditions must pass. Values are JSON integers in the inclusive `0..=u64::MAX` domain; zero admits only a zero-count folder for that dimension. The manifest validator rejects malformed values and rejects the policy on non-column or file-only contributions instead of silently ignoring it. Existing contributions without a policy retain their current behavior.

The Host, not the extension, evaluates admission. For a folder item, it obtains exact directory facts and checks the contribution policy before creating or dispatching the extension callback job. A pending, stale, partial, or unavailable result never admits the job. Extension code cannot override this decision and receives no callback for a rejected folder. File-item calculations remain unaffected by folder admission limits.

The first implementation applies this reusable policy to folder calculations dispatched through the existing data-column job path. Other contribution kinds that do not calculate folder items do not acquire meaningless settings or new behavior.

## Code Lines behavior

The Code Lines contribution declares `max_file_count = 999`, which implements the required `File Count < 1000` rule. It does not declare a folder-count maximum.

For a folder row, Code Lines has these states:

1. While exact directory facts are pending, it displays `等待 File Count…` and does not start Code Lines work.
2. At 0 through 999 files, the Host admits and starts Code Lines.
3. At 1000 or more files, the Host does not invoke Code Lines and displays `File Count 超過限制，因此未啟動`.
4. If File Count is unavailable, incomplete, or stale, the Host does not invoke Code Lines and displays `依賴 File Count，因此未啟動`.

The File Count column does not need to be visible for this dependency to run. Regular file rows continue to use existing Code Lines behavior without a folder-count gate.

## Error and lifecycle behavior

Directory-facts failures are terminal for the current request context but do not disable the extension package. A later refresh, service recovery, or newer MFT generation may retry. Errors remain isolated per folder and contribution.

Admission and dispatched work carry the request context and MFT generation used for the decision. Results from an obsolete tab generation or cancelled request are discarded. If the relevant MFT generation changes before dispatch, the Host returns the contribution to its dependency-pending state and obtains fresh facts.

Unavailable counts render as `—`; they are not coerced to zero. Threshold comparisons use saturating-safe unsigned arithmetic, and converting the root-inclusive MFT directory count to descendant folders cannot underflow.

## Compatibility and scope

The new built-in IDs require exhaustive-match updates but do not change extension-owned column namespaces. Session decoding continues to accept every previous built-in layout. Extension manifests without folder admission remain valid and preserve their current callback behavior.

This change does not add user-editable threshold controls, localization resources, filesystem scanning fallback, reparse-point traversal, direct-child-only counts, or changes to unrelated extension commands and views.

## Verification

Unit and integration coverage must verify:

- recursive file counting and root-excluded recursive folder counting;
- exclusion and non-traversal of reparse points, junctions, and symbolic links;
- MFT-only behavior for NTFS, virtual, unsupported, unavailable, partial, and stale results;
- stable-ID parsing, descriptor validation, default-hidden layout, resize/reorder persistence, and legacy session migration;
- independent column toggles, blank file rows, exact integer rendering, and sorting with unavailable values excluded;
- one deduplicated MFT query shared by both columns and multiple dependent extensions;
- cache invalidation on MFT generation change, refresh, watcher events, navigation, and cancellation;
- manifest validation and AND semantics for zero, one, or two admission limits;
- absence of an extension callback while facts are pending, unavailable, stale, partial, or over limit;
- unchanged behavior for existing contributions without a policy and for regular file calculations;
- Code Lines admission at 999 files, rejection at 1000 files, and the three dependency/limit status messages;
- Code Lines dependency acquisition while the File Count column is hidden.

Final validation must run the focused model, MFT, extension-host, application, and UI tests, followed by the repository's standard build/test/install validation. A headful test must show the two optional columns, verify their recursive values and sorting, and prove that Code Lines starts below the boundary but remains undispatched with the correct status at and above the boundary or when MFT facts are unavailable.
