## Context

The repository already pins Rust 1.97.1 in `rust-toolchain.toml` and the workspace `rust-version`. The active override resolves to `1.97.1-x86_64-pc-windows-msvc`, with rustc commit `8bab26f4f68e0e26f0bb7960be334d5b520ea452`, Cargo commit `c980f4866141969fab6254a680546a277789d6f0`, and LLVM 22.1.6.

`docs/MANUAL_TESTS.md` and `docs/POST_PARITY_ROADMAP_BASELINE.md` are dated evidence records whose listed Rust 1.95.0 environment was true for their original runs. `docs/STATUS.md` is a current reproducible-baseline description and should reflect the active pin. A mechanical replacement would conflate historical execution with current requirements.

## Goals / Non-Goals

**Goals:**

- Make the current Rust 1.97.1 requirement explicit in all three scoped documents.
- Preserve the actual Rust/Cargo 1.95.0 provenance of dated validation evidence.
- Record exact current rustc, Cargo, host, commit, and LLVM values in the current status baseline.
- State clearly that this documentation-only change does not rerun historical validation suites.

**Non-Goals:**

- Install or update Rust, Cargo, rustup, LLVM, MSVC, or the Windows SDK.
- Modify `rust-toolchain.toml`, Cargo manifests, dependencies, source code, tests, or evidence outcomes.
- Claim that historical parity or roadmap commands passed under Rust 1.97.1.

## Decisions

### Treat repository pins as the current authority

The current required version is derived from the checked-in `rust-toolchain.toml` channel and root Cargo `rust-version`, then verified against the active binaries. This is preferred over documenting a floating `stable` version because reproducibility requires an exact release.

### Preserve dated evidence values and add supersession notes

The manual-test document keeps its historical `rustc 1.95.0` row and adds a separate current-requirement row. The roadmap baseline keeps its Rust/Cargo 1.95.0 host statement and immediately adds a note that the current repository baseline is Rust/Cargo 1.97.1 and that the old run was not rerun.

Replacing those old values was rejected because the documents explicitly describe actual runs on specific dates.

### Refresh the current status table

The reproducible-build section of `docs/STATUS.md` is current-state documentation, so its capture date and Rust/Cargo details are replaced with the active values. The toolchain row identifies both the stable host toolchain name and the exact repository pin. Non-Rust environment details remain unchanged because they were not re-audited.

### Keep the change documentation-only

No build or test result is needed to establish a text-only synchronization. Validation checks version commands, pins, scoped diffs, stale-string context, and formatting. Full workspace or headful suites were rejected as unrelated to the documentation change and would not retroactively alter old evidence.

## Risks / Trade-offs

- [Readers may still mistake historical 1.95.0 for the current requirement] → Place explicit current-requirement notes adjacent to every retained historical value.
- [Current binary output may drift later] → Record exact commits and treat `rust-toolchain.toml` as the authority for future refreshes.
- [Updating the capture date may imply all environment rows were re-audited] → State that the refresh covers Rust/Cargo fields only and leave other dated facts explicitly unchanged.
- [Documentation may imply tests were rerun] → Add direct language that historical suites were not rerun under 1.97.1.

## Migration Plan

1. Update the manual-test environment section with a separate current required toolchain row and provenance note.
2. Add a supersession note beside the roadmap baseline's historical host record.
3. Refresh the Rust/Cargo rows and capture date in the current status baseline.
4. Compare each recorded value with the repository pins and active binaries.
5. Verify only the three scoped implementation documents and OpenSpec artifacts changed.

Rollback is a normal documentation revert; no runtime or data migration is involved.

## Open Questions

None. The historical/current distinction and three-file scope are approved.
