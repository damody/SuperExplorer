# Remote column filesystem applicability and Unix permissions

## Goal

Keep Details view truthful when browsing Local, ADB, and SFTP locations. Columns that cannot produce meaningful data for the active filesystem are omitted from the header, rows, sorting surface, and column menu. ADB and SFTP entries expose Unix permissions in symbolic and octal form.

## Filesystem identity

The UI derives one filesystem identity from the active location:

- `local` for ordinary Windows filesystem locations;
- `adb` for locations owned by the ADB provider;
- `sftp` for locations owned by the SFTP provider.

Unknown virtual providers do not inherit any of these identities. This prevents a new provider from accidentally enabling a contribution that has not declared support for it.

## Extension manifest contract

Every extension data-column contribution declares the filesystems on which it can run:

```json
"file_systems": ["local"]
```

The supported values are `local`, `adb`, and `sftp`. A contribution may declare more than one value. The extension author owns this declaration; users cannot override it.

An absent or empty `file_systems` array means the contribution is disabled on every filesystem. This fail-closed default applies to legacy manifests as well as new manifests. An unknown value is a manifest validation error for that contribution and produces one bounded diagnostic rather than silently broadening its scope.

The bundled Folder size, Main code lines, Code lines, and Lock owner contributions explicitly declare `["local"]`. This preserves their existing local behavior while removing them from ADB and SFTP views.

The validated filesystem set becomes part of the column descriptor passed across the extension protocol. Admission checks occur before the Host requests extension data, so an inapplicable column starts no background work and receives no local or remote entry payload.

## Effective column projection

The persisted ordered column layout remains the user's source of truth for order, width, and visibility. Details view derives an effective projection by intersecting that layout with columns applicable to the active filesystem.

An inapplicable column is absent from:

- the Details header and every row;
- the column selection and filter menus;
- auto-sizing, drag/reorder targets, and extension data requests;
- the available sorting surface.

Hiding a column through projection never mutates its persisted visibility, width, or position. Returning to a compatible filesystem restores the prior layout automatically.

If the persisted sort column is inapplicable, the effective remote sort temporarily falls back to Name ascending. The persisted sort descriptor is retained so returning to a compatible filesystem restores it.

Built-in applicability is Host-owned. ADB and SFTP retain Name, Date modified, Type, and Size when supplied by provider metadata. Windows Shell, MFT, content-analysis, and local-process columns are excluded. Permissions applies to ADB and SFTP and is excluded from Local Windows locations.

## Unix permissions metadata

`FileEntryMetadata` gains an optional Unix mode carrying the file type and permission bits required for presentation and numeric sorting.

- SFTP maps the protocol attributes' permissions/mode value without interpreting it as a Windows attribute.
- ADB obtains the mode as part of its bounded directory metadata query and maps it onto each entry.
- Missing, malformed, or unsupported mode data becomes `None`; it does not fail the directory listing or emit per-entry error logs.

The new built-in Permissions column formats a known mode as both symbolic and four-digit octal text:

- directory: `drwxr-xr-x (0755)`;
- regular file: `-rw-r--r-- (0644)`;
- symbolic link: `lrwxrwxrwx (0777)`.

Other recognized Unix file types use their standard leading character. Unknown file type bits use `?`. Special permission bits use the standard `s`, `S`, `t`, and `T` forms. A missing mode renders an em dash. Sorting compares the numeric mode and places missing values consistently after known values.

## Data flow

1. The active location resolves to a filesystem identity.
2. The column registry combines built-in applicability with validated extension manifest applicability.
3. Details view projects the persisted layout for the active filesystem.
4. Remote providers populate ordinary metadata and optional Unix mode.
5. The shared header, row, menu, sorting, and request coordinators consume the same effective projection.

Using one projection prevents header/cell drift and ensures hidden extension columns cannot continue background computation.

## Failure behavior

- Invalid manifest filesystem names reject the affected contribution with one actionable diagnostic.
- Missing manifest applicability is fail-closed and produces no repeated runtime errors.
- Missing remote permission metadata renders an em dash.
- A remote metadata failure retains the existing directory-listing error behavior; permission formatting never introduces a second error path.

## Verification

Targeted verification covers:

- manifest parsing for single, multiple, empty, absent, duplicate, and unknown filesystem values;
- bundled manifests explicitly declaring `local`;
- effective projection across Local, ADB, SFTP, and unknown virtual providers without mutating persisted layout;
- temporary Name sorting fallback and restoration semantics;
- suppression of extension work for inapplicable columns;
- ADB and SFTP mode mapping, including missing metadata;
- symbolic plus four-digit octal formatting for files, directories, links, and special bits;
- header, row, and column-menu alignment for remote Details views.

Only focused crate tests and compilation relevant to these paths are required. A complete regression run is outside this change.
