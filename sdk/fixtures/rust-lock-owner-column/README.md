# Rust Lock Owner column

This independent public-SDK example adds a Details column that discovers which
processes currently hold a file or have a current working directory inside a
folder. A process working in `D:\AI_Pic\ComfyUI\nested` is shown on both the
`nested` row and its visible `ComfyUI` ancestor, matching File Explorer's useful
folder-occupancy behavior.

The plugin receives only opaque item handles and owned process display data
through `LockOwnerQueryServiceV1`; discovered paths, command lines, environment
data, native handles, shutdown, termination, and close-handle authority never
cross the ABI. Inaccessible, protected, racing, or unsupported processes are
skipped, so an empty result is not proof that no process uses the folder.

The host performs one bounded process snapshot per batch, observes cancellation
and an absolute deadline, caches results briefly, and rejects stale F5,
navigation, tab, and feature generations. It does not poll. Press F5 to discard
the short-lived cache and re-run discovery; after a process exits or changes its
working directory, the corresponding cell clears on that refresh.

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

The headful case starts both `%SystemRoot%\System32\cmd.exe` and
`%SystemRoot%\SysWOW64\cmd.exe` directly, verifies WOW64 with
`IsWow64Process2`, checks exact and parent rows, exits both processes, presses
F5, and verifies that the values clear.
