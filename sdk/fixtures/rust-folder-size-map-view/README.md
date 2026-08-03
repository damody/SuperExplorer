# rust-folder-size-map-view

This standalone consumer is the Size Map example. It only imports the public
`explorer-extension-api` and `explorer-extension-ui-api` crates. The plugin
implements the ordinary Rust `SizeMapViewImplementationV1` trait; the SDK owns
the `abi_stable` adapter and the host owns directory scanning, GPUI drawing,
selection, navigation, and F5 generation handling.

Build it from the repository root:

```powershell
cargo build --manifest-path sdk/fixtures/rust-folder-size-map-view/Cargo.toml --target x86_64-pc-windows-msvc --locked --offline
```

Use the resulting DLL only with the explicit development argument:

```powershell
cargo run -p explorer-app -- --plugin-dll D:\SuperExplorer\sdk\fixtures\rust-folder-size-map-view\target\x86_64-pc-windows-msvc\debug\rust_folder_size_map_view.dll
```

The renderer receives copied node IDs, names, kinds, exact-byte availability,
status, viewport, theme, settings, and generation. It returns normalized
treemap rectangles only; it never receives a filesystem path, a native handle,
or a GPUI entity.

This independent example is packaged as the resulting DLL plus this README;
it is intentionally not added to `build_install.bat`. The installer continues
to bundle the single completed folder-size plugin while this view is selected
explicitly through `--plugin-dll` for product validation.
