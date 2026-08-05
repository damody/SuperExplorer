# Lua tokei code-lines column

This independent example registers a restricted Lua column and maps bounded JSON from a package-attested `windows-x64` tool. Lua has no `os`, `io`, `package`, `require`, shell, PATH, or arbitrary executable access; only a host-minted `ToolHandleV1` may execute the digest-checked payload.

Successful rows are persisted globally under
`%LOCALAPPDATA%/RustGpuiExplorer/cache/code-lines/lua-tokei-code-lines-column/v1`.
Canonical path identity, modification timestamp, and file size form the cache
key. Unchanged files bypass the bundled tool; changed metadata, corrupt cache
records, and failed tool results are misses and are not persisted.

```powershell
$r='sdk/fixtures/lua-tokei-code-lines-column'
cargo test --manifest-path "$r/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $r
```

Modify `lua/main.lua` to change registration and `parse_tokei_json` for display mapping. Never add PATH fallback.
Keep the Rust bridge in `src/lib.rs` limited to the restricted Lua surface,
opaque cache metadata, and host-minted tool handles. Dependency changes require
exact versions, a regenerated `Cargo.lock`, and refreshed provenance/SBOM files.
After this complete example gate, run `lua-tokei-code-lines-headful` locally;
CI is not an acceptance path.
