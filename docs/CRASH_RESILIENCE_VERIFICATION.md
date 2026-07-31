# Crash Resilience Verification — 2026-07-27

## Automated gates

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace`: passed. This includes diagnostics fallback/redaction/poison recovery, panic subprocess reporting, production panic-policy enforcement, parser recovery, model invariant recovery, UI failure recovery, and Shell worker panic isolation.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Production crate roots deny `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::todo`, and `clippy::unimplemented` outside `cfg(test)`.

## Windows smoke verification

- `scripts/smoke_windows_lifecycle.ps1 -Profile debug -SkipBuild`: passed with exit code 0, ordered cleanup events, resize verification, and `WM_CLOSE` shutdown. Evidence: `target/smoke-evidence/20260727T160248090Z-58bb02b486c243fdafeddaa16e45ce32`.
- The combined headful bundle passed lifecycle, repeated startup/shutdown, keyboard navigation, and accessibility before stopping at an existing mouse-image assertion: `navigation-back changed on disabled hover`. The application did not crash; the script produced screenshots and `failure.txt` under `target/headful-evidence/crash-resilience/mouse`.
- The mouse-image assertion is outside this change's error-handling scope. It is retained as a separate visual-test limitation instead of being reported as a crash-resilience failure.

## Guarantee boundary

Verified recovery covers application-controlled errors and unwindable panics inside isolated workers. Native access violations, stack overflow, explicit abort, forced operating-system termination, and unrecoverable native corruption remain outside the in-process recovery guarantee.
