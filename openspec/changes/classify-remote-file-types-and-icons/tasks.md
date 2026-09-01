## 1. Shared classification foundation

### 1.1 Filename grammar and categories

**目的：** Deliver one total classifier for Type text and icon categories.
**輸入：** Approved design and `remote-file-presentation` requirements.
**產出：** `explorer-model` classifier API and model tests.
**依賴：** None.
**Owner／Wave：** Primary agent / Wave 1.
**Gate／Evidence：** G-CLASSIFIER; `evidence/index.jsonl` and `evidence/artifacts/1.1/`.
**完成門檻：** Ordered grammar, all category families, and boundaries pass focused model tests.

- [x] 1.1.1 Add the public pure filename classification types, ordered dotfile/compound/final-extension grammar, Type-label formatter, and complete initial icon-family map in `explorer-model`.
- [x] 1.1.2 Add table-driven model tests for case folding, dotfile word formatting, compound near misses, extensionless boundaries, Unicode safety, and every icon family.
- [x] 1.1.3 Run the focused `explorer-model` tests and record command, exit status, output hash, and G-CLASSIFIER result.

## 2. Remote metadata integration

### 2.1 ADB/SFTP row labels

**目的：** Route both remote row constructors through the classifier without changing kind semantics.
**輸入：** Completed 1.1 classifier and current `RemoteEntryKind` contracts.
**產出：** Updated `explorer-app` conversion logic and remote-service tests.
**依賴：** 1.1.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G-REMOTE-METADATA; `evidence/index.jsonl` and `evidence/artifacts/2.1/`.
**完成門檻：** ADB/SFTP file labels classify correctly and every directory/link kind preserves its prior navigation semantics.

- [x] 2.1.1 Replace static regular-file labels in both remote row-conversion paths with classifier-derived labels and append file-link semantics only for `FileSymlink`.
- [x] 2.1.2 Expand remote-service tests for ordinary, compound, dotfile, extensionless, directory, file-link, directory-link, broken-link, and circular-link rows in both conversion paths.
- [x] 2.1.3 Run the focused `explorer-app` remote-service tests and record command, exit status, output hash, and G-REMOTE-METADATA result.

## 3. Remote icon presentation

### 3.1 Scalable built-in category renderer

**目的：** Render stable theme-compatible icons for classified remote files.
**輸入：** Completed 1.1 classifier, current UI token palette, and existing file-row fallback host.
**產出：** Category icon renderer plus geometry/category unit tests.
**依賴：** 1.1.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G-REMOTE-ICON; `evidence/index.jsonl` and `evidence/artifacts/3.1/`.
**完成門檻：** Every category has distinct deterministic rendering metadata and fits representative supported icon sizes.

- [x] 3.1.1 Add the scalable PDF, text/settings, image, archive, audio, video, code, executable/binary, office, and generic built-in file icon renderer using existing GPUI/theme primitives.
- [x] 3.1.2 Add UI unit tests for category-to-visual mapping, stable accessible labels, and representative small/large geometry.

### 3.2 Remote-only fallback selection

**目的：** Select classified icons only for ADB/SFTP files while preserving folder and local paths.
**輸入：** Completed 3.1 renderer and existing texture/fallback selection logic.
**產出：** Updated file-row fallback selection and regression tests.
**依賴：** 2.1 and 3.1.
**Owner／Wave：** Primary agent / Wave 3.
**Gate／Evidence：** G-REMOTE-ICON; `evidence/index.jsonl` and `evidence/artifacts/3.2/`.
**完成門檻：** Remote categories render after texture miss, containers keep folder precedence, and local fallback/Shell selection is unchanged.

- [x] 3.2.1 Integrate category fallback into file-row rendering behind the existing ADB/SFTP virtual-location predicate with container precedence.
- [x] 3.2.2 Add regression tests for required pdf/txt/jpg/tar.gz/bin.gz/tgz icons, file symlinks, unknown fallback, remote folders, and unchanged local selection.
- [x] 3.2.3 Run focused `explorer-ui` icon/fallback tests and record command, exit status, output hash, and the combined G-REMOTE-ICON result.

## 4. Integration and final review

### 4.1 Repository quality gates

**目的：** Prove the integrated change builds cleanly and satisfies its normative requirements.
**輸入：** Completed Waves 1–3 and all focused evidence.
**產出：** Formatting, test, check, and OpenSpec validation evidence.
**依賴：** 1.1, 2.1, 3.1, and 3.2.
**Owner／Wave：** Primary agent / Wave 4.
**Gate／Evidence：** G-INTEGRATION; `evidence/index.jsonl` and `evidence/artifacts/4.1/`.
**完成門檻：** Every command exits successfully and evidence contains no stale or missing leaf result.

- [x] 4.1.1 Run `cargo fmt --all -- --check` after formatting and record the result.
- [x] 4.1.2 Run the relevant `explorer-model`, `explorer-app`, and `explorer-ui` test suites and record independent exit results.
- [x] 4.1.3 Run workspace-relevant compile checks for the changed crates and record independent exit results.
- [x] 4.1.4 Run `openspec validate classify-remote-file-types-and-icons --strict`, task-structure validation, and incomplete-marker scans; record the results.

