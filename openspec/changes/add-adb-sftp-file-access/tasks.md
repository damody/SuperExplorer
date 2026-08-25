## 1. Contract and security foundation

### 1.1 Remote location contract

**目的：** Define the opaque, validated remote location and operation surface.
**輸入：** Approved proposal, design, and remote-provider-runtime spec.
**產出：** Model contracts and unit-test evidence.
**依賴：** None.
**Owner／Wave：** Primary／1.
**Gate／Evidence：** RPR-1 in `evidence/contract-tests.json`.
**完成門檻：** URI parsing cannot create invalid or secret-bearing descriptors.

- [x] 1.1.1 Add typed ADB/SFTP URI parsing and canonical display helpers without secret fields.
- [x] 1.1.2 Add virtual-location validation and stale-generation tests for remote providers.
- [ ] 1.1.3 Record RPR-1 unit-test evidence.

### 1.2 Secure profile persistence

**目的：** Persist non-secret SFTP profile metadata and isolate passwords.
**輸入：** Windows credential APIs and SFTP spec.
**產出：** Profile store, Credential Manager adapter, redaction tests.
**依賴：** 1.1.
**Owner／Wave：** Primary／1.
**Gate／Evidence：** SEC-1 in `evidence/security-tests.json`.
**完成門檻：** Passwords are absent from configuration, serde, debug, and diagnostics.

- [x] 1.2.1 Implement profile metadata persistence with aliases and pinned host fingerprints.
- [x] 1.2.2 Implement Windows Credential Manager password read/write/delete adapter.
- [ ] 1.2.3 Add redaction and changed-host-key rejection tests and record SEC-1 evidence.

## 2. Remote runtime

### 2.1 Provider execution boundary

**目的：** Add cancellable provider dispatch outside Shell COM.
**輸入：** Model contracts and existing Explorer request/event protocol.
**產出：** `explorer-remote` crate and application composition adapter.
**依賴：** 1.1.
**Owner／Wave：** Primary／2.
**Gate／Evidence：** RPR-2 in `evidence/runtime-tests.json`.
**完成門檻：** Every accepted remote request has bounded batches and one terminal event.

- [x] 2.1.1 Create the platform-neutral remote provider trait and correlated request dispatcher.
- [x] 2.1.2 Route `adb` and `sftp` virtual locations before the Shell STA path boundary.
- [ ] 2.1.3 Add cancellation, deadline, terminal-ledger, and stale-event tests; record RPR-2 evidence.

### 2.2 Cross-provider transfer engine

**目的：** Transfer data safely between all three endpoint types.
**輸入：** Provider stream interfaces and existing conflict decisions.
**產出：** Streamed copy/move implementation and operation outcome tests.
**依賴：** 2.1.
**Owner／Wave：** Primary／3.
**Gate／Evidence：** XFR-1 in `evidence/transfer-tests.json`.
**完成門檻：** Local↔ADB, Local↔SFTP, and ADB↔SFTP transfers preserve partial outcomes.

- [ ] 2.2.1 Implement bounded source/destination streaming with cancellation and cleanup.
- [ ] 2.2.2 Implement cross-provider copy conflict decisions and item-level progress/outcomes.
- [x] 2.2.3 Implement cross-provider move as verified copy followed by source deletion.
- [x] 2.2.4 Add partial-delete and cleanup tests and record XFR-1 evidence.

## 3. Protocol providers

### 3.1 ADB provider

**目的：** Make authorized Android devices and phone paths navigable and mutable.
**輸入：** Provider runtime and installed `adb.exe` contract.
**產出：** ADB discovery, file operations, and test seam.
**依賴：** 2.1.
**Owner／Wave：** Primary／3.
**Gate／Evidence：** ADB-1 in `evidence/adb-tests.json`.
**完成門檻：** Authorized devices browse and transfer files; unauthorized/offline devices do not mutate.

- [x] 3.1.1 Resolve `adb.exe` and implement bounded, cancellable argument-array execution.
- [x] 3.1.2 Implement device discovery and direct `adb://serial/path` directory listing.
- [x] 3.1.3 Implement ADB folder/file mutation and streaming endpoints.
- [ ] 3.1.4 Add fake-ADB unit tests and opt-in device integration test; record ADB-1 evidence.

### 3.2 SFTP provider

