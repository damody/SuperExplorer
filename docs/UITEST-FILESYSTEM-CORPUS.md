# 檔案系統 UITEST 測試語料與執行方式

這組測試只在 runner 擁有的 `target/uitest-runs/.../evidence/<case>/fixture` 建立資料，結束時會檢查路徑 containment 後再清理，不會修改 C:\、D:\ 根目錄或使用者原有檔案。

## 測試範圍

- `filesystem-corpus-contract`：兩次產生相同語料並比對，涵蓋空資料夾、巢狀資料夾、空檔、一位元組檔、重複內容、同大小不同內容、唯讀／隱藏／系統屬性、繁體中文、日文、韓文、emoji、組合字元與長路徑。
- `filesystem-corpus-headful`：以真實程式列舉語料，切換小／中／大圖示、清單、詳細資料，驗證 Unicode、搜尋結果更新與 18 層深路徑導覽。
- `mutation-safety-matrix`：以磁碟狀態作 oracle，驗證 F2 Unicode 改名、Shift 連選、Ctrl 切換、Ctrl+A、Ctrl+C/V、Ctrl+X/V、Backspace、Delete 與 F5。
- `ntfs-semantics-interop`：驗證 hard link、ADS、junction 與循環防護；建立 symbolic link 需要權限時會明確記為 SKIP。
- `large-directory-cancel-cache`：2,000 筆目錄、連續八次 F5、冷／暖啟動、快取損壞恢復及記憶體／handle／thread 上限。
- `large-directory-cancel-cache-soak`：20,000 筆版本，只在 `soak` suite 執行。
- `toolbar-extensions-tortoisegit-refresh`：驗證「其它／擴充功能」順序、popup 互斥、TortoiseGit 安裝狀態、更新命令、重新送出 Shell icon 請求後的畫面收斂，以及導覽／選取不變。

## 執行

```powershell
# 預設執行 quick、full、interop、visual，並產生當日 UTIT-YYYY-M-D.log
.\UTIT.bat

# 只跑單一案例
.\UTIT.bat --case filesystem-corpus-contract
.\UTIT.bat --case mutation-safety-matrix

# 額外跑 20,000 筆 soak
.\UTIT.bat --case large-directory-cancel-cache-soak

# 只檢查 manifest 與 OpenSpec 需求覆蓋
cargo run -p explorer-uitest -- --validate-only
```

## LuaFileSystem Unicode

語料產生器固定使用 `D:\test\build\tools\lua\lua.exe` 與同目錄的 `lfs.dll`。此 LFS 版本在 Windows 將 Lua UTF-8 路徑轉為 UTF-16，支援 Unicode 檔名、目錄、屬性、link 與長路徑。重建方式：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build_luafilesystem_utf8.ps1
& .\build\tools\lua\lua.exe .\scripts\test_luafilesystem_utf8.lua
```

## 證據與失敗資訊

每個案例會輸出 `report.json`，GUI 案例另有 PNG；語料案例會保留 `fixture-manifest.json`，但成功後刪除 fixture。runner 的 `report.json`、`summary.md`、`junit.xml`、stdout/stderr 會集中在當次 run 目錄。

`UTIT.bat` 最後呼叫 Lua reporter：成功案例在根目錄的當日 `UTIT-YYYY-M-D.log` 只列標題；失敗案例記錄命令、exit code、timeout、缺少 artifact、stdout/stderr 與 runner 診斷，檔尾包含 PASS／FAIL／SKIP／總數統計。
