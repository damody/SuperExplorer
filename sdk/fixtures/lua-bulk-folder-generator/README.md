# Lua bulk-folder generator

This restricted-Lua example declares a button, host form, and typed
create-directory plan. It generates 1–100,000 names from parent, prefix, start,
padding, suffix, and conflict policy; requests a second confirmation above
1,000; reports cancellation as partial; and only undoes still-empty directories
created by the same plan. Filesystem mutation remains in the host executor.

```powershell
$r='sdk/fixtures/lua-bulk-folder-generator'
cargo test --manifest-path "$r/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $r
```

Modify naming and typed-plan projection in `src/lib.rs` and registration in
`lua/main.lua`. Do not add direct filesystem mutation to Lua. Dependency
changes require exact versions, a regenerated `Cargo.lock`, and refreshed
provenance/SBOM files. After this complete example gate, run
`lua-bulk-folder-headful` and `extension-command-interaction-headful` locally;
CI is not an acceptance path.
