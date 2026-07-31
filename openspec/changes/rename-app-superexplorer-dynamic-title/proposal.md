## Why

目前安裝程式與文件已使用 `SuperExplorer`，但 Cargo 產物、Windows VERSIONINFO、腳本與測試仍以 `explorer-app.exe` 為主，造成產品識別不一致。視窗及工作列也需要可靠呈現作用中分頁的目前位置，讓多分頁導覽時能立即辨識正在操作的路徑。

## What Changes

- 將主程式 Cargo binary、Windows 檔案資訊、封裝輸入及所有 production-binary 查找統一為 `SuperExplorer.exe`。
- 保留 `explorer-app` composition-root package、內部 `explorer-*` crate 名稱與既有持久化資料路徑，維持程式碼及使用者資料相容性。
- 視窗建立、成功導覽、session restore 與作用中分頁切換後，將視窗／工作列標題更新為作用中檔案系統完整路徑。
- 對無檔案系統路徑的 Shell 虛擬位置使用可讀顯示名稱，缺少名稱時安全回退為 `SuperExplorer`。
- 更新 installer、交付腳本、headful smoke、UITEST manifest、README 與證據文件，並加入防止退回舊執行檔名稱或錯誤分頁標題的測試。

## Capabilities

### New Capabilities

- `superexplorer-product-identity`: 定義主程式對外名稱、執行檔與 Windows metadata，以及由作用中分頁位置驅動的動態視窗／工作列標題契約。

### Modified Capabilities

<!-- 無既有 canonical spec 的 requirement 需要修改；既有未封存 change 文件僅作相容性參考。 -->

## Impact

影響 `explorer-app` binary target、resource VERSIONINFO、GPUI 視窗生命週期與 UI 狀態接線、Restart Manager 自我辨識、NSIS／finalize scripts、headful smoke、UITEST manifest、README 與交付文件。不新增第三方 dependency，不重新命名內部 crates、helper executables、session schema 或 `%LOCALAPPDATA%\RustGpuiExplorer` 相容資料根目錄。
