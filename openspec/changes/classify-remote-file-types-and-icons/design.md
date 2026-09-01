## Context

ADB and SFTP providers already return an authoritative `RemoteEntryKind` and filename. `explorer-app::remote_service` currently maps the kind directly to a static Type label, while `explorer-ui` deliberately excludes remote virtual locations from Windows Shell and thumbnail loading and therefore renders one generic document fallback. The implementation must preserve that I/O boundary, avoid machine-dependent Windows associations, and work at every file-view icon size.

The approved source designs are `docs/superpowers/specs/2026-08-30-remote-file-types-and-icons-design.md`, the corrective `docs/superpowers/specs/2026-08-30-recognizable-remote-file-icons-design.md`, the official-asset expansion `docs/superpowers/specs/2026-08-31-official-fluent-color-remote-file-icons-design.md`, and the GPUI compatibility correction `docs/superpowers/specs/2026-08-31-gpui-fluent-icon-visibility-fix-design.md`.

## Goals / Non-Goals

**Goals:**

- Give ADB and SFTP files deterministic Type labels derived from filename extensions.
- Give single-component Linux dotfiles descriptive `Setting File` labels.
- Give common filename families stable built-in icons, including compound and single compressed extensions.
- Use one classifier for metadata and icon selection so the two presentations cannot drift.
- Preserve provider-authoritative directory and symlink decisions and all local Shell behavior.

**Non-Goals:**

- Content sniffing, MIME lookup, remote downloads, Windows registry association lookup, thumbnails, or remote icon overlays.
- Changing local filesystem Type labels or icons.
- Changing remote commands, transfers, persistence, sorting identities, capabilities, or protocol behavior.

## Decisions

### Shared pure classifier in `explorer-model`

`explorer-model` will expose a filename-classification value containing a stable `RemoteFileIconKind` category and methods that produce the base Type label. It accepts arbitrary UTF-8, performs no I/O, and is independent of ADB/SFTP types. Valid single-component dotfiles receive the dedicated `Settings` icon kind while retaining their approved `Setting File` Type text. `explorer-app` combines classification with `RemoteEntryKind`; `explorer-ui` uses its icon category only for ADB/SFTP virtual files.

This is preferred over independent app/UI tables because a single classifier prevents label/icon drift. It is preferred over placing the classifier in `explorer-remote` because UI/model presentation must not depend on provider implementations.

### Ordered filename grammar

Classification applies the approved order: single-component dotfile, longest known compound extension, ordinary final extension, then extensionless fallback. Extension matching is ASCII case-insensitive; display extensions use Unicode upper casing. Dotfile words are separated on `_` and `-`, empty pieces are ignored, and each non-empty word receives an upper-case first character plus unchanged remainder.

This deterministic grammar is preferred over `Path::extension` alone because it cannot represent dotfile semantics or compound archive labels.

### Provider metadata remains authoritative

Directories always remain `Remote folder`. `File` uses the classifier label; `FileSymlink` appends ` link`; `DirectorySymlink`, `BrokenSymlink`, and `CircularSymlink` retain their existing exact labels. Filenames never change `is_container`, commands, or navigation.

### Built-in categorical fallback icons

`explorer-ui::icons` will render PDF, text, settings, image, archive, audio, video, code, executable/binary, office document, and generic file categories with distinct embedded vector geometry. The glyph itself—not a shared page outline, bottom color strip, or hidden text badge—must remain distinct at 16–20 logical pixels. Larger views may add labels only as secondary reinforcement. Rendering uses the current theme palette, scales from the requested file-view icon size, and occurs only when an ADB/SFTP row lacks a texture. Folder fallback retains precedence for containers.

Built-in categories are preferred over Windows associations because the result stays stable across installations and compound archive names remain classifiable.

### Pinned official Fluent assets and expanded families

The corrective hand-drawn SVGs are superseded by selected official SVGs from `@fluentui/svg-icons@1.1.339`. The repository vendors only selected files and an adjacent provenance/license notice; it does not add npm or network access at build or runtime. Official 20px Color variants retain their upstream fills and gradients and are not tinted by GPUI. When the pinned package has no semantically exact color variant, the exact official 20px regular/filled glyph is used with a stable family tint. Common Office families receive separate official glyphs rather than sharing one `Office` category.

The model owns ordered, centralized compound and final-extension tables. Coverage includes common Office, Windows, Linux, Android, developer, document, archive, media, font, certificate/key, disk-image, database, and web/data families. A read-only scan of `adb://emulator-5554/` supplies representative Android/Linux cases but never becomes a runtime dependency. Unknown names remain generic.

This B-level correction reopens only the icon taxonomy/assets/tests and dependent integration evidence. Earlier metadata and remote-kind semantics remain valid unless their source hashes change.

### GPUI-compatible official Filled subset

