# Remote File Types and Icons Design

## Problem

ADB and SFTP rows currently derive the Type column only from `RemoteEntryKind`. Every regular file is therefore shown as `Remote file`, and remote files never enter the Shell icon-loading path, so they all receive the generic document fallback.

## Scope

This change gives ADB and SFTP files deterministic Type text and built-in icons based on their names. It does not inspect file contents, query MIME databases, use Windows file associations, download remote files, or change local-filesystem Shell icons.

## Filename classification

`explorer-model` will own one case-insensitive, allocation-conscious filename classifier. Both remote metadata construction and remote fallback-icon rendering consume its result.

Classification order is significant:

1. A name that begins with exactly one leading dot and contains no other dot is a dotfile setting, provided text follows the dot. The leading dot is removed, `_` and `-` become word separators, each word is title-cased, and ` Setting File` is appended. Examples: `.bashrc` becomes `Bashrc Setting File`, `.bash_logout` becomes `Bash Logout Setting File`, `.profile` becomes `Profile Setting File`, and `.gitignore` becomes `Gitignore Setting File`.
2. Known compound extensions are matched longest-first. Archive compounds include `tar.gz`, `tar.bz2`, `tar.xz`, `tar.zst`, `tar.lz`, and `tar.lz4`. Their Type text uses the normalized compound extension, for example `TAR.GZ File`.
3. A conventional final extension produces upper-case `EXT File`, for example `TXT File`, `JPG File`, and `GZ File`.
4. Empty names, names without an extension, a bare `.`, names ending in `.`, and names whose final suffix is empty fall back to `File`.

Directories retain `Remote folder`. File and directory symlinks retain link semantics: a classified file type appends ` link` (for example `TXT File link` or `Bashrc Setting File link`), while directory, broken, and circular links keep their existing labels.

## Icon classification

The same classifier returns a stable icon category. The initial built-in coverage is:

- PDF: `pdf`.
- Text/configuration: `txt`, `text`, `log`, `md`, `markdown`, `rtf`, `ini`, `cfg`, `conf`, `config`, `toml`, `yaml`, `yml`, `json`, `xml`, `csv`, `tsv`, and dotfiles.
- Images: `jpg`, `jpeg`, `png`, `gif`, `bmp`, `webp`, `tif`, `tiff`, `svg`, `ico`, `heic`, `heif`, `avif`, and `raw`.
- Archives/compressed files: `zip`, `7z`, `rar`, `tar`, `gz`, `tgz`, `bz2`, `tbz`, `tbz2`, `xz`, `txz`, `zst`, `tzst`, `lz`, `lz4`, `cab`, `iso`, and every recognized archive compound extension. Thus `tar.gz`, `bin.gz`, and `tgz` all use the archive icon.
- Audio: `mp3`, `wav`, `flac`, `aac`, `m4a`, `ogg`, `opus`, `wma`, and `mid`/`midi`.
- Video: `mp4`, `mkv`, `mov`, `avi`, `webm`, `wmv`, `m4v`, `mpeg`, and `mpg`.
- Code/script: `rs`, `c`, `h`, `cpp`, `hpp`, `cc`, `cs`, `java`, `kt`, `kts`, `go`, `py`, `pyw`, `js`, `jsx`, `ts`, `tsx`, `html`, `htm`, `css`, `scss`, `sass`, `less`, `php`, `rb`, `swift`, `sh`, `bash`, `zsh`, `fish`, `ps1`, `bat`, `cmd`, `lua`, `sql`, and `wasm`.
- Executable/binary: `exe`, `msi`, `appx`, `apk`, `aab`, `bin`, `dll`, `so`, `dylib`, `deb`, `rpm`, and `jar`.
- Office/documents: `doc`, `docx`, `odt`, `xls`, `xlsx`, `ods`, `ppt`, `pptx`, and `odp`.
- Unknown or extensionless files: generic file.

The UI will render small, theme-compatible built-in glyphs for these categories and retain the existing folder icon for containers. The same categorical fallback scales with all view modes and does not enter the Windows Shell cache. Local items continue using their Shell/thumbnail pipeline unchanged. Remote symlinks use the target file category when the target is a file; no new overlay is required in this change.

## Data flow and boundaries

1. ADB or SFTP providers return `RemoteEntry` with an authoritative entry kind and display name.
2. `explorer-app::remote_service` preserves the authoritative container/link kind and asks `explorer-model` to classify only file names.
3. The resulting Type string is stored in `FileEntryMetadata::type_display` as today.
4. `explorer-ui` detects ADB/SFTP virtual locations. If no loaded bitmap exists, it asks the same model classifier for a category and renders the corresponding built-in icon. Local fallback behavior is unchanged.

The filename never determines whether an item is a directory or symlink. Provider metadata remains authoritative for those security- and navigation-relevant decisions.

## Error handling and compatibility

Classification is pure and total: arbitrary UTF-8 names return a category and label without I/O or failure. Non-ASCII suffixes are displayed in Unicode upper case but remain in the generic icon category unless explicitly mapped. Existing special link labels and all commands, sorting identities, navigation capabilities, transfers, and remote protocol behavior remain unchanged.

## Testing and verification

Unit tests will cover case folding, conventional extensions, compound extensions, dotfile title formatting, extensionless and malformed edge cases, every icon-category family, and archive examples `tar.gz`, `bin.gz`, and `tgz`. Remote-service tests will cover both row construction paths and every `RemoteEntryKind`, including classified file symlinks. UI tests will verify remote-only categorical fallback selection, folder precedence, local behavior preservation, and scalable rendering integration.

Verification requires formatting, focused tests for `explorer-model`, `explorer-app`, and `explorer-ui`, relevant crate compilation, strict OpenSpec validation, and a final diff/spec/test review. A failed check is repaired and rerun; it is not accepted as completion.

## Rollback

The change is isolated to a pure classifier, remote metadata label construction, and remote fallback rendering. Reverting those call sites restores the previous `Remote file` and generic document behavior without data migration.
