# Rust Lock Owner 欄位

這個獨立 public SDK example 新增 Details 欄位，用來查詢目前持有檔案的
process。Plugin 只會透過 `LockOwnerQueryServiceV1` 收到 opaque item handle
與 owned process display data；路徑、native handle、shutdown、terminate 與
close-handle authority 都不會跨越 ABI。

Host 在有界背景工作中查詢，拒絕 navigation/refresh 舊 generation 結果，
並把 foreground deadline 當作 cancellation boundary。空查詢會清除 cell；
按 Refresh 或 F5 會走同一個正式 refresh action。

```powershell
$pluginRoot = 'sdk/fixtures/rust-lock-owner-column'
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $pluginRoot
```

修改 owned JSON display projection 或 renderer 時只調整 `src/lib.rs`，不得
加入 private product crate 或 process-control API。query 或 dependency 變更
時保留 Cargo 精確版本、重建 `Cargo.lock`，並同步更新 `provenance.json`、
`SBOM.json` 與 `LICENSES.json`。完整 example gate 後執行本機
`rust-lock-owner-headful`；CI 不作為驗收路徑。
