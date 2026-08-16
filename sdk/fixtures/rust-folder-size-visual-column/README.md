# Rust folder-size visual column example

This is the single development Plugin used for the current product-validation
slice. It implements the public Rust `ExtensionRegistrarImplementationV1` and
`VisualColumnImplementationV1` traits and is loaded through the host's
`abi_stable` root module.

From the repository root, build and test the independent consumer with the
pre-populated local Cargo registry cache. Third-party sources are never
committed or vendor-tracked. `--offline` deliberately performs no bootstrap;
if a locked crate is missing from the local cache, populate that cache through
the approved local bootstrap procedure before retrying:

```powershell
$pluginRoot = 'sdk/fixtures/rust-folder-size-visual-column'
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline

# Prerequisite: the local Cargo registry cache already contains every locked
# source. Missing cache entries are an explicit bootstrap failure, not a reason
# to enable network access or to commit a vendor directory.
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot sdk/fixtures/rust-folder-size-visual-column
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot sdk/fixtures/rust-folder-size-visual-column
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot sdk/fixtures/rust-folder-size-visual-column
```

The package is written to `dist/rust-folder-size-visual-column-0.1.0-<bundle-id>.sepack`.
To reproduce the UI from source, launch the app with the fixture DLL:

```powershell
cargo run -p explorer-app --locked --offline -- --plugin-dll D:\SuperExplorer\sdk\fixtures\rust-folder-size-visual-column\target\x86_64-pc-windows-msvc\debug\rust_folder_size_visual_column.dll
```

Open a filesystem directory in **Details** view. The visible **Folder size**
column consumes the Host-authoritative `folder.aggregate` value and renders a
proportional bar from the public cell context. The official runtime never asks
this renderer to enumerate or measure the filesystem. Right-click the **Folder size**
header to toggle **Show proportional bar**. The Extensions menu also lists
`folder-size (Column)` and `folder-size-renderer (GPUI Renderer)`. Run without
`--plugin-dll` to keep the built-in-only Details view.

![Completed folder-size values and proportional bars](screenshots/folder-size-column.png)

The manifest and registrar declare `folder.aggregate`. The Host owns coalescing,
partial/error state, sorting values, persistence, identity, expiry, and
invalidation. The old measure callback remains only as a bounded compatibility
surface for legacy local fixtures; the product host logs that it is bypassed and
does not call it for official Folder Size results.

To customize this example, edit `FolderSizeRenderer::render` for cell text/bar
appearance, then rerun the four local commands above. The package declares
separate `column`, `recalculate`, and `settings` feature identities; only the
column's implemented ABI root/provider/renderer contributions are advertised.
