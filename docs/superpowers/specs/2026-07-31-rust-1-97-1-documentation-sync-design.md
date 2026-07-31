# Rust 1.97.1 Documentation Sync Design

Date: 2026-07-31

## Objective

Synchronize SuperExplorer's current Rust toolchain documentation with the required `stable-x86_64-pc-windows-msvc` toolchain running Rust 1.97.1, while preserving truthful historical evidence from validation runs that actually used Rust 1.95.0.

## Current State

The executable toolchain and project metadata already agree:

- `rust-toolchain.toml` pins channel `1.97.1`.
- The root Cargo workspace sets `rust-version = "1.97.1"`.
- The active override is `1.97.1-x86_64-pc-windows-msvc`.
- `rustc` reports `1.97.1 (8bab26f4f 2026-07-14)`, full commit `8bab26f4f68e0e26f0bb7960be334d5b520ea452`, and LLVM `22.1.6`.
- Cargo reports `1.97.1 (c980f4866 2026-06-30)`, full commit `c980f4866141969fab6254a680546a277789d6f0`.

Three documents still contain Rust 1.95.0 values. Two are dated evidence records, so replacing those values would incorrectly claim that old runs used the new compiler.

## Decisions

### Preserve historical execution evidence

`docs/MANUAL_TESTS.md` records a 2026-07-26 manual validation environment. Its `rustc 1.95.0` value remains as the actual toolchain used for that evidence. Add a separate row identifying Rust 1.97.1 as the current required toolchain and state that future runs must use it.

`docs/POST_PARITY_ROADMAP_BASELINE.md` records a 2026-07-28 roadmap baseline executed with Rust/Cargo 1.95.0. Preserve that host record and add a supersession note identifying Rust/Cargo 1.97.1 as the current required baseline. The note must not imply that the historical command results were rerun.

### Update the current status baseline

`docs/STATUS.md` describes the current reproducible build baseline and will be updated to the exact Rust 1.97.1 and Cargo 1.97.1 releases, commits, host triple, LLVM version, and current capture date. The toolchain name remains `stable-x86_64-pc-windows-msvc`, with the repository pin explicitly identified as `1.97.1`.

### Avoid unrelated changes

Do not change `rust-toolchain.toml`, Cargo manifests, dependencies, source code, historical evidence paths, test outcomes, operating-system facts, or other tool versions. No tests are represented as rerun merely because documentation is synchronized.

## Validation

1. Confirm `rustup show active-toolchain` resolves to `1.97.1-x86_64-pc-windows-msvc` through the repository override.
2. Confirm `rustc --version --verbose` matches the recorded release, full commit, host, and LLVM version.
3. Confirm `cargo --version --verbose` matches the recorded release, full commit, and host.
4. Confirm the root `rust-version` and `rust-toolchain.toml` channel remain `1.97.1`.
5. Search the three documents to ensure each `1.95.0` occurrence is explicitly labeled as historical evidence and each identifies 1.97.1 as the current requirement.
6. Run `git diff --check` and verify only the design plus the three scoped documentation files change.

## Risks and Mitigations

- Historical and current versions may be confused. Mitigation: label them in separate rows or paragraphs using explicit dates and the words “historical execution” and “current required toolchain.”
- A future stable release may move beyond 1.97.1. Mitigation: treat the checked-in `rust-toolchain.toml` pin, not the floating stable channel, as the repository authority.
- Documentation could overstate validation. Mitigation: state that historical suites were not rerun under 1.97.1 as part of this documentation-only change.

## Scope

This is a documentation synchronization only. It does not upgrade the installed toolchain, change compiler configuration, or rerun the historical parity and roadmap validation suites.
