# rust-folder-size-map-view

這是獨立的 Size Map 範例，只依賴公開的 `explorer-extension-api` 與
`explorer-extension-ui-api`。Plugin 實作一般 Rust
`SizeMapViewImplementationV1` trait；SDK 負責 `abi_stable` adapter，host
負責遞迴掃描、增量總計、GPUI 繪製、選取、導覽與 F5 generation。ABI
不會傳遞檔案路徑、native handle、GPUI entity 或 host 私有型別。

從 repository root 執行：

```powershell
$pluginRoot = 'sdk/fixtures/rust-folder-size-map-view'
cargo build --manifest-path "$pluginRoot/Cargo.toml" --target x86_64-pc-windows-msvc --locked --offline
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $pluginRoot
```

使用 `--plugin-dll` 載入 DLL，選擇 **View → Size Map**。單擊矩形會與
Details 共用選取；雙擊資料夾會走 host 正式導覽；F5 會更新 generation
並拒絕舊結果。

Host 依 parent-before-child 順序傳入遞迴階層，子 rectangle 會畫在
parent rectangle 內，巢狀資料夾因此由最近的可見祖先擁有。有界 public
projection 超過 255 個節點時，host 先保留最大的 root siblings，之後只納入
parent 已存在的 descendants；其餘尾端合併成 **Other (N items)**，不會產生
orphan node。Other 本身是不可開啟的 accessibility group；每個
被合併項目仍保有獨立名稱、可用鍵盤聚焦、可由 UIA 搜尋與選取，並與
Details 共用 host 選取。只有所有被合併項目都完成測量時，Other 才會
標示為完整。

最終 smoke/UITEST 必須產生 `status = passed` 的 `report.json` 與畫面證據。
此範例以獨立 DLL 與 README 打包，不加入 `build_install.bat`；installer
仍只內建已完成的 folder-size visual column plugin。

## 修改指南

- 矩形配置、標籤或顏色在 `src/lib.rs` 修改；callback 必須保持 data-only
  且有界。
- contribution metadata 在 `plugin-project.json` 修改，feature、capability
  與 contribution ID 必須和 `src/lib.rs` registration 一致。
- dependency 變更時使用精確版本、重建 `Cargo.lock`，並同步更新
  `provenance.json` 與 `SBOM.json`。
- 完整範例 gate 後重跑上述命令與本機 `size-map-plugin-headful` UITEST；
  CI 不作為驗收路徑。
