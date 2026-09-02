# 安裝版Explorer拖放驗收

來源`D:\SuperExplorer\.tmp-full-meta.json`為34,629 bytes，SHA-256 `5887181648411FBCE53C6B8AB4E4945C7B0A37B7A9611AC41F3C22F01A66F009`。

- ADB：`build/auto-install-drag-adb-final/report.json`，真實Windows Explorer左鍵Copy 1/1通過，installed executable為`C:\Program Files\SuperExplorer\SuperExplorer.exe`，console出現terminal `DropExternal`；ADB oracle確認成功並已清除精確遠端副本。
- SFTP：`build/auto-install-drag-sftp-final3/report.json`，真實Windows Explorer左鍵Copy 1/1通過；interactive provider oracle下載逐位元比對成功，回報`bytes=34629`後才刪除精確遠端副本。

兩次Copy後本機來源仍存在且大小／SHA-256未變。SFTP前兩輪分別揭露runner的terminal gesture與UIA網址列競態；未通過remote oracle的輪次沒有被當成成功證據。
