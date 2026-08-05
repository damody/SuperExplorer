# Rust 7z 虛擬資料夾

這個 pure-Rust example 註冊 virtual resource 與 mutation plan。安全核心會
拒絕 absolute/traversal/NUL path、不分大小寫 normalized collision、過深、
過多項目、過大輸出/ratio，以及 stale 或已變更 container。read 有明確
上限。mutation 使用同 volume staging、重新開啟驗證、重驗原始 identity、
atomic replace，並保留完整 container undo backup。secret 是短生命週期
handle，不會序列化或寫入 log。

使用本目錄作為 `PluginRoot` 執行標準 offline test/validate/build/package。
透過 `VirtualEntry`、`validate_entries`、`bounded_read` 與
`transactional_replace` 擴充 backend；不得把未驗證 archive path 直接 join
後 extract。dependency 變更必須使用精確 Cargo 版本、重建 `Cargo.lock`
並更新 provenance/SBOM/license inventory。完整 example gate 後執行本機
從 repository root 執行完整本機 package gate：

```powershell
cargo test --manifest-path sdk/fixtures/rust-7z-virtual-folder/Cargo.toml --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot sdk/fixtures/rust-7z-virtual-folder
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot sdk/fixtures/rust-7z-virtual-folder
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot sdk/fixtures/rust-7z-virtual-folder
```

production route 支援巢狀瀏覽、preview／bounded read、copy／drag-out、加入檔案、建立資料夾、刪除、重新命名、archive 內移動、加密 archive、整體 archive undo 與檔案總管風格密碼提示。密碼不會持久保存。Mutation 使用同磁碟且有 quota 的 staging，重新開啟並解碼驗證每個 entry、重驗原始 container 後才 atomic replace。

只有完整 example gate 通過後，才以 repository 自有 UITEST runner 執行
`rust-7z-virtual-folder-headful`。
