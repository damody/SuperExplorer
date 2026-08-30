## Why

ADB and SFTP currently label every regular file as `Remote file` and render it with one generic document icon. Filename-based types and stable built-in icons make remote listings as scannable as local listings without downloading content or depending on machine-specific associations.

## What Changes

- Classify remote filenames case-insensitively, including longest-match compound archive extensions.
- Display Windows-style upper-case extension types for ordinary files and descriptive `Setting File` types for Linux dotfiles.
- Preserve authoritative directory and symlink semantics while adding classified file-link labels.
- Render built-in category icons for common documents, text/configuration, images, archives, audio, video, source code, binaries, and office files in ADB/SFTP listings.
- Keep local Shell/thumbnail behavior and all remote protocol, transfer, command, and navigation behavior unchanged.

## Capabilities

### New Capabilities

- `remote-file-presentation`: Deterministic filename-based Type labels and category icons for ADB and SFTP file rows.

### Modified Capabilities

None.

## Impact

- `explorer-model` gains the shared pure filename classifier and stable icon categories.
- `explorer-app` uses classification when converting ADB/SFTP entries to rows.
- `explorer-ui` renders category-aware built-in fallback icons only for ADB/SFTP virtual files.
- Tests change in those three crates; no dependency, persistence, ABI, remote-provider, or external-service changes are required.
