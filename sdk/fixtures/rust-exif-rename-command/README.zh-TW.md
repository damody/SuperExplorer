# Rust EXIF 重新命名命令

這個 public SDK example 將 in-process TIFF/EXIF reader 靜態連結進
`plugin.dll`，透過 host stream contract 讀取 bytes。它會區分 pixel
dimensions 與 density、展開文件化 token、清理 Windows basename、拒絕
缺少 tag 與不分大小寫 collision，最後提交會重驗 identity、可 undo 的
rename plan。它不使用 exiftool、外部 DLL、PATH、network 或 private crate。

使用本目錄作為 `PluginRoot` 執行標準 offline test/validate/build/package。
新增文件化 tag 時修改 `parse_tiff` 與 `render_pattern`；preview 必須保持
side-effect free。dependency 變更必須使用精確 Cargo 版本、重建
`Cargo.lock` 並更新 provenance/SBOM/license inventory。完整 example gate
後執行本機 `rust-exif-rename-headful` 與
`extension-command-interaction-headful`；CI 不作為驗收路徑。
