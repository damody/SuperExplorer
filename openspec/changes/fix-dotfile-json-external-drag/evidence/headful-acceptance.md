# Headful驗收

- 已安裝版ADB：`build/dotfile-drag-adb-installed-final-rerun/report.json`，1/1通過；來源basename、34,629 bytes與SHA-256均寫入報告，遠端副本已由受控cleanup移除。
- SFTP：`build/dotfile-drag-sftp-after/report.json`，真實Explorer左鍵Copy 1/1通過。已安裝版亦建立精確同名遠端檔；provider probe下載後逐位元比較為34,629 bytes，驗證成功才刪除。
- 最新release的`SuperExplorer.exe`、broker及worker已複製到既有安裝目錄，三者與`target/release` SHA-256相符；舊版備份位於`build/installed-superexplorer-backup-20260901-1533`。
- 已安裝版SFTP runner偶發在第二個視窗取得網址列或虛擬化dotfile列時發生UIA競態；遠端provider oracle已獨立證明傳輸內容，故不將runner UIA競態誤列為產品傳輸失敗。
- 兩次操作均為Copy，本機來源仍存在且hash未變；ADB與SFTP遠端測試副本均已清理。
