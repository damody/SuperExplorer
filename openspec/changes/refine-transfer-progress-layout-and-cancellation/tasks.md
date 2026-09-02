## 1. Operation surface and state

- [x] 1.1 Refactor the active operation render into a fixed 250px cancel region and remaining-width progress region while preserving terminal and partial-detail layouts.
- [x] 1.2 Add request-correlated cancelling UI state, immediate `正在取消` feedback, duplicate-action suppression, and cleanup on terminal or submission failure.
- [x] 1.3 Add focused UI/state tests for active/terminal structure, accessible cancel behavior, cancelling races, and truthful cancelled progress.

## 2. Progress publication cadence

- [x] 2.1 Change remote transfer byte publication to coalesce ordinary updates at a 200ms minimum interval while forcing lifecycle and terminal boundaries.
- [x] 2.2 Apply the same 200ms ordinary-update contract to Windows Shell progress without delaying failure, cancellation, or completion events.
- [x] 2.3 Add deterministic cadence tests proving coalescing, latest-value delivery, monotonicity, and immediate forced boundaries.

## 3. Immediate provider cancellation

- [x] 3.1 Audit and harden local streaming and transfer-engine stage checks so cancellation prevents later chunks, stages, and move cleanup.
- [x] 3.2 Harden ADB push/pull cancellation so the owned subprocess is promptly terminated and cannot publish late progress.
- [x] 3.3 Harden SFTP upload/download and recursive traversal cancellation so no later chunk or item starts after the token is observed.
- [x] 3.4 Add focused provider and cross-stage tests for cancellation timing, first-terminal-wins, no late progress, and source preservation.

## 4. Integration, packaging, and user verification

- [x] 4.1 Format code and run focused explorer-ui, explorer-app, explorer-remote, and explorer-shell-win tests plus the relevant application check.
- [x] 4.2 Run strict OpenSpec validation and scan the change artifacts for placeholders, contradictions, and missing proposal-to-test traceability.
- [x] 4.3 Build and install the current source with `build_test_install.bat`.
- [x] 4.4 From the installed application, verify Local-to-ADB and Local-to-SFTP preparation, 200ms progress updates on large files, compact layout, immediate cancellation, terminal details, and no source deletion; fix and repeat on any failure.
- [x] 4.5 Record final evidence and re-run the affected focused checks after every verification-driven correction.
