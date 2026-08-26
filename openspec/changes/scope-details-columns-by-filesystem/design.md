## Context

SuperExplorer persists one ordered Details-column layout and currently projects it without considering whether the active location is Local, ADB, or SFTP. Extension descriptors express row applicability and cost but not filesystem scope, so local-only columns appear remotely and may start work that cannot succeed. Remote providers also discard or never acquire Unix mode bits.

The approved source design is `docs/superpowers/specs/2026-08-26-remote-column-filesystem-applicability-and-unix-permissions-design.md`. This change crosses the extension manifest/protocol, model, ADB/SFTP providers, registry/state, and Details presentation, so it uses a detailed implementation plan. Per user direction, implementation changes are completed first and focused tests, compilation, artifact validation, and final review run together in the final verification phase rather than after each work package.

## Goals / Non-Goals

**Goals:**

- Make extension authors declare an immutable, fail-closed filesystem scope for each data column.
- Derive one effective column projection used by every Details consumer without mutating persisted layout.
- Prevent inapplicable extension data requests before payload preparation or dispatch.
- Transport ADB/SFTP Unix modes and render a built-in symbolic-plus-octal Permissions column.
- Preserve bounded diagnostics and missing-data behavior.

**Non-Goals:**

- User-editable filesystem scope.
- Filesystem identities beyond `local`, `adb`, and `sftp`.
- POSIX ownership, ACL, SELinux context, permission editing, or chmod.
- Enabling bundled content-analysis or Windows process columns remotely.
- A complete regression or installer/UI automation run.

## Decisions

### Filesystem identity is a closed model value

Add a shared enum/value for Local, ADB, and SFTP. Resolve Local from path-backed filesystem locations and remote identities from the virtual provider ID. Unknown virtual providers resolve to no supported filesystem identity. A closed type avoids string comparisons across UI code and prevents future providers from inheriting permissions accidentally.

Alternative rejected: provider-name blacklists in the UI. They duplicate policy across headers, rows, menus, and request coordinators and fail open for new columns.

### Manifest scope is author-owned and fail-closed

Data-column feature declarations gain `file_systems`, parsed as a set of the three allowed names. Missing and empty arrays produce an empty set. Duplicate known values normalize to one value. Any unknown value rejects the affected contribution during validation with one actionable diagnostic. The validated set is carried in extension protocol descriptors and cannot be changed by user settings.

Bundled Folder size, Main code lines, Code lines, and Lock owner manifests explicitly declare `local`. Existing third-party manifests remain structurally loadable, but columns without a declaration are inactive everywhere until updated. This is an intentional compatibility restriction and safer than silently granting access to remote entries.

### One effective projection governs all Details behavior

The registry/layout layer computes visible descriptors by intersecting persisted visibility with active-filesystem applicability. Built-in policy permits Name, Date modified, Type, and Size on ADB/SFTP when metadata exists; Permissions is remote-only; Windows Shell, MFT, content-analysis, and local-process columns are local-only.

Header, rows, column chooser/filter menus, drag targets, auto-size, sorting availability, and request admission consume this projection. The persisted order, width, visibility, and sort descriptor are never rewritten. When the saved sort column is absent from the projection, the effective sort becomes Name ascending until a compatible location returns.

Alternative rejected: mutate column visibility on navigation. That loses user intent and makes returning to Local irreversibly change the layout.

### Admission occurs before data preparation

The Host checks the descriptor's validated filesystem set before scheduling, reading local files, preparing streams, or invoking native/Lua providers. This makes hidden columns operationally inactive, not merely visually hidden. Stale results remain governed by existing generation checks.

### Unix mode is optional shared entry metadata

`FileEntryMetadata` carries an optional mode value wide enough for POSIX file-type, special, and permission bits. SFTP maps the server attribute value. ADB adds mode acquisition to its bounded directory metadata command/parser rather than issuing one command per row. Malformed or absent values become `None` and do not fail listing.

The built-in Permissions descriptor is applicable to ADB/SFTP. Formatting recognizes standard Unix file types and special-bit display (`s`, `S`, `t`, `T`), emits four permission octal digits, and uses `?` for unknown type bits. Missing mode renders an em dash. Numeric sorting places known modes before missing modes.

## Component and Data Flow

1. Manifest validation normalizes filesystem names into the extension descriptor contract.
2. Location classification supplies the active filesystem identity.
3. Registry/state computes the effective descriptor projection and effective sort.
4. Request coordinators admit only applicable extension descriptors.
5. ADB/SFTP listings map optional Unix mode into shared metadata.
6. Details header, cells, menus, sizing, drag, filter, and sort use the same projection and formatter.

## Failure Handling and Observability

- Unknown manifest values reject the contribution with one bounded package/feature/column diagnostic.
- Missing/empty scope is inactive without repeated warnings.
- Missing/malformed remote mode renders an em dash without per-row log spam.
- Existing directory-listing failures remain the only listing-level error path.
- Unknown virtual providers admit no scoped extension columns and no Permissions column.

## Security and Performance

Fail-closed applicability prevents undeclared providers from receiving entry identities or file inputs. ADB mode collection must remain one bounded directory operation; per-entry subprocesses are prohibited. Projection work is linear in the small column registry and reuses existing render/layout passes. No credentials or secret material enter descriptors, metadata, diagnostics, or evidence.

## Migration and Rollback

No persisted layout schema rewrite is required: applicability is an effective projection layered over existing settings. Add the new built-in column using the existing stable-ID migration path while leaving old column records intact. On rollback, older binaries ignore the new optional protocol/model field according to the repository's compatibility conventions; updated bundled manifests may need to retain fields tolerated by the older parser. If that assumption is disproven during implementation, treat it as a B-level correction and update design/spec/tasks before proceeding.

## Testing and Evidence Strategy

Implementation phases make code and fixture changes without running repeated full checks. The final phase runs the focused manifest/protocol/model/provider/UI tests, relevant crate compilation, `openspec validate --strict`, placeholder/traceability scans, and diff review once the implementation is integrated. Evidence records live under `openspec/changes/scope-details-columns-by-filesystem/evidence/` and identify task/subcheck, command or review procedure, expected and actual results, exit status, hashes where applicable, and timestamp.

## Adaptive Planning Rules

- **A — task refinement:** commands, ordering, leaf split, or file ownership may change without changing scope or contracts; record the refinement in evidence.
- **B — design/spec correction:** an implementation discovery within approved scope pauses affected work, updates design/spec/tasks, marks dependent evidence stale, and reruns validation.
- **C — material change:** new filesystem identities, user-editable scope, permission mutation, weaker fail-closed behavior, reduced final gates, new external writes, or destructive actions require user approval.

Blocking gates and required evidence cannot be weakened silently.

## Open Questions

None. The author-owned scope, fail-closed default, supported identities, remote permission format, and end-loaded verification sequence are approved.
