# Lua tokei Code Lines 欄位

這個獨立 example 使用 package-attested `ToolHandleV1` 呼叫 plugin 內 digest
核准的 Windows x64 payload。Lua 沒有 `os`、`io`、`package`、`require`、
shell、PATH 或任意 executable 權限；只能使用 host-minted tool handle。

成功結果會全域保存於
`%LOCALAPPDATA%/RustGpuiExplorer/cache/code-lines/lua-tokei-code-lines-column/v1`。
opaque canonical identity、修改時間與檔案大小完全相符時直接使用 cache；
metadata 改變、cache 損壞或 tool error 都是 cache miss，也不會保存失敗值。

```powershell
$r='sdk/fixtures/lua-tokei-code-lines-column'
cargo test --manifest-path "$r/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $r
```

在 `lua/main.lua` 修改 registration 與 `parse_tokei_json` display mapping，
不得加入 PATH fallback。`src/lib.rs` 只保留 restricted Lua surface、opaque
cache metadata 與 host-minted tool handle。dependency 變更必須使用精確版本、
重建 `Cargo.lock` 並更新 provenance/SBOM。完整 example gate 後執行本機
`lua-tokei-code-lines-headful`；CI 不作為驗收路徑。
