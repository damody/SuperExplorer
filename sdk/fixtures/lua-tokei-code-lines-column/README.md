# Lua tokei code-lines column

This independent example registers a restricted Lua column and maps bounded JSON from a package-attested `windows-x64` tool. Lua has no `os`, `io`, `package`, `require`, shell, PATH, or arbitrary executable access; only a host-minted `ToolHandleV1` may execute the digest-checked payload.

```powershell
$r='sdk/fixtures/lua-tokei-code-lines-column'
cargo test --manifest-path "$r/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $r
```

Modify `lua/main.lua` to change registration and `parse_tokei_json` for display mapping. Never add PATH fallback.
