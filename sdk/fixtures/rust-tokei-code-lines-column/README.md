# Rust tokei Code lines column

This is the smallest public-SDK consumer for the `rust-tokei` batch column.
The DLL uses `tokei = 14.0.0` as a statically linked Rust library; it never
launches `tokei.exe` or another child process. The host supplies at most 128
items and bounded, generation-attested `InputStreamV1` values. Each stream is
limited to 8 MiB and invalid UTF-8, unknown extensions, and oversized inputs
return `UNSUPPORTED`, never a fake zero.

The structured value contains `language`, `code`, `comments`, `blanks`, and
`total`; the stable sort key is the exact unsigned `code` count. Directory
inputs aggregate all supported files by language, select the greatest code
sum with an ascending-name tie break, and expose only that main language.

Successful results are persisted globally under
`%LOCALAPPDATA%/RustGpuiExplorer/cache/code-lines/rust-tokei-code-lines-column/v2`.
The host supplies an opaque canonical-file identity, modification timestamp,
and file size. An exact metadata match returns the cached result before the
plugin reads or analyzes the input stream; changed metadata, corrupt records,
unsupported files, and errors are cache misses and are never stored.

The DLL registers two linked contributions: the `COLUMN` contribution owns the
batch provider, while the `GPUI_RENDERER` contribution owns the public
`VisualColumnImplementationV1` renderer. The `Main code lines` renderer draws
`Language: N` with comma grouping, such as `Rust: 1,250`, without a
proportional bar. Optional detail shows the selected language's comments,
blanks, and total. Rendering is data-only and does not perform I/O.

Build and test from this directory using Cargo's standard local registry cache.
`Cargo.toml` is the dependency-version source of truth, and no third-party
source tree is tracked in this fixture:

```powershell
$pluginRoot = 'sdk/fixtures/rust-tokei-code-lines-column'
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline
cargo build --manifest-path "$pluginRoot/Cargo.toml" --release --locked --offline --target x86_64-pc-windows-msvc

# The wrappers perform the same sealed --locked --offline build.
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $pluginRoot
```

The package payload is the resulting
`target/x86_64-pc-windows-msvc/release/rust_tokei_code_lines_column.dll`.

`samples/` contains the mixed-language, empty, binary, and unknown-extension
inputs used by `scripts/smoke_tokei_plugin_headful.ps1`. The headful smoke
stores its profile and extension state below the selected output directory so
it does not reuse operator settings.

## Modification guide

- Change language analysis and cache behavior in `src/lib.rs`; retain exact
  modification-time nanoseconds and source-size matching before a cache hit.
- Keep the renderer data-only. Settings may alter presentation but must not
  change the exact U64 sort value returned by the batch provider.
- Dependency changes require exact versions, a regenerated `Cargo.lock`, and
  refreshed `provenance.json`, `SBOM.json`, and `LICENSES.json`.
- After the example is complete, run `rust-tokei-code-lines-headful` locally;
  CI is not an acceptance path.
