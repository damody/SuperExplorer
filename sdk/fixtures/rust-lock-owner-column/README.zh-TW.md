# Rust 鎖定擁有者欄位

這個獨立的公開 SDK 範例會新增一個「詳細資料」欄位，用來顯示目前持有檔案鎖定，或把工作目錄設在資料夾子樹內的程序。例如程序位於 `D:\AI_Pic\ComfyUI\nested` 時，`nested` 列與畫面上可見的 `ComfyUI` 祖先列都會顯示該程序，提供與檔案總管資料夾占用提示一致的實用行為。

外掛只能透過 `LockOwnerQueryServiceV1` 收到不透明的項目 handle 與 Host 擁有的程序顯示資料。偵測到的路徑、命令列、環境變數、原生 handle，以及關閉、終止程序或關閉 handle 的權限，都不會跨越 ABI。無法存取、受保護、正在結束或版面配置不受支援的程序會被略過，因此空白結果不代表一定沒有程序正在使用該資料夾。

Host 對每個批次只建立一次有界的程序快照，並共用即時取消訊號與單一絕對期限。結果只會短暫快取，不會輪詢；舊的 F5、導覽、分頁或功能世代結果都會被拒絕。程序結束或移出該子樹後，按 F5 會捨棄短期快取、重新偵測，並清除對應欄位。

```powershell
$pluginRoot = 'sdk/fixtures/rust-lock-owner-column'
cargo test --manifest-path "$pluginRoot/Cargo.toml" --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot $pluginRoot
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot $pluginRoot
```

若要修改此範例，只應調整 `src/lib.rs` 中由外掛擁有的 JSON 顯示投影或 renderer；不要加入產品私有 crate 或程序控制 API。查詢或相依套件變更時，請維持精確 Cargo 版本、重新產生 `Cargo.lock`，並更新 `provenance.json`、`SBOM.json` 與 `LICENSES.json`。

完成上述離線範例 gate 後，仍須在本機執行 `rust-lock-owner-headful`；CI 不能取代此驗收。該測項會直接啟動 `%SystemRoot%\System32\cmd.exe` 與 `%SystemRoot%\SysWOW64\cmd.exe`，用 `IsWow64Process2` 驗證 WOW64，檢查精確列與父層列，接著結束兩個程序、按 F5，並確認所有值都已清除。
