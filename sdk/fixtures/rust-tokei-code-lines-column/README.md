# Rust tokei Code lines column

This is the smallest public-SDK consumer for the `rust-tokei` batch column.
The DLL uses `tokei = 14.0.0` as a statically linked Rust library; it never
launches `tokei.exe` or another child process. The host supplies at most 128
items and bounded, generation-attested `InputStreamV1` values. Each stream is
limited to 8 MiB and invalid UTF-8, unknown extensions, and oversized inputs
return `UNSUPPORTED`, never a fake zero.

The structured value contains `language`, `code`, `comments`, `blanks`, and
`total`; the stable sort key is the exact unsigned `code` count.

The DLL registers two linked contributions: the `COLUMN` contribution owns the
batch provider, while the `GPUI_RENDERER` contribution owns the public
`VisualColumnImplementationV1` renderer. The renderer draws the exact code
count, a bar proportional to the largest sibling U64 value, and (when the host
setting contains `comments`) a detail line with language, comments, blanks,
and total. Rendering is data-only and does not perform I/O.

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
