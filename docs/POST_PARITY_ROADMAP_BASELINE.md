# Post-Parity Roadmap Baseline Evidence

Execution date: 2026-07-28 (Asia/Taipei)  
OpenSpec change: `complete-explorer-post-parity-roadmap`  
Baseline commit before roadmap production edits: `9707637`  
Execution state: dirty workspace containing concurrent folder-scoped Lua, drag/drop, UI, UITEST and smoke-script work; roadmap changes are listed separately below.

## Host and Environment

- Windows 11 Professional x64, build 26200.
- Rust stable `1.95.0` / Cargo `1.95.0`, `x86_64-pc-windows-msvc`.
- The current desktop has one active 175% monitor. Formal 100/125/150/200% raster and mixed-DPI evidence remains hardware-qualified rather than simulated.
- The run used the locked workspace and existing GPUI-CE gitlink.

## Roadmap Baseline Changes Included in This Run

- `explorer-common`: monotonic request deadline, exactly-once terminal gate, centralized versioned roadmap limits and validation.
- `explorer-model`: request deadline composition, validated/reconstructible location boundaries, typed Home/Quick Access synthetic roots, redacted and bounded deserialization.
- Architecture policy: broker-only Preview Handler activation, no roadmap-owned unbounded channels, no path-derived `ShellItemId`, and required session/broker version markers once those boundaries exist.
- UITEST manifest: phase-gated coverage entries for the five roadmap capabilities. Missing future validation scripts produce prerequisite SKIP, never PASS.
- Planning evidence: `docs/POST_PARITY_ROADMAP_AUDIT.md`.

## Commands and Results

The following sequence completed with exit code 0 in 65.9 seconds:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\check_architecture.ps1
cargo run -p explorer-uitest --locked -- --validate-only
openspec validate complete-explorer-post-parity-roadmap --strict
```

Observed outcomes:

- Workspace check, all-features/all-target Clippy with warnings denied, all-target tests, architecture policy, UITEST validation and OpenSpec strict validation passed.
- `explorer-common` passed 14 tests, including the new deadline, terminal race and limits tests.
- `explorer-model` passed 39 tests, including elapsed-deadline and location-boundary tests.
- UITEST parsed 41 cases and mapped all 185 active OpenSpec requirements; the default quick selection contained 15 cases.
- Architecture output confirmed Shell-free UI, platform-neutral automation, bounded/versioned roadmap work, broker-only Preview activation and no production dependency on test-only crates.

## Expected Non-Pass Cases

Ignored tests were environment- or cost-qualified, not failures. They included the billable DeepSeek live request, explicit 100,000-file performance run, interactive Explorer drag/drop, real `D:`/namespace provider cases, and installed 7-Zip/TortoiseGit handler cases where the relevant interactive/provider prerequisite is required. Roadmap validation scripts are intentionally absent at this baseline and therefore their registered UITEST cases will report prerequisite SKIP until each capability phase supplies its script and artifacts.

No baseline command failed and no known regression was accepted silently.
