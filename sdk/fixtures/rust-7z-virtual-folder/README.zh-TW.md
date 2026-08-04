# Rust 7z 虛擬資料夾

純 Rust 安全核心拒絕絕對路徑、parent traversal、NUL、正規化碰撞與資源炸彈；stream 有界。mutation 使用同磁碟 staging、重開驗證、原檔 identity recheck、atomic replace 與整個 archive undo。密碼只使用短生命週期 secret handle，不序列化或寫入 log。