### 4.2 Traceability and diff audit

**目的：** Confirm no requested case, special kind, or unrelated behavior was omitted or changed.
**輸入：** All implementation and validation evidence.
**產出：** Final review report and complete evidence index.
**依賴：** 4.1.
**Owner／Wave：** Primary agent / Wave 5.
**Gate／Evidence：** G-INTEGRATION; `evidence/index.jsonl` and `evidence/artifacts/4.2/`.
**完成門檻：** Proposal → design → requirements/scenarios → code/tests/tasks traceability is complete, diff is scoped, and all leaf records are passed or evidence-backed not-applicable.

- [x] 4.2.1 Audit the final diff for user-owned-change preservation, remote-only scope, no duplicate classifier tables, no hidden I/O, and no missing requested extension families.
- [x] 4.2.2 Write the final traceability review with file hashes and reconcile every task ID in `evidence/index.jsonl` before marking the change complete.

## 5. Recognizable icon correction

### 5.1 Small-size vector identities

**目的：** Replace the color-strip tiles with category-specific geometry that remains recognizable in Details view.
**輸入：** Corrective approved design and existing shared classifier/icon fallback path.
**產出：** Dedicated Settings classification, embedded vector glyphs, renderer, and focused tests.
**依賴：** 1.1, 3.1, and 3.2.
**Owner／Wave：** Primary agent / Corrective Wave 1.
**Gate／Evidence：** G-REMOTE-ICON-V2; `evidence/index.jsonl` and `evidence/artifacts/5.1/`.
**完成門檻：** Eleven categories have unique visible geometry at 16/20px; dotfiles use Settings; prior required mappings and remote/local boundaries pass.

- [x] 5.1.1 Add `Settings` to the shared icon classification and route valid single-component dotfiles to it without changing Type labels.
- [x] 5.1.2 Replace the shared page-and-color-strip renderer with eleven category-specific embedded vector glyphs and stable geometry identities.
- [x] 5.1.3 Add model/UI tests for dotfile settings selection, unique geometry, 16/20px visibility, scaling, required extension mappings, and remote-only precedence.
- [x] 5.1.4 Run focused model/UI tests and changed-crate compile checks; record G-REMOTE-ICON-V2 evidence.

### 5.2 Corrective final audit

**目的：** Revalidate the amended specification and prove prior remote Type behavior remains intact.
**輸入：** Completed 5.1 implementation and historical evidence.
**產出：** Updated strict validation, traceability, screenshot-problem regression review, and reconciled evidence.
**依賴：** 5.1.
**Owner／Wave：** Primary agent / Corrective Wave 2.
**Gate／Evidence：** G-INTEGRATION-V2; `evidence/index.jsonl` and `evidence/artifacts/5.2/`.
**完成門檻：** Formatting, strict OpenSpec, task structure, evidence hashes, and the screenshot-derived regression audit pass with 23/23 tasks resolved.

- [x] 5.2.1 Run formatting, strict OpenSpec validation, task-structure validation, and scoped diff checks; record G-INTEGRATION-V2 evidence.
- [x] 5.2.2 Audit the final 16–20px renderer against the reported color-strip failure, update traceability/hashes, and reconcile all 23 task records.

## 6. Official Fluent asset and taxonomy expansion

### 6.1 Upstream asset inventory and vendoring

**目的：** Replace locally drawn category art with auditable official Fluent SVG assets.
**輸入：** Approved 2026-08-31 design, pinned `@fluentui/svg-icons@1.1.339` package, and current embedded asset registry.
**產出：** Selected SVG files, provenance/license manifest, asset registry mappings, and asset tests.
**依賴：** 5.1 and user approval of the official-asset expansion.
**Owner／Wave：** Primary agent / Official Asset Wave 1.
**Gate／Evidence：** G-FLUENT-ASSETS; `evidence/index.jsonl` and `evidence/artifacts/6.1/`.
**完成門檻：** Every mapped family resolves to a pinned official payload, provenance/hashes are recorded, color assets preserve upstream paint, and no runtime dependency is added.

- [x] 6.1.1 Inventory the pinned package and select exact official 20px Color or exact regular/filled fallbacks for every approved family.
- [x] 6.1.2 Vendor only the selected SVGs plus version, upstream path, license, and SHA-256 provenance records into the Explorer UI asset tree.
- [x] 6.1.3 Replace hand-drawn remote asset registration/rendering with vendored Fluent payloads while preserving color fills and tinting only documented monochrome fallbacks.
- [x] 6.1.4 Add asset tests for complete mappings, distinct family payloads, SVG parseability, color preservation, hashes, and offline lookup.
- [x] 6.1.5 Run focused asset/renderer tests and record G-FLUENT-ASSETS evidence.

### 6.2 Broad extension and Office taxonomy

