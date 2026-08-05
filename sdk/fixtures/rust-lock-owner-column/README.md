# Rust Lock Owner column

This independent public-SDK example adds a Details column that discovers which
processes currently hold a file. The plugin receives only opaque item handles
and owned process display data through `LockOwnerQueryServiceV1`; paths, native
handles, shutdown, termination, and close-handle authority never cross the ABI.

The host performs bounded background queries, rejects stale F5/navigation
generations, and treats the foreground deadline as a cancellation boundary.
An empty query clears the cell. Press F5 to perform a manual refresh.

```powershell
$pluginRoot = 'sdk/fixtures/rust-lock-owner-column'
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $pluginRoot
```

To modify the example, change the owned JSON display projection or renderer in
`src/lib.rs`; do not add private product crates or process-control APIs.
When the query or dependencies change, keep exact Cargo versions, regenerate
`Cargo.lock`, and refresh `provenance.json`, `SBOM.json`, and `LICENSES.json`.
After this complete example gate, run `rust-lock-owner-headful` locally; CI is
not an acceptance path.
