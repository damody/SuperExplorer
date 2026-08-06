## 1. Contract and provider enforcement

### 1.1 MFT-only service contract

**目的：** `FolderSizeServiceV1` has explicit cache, MFT, and unavailable outcomes with no reachable slow fallback.
**輸入：** Approved design, current folder-size service, MFT aggregate/index implementation.
**產出：** Provider API/implementation and focused tests.
**依賴：** None.
**Owner／Wave：** Primary agent / Wave 1.
**Gate／Evidence：** G1; `target/openspec-evidence/mft-only-folder-size-consumers/evidence-index.jsonl`.
**完成門檻：** * No production folder-size branch invokes Everything or recursive traversal; focused tests pass.

- [x] 1.1.1 Inventory production calls that can enter Everything or recursive folder-size measurement and record the disposition for each call site.
- [x] 1.1.2 Add an explicit MFT-unavailable provider outcome that remains retryable after refresh or service recovery.
- [x] 1.1.3 Remove Everything fallback from the shared folder-size provider decision path.
- [x] 1.1.4 Remove recursive traversal fallback from the shared folder-size provider decision path.
- [x] 1.1.5 Add tests proving cache miss plus MFT failure terminates unavailable without calling either forbidden backend.
- [x] 1.1.6 Add a recovery test proving a later valid service index succeeds after an unavailable result.

### 1.2 Shared Host cache contract

**目的：** All consumers reuse one Host-owned cache identity and invalidation policy.
**輸入：** Existing persistent data-column cache and provider contract from 1.1.
**產出：** Shared admission/query path and cache tests.
**依賴：** 1.1.
**Owner／Wave：** Primary agent / Wave 1.
**Gate／Evidence：** G2; evidence index.
**完成門檻：** * Unchanged multi-consumer requests reuse one entry; timestamp/schema changes miss.

- [x] 1.2.1 Route folder-size cache admission through canonical identity, modification timestamp, and schema for every consumer.
- [x] 1.2.2 Add a test proving built-in Size, Folder size, and Size Map can reuse one admitted value without duplicate provider work.
- [x] 1.2.3 Add a test proving a modification timestamp change invalidates the cached value.
- [x] 1.2.4 Add a test proving a cache schema change rejects prior entries.

## 2. Consumer integration

### 2.1 Built-in Size display and requests

**目的：** Built-in Size independently requests and displays recursive folder bytes while files retain ordinary length.
**輸入：** Shared provider/cache and Shell entry metadata.
**產出：** UI request scheduling, render data, and tests.
**依賴：** 1.2.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G3; evidence index plus UTIT artifacts.
**完成門檻：** * Extension-off folder Size works; file and ZIP behavior remains correct; unavailable folders are blank.

- [x] 2.1.1 Decouple folder-size request scheduling from Folder size extension visibility when built-in Size is visible.
- [x] 2.1.2 Supply optional shared recursive bytes to built-in Size cell rendering.
- [x] 2.1.3 Preserve ordinary file length and exclude ZIP/Shell archive containers from folder requests and Folder size rendering.
- [x] 2.1.4 Render unavailable folder Size as blank rather than zero or `Calculating...`.
- [x] 2.1.5 Add unit tests for folder, file, ZIP, unavailable, and extension-disabled Size presentation.
- [x] 2.1.6 Add or update UTIT coverage for built-in Size with the Folder size extension disabled.

### 2.2 Built-in Size sorting

**目的：** Size sorting uses the same optional byte source displayed by each row.
**輸入：** Shared recursive bytes from 2.1 and existing sort model.
**產出：** Comparator/model integration and tests.
**依賴：** 2.1.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G4; evidence index.
**完成門檻：** * Mixed known file/folder values sort numerically and missing folders preserve missing ordering.

