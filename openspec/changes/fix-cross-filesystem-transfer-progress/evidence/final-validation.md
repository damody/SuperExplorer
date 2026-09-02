# Final validation

Date: 2026-09-02

## Implemented contract

- Unified item/byte/phase/current-item progress is consumed by Local, ADB, SFTP and UI.
- The request-scoped reporter coalesces non-forced events, uses bounded `try_send`, degrades an
  exceeded estimate to unknown, flushes lifecycle boundaries, and closes on every terminal path.
- Local, ADB and SFTP callbacks count bytes only after a successful destination write.
- Remote folder estimation recursively aggregates regular files, honours cancellation and traversal
  limits, and degrades unknown sizes, symlinks and overflow to indeterminate.
- Remote-to-remote staging uses the same callback across download and upload and estimates `2N`, so
  counters do not reset at the stage boundary. Source deletion remains gated by destination success.
- UI renders known byte ratio, zero-byte item ratio, unknown-total indeterminate state, detailed
  source/destination/current-item text and non-100% failed/cancelled/partial terminals.

## Automated gates

- `explorer-model operation`: 6 passed.
- `explorer-remote --lib`: 47 passed.
- `explorer-app remote_service::tests`: 21 passed.
- `explorer-ui operation_`: 9 passed.
- Shell bounded progress-lane test: passed.
- Shell large real-copy cancellation / exactly-one terminal / no-late-progress test: passed.
- `cargo check -p explorer-app`: passed.
- Formatting and focused compilation: passed.

## Live endpoint gates

- Windows Explorer `an.txt` drop to ADB: passed.
- Windows Explorer `an.txt` drop to SFTP: passed.
- ADB 512 MiB native upload/download emitted monotonic intermediate progress: passed.
- ADB→SFTP and SFTP→ADB 4 MiB recursive staged transfers emitted multiple byte callbacks, retained
  content integrity, and cleaned owned fixtures: passed.
- Local upload and remote download were exercised as the two stages of the same six-direction live
  matrix; Local/remote direction routing is additionally covered by app and drag tests.

## Packaging and safety

- `build_test_install.bat` release build, NSIS package, silent install, three-binary hash validation
  and installed launch: passed.
- No credential value is stored in evidence or diagnostics.
