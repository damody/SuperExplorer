## Context

ADB and SFTP providers already return an authoritative `RemoteEntryKind` and filename. `explorer-app::remote_service` currently maps the kind directly to a static Type label, while `explorer-ui` deliberately excludes remote virtual locations from Windows Shell and thumbnail loading and therefore renders one generic document fallback. The implementation must preserve that I/O boundary, avoid machine-dependent Windows associations, and work at every file-view icon size.

The approved source design is `docs/superpowers/specs/2026-08-30-remote-file-types-and-icons-design.md`.

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

`explorer-model` will expose a filename-classification value containing a stable `RemoteFileIconKind` category and methods that produce the base Type label. It accepts arbitrary UTF-8, performs no I/O, and is independent of ADB/SFTP types. `explorer-app` combines it with `RemoteEntryKind`; `explorer-ui` uses its icon category only for ADB/SFTP virtual files.

This is preferred over independent app/UI tables because a single classifier prevents label/icon drift. It is preferred over placing the classifier in `explorer-remote` because UI/model presentation must not depend on provider implementations.

### Ordered filename grammar

Classification applies the approved order: single-component dotfile, longest known compound extension, ordinary final extension, then extensionless fallback. Extension matching is ASCII case-insensitive; display extensions use Unicode upper casing. Dotfile words are separated on `_` and `-`, empty pieces are ignored, and each non-empty word receives an upper-case first character plus unchanged remainder.

This deterministic grammar is preferred over `Path::extension` alone because it cannot represent dotfile semantics or compound archive labels.

### Provider metadata remains authoritative

Directories always remain `Remote folder`. `File` uses the classifier label; `FileSymlink` appends ` link`; `DirectorySymlink`, `BrokenSymlink`, and `CircularSymlink` retain their existing exact labels. Filenames never change `is_container`, commands, or navigation.

### Built-in categorical fallback icons

`explorer-ui::icons` will add a small `RemoteFileIcon` renderer for PDF, text/settings, image, archive, audio, video, code, executable/binary, office document, and generic file categories. It will use GPUI primitives and the current theme palette, scale from the requested file-view icon size, and render only when an ADB/SFTP row lacks a texture. Folder fallback retains precedence for containers.

Built-in categories are preferred over Windows associations because the result stays stable across installations and compound archive names remain classifiable. They are preferred over adding external bitmap/SVG assets because existing GPUI primitives can provide scalable, theme-compatible, dependency-free glyphs.

### Validation and evidence

Blocking gates are:

- **G-CLASSIFIER:** Model tests prove ordered classification, edge cases, case folding, and all icon families.
- **G-REMOTE-METADATA:** App tests prove both remote row-conversion paths and every remote entry kind preserve authoritative semantics.
- **G-REMOTE-ICON:** UI tests prove remote-only category selection, container precedence, representative scaling, and unchanged local fallback selection.
- **G-INTEGRATION:** formatting, focused crate tests/checks, strict OpenSpec validation, and final diff/spec review pass.

Evidence is stored under `openspec/changes/classify-remote-file-types-and-icons/evidence/`, with one JSONL index record per atomic task. Records include task ID, command or review procedure, expected/actual result, exit status or reviewer, artifact path/hash, gates, and timestamp.

### Adjustment policy

- **A — task refinement:** leaf splitting/order, exact test filters, or evidence command refinement may change without changing requirements, gates, thresholds, or public behavior.
- **B — design/spec correction:** an implementation-discovered correction within approved scope pauses affected work; design/spec/tasks are updated and revalidated, completed dependent evidence is marked stale, and the correction lineage is recorded.
- **C — material change:** scope, public behavior, platform, permissions, external writes, dependencies, or weakening a gate requires user approval. The user's instruction to make routine decisions does not authorize expanding these boundaries.

## Risks / Trade-offs

- **[Risk] A name-derived label can disagree with file content.** → Keep the feature explicitly presentational and never use classification for opening, navigation, security, or transfer decisions.
- **[Risk] A large extension map becomes inconsistent.** → Centralize it in one exhaustive classifier and use table-driven tests for every family.
- **[Risk] New fallback rendering affects local items.** → Gate selection on ADB/SFTP virtual locations and add local-preservation tests.
- **[Risk] Compound matching produces unexpected labels.** → Match only an explicit longest-first compound list and test near misses such as `bin.gz` (GZ label, archive icon) versus `tar.gz` (TAR.GZ label, archive icon).
- **[Trade-off] Built-in icons do not reflect installed applications.** → Accept this for deterministic ADB/SFTP presentation; local files retain Windows Shell associations.

## Migration Plan

1. Add and test the model classifier without changing call sites.
2. Route both remote metadata constructors through the classifier and update remote-service tests.
3. Add the built-in icon renderer and select it only for ADB/SFTP fallbacks.
4. Run all blocking gates and perform the final traceability/diff review.

No stored data migration or feature flag is required. Reverting the classifier call sites and icon fallback branch restores the previous labels and generic icon.

## Open Questions

None. Unknown extensions deliberately receive an `EXT File` label and the generic icon; additional icon families can be added later without changing the filename grammar.
