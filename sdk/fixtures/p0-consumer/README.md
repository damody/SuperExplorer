# p0-consumer demo

This is the single development Plugin used for the current product-validation
slice. It implements the public Rust `ExtensionRegistrarImplementationV1` and
`VisualColumnImplementationV1` traits and is loaded through the host's
`abi_stable` root module.

From the repository root:

```powershell
cargo build --manifest-path sdk/fixtures/p0-consumer/Cargo.toml --target x86_64-pc-windows-msvc --offline
cargo run -p explorer-app -- --plugin-dll D:\SuperExplorer\sdk\fixtures\p0-consumer\target\x86_64-pc-windows-msvc\debug\p0_consumer.dll
```

Open a filesystem directory in **Details** view. The visible **Folder size**
column recursively measures child folders on the Plugin worker and renders a
proportional bar from the public cell context. Right-click the **Folder size**
header to toggle **Show proportional bar**. The Extensions menu also lists
`folder-size (Column)` and `folder-size-renderer (GPUI Renderer)`. Run without
`--plugin-dll` to keep the built-in-only Details view.
