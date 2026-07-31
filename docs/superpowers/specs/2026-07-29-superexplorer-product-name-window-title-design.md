# SuperExplorer 產品改名與動態視窗標題設計

## 目標

將 Windows 對外可見的應用程式名稱與執行檔統一為 `SuperExplorer`，並讓視窗標題及工作列顯示目前作用中分頁正在瀏覽的位置。內部 Rust crate 與既有使用者資料目錄維持相容，避免純改名造成大範圍 API 變動或遺失現有設定、索引及圖示快取。

## 範圍

- Cargo 仍以 `explorer-app` 作為 composition-root package，但明確宣告輸出 binary 為 `SuperExplorer.exe`。
- Windows VERSIONINFO 的產品名稱、檔案描述、內部名稱與原始檔名改為 `SuperExplorer`／`SuperExplorer.exe`。
- 視窗建立、成功導覽及作用中分頁切換時，標題更新為作用中位置：檔案系統位置使用完整 Windows 路徑，Shell 虛擬位置使用可讀顯示名稱。
- 安裝、封裝、啟動、視覺驗證、headful smoke 與 UITEST 對 production binary 的查找統一改為 `SuperExplorer.exe`。
- README 與交付文件的使用方式及輸出路徑同步更新；需要指定 Cargo package 時仍使用 `cargo run -p explorer-app`。

## 相容性界線

- 不重新命名 `crates/explorer-app`、其他 `explorer-*` crates 或 Rust 公開模組。
- 不遷移 `%LOCALAPPDATA%\RustGpuiExplorer`、暫存測試根目錄、session schema 或 Shell parsing name；這些是持久化相容識別，不是使用者可見產品名稱。
- 不改名 helper/broker binaries，除非既有封裝契約要求；主程式識別與 helper protocol 保持分離。
- Restart Manager 與自我程序辨識同時接受新舊主程式檔名，確保開發產物或舊版程序不會被誤當成可關閉的外部 owner。

## 動態標題資料流

作用中分頁是唯一標題來源。應用程式啟動時先以初始分頁位置設定標題；導覽完成或切換作用中分頁後，由 UI 狀態產生新的標題文字並透過 GPUI window API 套用。背景分頁載入完成不得覆寫標題。重新命名目前資料夾、session restore 或 location display-name 更新後，也必須重新投影作用中位置。

檔案系統路徑優先使用可複製的完整路徑（例如 `D:\test`），drive root 保留反斜線（例如 `C:\`）。無檔案系統路徑的 Shell 位置使用目前 breadcrumb／tab 已有的顯示名稱；若模型尚未提供名稱，才回退到 `SuperExplorer`，不得顯示空白標題或內部 parsing token。

## 驗證

- Cargo metadata／build 證明 package 仍為 `explorer-app`，輸出為 `target\debug\SuperExplorer.exe` 與 release 對應檔。
- VERSIONINFO 測試驗證 `ProductName`、`FileDescription`、`InternalName`、`OriginalFilename`。
- UI/model 單元測試涵蓋初始位置、導覽、分頁切換、背景分頁事件及虛擬位置 fallback。
- headful UI Automation 測試在至少 C:、D: 隔離測試資料夾間切換，驗證 NativeWindowHandle／工作列所見標題與作用中完整路徑一致。
- 封裝、installer smoke 與 UITEST manifest 不再依賴 `explorer-app.exe`。

