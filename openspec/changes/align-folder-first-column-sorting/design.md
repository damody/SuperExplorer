## Context

The main file surface builds an immutable `DirectoryPresentation` from a directory snapshot, filters it, and sorts snapshot indices. Both the built-in comparator and the runtime extension-byte sorting path currently perform their own `is_container` checks. The visible behavior is intended to match Windows File Explorer, but duplicated classification logic and incomplete direction/column coverage make that contract fragile.

Constraints are to retain current snapshot sharing, filtering, optional-value ordering, deterministic tie-breakers, and archive navigation. `is_container` cannot be the sorting authority because Windows marks ZIP files as browsable containers. No renderer or persisted-model change is needed.

## Goals / Non-Goals

**Goals:**

- Make real-folder-before-file ordering an explicit invariant shared by all production column-sort paths.
- Keep ZIP and other browsable filesystem archives in the file group while preserving their `is_container` navigation capability.
- Preserve selected-column ascending or descending comparison independently within the folder and file groups.
- Preserve deterministic ordering and existing missing-value behavior.
- Verify built-in and runtime extension byte sorting with mixed folder/file tests.

**Non-Goals:**

- Add visible section headings or separators.
- Implement configurable grouping or Windows "Group by" modes.
- Change natural, locale, case, or shell collation semantics.
- Change provider classification, public APIs, extension ABI, or persisted settings.

## Decisions

### Derive a sorting-specific folder classification

A local filesystem entry belongs to the folder group only when its filesystem attributes contain `FILE_ATTRIBUTE_DIRECTORY` (`0x10`). A present file size is explicit evidence that a browsable container is an on-disk file. Entries without usable local filesystem evidence, including remote, virtual, and Shell namespace folders, fall back to `is_container`. This is presentation-only classification and does not modify the entry or its ability to open.

Every production sort path invokes the shared helper before comparing column values. Because sort direction is applied only to the value comparison, descending order cannot move files ahead of real folders.

This is preferred over changing `is_container`, which would break archive navigation, and over extension-name checks, which would miss other registered browsable file types. It also avoids filesystem I/O in the comparator by using snapshot metadata.

### Keep `DirectoryPresentation` as the ordering boundary

Rendering and callers will continue to consume ordered indices. The change stays in `file_view.rs`, avoiding duplicate ordering in UI rendering. Runtime extension byte values continue to reorder a cloned presentation but use the same classification helper.

### Retain existing fallback and tie-breakers

Within a group, missing optional values remain last in both directions. Equal primary values use display name followed by provider identity. If snapshot sort keys are unavailable, snapshot index remains the stable fallback after classification has been established.

### Verify behavior at the presentation layer

Tests will construct mixed snapshots containing a true directory, a ZIP-like entry with `is_container=true` plus file attributes/size, and a normal file. They assert complete visible sequences for ascending and descending built-in columns and extension-byte values, and assert the archive remains navigable.

## Risks / Trade-offs

- [Risk] Local metadata is unavailable → A present size classifies the item as a file; otherwise fall back to `is_container` so provider folders remain usable.
- [Risk] A ZIP is recognized only by extension → Classification deliberately uses filesystem metadata rather than names or associations.
- [Risk] Refactoring a comparator changes equal-value ordering → Preserve current name, provider-ID, and snapshot-index fallbacks and assert deterministic sequences.
- [Risk] Tests cover only name sorting → Include a metadata column and extension byte values in both directions.
- [Trade-off] Exact Windows shell collation is not introduced → This change aligns grouping behavior without broadening into locale/natural-sort work.

## Migration Plan

Land the comparator refactor and regression tests together. No data migration or staged rollout is required. Rollback is a source revert because no persisted or public contract changes are introduced.

## Open Questions

None. The user selected folder-first display without group headings and authorized Windows-aligned implementation decisions.
