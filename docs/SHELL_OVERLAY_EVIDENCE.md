# Windows Shell overlay 實機證據

## 實作契約

檔案與資料夾圖示由 Windows Shell `SHGetFileInfoW` 取得，並指定
`SHGFI_ICON | SHGFI_ADDOVERLAYS | SHGFI_OVERLAYINDEX`。程式不自行猜測 Git
狀態或重畫 TortoiseGit badge；因此相同路徑、相同 DPI 與相同 Windows Shell
狀態下，應顯示 Explorer 實際選出的合成圖示。取得的 alpha-correct RGBA 先放入
bounded memory LRU，再寫入具 Windows build、DPI、theme、association generation 與
overlay generation 的版本化磁碟快取。

## 2026-07-27 本機註冊盤點

指令：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/audit_shell_icon_overlays.ps1 -OutputDirectory target/overlay-evidence/20260727-registry
```

證據：`target/overlay-evidence/20260727-registry/report.json`

- 共 18 個 `ShellIconOverlayIdentifiers`；報告保存 registry enumeration order、原始含空白名稱、CLSID、index 與 first-15 判定。
- OneDrive 佔 index 0–6；TortoiseGit Normal、Modified、Conflict、Locked、ReadOnly、Deleted、Added、Ignored 佔 index 7–14。
- TortoiseGit Unversioned 位於 index 15，超出 Windows overlay image-list 的 first-15 環境限制，報告明確標為 unavailable；程式不偽造替代 badge。

## 真實 Git working tree smoke

指令：

```powershell
cargo test -p explorer-shell-win real_tortoise_git_clean_modified_and_added_overlays_are_distinct -- --ignored --nocapture
```

證據：`target/overlay-evidence/20260727-registry/tortoise-git-smoke.log`

測試在 `D:\test\target` 下建立唯一專用 temporary Git repository，產生 clean、modified、staged added 與 unversioned 四種真實狀態，等待 Shell extension 收斂後，直接走 production Shell icon loader 讀取 32 px、168 DPI RGBA 並雜湊；RAII temporary directory 在測試結束後只清除該 fixture。

本機重跑觀察到 clean 與 modified/added 不同，四個輸入至少取得三種不同 Shell 合成 bitmap；installed TortoiseGit 對 staged added 與 modified 回傳同一合成 bitmap。這是同機 Explorer Shell provider 的實際結果，因此程式原樣保留，不能為了測試人造不同圖示。
