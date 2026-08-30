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
The system SHALL render built-in category icons for common ADB/SFTP file families and SHALL use the same filename classifier that supplies Type labels.

#### Scenario: Required representative icons
- **WHEN** remote files are named `manual.pdf`, `readme.txt`, `photo.jpg`, `backup.tar.gz`, `firmware.bin.gz`, and `bundle.tgz`
- **THEN** PDF, text, and image receive their respective icons and all three compressed examples receive the archive icon

#### Scenario: Common category coverage
- **WHEN** a remote filename has an extension in the specified text/configuration, image, archive, audio, video, code/script, executable/binary, office-document, or PDF family
- **THEN** the system renders the corresponding stable built-in category icon

#### Scenario: Unknown category fallback
- **WHEN** a remote file is extensionless or its extension has no mapped icon family
- **THEN** the system renders the generic built-in file icon while retaining the deterministic Type label

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
