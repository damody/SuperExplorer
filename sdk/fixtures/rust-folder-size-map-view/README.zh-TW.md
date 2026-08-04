# rust-folder-size-map-view

這是獨立的資料夾 Size Map 範例，只依賴公開的
`explorer-extension-api` 與 `explorer-extension-ui-api`。Plugin 作者實作一般
Rust `SizeMapViewImplementationV1` trait；SDK 負責 `abi_stable` adapter，Host
負責遞迴計量、GPUI 繪製、選取、導覽與 F5 generation。ABI 不傳路徑、native
handle、GPUI entity 或 private host type。

所有依賴版本都直接固定在本範例的 `Cargo.toml`。以下 helper 只從預先存在的
Cargo cache 準備被 Git 忽略的本機 directory source，不會改寫 dependency：

```powershell
$pluginRoot = 'sdk/fixtures/rust-folder-size-map-view'
$cargoConfig = powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/prepare-local-cargo-source.ps1 -PluginRoot $pluginRoot
cargo build --manifest-path "$pluginRoot/Cargo.toml" --target x86_64-pc-windows-msvc --locked --offline --config $cargoConfig
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline --config $cargoConfig
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $pluginRoot
```

開發時以 `--plugin-dll` 明確載入 DLL，然後從 **檢視 → Size Map** 開啟。
點擊方塊會與 Details 共用選取，雙擊資料夾會導覽，F5 會建立新 generation
並丟棄舊結果。只有最後 smoke/UITEST 產生且 `status` 為 `passed` 的 report
才算目前證據。本範例不加入 installer；installer 仍只附帶已完成的
folder-size visual column Plugin。
