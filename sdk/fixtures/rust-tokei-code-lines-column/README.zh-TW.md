# Rust tokei Code lines 欄位範例

這個範例以 `tokei = 14.0.0` 靜態連結實作 Code lines 欄位，不啟動外部
程式。主機提供受限制的檔案串流，外掛以公共 cell render context 繪製
程式碼行數與比例條，並可顯示 comments、blanks 與 total 詳情。

依賴版本直接固定在 `Cargo.toml`；第三方來源不會提交到 repository。離線
建置直接使用 Cargo 標準本機 registry cache 與 `--locked --offline`。
