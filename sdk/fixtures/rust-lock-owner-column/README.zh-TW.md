# Rust 鎖定擁有者欄位

這個獨立的 public SDK 範例會新增「詳細資料」欄位，用來顯示目前持有檔案，或把目前工作目錄放在資料夾內的程序。若程序位於 `D:\AI_Pic\ComfyUI\nested`，`nested` 與畫面上可見的父層 `ComfyUI` 都會顯示該程序，行為盡量與 Windows 檔案總管一致。

擴充功能只會透過 `LockOwnerQueryServiceV1` 收到 opaque item handle 與 host 擁有的程序顯示資料。偵測到的路徑、命令列、環境變數、原生 handle，以及關閉、終止或關閉 handle 的權限都不會跨越 ABI。無法存取、受保護、正在結束或版面配置不受支援的程序會被略過，因此空白結果不代表一定沒有程序正在使用該資料夾。

Host 每個批次只建立一次有上限的程序快照，並遵守取消與絕對期限。結果只會短暫快取，且會拒絕來自舊 F5、導覽、分頁或功能狀態的結果；系統不會輪詢。按 F5 會清除短期快取並重新偵測。程序結束或把工作目錄移出子樹後，欄位會在這次重新整理時清空。

```powershell
$pluginRoot = 'sdk/fixtures/rust-lock-owner-column'
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $pluginRoot
```

若要修改範例，請調整 `src/lib.rs` 內的 owned JSON 顯示投影或 renderer；不要加入產品私有 crate 或程序控制 API。查詢或相依套件變更時，必須保留精確 Cargo 版本、重新產生 `Cargo.lock`，並更新 `provenance.json`、`SBOM.json` 與 `LICENSES.json`。

完成上述離線範例 gate 後，請在本機執行 `rust-lock-owner-headful`；CI 不能取代這個驗收。該測項會直接啟動 `%SystemRoot%\System32\cmd.exe` 與 `%SystemRoot%\SysWOW64\cmd.exe`，用 `IsWow64Process2` 驗證 WOW64，檢查精確與父層列，接著結束兩個程序、按 F5，並確認欄位已清空。
