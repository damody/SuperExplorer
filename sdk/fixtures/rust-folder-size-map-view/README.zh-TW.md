# rust-folder-size-map-view

這個官方 Size Map 範例是純資料佈局與 renderer。Host 透過宣告式 `folder.tree` 需求提供有界、generation-safe 的共用資料夾樹；plugin 不掃描檔案系統，也不自行啟動 scan coordinator。

在 repository root 執行：

```powershell
$pluginRoot = 'sdk/fixtures/rust-folder-size-map-view'
cargo build --manifest-path "$pluginRoot/Cargo.toml" --target x86_64-pc-windows-msvc --locked --offline
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $pluginRoot
```

`plugin-project.json` 的 Size Map contribution 與 registrar 都必須宣告 `abi`、`folder.tree`。Host 保證 parent-before-child 節點、partial/terminal 狀態、F5 generation 與 stale-result rejection；plugin 只計算 treemap rectangle、選取與 accessibility 表現。

開啟方式：以 `--plugin-dll` 載入 DLL，從 **View > Size Map** 切換。F5 會要求新的 Host generation，舊佈局不得覆蓋新結果。

UITEST 使用 `size-map-plugin-headful`，並保留 `report.json` 與畫面截圖作為證據。
