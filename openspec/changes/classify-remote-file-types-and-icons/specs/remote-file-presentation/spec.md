## ADDED Requirements

### Requirement: Deterministic remote file Type labels
The system SHALL derive the Type label of ADB and SFTP files from the displayed filename with a pure, case-insensitive classifier and SHALL NOT inspect or download file content.

#### Scenario: Conventional extension
- **WHEN** an ADB or SFTP regular file is named `report.txt` or `PHOTO.JpG`
- **THEN** its Type label is `TXT File` or `JPG File`, respectively

#### Scenario: Known compound extension
- **WHEN** an ADB or SFTP regular file is named `backup.tar.gz`
- **THEN** its Type label is `TAR.GZ File`

#### Scenario: Compressed non-tar filename
- **WHEN** an ADB or SFTP regular file is named `firmware.bin.gz`
- **THEN** its Type label is `GZ File`

#### Scenario: Extensionless boundary
- **WHEN** an ADB or SFTP regular file has no usable suffix, is a bare `.`, or ends in `.`
- **THEN** its Type label is `File`

### Requirement: Linux dotfile Setting labels
The system SHALL recognize a filename with one leading dot and no other dot as a dotfile setting, remove the leading dot, split `_` and `-` into words, title-case each word, and append ` Setting File`.

#### Scenario: Simple dotfile
- **WHEN** an ADB or SFTP regular file is named `.bashrc`
- **THEN** its Type label is `Bashrc Setting File`

#### Scenario: Multiword dotfile
- **WHEN** an ADB or SFTP regular file is named `.bash_logout`
- **THEN** its Type label is `Bash Logout Setting File`

#### Scenario: Dot-prefixed directory
- **WHEN** an ADB or SFTP directory is named `.ssh`
- **THEN** its Type label remains `Remote folder`

### Requirement: Authoritative remote kind semantics
The system SHALL use provider metadata, never filename classification, to determine container and symlink semantics.

#### Scenario: Classified regular-file symlink
- **WHEN** the provider reports `notes.txt` as a file symlink
- **THEN** the row is not a container and its Type label is `TXT File link`

#### Scenario: Directory and exceptional symlinks
- **WHEN** the provider reports a directory symlink, broken symlink, or circular symlink
- **THEN** the existing exact Type label and container behavior for that kind are preserved

### Requirement: Stable built-in icon categories
The system SHALL render built-in category icons for common ADB/SFTP file families and SHALL use the same filename classifier that supplies Type labels. Every category SHALL have vector geometry that is distinguishable from every other category at 16–20 logical pixels without relying only on color or a text badge.

#### Scenario: Required representative icons
- **WHEN** remote files are named `manual.pdf`, `readme.txt`, `photo.jpg`, `backup.tar.gz`, `firmware.bin.gz`, and `bundle.tgz`
- **THEN** PDF, text, and image receive their respective icons and all three compressed examples receive the archive icon

#### Scenario: Common category coverage
- **WHEN** a remote filename has an extension in the specified text/configuration, image, archive, audio, video, code/script, executable/binary, office-document, or PDF family
- **THEN** the system renders the corresponding stable built-in category icon

#### Scenario: Unknown category fallback
- **WHEN** a remote file is extensionless or its extension has no mapped icon family
- **THEN** the system renders the generic built-in file icon while retaining the deterministic Type label

#### Scenario: Linux dotfile settings icon
- **WHEN** an ADB or SFTP regular file is a valid single-component dotfile such as `.bashrc` or `.profile`
- **THEN** it renders the dedicated settings glyph rather than the ordinary text glyph while retaining its `Setting File` Type label

#### Scenario: Details-view silhouette recognition
- **WHEN** generic, PDF, text, settings, image, archive, audio, video, code, executable, and office categories are rendered at 16 or 20 logical pixels
- **THEN** each category exposes a unique geometry identity and visible category-specific mark without requiring a large-view badge

### Requirement: Remote-only presentation change
The system SHALL restrict categorical fallback icons and filename-based Type labels to ADB/SFTP rows and SHALL preserve local Windows Shell and thumbnail behavior.

