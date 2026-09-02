# 基線

舊流程由`build_test_install.bat`轉送SuperExplorer component後，`build_install.lua`以detached `process.start`開啟NSIS GUI，父流程不等待安裝完成即顯示成功。NSIS test installer使用`$PROGRAMFILES64\SuperExplorer`，並將`InstallDir`寫入32-bit registry view的`HKLM\Software\SuperExplorer`；silent參數為`/S`。

實作後真實安裝前的第一輪resolver只查64-bit view，因此在registry查詢gate明確失敗；這證明流程不會在無法確認安裝位置時假成功。resolver已依NSIS實證修正為依序查64與32-bit view。