Screenshot evidence shows that GPUI renders the Fluent Color variants transparently while the Filled PDF and Archive glyphs remain visible. The Color-asset decision is therefore superseded within the approved official-Fluent scope. Every category uses the corresponding official 20px Filled SVG from the same pinned package. The asset loader injects `currentColor`, and the renderer applies the stable category tint. The compatibility subset prohibits gradients, paint-server URLs, external references, scripts, embedded images, filters, masks, and `foreignObject`.

This B-level correction reopens official asset selection, paint handling, asset visibility tests, and dependent integration evidence. It does not reopen classification or Type-label behavior.

### Validation and evidence

Blocking gates are:

- **G-CLASSIFIER:** Model tests prove ordered classification, edge cases, case folding, and all icon families.
- **G-REMOTE-METADATA:** App tests prove both remote row-conversion paths and every remote entry kind preserve authoritative semantics.
- **G-REMOTE-ICON:** UI tests prove remote-only category selection, container precedence, representative scaling, and unchanged local fallback selection.
- **G-INTEGRATION:** formatting, focused crate tests/checks, strict OpenSpec validation, and final diff/spec review pass.
- **G-FLUENT-ASSETS:** every selected upstream asset is pinned, present, parseable, provenance-recorded, and mapped without unintended recoloring.
- **G-EXTENSION-MATRIX:** every declared compound/final extension and representative ADB filename has a table-driven expected Type/icon result with case and near-miss coverage.
- **G-INTEGRATION-V3:** formatting, focused tests/checks, strict OpenSpec validation, stale-evidence replacement, and final diff/spec review pass after the official-asset expansion.
- **G-VISIBLE-FLUENT:** all 24 official Filled payloads contain visible geometry, load as `currentColor`, contain no unsupported paint features, and remain distinct.
- **G-INTEGRATION-V4:** focused tests/checks, strict OpenSpec validation, screenshot regression audit, and evidence reconciliation pass after the visibility correction.

Evidence is stored under `openspec/changes/classify-remote-file-types-and-icons/evidence/`, with one JSONL index record per atomic task. Records include task ID, command or review procedure, expected/actual result, exit status or reviewer, artifact path/hash, gates, and timestamp.

### Adjustment policy

- **A — task refinement:** leaf splitting/order, exact test filters, or evidence command refinement may change without changing requirements, gates, thresholds, or public behavior.
- **B — design/spec correction:** an implementation-discovered correction within approved scope pauses affected work; design/spec/tasks are updated and revalidated, completed dependent evidence is marked stale, and the correction lineage is recorded.
- **C — material change:** scope, public behavior, platform, permissions, external writes, dependencies, or weakening a gate requires user approval. The user's instruction to make routine decisions does not authorize expanding these boundaries.

## Risks / Trade-offs

- **[Risk] A name-derived label can disagree with file content.** → Keep the feature explicitly presentational and never use classification for opening, navigation, security, or transfer decisions.
- **[Risk] A large extension map becomes inconsistent.** → Centralize it in one exhaustive classifier and use table-driven tests for every family.
- **[Risk] New fallback rendering affects local items.** → Gate selection on ADB/SFTP virtual locations and add local-preservation tests.
- **[Risk] Category marks collapse into the same silhouette in Details view.** → Use separate vector paths and test unique geometry identifiers plus 16px/20px bounds; prohibit color-only differentiation.
- **[Risk] Compound matching produces unexpected labels.** → Match only an explicit longest-first compound list and test near misses such as `bin.gz` (GZ label, archive icon) versus `tar.gz` (TAR.GZ label, archive icon).
- **[Trade-off] Built-in icons do not reflect installed applications.** → Accept this for deterministic ADB/SFTP presentation; local files retain Windows Shell associations.
- **[Risk] Upstream color coverage is incomplete for exact Office/file-format glyphs.** → Prefer the exact official monochrome Fluent glyph with a stable tint over inventing a shape; record color-versus-monochrome selection in the manifest.
- **[Risk] Vendored assets lose provenance or silently drift.** → Pin package/version and SHA-256 hashes in an adjacent manifest/notice and test every mapped payload.
- **[Risk] An SVG loads but renders transparent because GPUI ignores its paint server.** → Restrict remote assets to official Filled SVG primitives and block gradients/paint URLs in tests.

## Migration Plan

1. Add and test the model classifier without changing call sites.
2. Route both remote metadata constructors through the classifier and update remote-service tests.
3. Add the built-in icon renderer and select it only for ADB/SFTP fallbacks.
4. Run all blocking gates and perform the final traceability/diff review.
5. Replace corrective glyphs with pinned official Fluent SVGs, expand the extension taxonomy, and rerun G-FLUENT-ASSETS, G-EXTENSION-MATRIX, and G-INTEGRATION-V3.

No stored data migration or feature flag is required. Reverting the classifier call sites and icon fallback branch restores the previous labels and generic icon.

## Open Questions

None. Unknown extensions deliberately receive an `EXT File` label and the generic icon. The approved official-asset expansion defines the current broad family matrix without content sniffing.
