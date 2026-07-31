## 1. 建置產物與 Windows 產品識別

- [x] 1.1 在 `crates/explorer-app/Cargo.toml` 宣告名稱為 `SuperExplorer`、路徑仍為 `src/main.rs` 的明確 binary target，確認 `explorer-app` package 與 lib/test target 名稱不變。
- [x] 1.2 更新 `crates/explorer-app/app.rc`，將 FileDescription、ProductName、InternalName 與 OriginalFilename 統一為 `SuperExplorer`／`SuperExplorer.exe`，保留既有版本欄位與 manifest resource。
- [x] 1.3 新增或更新 Windows artifact metadata 測試，直接讀取建置產物 VERSIONINFO 並逐欄驗證產品名稱、描述、內部名稱及原始檔名。
- [x] 1.4 執行 `cargo metadata` 與 debug build，證明 package selector 仍是 `explorer-app`，主程式只輸出並可啟動 `target\debug\SuperExplorer.exe`。

## 2. 作用中位置標題投影

- [x] 2.1 盤點 `TabState` history、`LocationDescriptor`、navigation display title 與 session restore 的資料來源，建立不讀取 address editing buffer 的純標題投影函式。
- [x] 2.2 對 filesystem location 回傳完整 Windows 路徑，補齊 drive root 反斜線、Unicode、UNC 與長路徑不被截斷的處理。
- [x] 2.3 對 Shell 虛擬位置回傳 trim 後的可讀 display title，空白、缺值或僅有內部 parsing identity 時回退 `SuperExplorer`。
- [x] 2.4 加入標題投影單元測試，涵蓋 `C:\`、`D:\test`、Unicode／UNC 路徑、本機虛擬位置、空 display title 與未成功網址輸入。
- [x] 2.5 加入多分頁狀態測試，驗證切換及關閉作用中分頁會投影新位置，背景分頁 terminal event 不會改變標題。

## 3. GPUI 視窗與工作列同步

- [x] 3.1 將初始 `WindowOptions` 固定舊產品字串改為正確的初始位置標題；模型尚未建立時使用 `SuperExplorer` fallback。
- [x] 3.2 在 Explorer root view 的 window update 路徑比較 projected title 與 last-applied title，只在改變時呼叫 `Window::set_window_title`。
- [x] 3.3 確認成功導覽、back／forward／up、網址列提交、session restore、切換／新增／關閉分頁及目前位置重新命名後皆會觸發標題重新投影。
- [x] 3.4 加入 GPUI test context 測試，使用 `window_title()` 驗證初始化、跨分頁切換、背景事件及失敗導覽後的 native title。
- [x] 3.5 加入跨 C:/D: 隔離資料夾的 headful UI Automation case，驗證實際 HWND／工作列標題等於作用中完整路徑，並記錄失敗時的預期與實際值。

## 4. 封裝、腳本與測試執行器遷移

- [x] 4.1 更新 `finalize_windows_artifact.ps1` 的 build artifact、staging、manifest extraction 與 OriginalFilename 驗證為 `SuperExplorer.exe`。
- [x] 4.2 更新 NSIS 輸入契約，直接封裝 Cargo 產生的 `SuperExplorer.exe`，驗證安裝檔、捷徑、DisplayIcon、完成頁啟動與 uninstall ownership 全部使用新檔名。
- [x] 4.3 盤點並更新所有會查找或啟動 production UI 的 smoke／capture／roadmap scripts；保留 Cargo 指令中的 `-p explorer-app`，錯誤訊息改報新產物路徑。
- [x] 4.4 更新 `uitest/manifest.json` 與 runner fixtures 的 debug／release prerequisites 及 process launch 路徑，確保重跑單一 case 的命令仍有效。
- [x] 4.5 加入靜態 regression test，禁止 production binary path、installer input 或 headful prerequisite 再引用 `explorer-app.exe`，但允許內部 package、crate 路徑與舊程序 denylist 相容值。
- [x] 4.6 更新 Restart Manager 自我 owner 測試，確認大小寫不敏感地拒絕 `SuperExplorer.exe` 與 legacy `explorer-app.exe`，不放寬其他 owner 的安全政策。

## 5. 文件、相容性與完整驗證

- [x] 5.1 更新 README 三種語言、`docs/FINAL_HANDOFF.md`、`docs/UITEST.md` 與相關執行／建置範例，使 executable 路徑使用 `SuperExplorer.exe`、Cargo package 命令仍使用 `explorer-app`。
- [x] 5.2 新增相容性測試或明確斷言，確認 session、search index、icon cache、thumbnail cache 仍解析至 `%LOCALAPPDATA%\RustGpuiExplorer`，沒有因改名建立新的空白資料根。
- [x] 5.3 執行 `cargo fmt --check`、受影響 packages 的單元／integration tests、`cargo test -p explorer-app`、Windows resource validation 與 debug headful title smoke。
- [x] 5.4 建置 release artifact 並執行 finalize／installer smoke，確認安裝後 `SuperExplorer.exe` 可啟動、工作列標題可更新且 uninstall 完整移除 owned 主程式。
- [x] 5.5 執行與本變更相關的 UITEST cases，保存跨磁碟標題、package identity、VERSIONINFO、installer 與相容路徑證據；若整套測試存在無關失敗，逐項記錄但不得誤標本變更完成。
- [x] 5.6 對照 `superexplorer-product-identity` 每個 scenario 完成 requirement-to-test traceability，確認沒有 placeholder、舊 product string 或未追蹤的 `explorer-app.exe` production consumer。

