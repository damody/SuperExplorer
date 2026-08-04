# Rust EXIF 重新命名命令

EXIF/TIFF reader 靜態連結於 `plugin.dll`，不依賴 exiftool、外部 DLL、PATH 或網路。預覽會區分像素尺寸與密度、拒絕缺少 tag 與不分大小寫碰撞、清理 Windows basename，再交由 host 執行具 identity recheck 與 undo 的 rename plan。