- [x] 2.2.1 Feed known recursive folder bytes into the built-in Size sort value without overwriting file lengths.
- [x] 2.2.2 Preserve existing missing-value ordering without coercing unavailable folder values to zero.
- [x] 2.2.3 Add ascending and descending mixed-row sorting tests including an unavailable folder.

### 2.3 Size Map shared-source integration

**目的：** Size Map consumes the shared MFT-only Host service and cannot start a recursive scanner.
**輸入：** Provider/cache from Phase 1 and current Size Map pipeline.
**產出：** Size Map adapter/refactor and tests.
**依賴：** 1.2.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G5; evidence index.
**完成門檻：** * Shared snapshot reuse passes and MFT failure produces unavailable with no forbidden backend invocation.

- [x] 2.3.1 Route Size Map hierarchy/totals through the shared MFT aggregate/index contract.
- [x] 2.3.2 Remove the reachable Size Map recursive measurement fallback.
- [x] 2.3.3 Add tests for shared snapshot reuse and unavailable MFT behavior.

## 3. Status, service, and integration evidence

### 3.1 Backend status and MFT bounds

**目的：** Backend state is accurate and the privileged MFT implementation remains bounded.
**輸入：** Provider outcomes and installed service configuration.
**產出：** Status implementation, tests, and service evidence.
**依賴：** 1.1, 2.1, 2.3.
**Owner／Wave：** Primary agent / Wave 3.
**Gate／Evidence：** G6; evidence index and service report.
**完成門檻：** * All three status labels pass, worker maximum is eight, service is automatic LocalSystem, and UI never opens raw volume.

- [x] 3.1.1 Add `MFT unavailable` to the backend status contract and render it without an active ellipsis.
- [x] 3.1.2 Add status tests for Host cache, active/complete MFT service, and MFT unavailable.
- [x] 3.1.3 Run the MFT aggregate worker-bound and exact-total tests proving concurrency never exceeds eight.
- [x] 3.1.4 Record installed service State, StartMode, StartName, and binary path.
- [x] 3.1.5 Verify by code inspection/test that the UI process does not open the raw NTFS volume.

### 3.2 Build, UTIT, and installed UI proof

**目的：** The shipping installer and installed application demonstrate the complete behavior.
**輸入：** All implementation packages and current installer pipeline.
**產出：** Test logs, installer, screenshots, UIA reports, hashes, and evidence index.
**依賴：** 2.1, 2.2, 2.3, 3.1.
**Owner／Wave：** Primary agent / Wave 4.
**Gate／Evidence：** G7; `target/openspec-evidence/mft-only-folder-size-consumers/`.
**完成門檻：** * All commands exit zero; installed hash matches release; enabled/disabled screenshots satisfy every observable requirement; no `Calculating...` remains.

- [x] 3.2.1 Run focused Rust unit tests for provider, cache, UI presentation, sorting, Size Map, and status behavior.
- [x] 3.2.2 Run the relevant UTIT suite and store its machine-readable report.
- [x] 3.2.3 Run `cargo fmt --all -- --check`, `git diff --check`, and strict OpenSpec validation.
- [x] 3.2.4 Build `build_test_install.bat --no-launch` and record exit status, NSIS CRC, installer size, and SHA-256.
- [x] 3.2.5 Install the generated package and prove installed/release executable hashes match.
- [x] 3.2.6 Capture D:\ Details-view evidence with Folder size enabled showing folder values, ZIP blank, zero calculating cells, and backend status.
- [x] 3.2.7 Capture D:\ Details-view evidence with Folder size disabled showing built-in Size folder values and ordinary file size.
- [x] 3.2.8 Restart the installed application and capture Host-cache reuse evidence without duplicate slow work.
- [x] 3.2.9 Write one evidence-index record per resolved leaf with task ID, command/artifact, expected/actual result, exit status or reviewer, hashes, gate, timestamp, and any adjustment ID.
- [x] 3.2.10 Perform final proposal-to-design-to-scenario-to-task traceability review and leave no unresolved P0/P1 findings.

