## 1. Synchronize Documentation

- [x] 1.1 Add an adjacent current-required-toolchain row and no-rerun note to `docs/MANUAL_TESTS.md` while preserving its historical rustc 1.95.0 result.
- [x] 1.2 Add an adjacent Rust/Cargo 1.97.1 supersession and no-rerun note to `docs/POST_PARITY_ROADMAP_BASELINE.md` while preserving its historical 1.95.0 host record.
- [x] 1.3 Refresh only the Rust/Cargo capture fields in `docs/STATUS.md` with the active 1.97.1 releases, full commits, host, and LLVM 22.1.6.

## 2. Validate Provenance and Scope

- [x] 2.1 Verify `rust-toolchain.toml`, workspace `rust-version`, active rustc, and active Cargo all agree with the new current-baseline text.
- [x] 2.2 Verify every retained 1.95.0 occurrence in the three scoped documents is explicitly historical and paired with the 1.97.1 current requirement.
- [x] 2.3 Run OpenSpec strict validation and `git diff --check`, then confirm implementation changes are limited to the three approved documents and this change's artifacts.
