# Lua 批次資料夾產生器

受限 Lua 只宣告 button、host form 與 typed create-directory plan。它可依
parent、prefix、start、padding、suffix 與 conflict policy 產生 1–100,000
個名稱；超過 1,000 需要第二次確認；取消後回報真實 partial；undo 只會
刪除仍為空白且由同一 plan 建立的資料夾。實際檔案 mutation 一律留在
host executor。

```powershell
$r='sdk/fixtures/lua-bulk-folder-generator'
cargo test --manifest-path "$r/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $r
```

在 `src/lib.rs` 修改 naming/typed plan projection，在 `lua/main.lua` 修改
registration；Lua 不得直接 mutation filesystem。dependency 變更必須使用
精確版本、重建 `Cargo.lock` 並更新 provenance/SBOM。完整 example gate 後
執行本機 `lua-bulk-folder-headful` 與
`extension-command-interaction-headful`；CI 不作為驗收路徑。
