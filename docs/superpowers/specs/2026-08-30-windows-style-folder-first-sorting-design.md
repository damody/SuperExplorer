# Windows-style folder-first sorting design

## Goal

Make every file-surface column sort match the core Windows File Explorer behavior: folders form one contiguous group before files, and each group is independently ordered by the selected column and direction. No group headings or separators are added.

## Behavior

- Folders always precede files for both ascending and descending sorts.
- Within the folder group and within the file group, the selected built-in or extension column determines order.
- Ascending and descending direction reverses the selected value comparison inside each group; it never reverses the folder/file group order.
- Missing optional column values remain after present values inside their own group in either direction, matching the existing size-column contract.
- Equal primary values use the existing name comparison and stable provider identity as deterministic tie-breakers.
- Filtering and hidden-item visibility run before sorting and do not change classification.
- Folder-like containers from local, remote, shell, and extension-backed providers use the existing `is_container` classification.

## Architecture

`DirectoryPresentation` remains the single production owner of visible ordering. Its built-in comparator and runtime extension-column comparator will share an explicit folder-before-file comparison helper so their group semantics cannot drift. Callers continue to consume ordered snapshot indices and require no rendering changes.

The test-only comparator in the window chrome tests will be kept consistent with production semantics or replaced by assertions through `DirectoryPresentation` where practical. No persisted setting, public API, or data-model migration is required.

## Alternatives considered

1. **Shared classification-first comparator (chosen).** Applies the invariant before every column-value comparison and keeps all sorting paths consistent.
2. **Partition after sorting.** Simple, but duplicates traversal and can obscure stability and missing-value behavior.
3. **Patch only the observed column.** Smallest edit, but leaves other built-in and extension columns vulnerable to inconsistent behavior.

## Error and edge handling

Sorting is in-memory and cannot fail. If cached sort keys are unavailable, the existing stable snapshot-index fallback remains within the folder or file group. Empty groups, a directory containing only one kind, equal values, and absent extension values must remain deterministic and panic-free.

## Verification

- Unit tests cover name and non-name built-in columns in ascending and descending directions.
- Unit tests prove folders remain first while each group reverses independently.
- Extension-byte sorting receives the same folder-first coverage.
- Mixed present/missing values are checked without allowing a file to cross ahead of a folder.
- Run formatting and focused `explorer-ui` tests, followed by the broader relevant package test suite if focused checks pass.

## Non-goals

- Visible `Folders` and `Files` section headings.
- User-configurable group ordering.
- Windows Explorer grouping modes such as “Group by type/date”.
- Locale-aware natural sorting changes beyond the repository's existing sort-key behavior.
