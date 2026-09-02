# 根因

指定JSON並非因前導點或`.json`被產品邏輯拒絕。以目前debug/release二進位執行真實Windows Explorer OLE拖放時，ADB與SFTP均進入共用external-drop dispatch並建立內容正確的遠端檔案。

使用者原先啟動的安裝版`C:\Users\Damody\AppData\Local\Programs\SuperExplorer\SuperExplorer.exe`時間戳仍為2026-08-05，缺少先前完成的OLE terminal事件修正。`build_test_install.bat`只保證建置、封裝及預設啟動互動式安裝器，不能保證安裝器已完成覆蓋。因此可見失敗的第一層是部署版本落後，不是dotfile basename、ADB或SFTP provider分支。

本次仍補強共用來源準備：非絕對、無filename或已消失來源會在dispatch前以具體原因fail closed；合法`.tmp-full-meta.json`使用平台filename語意原樣保留basename，不加入副檔名或provider特判。