**目的：** Make securely configured SFTP profiles navigable and mutable.
**輸入：** Secure profile store, provider runtime, SSH libraries.
**產出：** SSH host-key verification, SFTP provider, integration test seam.
**依賴：** 1.2 and 2.1.
**Owner／Wave：** Primary／3.
**Gate／Evidence：** SFTP-1 in `evidence/sftp-tests.json`.
**完成門檻：** A trusted active profile lists and transfers without exposing its password.

- [x] 3.2.1 Add audited pure-Rust SSH/SFTP dependencies and runtime ownership boundary.
- [ ] 3.2.2 Implement first-trust and changed-key host fingerprint handling.
- [x] 3.2.3 Implement SFTP list, stream, create, rename, and permanent-delete operations.
- [ ] 3.2.4 Add fake-server tests and opt-in supplied-server test via environment secret; record SFTP-1 evidence.

## 4. Explorer UI and integration

### 4.1 Navigation and connection UI

**目的：** Expose remote endpoints without leaking secrets.
**輸入：** Provider runtime and profile APIs.
**產出：** Navigation sections, address-bar parsing, SFTP connection dialog.
**依賴：** 3.1 and 3.2.
**Owner／Wave：** Primary／4.
**Gate／Evidence：** UI-1 in `evidence/ui-tests.json`.
**完成門檻：** Users can enter the routes and distinguish connection failures safely.

- [x] 4.1.1 Add Android Devices and SFTP navigation sections and availability states.
- [x] 4.1.2 Add direct remote address parsing and title/breadcrumb display.
- [ ] 4.1.3 Add Add/Connect SFTP dialog with password masking and Credential Manager submission.
- [ ] 4.1.4 Add UI interaction/redaction tests and record UI-1 evidence.

### 4.2 Remote operation safety

**目的：** Present permanent remote mutations and transfer results truthfully.
**輸入：** Remote operation outcomes and existing file operation UI.
**產出：** Remote confirmation and status integration.
**依賴：** 2.2, 3.1, and 3.2.
**Owner／Wave：** Primary／4.
**Gate／Evidence：** UI-2 in `evidence/ui-tests.json`.
**完成門檻：** Remote deletion cannot be sent without a permanent-delete confirmation.

- [ ] 4.2.1 Add provider-aware permanent-delete confirmation text.
- [ ] 4.2.2 Render remote loading, offline, permission, partial, and completed outcomes.
- [ ] 4.2.3 Add confirmation-cancel and partial-move interaction tests; record UI-2 evidence.

### 4.3 Clipboard and native Explorer interoperability

**目的：** Make file keyboard shortcuts and OLE drag/drop work across Local, ADB, and SFTP without consuming text or image clipboard data.
**輸入：** Remote transfer engine, existing Shell clipboard and OLE drag adapters.
**產出：** Versioned remote clipboard format, focus routing, staging drag bridge, and targeted tests.
**依賴：** 2.2, 3.1, and 3.2.
**Owner／Wave：** Primary／4.
**Gate／Evidence：** INT-1 in `evidence/clipboard-drag-tests.json`.
**完成門檻：** Every endpoint pair supports keyboard and pointer transfer; text/image clipboard tests remain unchanged.

- [ ] 4.3.1 Add a bounded versioned remote-file clipboard payload with copy/cut intent.
- [ ] 4.3.2 Route Ctrl+C/X/V by focus and clipboard format so editable text and images do not trigger file operations.
- [x] 4.3.3 Route native `CF_HDROP` sources to Local, ADB, and SFTP destinations.
- [x] 4.3.4 Materialize remote drag-out selections into scoped staging before the OLE drag loop.
- [ ] 4.3.5 Add targeted keyboard, image-clipboard, native drop, and remote drag-out tests; record INT-1 evidence.

## 5. Verification and release readiness

### 5.1 Build and regression gates

**目的：** Verify correctness without weakening existing Windows Explorer behavior.
**輸入：** Completed provider, UI, and integration tests.
**產出：** Command logs and a linked evidence index.
**依賴：** 4.2.
**Owner／Wave：** Primary／5.
**Gate／Evidence：** REL-1 in `evidence/final-validation.json`.
**完成門檻：** Formatting, targeted tests, full workspace test, and strict spec validation pass.

- [x] 5.1.1 Run formatting and affected-crate test suites.
- [x] 5.1.2 Run the opt-in SFTP and ADB integration gates with secrets supplied only externally.
- [ ] 5.1.3 Run full workspace regression tests and `openspec validate --strict`.
- [ ] 5.1.4 Create evidence index with command, result, timestamp, and source revision for every gate.
