# Official Fluent Color Remote File Icons

## Goal

Replace the locally drawn ADB/SFTP file-category glyphs with recognizable Microsoft Fluent UI System Icons color SVG assets and broaden the extension taxonomy, while preserving folder handling, symlink behavior, and the local-file icon path.

## Source and licensing

- Source package: `@fluentui/svg-icons` version `1.1.339`.
- Package license metadata: MIT.
- Download only the selected SVG files into the repository; do not add a runtime npm dependency.
- Record the package version, upstream repository, selected original paths, and license in a notice adjacent to the vendored assets.

## Selected approach

Use official 20-pixel Color SVG variants as embedded application assets. Color variants preserve their original multiple fills and gradients; the renderer must not apply the existing monochrome `text_color` tint. Every remote category receives a distinct silhouette. Where Fluent has no exact file-format icon, use the closest semantically recognizable official icon and retain the existing accessible label.

Use distinct official assets for common Office families rather than one shared Office icon: Word, Excel, PowerPoint, OneNote, database/Access-like files, and mail/Outlook-like files. Additional families cover PDF, plain/rich text, settings/configuration, image, archive/zip, audio, video, source code, shell/script, executable/binary, Android package, font, certificate/key, disk image, database, web, markup/data, and generic document. Exact upstream names may be adjusted only when the pinned package lacks the preferred asset.

The extension tables must be broad and auditable. They include common Windows, Linux, Android, developer, media, archive, document, and compound extensions. The implementation must include extensions observed from a read-only scan of `adb://emulator-5554/`, including Android/Linux system examples such as `conf`, `xml`, `json`, `json.gz`, `prop`, `pb`, `cil`, `policy`, `rc`, `sh`, `so`, `o`, `bc`, `prof`, `bprof`, `pem`-style certificate material, and extensionless executables/configuration files where classification is safe. Unknown values still fall back to the generic official document icon.

## Architecture and data flow

The shared model maps filenames to an expanded `RemoteFileIconKind`. Ordered compound-extension matching runs before final-extension matching. `remote_file_icon_spec` maps the kind to a vendored asset name and accessible label. The GPUI asset registry serves the unmodified SVG bytes from a dedicated `remote-file/fluent-color/` namespace. The renderer scales the SVG inside the requested host size without recoloring it.

No network access occurs at runtime. ADB and SFTP continue to use this fallback only when their existing bitmap/shell icon path is unavailable. Local filesystem rows remain unchanged.

## Failure handling

Unknown filenames use the official generic document icon. Missing compile-time mappings fail focused asset-registry tests. Download provenance is auditable through the notice and file hashes. Vendored SVG parsing must not rely on unsupported external references.

## Verification

- Assert every category mapping resolves to the intended vendored SVG payload and semantically distinct families do not accidentally share an asset.
- Assert color assets retain multiple explicit fills or gradients and are not `currentColor` monochrome glyphs.
- Assert the renderer does not apply a tint and remains bounded at 16px, 20px, and larger sizes.
- Add a table-driven extension matrix covering every declared extension, compound extension, uppercase variants, boundary/near-miss cases, and representative filenames from the connected ADB device.
- Re-run classifier, remote icon selection, asset registry, compilation, formatting, OpenSpec strict validation, and evidence reconciliation.

## Scope exclusions

Do not change filename classification, Type wording, local Windows shell icons, folder icons, thumbnail precedence, or introduce an npm/runtime dependency.
