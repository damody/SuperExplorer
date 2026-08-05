# Rust tokei Code Lines 欄位

這個獨立 example 使用 public SDK batch column API，並把精確版本
`tokei = 14.0.0` 靜態連結進 plugin；不會啟動 `tokei.exe` 或其他子程序。
Host 每批最多提供 128 個項目及有界、帶 generation 證明的 `InputStreamV1`。
每個 stream 上限為 8 MiB；無效 UTF-8、未知副檔名與過大輸入回傳
`UNSUPPORTED`，不會偽造為零。

成功結果會全域保存於
`%LOCALAPPDATA%/RustGpuiExplorer/cache/code-lines/rust-tokei-code-lines-column/v1`。
Host 提供 opaque canonical identity、修改時間秒/奈秒與檔案大小；三者完全
相符時，plugin 會在讀取 stream 前直接使用 cache。metadata 改變、cache
損壞、unsupported 或 error 都是 cache miss，也不會寫入成功 cache。

```powershell
$r='sdk/fixtures/rust-tokei-code-lines-column'
cargo test --manifest-path "$r/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $r
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $r
```

## 修改指南

- 語言分析與 cache 行為在 `src/lib.rs` 修改；cache hit 前必須精確比對修改
  時間奈秒與來源大小。
- renderer 必須保持 data-only；settings 可以改呈現，但不能改 batch provider
  回傳的精確 U64 sort value。
- dependency 變更必須使用精確版本並重建 `Cargo.lock`，同步更新
  `provenance.json`、`SBOM.json` 與 `LICENSES.json`。
- example 完整後才執行本機 `rust-tokei-code-lines-headful`；CI 不作為驗收路徑。
