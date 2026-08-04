# Rust 資料夾大小視覺欄範例

這個最小範例以 Rust 與 `abi_stable` 提供資料夾大小欄位。建置與 ABI
contract 都在離線、鎖定依賴的環境執行；完整的產品 UI 驗證由專案 UITEST
流程負責。

```powershell
cargo build --manifest-path sdk/fixtures/rust-folder-size-visual-column/Cargo.toml --target x86_64-pc-windows-msvc --release --locked --offline
```

完成且穩定的精確值最多快取 256 筆，位置為
`%LOCALAPPDATA%\RustGpuiExplorer\plugins\rust-folder-size-visual-column\folder-size\v1`。
目錄修改日期或計量限制改變時會重新在背景掃描；partial/error 不會寫成精確快取。