#### Scenario: Remote container precedence
- **WHEN** an ADB/SFTP row is a container even if its name resembles a file extension
- **THEN** the folder fallback icon takes precedence over filename classification

#### Scenario: Local file preservation
- **WHEN** a local filesystem row is rendered
- **THEN** its Shell icon, overlay, thumbnail, and existing Type-label paths remain unchanged

#### Scenario: View-size scaling
- **WHEN** an ADB/SFTP file is rendered in any supported file-view icon size
- **THEN** its built-in category icon fits the requested square host without changing row geometry

### Requirement: Official Fluent asset provenance
The system SHALL render selected file-family icons from pinned Microsoft Fluent UI System Icons package `@fluentui/svg-icons@1.1.339`, SHALL keep vendored source provenance and hashes, and SHALL perform no runtime asset download.

#### Scenario: Color asset rendering
- **WHEN** a mapped family has an official 20px Color SVG variant
- **THEN** the renderer preserves its upstream fills or gradients and does not replace them with a single theme tint

#### Scenario: Exact monochrome fallback
- **WHEN** the pinned package lacks an exact Color variant but provides an exact official regular or filled glyph
- **THEN** the renderer uses that official glyph with a stable family tint instead of a locally invented silhouette

#### Scenario: Offline resolution
- **WHEN** an ADB or SFTP row resolves a category icon without network connectivity
- **THEN** its icon loads from the embedded vendored asset namespace

### Requirement: Broad auditable extension taxonomy
The system SHALL keep centralized, ordered extension tables that assign common Office, Windows, Linux, Android, developer, document, archive, media, font, certificate/key, disk-image, database, and web/data filenames to explicit icon families. Longest known compound extensions SHALL take precedence over final extensions.

#### Scenario: Distinct common Office families
- **WHEN** remote files use Word (`doc`, `docx`, `docm`, `dot`, `dotx`, `odt`, `rtf`), spreadsheet (`xls`, `xlsx`, `xlsm`, `xlsb`, `csv`, `ods`), presentation (`ppt`, `pptx`, `pptm`, `pps`, `ppsx`, `odp`), notebook (`one`, `onetoc2`), database (`accdb`, `mdb`), or mail (`pst`, `ost`, `msg`, `eml`) extensions
- **THEN** each Office family receives its own stable official Fluent glyph rather than one shared Office icon

#### Scenario: Android and Linux system formats
- **WHEN** representative ADB files use `conf`, `xml`, `json`, `json.gz`, `prop`, `pb`, `cil`, `policy`, `rc`, `sh`, `so`, `o`, `bc`, `prof`, or `bprof`
- **THEN** each maps to the documented settings, markup/data, archive, script, executable/binary, or developer family without content I/O

#### Scenario: Compound boundary
- **WHEN** a known compound extension such as `tar.gz`, `tar.xz`, `tar.zst`, `json.gz`, or `svg.gz` is compared with a near miss
- **THEN** only the exact case-insensitive suffix match receives the compound classification and the near miss follows ordinary final-extension rules

#### Scenario: Matrix completeness
- **WHEN** the declared extension tables change
- **THEN** a table-driven test enumerates every declared entry and proves its expected Type label and icon family, including upper-case representatives

### Requirement: GPUI-visible official glyphs
The system SHALL use GPUI-compatible official Fluent Filled SVGs for every remote file category and SHALL NOT use an SVG feature that causes a mapped glyph to render transparent.

#### Scenario: Every mapped category is visible
- **WHEN** any of the 24 remote file categories is rendered at 16 or 20 logical pixels
- **THEN** its embedded SVG contains non-empty visible path geometry painted through `currentColor`

#### Scenario: Unsupported Color paint is rejected
- **WHEN** a vendored remote SVG contains a gradient, `url(#...)` paint, external reference, script, embedded image, filter, mask, or `foreignObject`
- **THEN** the asset compatibility test fails

#### Scenario: Screenshot regression
- **WHEN** settings, text, code, archive, and PDF files appear together in an ADB/SFTP Details listing
- **THEN** every row shows a non-transparent category glyph and the existing archive/PDF glyphs remain visible
