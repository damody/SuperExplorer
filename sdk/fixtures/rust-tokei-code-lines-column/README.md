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

Build and test from this directory with an empty Cargo home:

```powershell
cargo test --lib --locked --offline
cargo build --release --locked --offline --target x86_64-pc-windows-msvc
```

The package payload is the resulting
`target/x86_64-pc-windows-msvc/release/rust_tokei_code_lines_column.dll`.

The checked-in `.cargo/config.toml` points at the SDK's sealed vendor tree.
`samples/` contains the mixed-language, empty, binary, and unknown-extension
inputs used by the smoke script.
