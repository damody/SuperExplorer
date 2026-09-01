# SuperExplorer 測試安裝器自動更新設計

## 問題

`build_test_install.bat`目前會建置release、封裝NSIS安裝器並以非同步方式開啟安裝GUI，但批次檔在安裝完成前就回報成功。使用者若未完成GUI安裝，系統仍會執行舊版`SuperExplorer.exe`，因此已在工作樹修正的Explorer外部拖放行為不會出現在實際安裝版。

## 核准行為

不帶參數執行`build_test_install.bat`時，流程必須依序完成：

1. 從目前工作樹建置及驗證SuperExplorer release輸入。
2. 產生SuperExplorer測試NSIS安裝器。
3. 以NSIS silent模式同步執行本次產生的安裝器。
4. 等待安裝器結束並保留其退出碼。
5. 比對安裝目錄中SuperExplorer、extension broker與extension worker和本次release輸入的SHA-256。
6. 僅在安裝成功且所有必要雜湊相符時回報成功。
7. 啟動已驗證的安裝版SuperExplorer。

`--no-launch`維持既有「只建置及封裝，不安裝、不啟動」語意。`--check`維持只檢查工具與輸入。`--skip-build`仍可重用已存在的release輸入，但後續安裝及hash gate不變。

## 元件邊界

- `build_test_install.bat`只負責SuperExplorer測試入口、參數轉送及最終使用者訊息。
- `build/build_install.lua`負責建置、封裝，以及依選項選擇同步安裝或只發布。
- 共用process helper提供同步子程序執行；不得用detached `start`判定安裝成功。
- 安裝後驗證集中在可測試的Lua helper，解析NSIS所使用的安裝位置並逐一比較必要binary hash。
- 正式combined installer與SuperDesktop-only測試入口不改為自動安裝。

## 失敗與安全語意

安裝器不存在、silent install退出碼非零、安裝檔不存在、hash不符或最新版啟動失敗都必須讓批次檔回傳非零退出碼，且訊息指出失敗階段與檔案名稱。不得在驗證失敗時顯示成功。安裝仍由既有NSIS邏輯協調正在執行的SuperExplorer與MFT service，不以直接覆寫檔案繞過安全處理。

## 驗證

聚焦驗證包括：選項解析及分支測試、silent同步安裝退出碼、release／installed三個binary雜湊相符、批次檔成功訊息正確，以及使用`D:\SuperExplorer\.tmp-full-meta.json`從Windows Explorer拖入`adb://emulator-5554/sdcard/Download`與`sftp://45.32.49.125/home/linuxuser`。ADB與SFTP均以遠端內容oracle驗證basename、34,629 bytes和SHA-256，完成後只清理由本次測試證明所有權的副本。不執行完整迴歸。

## 回復方式

程式碼可回復為只發布並開啟互動式安裝器；已安裝檔案由既有NSIS安裝／解除安裝機制管理。任何測試遠端副本仍採精確名稱及內容驗證後清理。
