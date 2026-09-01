## Why

ADB and SFTP currently label every regular file as `Remote file` and render it with one generic document icon. Filename-based types and stable built-in icons make remote listings as scannable as local listings without downloading content or depending on machine-specific associations.

## What Changes

- Classify remote filenames case-insensitively, including longest-match compound archive extensions.
- Display Windows-style upper-case extension types for ordinary files and descriptive `Setting File` types for Linux dotfiles.
- Preserve authoritative directory and symlink semantics while adding classified file-link labels.
- Render built-in category icons for common documents, text/configuration, images, archives, audio, video, source code, binaries, and office files in ADB/SFTP listings.
- Give each category a distinct vector silhouette that remains recognizable in 16–20px Details view, including a dedicated settings icon for single-component Linux dotfiles.
- Vendor selected official Microsoft Fluent UI System Icons from pinned `@fluentui/svg-icons@1.1.339`, using color variants where available and dedicated official Office/document glyphs for Word, Excel-like spreadsheets, PowerPoint-like presentations, OneNote-like notebooks, databases, and mail.
- Expand the auditable filename-family matrix across common Office, Windows, Linux, Android, developer, document, archive, media, font, certificate/key, disk-image, database, and web/data extensions, including formats observed on `adb://emulator-5554/`.
- Replace Fluent Color SVG variants that GPUI renders transparently with the corresponding official Fluent Filled SVG variants, retaining distinct silhouettes and category tints.
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
- Tests change in those three crates. Selected SVGs and an upstream notice are vendored at build time; there is no runtime dependency, persistence, ABI, remote-provider, or external-service change.
