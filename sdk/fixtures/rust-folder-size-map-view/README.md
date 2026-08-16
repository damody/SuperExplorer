# rust-folder-size-map-view

This standalone consumer is the Size Map example. It only imports the public
`explorer-extension-api` and `explorer-extension-ui-api` crates. The plugin
implements the ordinary Rust `SizeMapViewImplementationV1` trait; the SDK owns
the `abi_stable` adapter and the host owns recursive directory scanning,
incremental totals, GPUI drawing, selection, navigation, and F5 generation
handling. No filesystem path, native handle, GPUI entity, or render context
crosses the ABI.

Build it from the repository root. Dependency versions come directly from
this example's `Cargo.toml`; offline builds use Cargo's standard local cache:

```powershell
$pluginRoot = 'sdk/fixtures/rust-folder-size-map-view'
cargo build --manifest-path "$pluginRoot/Cargo.toml" --target x86_64-pc-windows-msvc --locked --offline
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $pluginRoot
```

All SDK and ABI dependencies in `Cargo.toml` use exact (`=`) versions. Keep
those pins and regenerate `Cargo.lock` deliberately when upgrading the SDK;
do not widen them to caret requirements. Offline commands must remain
`--locked --offline` and must not bootstrap from the network.

Use the resulting DLL only with the explicit development argument:

```powershell
cargo run -p explorer-app -- --plugin-dll D:\SuperExplorer\sdk\fixtures\rust-folder-size-map-view\target\x86_64-pc-windows-msvc\debug\rust_folder_size_map_view.dll
```

The renderer receives copied node IDs, names, kinds, exact-byte availability,
status, viewport, theme, settings, and generation. It returns normalized
treemap rectangles only. Its feature and contribution both declare the
Host-projected `folder.tree` requirement; it does not require filesystem access.
Select **View → Size Map**, click a rectangle to share
selection with Details, double-click a folder to navigate, and press **F5** to
exercise generation/stale-result recovery. The final local smoke writes
`report.json` plus screenshots under its output directory; only a report whose
status is `passed` counts as current evidence.

The host sends a parent-before-child recursive hierarchy. Child rectangles are
drawn inside their parent rectangle, so nested folders remain visually owned by
the closest visible ancestor. When the bounded public projection exceeds 255
individual nodes, the host keeps the largest root siblings first and then only
admits descendants whose parent is already present. The remaining tail becomes
an **Other (N items)** rectangle without exposing orphan nodes.
Other is a non-openable accessibility group. Every omitted item remains an
individually named, keyboard-focusable UIA child and selecting one uses the
same host-owned selection as Details. Exact totals are preserved; the group is
reported partial unless every omitted item has a complete measurement.

## Modification guide

- Change rectangle layout, labels, or colors in `src/lib.rs`; keep the callback
  data-only and bounded.
- Change contribution metadata in `plugin-project.json` and keep its feature,
  capability, and contribution IDs aligned with registration in `src/lib.rs`.
- When dependencies change, update exact requirements, regenerate `Cargo.lock`,
  and refresh `provenance.json` and `SBOM.json` before packaging.
- Re-run the commands above and the `size-map-plugin-headful` local UITEST after
  the complete example gate; do not use CI as an acceptance path.

This independent example is packaged as the resulting DLL plus this README;
it is intentionally not added to `build_install.bat`. The installer continues
to bundle the single completed folder-size plugin while this view is selected
explicitly through `--plugin-dll` for product validation.
