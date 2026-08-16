# Rust 資料夾大小視覺欄位

這個官方範例是純顯示 renderer。Host 透過宣告式 `folder.aggregate` 資料需求提供完整、已授權的資料夾大小；plugin 不遞迴掃描檔案系統，也不擁有持久快取、失效或後端選擇策略。

在 repository root 執行：

```powershell
$pluginRoot = 'sdk/fixtures/rust-folder-size-visual-column'
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $pluginRoot
```

`plugin-project.json` 的 renderer contribution 必須宣告 `abi` 與 `folder.aggregate`；registrar 必須使用相同需求。舊版 visual-measure callback 只保留型別化的相容性診斷，不再執行官方資料夾量測。

Host 負責共用 MFT service、遞迴 fallback、modified-date 快取、generation 與失效。renderer 僅把 Host 提供的 byte value 格式化並繪製比例條。

![資料夾大小欄位](screenshots/folder-size-column.png)
