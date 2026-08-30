## 1. Shared classification foundation

### 1.1 Filename grammar and categories

**目的：** Deliver one total classifier for Type text and icon categories.
**輸入：** Approved design and `remote-file-presentation` requirements.
**產出：** `explorer-model` classifier API and model tests.
**依賴：** None.
**Owner／Wave：** Primary agent / Wave 1.
**Gate／Evidence：** G-CLASSIFIER; `evidence/index.jsonl` and `evidence/artifacts/1.1/`.
**完成門檻：** Ordered grammar, all category families, and boundaries pass focused model tests.

- [ ] 1.1.1 Add the public pure filename classification types, ordered dotfile/compound/final-extension grammar, Type-label formatter, and complete initial icon-family map in `explorer-model`.
- [ ] 1.1.2 Add table-driven model tests for case folding, dotfile word formatting, compound near misses, extensionless boundaries, Unicode safety, and every icon family.
- [ ] 1.1.3 Run the focused `explorer-model` tests and record command, exit status, output hash, and G-CLASSIFIER result.

## 2. Remote metadata integration

### 2.1 ADB/SFTP row labels

**目的：** Route both remote row constructors through the classifier without changing kind semantics.
**輸入：** Completed 1.1 classifier and current `RemoteEntryKind` contracts.
**產出：** Updated `explorer-app` conversion logic and remote-service tests.
**依賴：** 1.1.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G-REMOTE-METADATA; `evidence/index.jsonl` and `evidence/artifacts/2.1/`.
**完成門檻：** ADB/SFTP file labels classify correctly and every directory/link kind preserves its prior navigation semantics.

- [ ] 2.1.1 Replace static regular-file labels in both remote row-conversion paths with classifier-derived labels and append file-link semantics only for `FileSymlink`.
- [ ] 2.1.2 Expand remote-service tests for ordinary, compound, dotfile, extensionless, directory, file-link, directory-link, broken-link, and circular-link rows in both conversion paths.
- [ ] 2.1.3 Run the focused `explorer-app` remote-service tests and record command, exit status, output hash, and G-REMOTE-METADATA result.

## 3. Remote icon presentation

### 3.1 Scalable built-in category renderer

**目的：** Render stable theme-compatible icons for classified remote files.
**輸入：** Completed 1.1 classifier, current UI token palette, and existing file-row fallback host.
**產出：** Category icon renderer plus geometry/category unit tests.
**依賴：** 1.1.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G-REMOTE-ICON; `evidence/index.jsonl` and `evidence/artifacts/3.1/`.
**完成門檻：** Every category has distinct deterministic rendering metadata and fits representative supported icon sizes.

- [ ] 3.1.1 Add the scalable PDF, text/settings, image, archive, audio, video, code, executable/binary, office, and generic built-in file icon renderer using existing GPUI/theme primitives.
- [ ] 3.1.2 Add UI unit tests for category-to-visual mapping, stable accessible labels, and representative small/large geometry.

### 3.2 Remote-only fallback selection

**目的：** Select classified icons only for ADB/SFTP files while preserving folder and local paths.
**輸入：** Completed 3.1 renderer and existing texture/fallback selection logic.
**產出：** Updated file-row fallback selection and regression tests.
**依賴：** 2.1 and 3.1.
**Owner／Wave：** Primary agent / Wave 3.
**Gate／Evidence：** G-REMOTE-ICON; `evidence/index.jsonl` and `evidence/artifacts/3.2/`.
**完成門檻：** Remote categories render after texture miss, containers keep folder precedence, and local fallback/Shell selection is unchanged.

- [ ] 3.2.1 Integrate category fallback into file-row rendering behind the existing ADB/SFTP virtual-location predicate with container precedence.
- [ ] 3.2.2 Add regression tests for required pdf/txt/jpg/tar.gz/bin.gz/tgz icons, file symlinks, unknown fallback, remote folders, and unchanged local selection.
- [ ] 3.2.3 Run focused `explorer-ui` icon/fallback tests and record command, exit status, output hash, and the combined G-REMOTE-ICON result.

## 4. Integration and final review

### 4.1 Repository quality gates

**目的：** Prove the integrated change builds cleanly and satisfies its normative requirements.
**輸入：** Completed Waves 1–3 and all focused evidence.
**產出：** Formatting, test, check, and OpenSpec validation evidence.
**依賴：** 1.1, 2.1, 3.1, and 3.2.
**Owner／Wave：** Primary agent / Wave 4.
**Gate／Evidence：** G-INTEGRATION; `evidence/index.jsonl` and `evidence/artifacts/4.1/`.
**完成門檻：** Every command exits successfully and evidence contains no stale or missing leaf result.

- [ ] 4.1.1 Run `cargo fmt --all -- --check` after formatting and record the result.
- [ ] 4.1.2 Run the relevant `explorer-model`, `explorer-app`, and `explorer-ui` test suites and record independent exit results.
- [ ] 4.1.3 Run workspace-relevant compile checks for the changed crates and record independent exit results.
- [ ] 4.1.4 Run `openspec validate classify-remote-file-types-and-icons --strict`, task-structure validation, and incomplete-marker scans; record the results.

### 4.2 Traceability and diff audit

**目的：** Confirm no requested case, special kind, or unrelated behavior was omitted or changed.
**輸入：** All implementation and validation evidence.
**產出：** Final review report and complete evidence index.
**依賴：** 4.1.
**Owner／Wave：** Primary agent / Wave 5.
**Gate／Evidence：** G-INTEGRATION; `evidence/index.jsonl` and `evidence/artifacts/4.2/`.
**完成門檻：** Proposal → design → requirements/scenarios → code/tests/tasks traceability is complete, diff is scoped, and all leaf records are passed or evidence-backed not-applicable.

- [ ] 4.2.1 Audit the final diff for user-owned-change preservation, remote-only scope, no duplicate classifier tables, no hidden I/O, and no missing requested extension families.
- [ ] 4.2.2 Write the final traceability review with file hashes and reconcile every task ID in `evidence/index.jsonl` before marking the change complete.
