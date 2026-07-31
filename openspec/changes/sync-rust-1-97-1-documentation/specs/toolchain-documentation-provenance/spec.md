## ADDED Requirements

### Requirement: Current toolchain authority
Current toolchain documentation SHALL identify the repository-pinned toolchain as Rust 1.97.1 for `x86_64-pc-windows-msvc` and SHALL agree with both `rust-toolchain.toml` and the workspace `rust-version`.

#### Scenario: Current version verification
- **WHEN** the repository pins and active toolchain are inspected
- **THEN** the current documentation identifies Rust 1.97.1 and the `x86_64-pc-windows-msvc` host without substituting a floating version

### Requirement: Exact reproducible baseline
The current status baseline SHALL record rustc release 1.97.1 with full commit `8bab26f4f68e0e26f0bb7960be334d5b520ea452`, Cargo release 1.97.1 with full commit `c980f4866141969fab6254a680546a277789d6f0`, and LLVM 22.1.6.

#### Scenario: Active binary comparison
- **WHEN** `rustc --version --verbose` and `cargo --version --verbose` are compared with `docs/STATUS.md`
- **THEN** the documented release, commit, host, and LLVM values match the active repository override

### Requirement: Historical evidence preservation
Dated evidence documents that actually ran under Rust or Cargo 1.95.0 MUST retain those historical values and SHALL label them as historical execution rather than the current requirement.

#### Scenario: Manual validation provenance
- **WHEN** the 2026-07-26 manual validation environment is read
- **THEN** it retains the actual rustc 1.95.0 value and separately identifies Rust 1.97.1 as the current required toolchain

#### Scenario: Roadmap baseline provenance
- **WHEN** the 2026-07-28 roadmap baseline environment is read
- **THEN** it retains the actual Rust/Cargo 1.95.0 values and separately identifies Rust/Cargo 1.97.1 as the superseding current baseline

### Requirement: No retroactive validation claim
The synchronized documents SHALL state that the historical validation suites were not rerun under Rust 1.97.1 as part of this documentation-only change.

#### Scenario: Reader evaluates evidence scope
- **WHEN** a reader reviews the current-requirement notes in the dated evidence documents
- **THEN** the reader can determine that the old results remain tied to their original toolchain and were not regenerated under 1.97.1

### Requirement: Scoped documentation-only change
Implementation SHALL NOT modify Rust configuration, manifests, dependencies, source code, tests, historical outcomes, or unrelated environment facts.

#### Scenario: Final diff review
- **WHEN** the implementation diff is reviewed
- **THEN** runtime changes are absent and the only implementation documents changed are `docs/MANUAL_TESTS.md`, `docs/POST_PARITY_ROADMAP_BASELINE.md`, and `docs/STATUS.md`
