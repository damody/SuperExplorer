## Why

SuperExplorer already pins and runs Rust 1.97.1, but three status and evidence documents still contain Rust 1.95.0 values. The documentation must distinguish current toolchain requirements from truthful historical execution records instead of rewriting old evidence as though it had been rerun.

## What Changes

- Add the current Rust 1.97.1 requirement to the dated manual-test and roadmap-baseline documents while preserving their actual Rust 1.95.0 execution records.
- Update the current reproducible build baseline in `docs/STATUS.md` with the exact Rust, Cargo, host, commit, and LLVM values observed from the repository-pinned toolchain.
- Explicitly state that historical suites were not rerun under Rust 1.97.1 as part of this documentation-only synchronization.
- Validate documentation against `rust-toolchain.toml`, the workspace `rust-version`, and the active Rust/Cargo binaries.

## Capabilities

### New Capabilities

- `toolchain-documentation-provenance`: Requires current toolchain documentation to match the repository pin while preserving dated historical execution provenance.

### Modified Capabilities

None.

## Impact

- Updates only `docs/MANUAL_TESTS.md`, `docs/POST_PARITY_ROADMAP_BASELINE.md`, and `docs/STATUS.md` in implementation, in addition to this change's OpenSpec artifacts.
- Does not alter Rust installation, `rust-toolchain.toml`, Cargo manifests, dependencies, source code, tests, or prior test outcomes.
- Makes future documentation reviews less likely to mistake historical Rust 1.95.0 evidence for the current required baseline.
