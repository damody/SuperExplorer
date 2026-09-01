# Navigable Archives Sort as Files

## Goal

Keep real folders before files for every file-surface column sort while treating
ZIP and other filesystem archive files as files even when Windows Shell exposes
them as browsable containers.

## Classification contract

Sorting uses a presentation-specific folder classification rather than the
navigation-oriented `FileEntry::is_container` flag.

- For a local filesystem item with filesystem metadata, the
  `FILE_ATTRIBUTE_DIRECTORY` bit determines whether it belongs to the folder
  group.
- A local filesystem item with a file size is a file even if Shell reports it as
  a browsable container. This explicitly covers ZIP files.
- For Shell namespace, remote, and virtual entries that have no local
  filesystem-directory metadata, sorting falls back to the provider-neutral
  `is_container` value.
- This classification changes presentation ordering only. It does not change
  whether an archive can be opened or enumerated.

## Integration

The built-in column comparator and runtime extension-byte comparator call one
shared classification helper before comparing values. Sort direction affects
values within the folder group and file group, never the group boundary.
Existing missing-value and deterministic tie-break behavior remains unchanged.

## Failure behavior

If local filesystem metadata is unavailable, a present file size is sufficient
to classify the item as a file. Otherwise the comparator falls back to
`is_container` so unavailable folders and non-filesystem providers retain usable
ordering without filesystem I/O in the render path.

## Testing

- Construct a real-folder entry, a ZIP entry with `is_container=true` and file
  attributes/size, and a normal file.
- Assert folders precede both files for ascending and descending name and
  metadata sorts.
- Assert runtime extension-byte sorting uses the same boundary.
- Assert the ZIP entry remains `is_container=true`, proving navigation behavior
  is unchanged while its sorting classification is file-like.

## Scope

No visible group headers, persisted setting, Shell association change, archive
navigation change, or extension ABI change is included.