**目的：** Make common desktop and observed ADB filenames visually classifiable through one auditable model table.
**輸入：** Approved extension families, read-only `emulator-5554` filename sample, and completed 6.1 asset identities.
**產出：** Expanded `RemoteFileIconKind`, centralized extension tables, and exhaustive classifier matrix tests.
**依賴：** 6.1.
**Owner／Wave：** Primary agent / Official Asset Wave 2.
**Gate／Evidence：** G-EXTENSION-MATRIX; `evidence/index.jsonl` and `evidence/artifacts/6.2/`.
**完成門檻：** Every declared extension and compound entry has a passing expected Type/icon assertion; Office families are distinct; required ADB cases, upper-case variants, and near misses pass.

- [x] 6.2.1 Extract and summarize extension frequencies from a read-only `adb -s emulator-5554` scan without copying remote content.
- [x] 6.2.2 Expand icon kinds and longest-first compound/final-extension tables across every approved family, including distinct Word, spreadsheet, presentation, notebook, database, and mail mappings.
- [x] 6.2.3 Add exhaustive table-driven tests that enumerate every declared mapping plus uppercase, compound-boundary, unknown, dotfile, Unicode, and representative ADB cases.
- [x] 6.2.4 Run focused model, app metadata, and UI selection tests and record G-EXTENSION-MATRIX evidence.

### 6.3 Expansion integration and final audit

**目的：** Prove the official assets and expanded taxonomy integrate without regressing remote/local boundaries or user-owned changes.
**輸入：** Completed 6.1 and 6.2 implementation and all historical evidence lineage.
**產出：** Compile/format/spec results, stale-evidence replacements, final traceability report, and reconciled evidence index.
**依賴：** 6.1 and 6.2.
**Owner／Wave：** Primary agent / Official Asset Wave 3.
**Gate／Evidence：** G-INTEGRATION-V3; `evidence/index.jsonl` and `evidence/artifacts/6.3/`.
**完成門檻：** Changed crates compile; focused tests, formatting, strict OpenSpec, task structure, diff audit, and every evidence hash pass with 36/36 leaves resolved.

- [x] 6.3.1 Run formatting and changed-crate compile checks; record independent results.
- [x] 6.3.2 Run strict OpenSpec validation, detailed task validation, placeholder scans, and scoped `git diff --check`.
- [x] 6.3.3 Audit classifier-to-asset traceability, remote-only behavior, license/provenance, unsupported SVG constructs, and preservation of unrelated dirty-worktree changes.
- [x] 6.3.4 Reconcile all 36 unique task records and hashes, superseding stale source-hash evidence without deleting historical lineage.

## 7. GPUI visibility correction

### 7.1 Official Filled asset compatibility

**目的：** Eliminate transparent remote icons by using only GPUI-compatible official Fluent Filled SVGs.
**輸入：** Screenshot failure evidence, approved visibility-fix design, pinned package, and current 24-family map.
**產出：** Replaced SVG payloads, updated provenance/hashes, uniform tint path, and compatibility tests.
**依賴：** 6.1 and 6.2.
**Owner／Wave：** Primary agent / Visibility Wave 1.
**Gate／Evidence：** G-VISIBLE-FLUENT; `evidence/index.jsonl` and `evidence/artifacts/7.1/`.
**完成門檻：** All 24 assets are official Filled SVGs with visible path geometry and `currentColor`; unsupported paint features are absent; focused tests pass.

- [x] 7.1.1 Replace all Color payloads with their corresponding pinned official 20px Filled SVGs and update upstream paths/hashes.
- [x] 7.1.2 Simplify asset loading and icon specs so all 24 official Filled glyphs use the same `currentColor` tint path.
- [x] 7.1.3 Add compatibility tests that reject empty geometry, gradients, paint URLs, external/active content, filters, masks, and non-tintable payloads.
- [x] 7.1.4 Run focused asset, icon, remote selection, model, and app tests; record G-VISIBLE-FLUENT evidence.

### 7.2 Visibility integration audit

**目的：** Prove the screenshot regression is closed without changing classification or local behavior.
**輸入：** Completed 7.1 assets/tests and historical evidence lineage.
**產出：** Compile/format/spec results, screenshot regression review, and reconciled evidence.
**依賴：** 7.1.
**Owner／Wave：** Primary agent / Visibility Wave 2.
**Gate／Evidence：** G-INTEGRATION-V4; `evidence/index.jsonl` and `evidence/artifacts/7.2/`.
**完成門檻：** Formatting, compile, strict OpenSpec, task structure, diff audit, and all 43 evidence records pass; changed-source history names replacements.

- [x] 7.2.1 Run formatting, changed-crate compilation, strict OpenSpec validation, detailed task validation, and `git diff --check`.
- [x] 7.2.2 Audit all 24 loaded payloads against the screenshot failure and confirm remote-only/local-preservation boundaries.
- [x] 7.2.3 Reconcile all 43 unique task records and hashes while retaining superseded historical evidence.
