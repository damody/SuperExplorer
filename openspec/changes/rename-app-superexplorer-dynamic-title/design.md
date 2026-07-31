## Context

主程式的 Cargo package 名稱是 `explorer-app`，預設 binary 因而輸出為 `explorer-app.exe`；但 NSIS、README 標題與部分產品文件已採用 `SuperExplorer`。Windows resource 仍宣告舊的 Rust GPUI Explorer 名稱，許多 smoke／UITEST 也直接拼接舊執行檔路徑。GPUI 初始 `TitlebarOptions` 使用固定文字，導覽及分頁狀態改變時沒有投影至 native window title。

這是橫跨 build metadata、Windows resource、UI/model、封裝腳本與 headful 測試的變更。工作區同時可能有使用者修改，因此實作必須限制在命名／標題相關檔案，不能機械式改寫所有 `explorer-app` 字串。

## Goals / Non-Goals

**Goals:**

- 讓 Cargo-built 與 packaged 主程式皆命名為 `SuperExplorer.exe`。
- 讓 Windows 產品資訊一致顯示 `SuperExplorer`。
- 讓 native window／工作列標題始終對應作用中分頁目前位置。
- 更新 production binary consumers、驗證腳本與文件，避免新舊名稱漂移。
- 保持內部 crate、持久化資料與 helper protocol 相容。

**Non-Goals:**

- 不重新命名 `explorer-app` package、crate 目錄或其他 `explorer-*` packages。
- 不遷移 `%LOCALAPPDATA%\RustGpuiExplorer`、session schema、icon/search/thumbnail caches 或 Shell parsing names。
- 不重新命名 extension broker、worker 或測試 helper binaries。
- 不在標題後附加產品名稱；檔案系統位置的標題只顯示完整路徑。

## Decisions

### 1. 以明確 Cargo binary target 分離 package 與 executable 名稱

在 `explorer-app/Cargo.toml` 宣告 `[[bin]] name = "SuperExplorer"` 並指向既有 `src/main.rs`。這讓依賴、`cargo test -p explorer-app` 與 architecture contract 保持不變，同時使 build artifact 原生產生正確檔名。相較重新命名 package，此方案不會擴散到 lockfile package identity、integration-test imports 及數百個指令；相較封裝時才改名，也能讓 debug/headful 測試驗證與實際產品相同的 binary identity。

### 2. 標題由作用中模型狀態純函式投影

在 UI/model 邊界提供可測試的 `active_window_title` 投影：目前 history location 為 filesystem 時回傳正規完整路徑；Shell 虛擬位置回傳非空 `display_title`；兩者皆不可用時回退 `SuperExplorer`。標題不讀取 address editing buffer，避免使用者輸入尚未導覽成功的文字污染工作列。

Root view 在 render/update 生命週期比較前次已套用標題，只在文字實際改變時呼叫 GPUI `Window::set_window_title`。因此新增／切換／關閉分頁、成功導覽、session restore 與 display metadata 更新都由同一狀態投影收斂；背景分頁完成事件因未改變 active projection，不會覆寫標題。初始 `WindowOptions` 使用初始位置可得的標題；若建立視窗前尚無模型則以 `SuperExplorer` 作短暫 fallback。

### 3. Windows metadata 與 consumer 路徑一次切換

`app.rc` 的 FileDescription、InternalName、OriginalFilename、ProductName 改為新值。finalize script 驗證新 binary、PE metadata 及 manifest；installer 直接接收 `SuperExplorer.exe`，不再依賴 `/oname` 將舊檔重新命名。所有會啟動 production UI 的 scripts／UITEST prerequisite 改用新路徑，但 Cargo package selector 保留 `-p explorer-app`。

### 4. 相容識別保留雙名稱防護

Restart Manager 的自我程序 denylist 同時辨識 `superexplorer.exe` 與 `explorer-app.exe`，以防舊版或尚未清理的開發程序被視為外部 lock owner。持久化路徑仍維持 `RustGpuiExplorer`，不做一次性搬移或雙寫；這可避免改名導致 session、索引及圖示快取消失。

## Risks / Trade-offs

- [Cargo binary 名稱變更使舊腳本找不到產物] → 全域盤點實際 binary consumers，加入禁止 production 路徑殘留 `explorer-app.exe` 的靜態測試。
- [render 期間重複設定標題造成無效 native 呼叫] → 保存 last-applied title，只在 projection 改變時呼叫 GPUI。
- [背景分頁事件覆寫標題] → 標題只從 `active_tab_id` 對應狀態計算，不直接採用事件 payload。
- [虛擬位置暴露內部 parsing name] → 僅接受非空 display title，否則回退 `SuperExplorer`。
- [外部使用者仍依賴舊 debug 檔名] → 視為有意的產品 artifact 改名；文件與錯誤訊息提供新路徑，內部 Cargo package 命令不變。

## Migration Plan

1. 加入新 binary target 與 Windows VERSIONINFO，先讓 Cargo 產出 `SuperExplorer.exe`。
2. 實作及單元測試標題投影與 GPUI window 同步。
3. 更新 finalize、installer、smoke、UITEST 與文件 consumers。
4. 執行 debug/release build、targeted tests、VERSIONINFO validation 與跨 C:/D: headful title smoke。
5. 若需要回滾，可還原 binary target 與 consumers；沒有資料 schema 或持久化目錄需要反向遷移。

## Open Questions

無。作用中完整檔案系統路徑、虛擬位置顯示名稱、內部 crate／資料路徑保留均已核准。

