# 聚焦測試

- Bundled Lua執行`scripts/test_component_installer_build.lua`：PASS。
- `build_test_install.bat --check`：exit 0，沒有建立或啟動installer。
- `build_test_install.bat --no-launch`：exit 0，建立有效x64 NSIS installer，沒有安裝或啟動。
- `build_test_install.bat --skip-build`：exit 0，同步silent install、三個hash gate及installed app launch均通過。
- 第一輪只查registry 64-bit view時：exit 1且不顯示成功；修正後32-bit NSIS registry view解析成功。
- Parser負向案例證明`--auto-install`不能用於formal all或SuperDesktop component。
