## Why

Column sorting must preserve the familiar Windows File Explorer boundary between real folders and files. The first implementation used the navigation-oriented `is_container` flag, which incorrectly places browsable ZIP files in the folder group.

## What Changes

- Define one observable real-folder-first ordering contract for every file-surface column.
- Treat ZIP and other browsable filesystem archives as files without changing their navigation behavior.
- Keep folders before files for both ascending and descending directions.
- Sort folders and files independently by the selected column, with deterministic name and provider-identity tie-breakers.
- Use local filesystem directory metadata when available and fall back to provider container classification only for non-filesystem or metadata-unavailable entries.
- Apply the same classification rule to built-in and runtime extension byte columns.
- Add regression coverage for direction changes, optional values, and mixed folder/file input.
- Do not add group headings, new preferences, persisted state, or Windows "Group by" modes.

## Capabilities

### New Capabilities

- `file-surface-column-sorting`: Defines classification-first, deterministic column ordering for folders and files on the main file surface.

### Modified Capabilities

None.

## Impact

- Primary code: `crates/explorer-ui/src/file_view.rs`; existing `FileEntryMetadata::filesystem_attributes` and `size_bytes` provide the classification evidence.
- Test alignment may affect comparator helpers and assertions in `crates/explorer-ui/src/chrome.rs`.
- No public API, extension ABI, persisted session schema, dependency, or migration impact.
