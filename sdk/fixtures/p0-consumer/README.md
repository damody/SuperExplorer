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

The foreground measurement hint never cancels an in-flight folder walk. The
background worker publishes an exact value once the scan has completed; partial
or error results are never used for numeric sorting or stored as exact values.
Completed values are cached by this plugin under
`%LOCALAPPDATA%\RustGpuiExplorer\plugins\p0-consumer\folder-size\v1` (at most
256 records). A cache entry is reused only when its canonical directory identity,
directory modified timestamp, recursive limits, and cache schema match. Changing
the directory timestamp or settings causes a fresh background scan.
