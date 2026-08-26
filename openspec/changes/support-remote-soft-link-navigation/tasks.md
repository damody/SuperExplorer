## 1. Remote Entry Contract

### 1.1 Provider-neutral classification

**目的：** Establish one exhaustive remote item-kind contract for providers and UI adaptation.
**輸入：** Approved proposal, design, and remote-soft-link-navigation spec.
**產出：** `explorer-remote` public model changes and unit tests.
**依賴：** None.
**Owner／Wave：** Primary／1.
**Gate／Evidence：** SLINK-CONTRACT in `evidence/contract.json`.
**完成門檻：** Every item kind has an asserted container decision and stable Type label, and all
constructors compile against the enum contract.

- [x] 1.1.1 Add the exhaustive `RemoteEntryKind` contract and replace `RemoteEntry.is_directory`.
- [x] 1.1.2 Add unit tests for container and Type-label behavior of every item kind.
- [x] 1.1.3 Update provider and test-fixture constructors to use the typed contract.
- [x] 1.1.4 Record the focused contract-test command and result in `evidence/contract.json`.

## 2. Provider Classification

### 2.1 ADB symbolic-link classification

**目的：** Produce safe, structured ADB directory entries with resolved link state.
**輸入：** 1.1 contract, ADB argument-array runner, validated and safely encoded remote paths.
**產出：** ADB listing parser/probe and fake-runner regression tests.
**依賴：** 1.1.
**Owner／Wave：** Primary／2.
**Gate／Evidence：** SLINK-ADB in `evidence/adb.json`.
**完成門檻：** Fake ADB coverage distinguishes ordinary entries, file/folder links, broken links,
circular links, malformed records, and cancellation without raw path interpolation.

- [x] 2.1.1 Implement the fixed ADB structured listing probe and strict record parser.
- [x] 2.1.2 Map resolved ADB link states into `RemoteEntryKind` while preserving link-side locations.
- [x] 2.1.3 Add fake-runner tests for ordinary, relative/absolute link, broken, circular, hostile-name,
and cancellation cases.
- [x] 2.1.4 Record the focused ADB test command and result in `evidence/adb.json`.

### 2.2 SFTP symbolic-link classification

**目的：** Resolve SFTP link targets in-session with normalized paths and bounded cycle detection.
**輸入：** 1.1 contract and existing SFTP session/listing implementation.
**產出：** Pure path-resolution helpers, SFTP classification integration, and tests.
**依賴：** 1.1.
**Owner／Wave：** Primary／2.
**Gate／Evidence：** SLINK-SFTP in `evidence/sftp.json`.
**完成門檻：** Tests cover relative/absolute normalization, file/folder targets, broken targets,
repeated paths, hop exhaustion, and cancellation.

- [x] 2.2.1 Implement normalized SFTP link-target resolution with visited-path and 40-hop bounds.
- [x] 2.2.2 Integrate link classification into SFTP listing without extra probes for ordinary entries.
- [x] 2.2.3 Add deterministic helper/provider tests for every SFTP terminal link state and cancellation.
- [x] 2.2.4 Record the focused SFTP test command and result in `evidence/sftp.json`.

## 3. Explorer Integration and Verification

### 3.1 Shared UI adapter behavior

**目的：** Make all navigation surfaces consume the same remote link classification and labels.
**輸入：** Completed provider classifications and existing `FileEntry` navigation contract.
**產出：** `explorer-app` adapter mapping and regression tests.
**依賴：** 2.1 and 2.2.
**Owner／Wave：** Primary／3.
**Gate／Evidence：** SLINK-UI in `evidence/ui.json`.
**完成門檻：** Directory links are containers in file rows and child-container queries; broken and
circular links are distinct selectable non-containers; link-side locations are unchanged.

- [x] 3.1.1 Map remote kinds to `FileEntry.is_container` and the six specified Type labels.
- [x] 3.1.2 Use the same container decision for breadcrumb and navigation-pane child filtering.
- [x] 3.1.3 Add adapter/navigation tests for directory-link entry, invalid-link rejection, labels, and
link-side path preservation.
- [x] 3.1.4 Record the focused application/UI test commands and results in `evidence/ui.json`.

### 3.2 Final validation and traceability

**目的：** Demonstrate that implementation satisfies every scenario without regressing the workspace.
**輸入：** 3.1 integrated implementation and all focused evidence.
**產出：** Formatting/check/test logs and final evidence index.
**依賴：** 3.1.
**Owner／Wave：** Primary／4.
**Gate／Evidence：** SLINK-REL in `evidence/final-validation.json` and `evidence/index.json`.
**完成門檻：** Formatting, affected tests, workspace check, and strict OpenSpec validation pass;
every leaf has a unique evidence record or immutable subcheck reference.

- [x] 3.2.1 Run `cargo fmt --all -- --check` and record the result.
- [x] 3.2.2 Run affected-crate tests and record the result.
- [x] 3.2.3 Run `cargo check --workspace --locked` and record the result.
- [x] 3.2.4 Run `openspec validate support-remote-soft-link-navigation --strict` and record the result.
- [x] 3.2.5 Create `evidence/index.json` with task IDs, commands/artifacts, expected and actual results,
exit status, hashes, related gates, adjustment IDs, and timestamps.
- [x] 3.2.6 Review proposal-to-spec-to-task traceability and record no unresolved blocking gaps.
