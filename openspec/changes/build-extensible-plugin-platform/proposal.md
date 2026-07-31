## Why

SuperExplorer 目前缺少第三方作者可穩定建置、載入及整合的擴充平台；詳細資料欄位、檢視模式、命令、設定與虛擬資料夾仍依賴封閉的內部型別。若沒有先固定 Rust／GPUI／ABI 環境並以完整官方範例驗證公開接口，作者製作的 DLL、Lua 套件與 UI contribution 將無法可靠整合或重現。

## What Changes

- 建立統一 `.sepack` 套件、manifest、作者聯絡資料、capability 權限、套件解析、feature desired/effective state、runtime gate、診斷及 Safe Mode。
- 建立 Rust Plugin SDK：以單一 `abi_stable` root module 註冊多個功能，並為直接使用 GPUI 的 UI 外掛提供精確 toolchain／dependency fingerprint。
- 建立 P0-0 SDK snapshot 流程：固定 Rust `1.97.1` 與 `abi_stable 0.11.3`，開發期從 `damody/gpui-ce-explorer/main` 解析精確 commit、測試後發布不可變 snapshot；正式 Release 凍結 commit、Cargo.lock、vendor tree 與 bundle ID。
- **BREAKING**：將固定的詳細資料欄位、檢視模式與相關 session persistence 改造成動態 registry，使外掛欄位與 GPUI view mode 可排序、選取、保存與安全 fallback。
- 建立有界背景工作、typed value、增量結果、取消、快取、refresh generation、UI batching 與慢 callback 診斷。
- 建立 Rust／Lua 共用的命令、擴充按鈕、參數表單、operation preview、衝突處理、取消與 undoable operation plan。
- 擴充 Lua registrar，加入欄位、命令、表單與受控 bundled executable 執行；外部工具必須隨 `.sepack` 封裝並通過 hash／target／授權驗證。
- 建立受限唯讀的 Windows Lock Owner host service、可修改 7z 的 Virtual Folder／Stream／Mutation 介面，以及可供 Size Map 使用的動態 View Mode／Directory Tree Scan 介面。
- 在「資料夾選項」加入「擴充功能」分頁，支援全域、套件與個別 feature 開關、作者聯絡方式、blocked/faulted/pending-restart 狀態及正確 Apply／OK／Cancel 語意。
- 建立純資料 Skin schema 與 loader，支援圖片／圖示／字型、按鈕狀態、不規則視覺外框、透明背景／點穿遮罩及逐項安全 fallback。
- 交付八個可從獨立 consumer workspace 建置、測試、修改及封裝的完整原始碼官方範例，作為 stable SDK 的必要 release gate。
- 第一階段不實作 Steamworks、Workshop 上傳下載、DLC entitlement、付費與分潤；只保留 Package Source／Entitlement Provider 抽象接點。

## Capabilities

### New Capabilities

- `extension-package-and-feature-lifecycle`: `.sepack`、manifest、套件來源／解析、feature 狀態、capability、載入、停用、診斷與 Safe Mode。
- `rust-plugin-abi-and-ui-toolchain`: Rust root module、`abi_stable` 邊界、GPUI contribution、P0-0 snapshot bundle、fingerprint、相容與 Release freeze。
- `extension-jobs-values-and-dynamic-columns`: 外掛工作排程、typed values、增量結果、取消、快取、動態詳細資料欄位、排序與 GPUI cell renderer。
- `extension-commands-forms-and-operation-plans`: 命令、按鈕、typed form、操作預覽、驗證、執行、取消、衝突與復原。
- `lua-extension-registrar-and-tool-execution`: Lua 欄位／命令／表單 registrar、capability enforcement、bundled tool resolver 與受控子程序生命週期。
- `lock-owner-host-service`: 透過受限公開服務查詢檔案鎖定程序、短 TTL、F5 refresh generation 與 stale-result rejection。
- `virtual-folder-stream-and-mutation`: 虛擬位置、檔案 stream、7z 導覽／預覽／複製／修改、staging transaction、密碼與資源限制。
- `extension-view-modes-and-directory-tree-scan`: 動態檢視模式、GPUI view renderer、遞迴目錄樹 delta、共享選取、正式導覽、F5 與 fallback。
- `extension-options-management`: 「擴充功能」選項頁、動態 catalog、desired/effective state、套用交易、feature drain 與重新啟動提示。
- `extension-skin-customization`: 純資料 Skin schema、資產驗證、按鈕狀態、不規則視覺外框、透明背景、hit-test mask、可存取性與 fallback。
- `source-example-plugin-suite`: 八個完整官方範例、獨立 consumer build、文件、fixture、測試、CI、`.sepack` 與 SDK release gate。

### Modified Capabilities

無；repository 目前沒有 `openspec/specs/` baseline capability。本 change 會建立上述新規格，並在實作時遷移既有內部行為。

## Impact

- 新增公開 SDK／host crates，例如 `explorer-extension-api`、`explorer-extension-ui-api` 與 `explorer-extension-host`。
- 修改 `explorer-ui` 的詳細資料欄位、檢視模式、資料夾選項、session persistence、navigation 與 GPUI composition root。
- 擴充 `explorer-jobs`、`explorer-automation`、`explorer-model`、`explorer-shell-win` 及檔案操作／undo pipeline。
- 將宿主與 SDK 統一到 Rust `1.97.1`、`abi_stable 0.11.3` 及同一個 `damody/gpui-ce-explorer` snapshot fingerprint。
- 新增 UI Plugin SDK bundle、offline vendor、canonical lock、AI prompt、build／validate／package scripts與八個官方範例 workspace。
- 增加原生 DLL 的程序內風險面，因此必須加入載入前驗證、callback guard、runtime gate、資源配額與 Safe Mode；Rust DLL 不支援執行期熱卸載。
