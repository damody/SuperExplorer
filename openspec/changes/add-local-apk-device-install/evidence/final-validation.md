# Final validation

## Implemented surface

- System-first validated ADB resolution (`PATH`, Android SDK roots, managed fallback).
- Bounded device discovery with serial identity, model/device display-name fallback, and disabled non-device states.
- Local single-file case-insensitive APK eligibility in the native Shell menu and an `安裝` device submenu.
- Broker-safe install/download outcomes returned to the long-lived application process.
- Exact argument-array `adb -s <serial> install -r <absolute apk>` execution.
- Explicit Google Platform-Tools download, bounded bytes/time, safe ZIP path/type/count/expanded-size validation, version probing, atomic activation, and rollback.

## Automated results

- `cargo test -p explorer-remote --no-fail-fast`: 50 passed before the final install-argument test; the added focused install-argument test also passed.
- `cargo test -p explorer-app --lib -- --test-threads=1`: 117 passed, 1 environment-gated ignored, 0 failed.
- `cargo test -p explorer-extension-broker --lib --no-fail-fast`: 10 passed.
- `cargo test -p explorer-model context_menu --no-fail-fast`: 5 passed.
- `cargo check -p explorer-app`: passed.
- `cargo build -p explorer-app`: passed.
- `cargo build -p explorer-extension-broker --bins`: passed.
- Focused `git diff --check`: passed.

The first parallel explorer-app test run reached all printed test successes but the harness terminated with `STATUS_ACCESS_VIOLATION`; the required follow-up serial run passed completely. Workspace-wide `clippy -D warnings` remains blocked by pre-existing warnings in unrelated dirty `explorer-model` files; affected crates compile and their focused tests pass.

## Real device and user-input fixture

- APK: `D:\SuperExplorer\qq9.3.55.apk`
- SHA-256: `851242D139BB01ED8C787EADC30D7EC391437DE550C697B5F4C65C32EC84286F`
- Size: 389,727,209 bytes
- System ADB: `C:\Users\Damody\AppData\Local\Android\Sdk\platform-tools\adb.exe`, version `37.0.0-14910828`
- Device: serial `emulator-5554`, model `ASUSAI2501B`
- Command result: `Performing Streamed Install` followed by `Success`.

## Headful user-perspective review

Two isolated debug launches successfully opened the requested `D:\SuperExplorer` location without interrupting the already-running installed SuperExplorer. The 389 MB APK exists but was outside the virtualized UIA realized-row set; UIA type-ahead did not materialize it, so the automated screenshot/menu traversal could not select that row. The attempts and logs are retained under `evidence/headful/qq-menu*`. No product crash or APK mutation occurred. The exact install path was independently proven on the real authorized device and the menu eligibility/device mapping is covered at the model/process boundary.

## Final review

The diff was reviewed for command injection, serial/name confusion, ZIP traversal/link attacks, download bounds, activation rollback, system environment mutation, and broker lifetime. No unresolved P0/P1 defect was found in the changed surface. Device names are presentation-only; install targets use captured serials. Existing system ADB is preferred and never overwritten. Managed download remains explicit and per-user.
