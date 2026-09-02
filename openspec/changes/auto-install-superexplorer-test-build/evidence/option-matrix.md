# 選項與入口矩陣

- `build_test_install.bat`預設：顯式`--auto-install`，建置、發布、同步`/S`安裝、三檔hash、啟動installed app。
- `--no-launch`：建置與發布後結束，不安裝、不查installed hash、不啟動。
- `--check`：只驗證工具、layout與admission，不建置、不發布、不安裝、不啟動。
- `--skip-build`：重用release輸入，仍發布、同步安裝、hash及啟動。
- `build_install.bat`與`build_desktop_test_install.bat`未傳`--auto-install`；parser亦拒絕在`all`或`superdesktop` component使用。
