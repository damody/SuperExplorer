# build_install 內附單一 Plugin 設計

## 目標

`build_install.bat` 產生的 SuperExplorer 安裝程式必須包含目前唯一受支援的
`p0-consumer` Rust Plugin。安裝後從桌面捷徑、開始選單捷徑或安裝完成頁啟動時，
SuperExplorer 會透過既有的 `--plugin-dll <absolute-path>` 明確載入該 Plugin。

本變更維持 0→1 原則：只處理一個 Plugin，不加入 Plugin 掃描、設定檔、launcher、
多 Plugin registry、CI、UITEST 或新的測試 framework。

## 建置流程

`build/build_install.lua` 在一般建置模式中，除了既有 release application、broker 與
worker，也使用 `sdk/fixtures/p0-consumer/Cargo.toml` 建置 release
`p0_consumer.dll`。Plugin 建置使用固定的 `x86_64-pc-windows-msvc` target 與 offline
模式；任何建置錯誤都會終止 installer build。

`--skip-build` 不重建 Plugin，但仍要求預期的 release DLL 已存在且通過與其他 Windows
binary 相同的最小 PE/size 驗證。`--check` 只驗證工具及靜態輸入，不要求先存在 build
artifact，也不建立或啟動安裝程式。

驗證完成後，Lua 以明確的 `PLUGIN_DLL` NSIS define 傳遞唯一 DLL 路徑。不得用 wildcard、
時間戳或掃描方式選擇 Plugin binary。

## 安裝配置與啟動

`installer/SuperExplorer.nsi` 將 DLL 寫入：

```text
$INSTDIR\plugins\p0_consumer.dll
```

桌面與開始選單捷徑的 target 仍是 `$INSTDIR\SuperExplorer.exe`，arguments 固定為：

```text
--plugin-dll "$INSTDIR\plugins\p0_consumer.dll"
```

安裝完成頁使用相同參數。直接執行 `SuperExplorer.exe` 時仍不自動載入 Plugin，因此不改變
既有「無參數不掃描 unsigned local DLL」的安全與產品驗證邊界。

Uninstaller 明確刪除 `p0_consumer.dll`，接著移除空的 `plugins` 目錄。它不遞迴刪除未知
Plugin 或使用者檔案。

## 失敗處理

- Plugin manifest 或 source 不存在：installer build 在建置前失敗。
- Plugin build 失敗：保留精確 build log，且不呼叫 NSIS。
- Plugin DLL 缺失、非 PE 或過小：驗證失敗，且不產生或啟動 installer。
- NSIS 未收到 `PLUGIN_DLL`：compile-time error。
- 不 fallback 到舊 DLL、debug DLL或其它 Plugin。

## 驗證

允許的最小驗證為：

1. bundled Lua syntax／`build_install.bat --check`。
2. fixture release offline build。
3. 實際產生 `SuperExplorer-Setup-1.2026.8.4-x64.exe`。
4. 檢查 installer build log，確認 NSIS 收到並封裝明確的 release DLL。

本 slice 不執行 CI 或 UITEST，也不新增 contract、integration、evidence、snapshot、coverage、
mock 或 fake framework。後續產品功能仍按 OpenSpec 中明確啟用的最小 Vertical Slice 逐項完成。
