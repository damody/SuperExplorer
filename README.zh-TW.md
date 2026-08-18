# [**SuperExplorer**](https://github.com/damody/SuperExplorer)

五項 Explorer post-parity roadmap 的功能、安裝檔、資料路徑、測試證據與限制請見：[roadmap 最終交接](docs/POST_PARITY_ROADMAP_HANDOFF.md)。

[English](README.md) | [繁體中文](README.zh-TW.md) | [简体中文](README.zh-CN.md)

以 Rust 與 [GPUI-CE](https://github.com/gpui-ce/gpui-ce) 開發的 Windows 11 檔案總管。本專案結合原生 Windows Shell 整合層，以及參考 Windows 檔案總管設計的自訂 GPUI 介面。

> 本專案仍在積極開發中，僅支援 Windows，尚未涵蓋 Windows 檔案總管的所有 Shell 功能。

## 功能特色

- 支援分頁資料夾瀏覽，以及上一頁、下一頁、上一層、位址列與搜尋操作。
- 實際資料夾列舉、檔案系統監看、排序與多種檢視版面。
- 原生檔案操作，包括建立、重新命名、複製、移動、刪除、衝突處理、取消與復原日誌。
- 整合 Windows 剪貼簿、OLE 拖放、Shell 圖示、覆疊圖示與原生快顯功能表。
- 優先探測索引搜尋，無法使用時改用有界限的檔案系統搜尋。
- 支援淺色、深色與高對比主題、DPI 感知版面、鍵盤導覽、輸入法及 UI Automation 語意。
- 提供單元、整合、架構、視覺、協助工具、生命週期與 Windows 互通性驗證腳本。
- 強化 Windows 拖放行為，修正 OLE 拖放狀態轉換，降低高頻指標輸入時的互動抖動。

## 系統需求

- Windows 11 x64。
- 支援 submodule 的 [Git](https://git-scm.com/)。
- Rust `1.85.0` 以上版本，使用 `x86_64-pc-windows-msvc` 工具鏈。
- Visual Studio 2022 Build Tools 或 Visual Studio，並安裝「使用 C++ 的桌面開發」工作負載。
- 與 MSVC 工具鏈相容的 Windows SDK。
- 建議使用 PowerShell 7 執行驗證腳本。

## 開始使用

複製儲存庫並初始化 GPUI-CE submodule：

```powershell
git clone --recurse-submodules https://github.com/damody/file_explorer.git
cd file_explorer
```

若複製時未包含 submodule：

```powershell
git submodule update --init --recursive
```

建置並執行應用程式：

```powershell
cargo run -p explorer-app --locked
```

若要在啟動時開啟指定資料夾：

```powershell
$env:EXPLORER_INITIAL_PATH = 'D:\'
cargo run -p explorer-app --locked
```

## 發行版本建置

建置 Windows 執行檔，並完成資訊清單與版本資源處理：

```powershell
./scripts/finalize_windows_artifact.ps1 -Profile release
```

執行檔會輸出至 `target/release/SuperExplorer.exe`。

## 驗證

執行主要的儲存庫檢查：

```powershell
cargo run -p explorer-uitest -- --suite quick
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Windows 圖形介面與視覺檢查需要互動式桌面工作階段。常用進入點包括：

```powershell
./scripts/run_headful_validation.ps1 -SkipBuild -OutputDirectory target/headful-evidence/local
./scripts/capture_dpi_matrix.ps1 -OutputDirectory target/dpi-evidence/local
./scripts/check_architecture.ps1
./scripts/check_ui_tokens.ps1
```

完整流程請參閱[手動測試](docs/MANUAL_TESTS.md)與[視覺測試](docs/VISUAL_TESTING.md)。

## 工作區結構

| 路徑 | 職責 |
| --- | --- |
| `crates/explorer-app` | 應用程式啟動、Windows 前置需求與 GPUI 組合根節點 |
| `crates/explorer-common` | 共用診斷與錯誤型別 |
| `crates/explorer-jobs` | 背景工作協調 |
| `crates/explorer-model` | 導覽、操作、視窗與領域模型 |
| `crates/explorer-search` | 查詢剖析與搜尋引擎 |
| `crates/explorer-shell-win` | 原生 Windows Shell、剪貼簿、OLE、圖示與檔案操作 |
| `crates/explorer-ui` | GPUI 介面、狀態、版面、主題與互動 |
| `crates/explorer-test-support` | 共用測試固定資料與輔助工具 |
| `vendor/gpui-ce` | 固定版本的 GPUI-CE Git submodule |
| `scripts` | 建置、冒煙測試、互通性、協助工具與視覺驗證腳本 |
| `docs` | 狀態、證據、測試指南與實作說明 |

## 專案文件

- [目前狀態](docs/STATUS.md)
- [最終交接與已知缺口](docs/FINAL_HANDOFF.md)
- [同等性矩陣](docs/PARITY_MATRIX.md)
- [檢查點證據](docs/CHECKPOINT_EVIDENCE.md)
- [實作計畫](docs/IMPLEMENTATION_PLAN.md)

## 已知限制

- 應用程式目前僅支援 Windows。
- 現階段以檔案系統為主；完整 Shell 命名空間、縮圖與預覽處理常式，以及由 Broker 隔離的第三方擴充仍屬後續強化項目。
- 部分 OLE 拖放、混合 DPI、朗讀程式與 Explorer 到應用程式的情境，需要在實際的互動式 Windows 桌面手動驗證。
- 搜尋可用性與行為取決於 Windows Search 設定；索引搜尋無法使用時，應用程式會採用有界限的後備搜尋。

詳細驗證狀態與剩餘缺口請參閱[最終交接文件](docs/FINAL_HANDOFF.md)。

## 授權與貢獻

SuperExplorer 是**專有、Source Available（原始碼可檢視）軟體**，不是開源軟體。您僅能依下列適用文件檢視原始碼、準備貢獻或開發相容插件；禁止未經授權重新散布、發布修改後的核心版本或商業利用核心程式。

- [終端使用者授權協議](docs/EULA.zh-TW.md)
- [Plugin SDK 授權條款](docs/PLUGIN-SDK-LICENSE.zh-TW.md)
- [貢獻指南](docs/CONTRIBUTING.zh-TW.md)
- [貢獻者授權協議](docs/CLA.zh-TW.md)
- [插件發布協議](docs/PLUGIN-PUBLISHING-AGREEMENT.zh-TW.md)

包括 `vendor/`、`third_party/` 及 `build/tools/` 下素材在內的第三方元件，仍依其各自的授權與 NOTICE 文件規範。
