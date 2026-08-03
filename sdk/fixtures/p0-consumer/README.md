# p0-consumer demo

This is the single development plugin used for the current 0→1 validation slice.
It implements the public Rust `ExtensionRegistrarImplementationV1` trait and is
loaded through the host's `abi_stable` root module.

From the repository root:

```powershell
cargo build --manifest-path sdk/fixtures/p0-consumer/Cargo.toml --target x86_64-pc-windows-msvc --offline
cargo run -p explorer-app -- --plugin-dll D:\SuperExplorer\sdk\fixtures\p0-consumer\target\x86_64-pc-windows-msvc\debug\p0_consumer.dll
```

Open the **Extensions** menu. It shows a read-only entry containing
`p0-consumer`, `abi-root`, and `Column`. Run without `--plugin-dll` to start the
application without that entry.
