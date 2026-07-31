# 工具列擴充功能與 TortoiseGit 圖示刷新設計

## 背景與目標

目前命令列右側依序提供「排序」、「檢視」與只有省略號圖示的 More 按鈕。使用者需要把 More 的可見標籤改為「其它」，並在其右側新增獨立的「擴充功能」下拉選單。若 Windows 上偵測到 TortoiseGit，選單提供「更新 TortoiseGit 狀態」，讓目前資料夾可見項目的 Shell Git overlay 圖示立即重新查詢。

## 採用方案

採用「應用程式啟動時偵測能力，UI 以 typed action 觸發既有 overlay epoch 失效」：

- Windows Shell adapter 只負責判斷標準 Program Files 安裝位置是否存在 `TortoiseGitProc.exe`，不啟動外部程式、不解析 Git repository，也不繪製私有 overlay。
- application composition root 把偵測結果注入 `ExplorerRoot`／`AppViewState`，讓 presentation 不直接依賴 Windows API。
- 「更新 TortoiseGit 狀態」只使應用程式的 overlay 世代、可見圖示快取、negative cache 與 pending consumer 失效，接著透過原有 Shell icon pipeline 重新取得目前資料夾圖示。
- 未安裝時仍顯示「擴充功能」按鈕；選單內顯示停用的「沒有可用的擴充功能」，避免工具列因環境不同而位移。

不採用直接執行 `TortoiseGitProc.exe`，因為刷新 app 內 Shell overlay 不需要開啟外部視窗或執行全系統 icon cache 重建。不採用應用程式自行執行 Git status，因為既有設計要求 overlay 像素與狀態由 Windows Shell／TortoiseGit handler 所有。

## UI 與互動

- More 按鈕保留既有 `command-more-menu` identity、選單內容與 typed actions，只把可見內容由 `...` 圖示改為文字「其它」，accessibility label 同步改為「其它」。
- 新增 `command-extensions-menu`，可見文字為「擴充功能」，位置在「其它」右側。
- 擴充功能 popup 是按鈕的 direct child，使用既有 absolute top/right 與 deferred top-layer 規則，避免座標及 hit-test 回歸。
- 新選單與排序、檢視、其它選單互斥；點擊外部、Esc、切換導覽或執行命令時關閉。
- TortoiseGit 可用時顯示可操作的「更新 TortoiseGit 狀態」；不可用時顯示停用提示。
- 鍵盤 Enter／Space 可執行唯一可用命令，Esc 關閉選單；焦點仍由 CommandBar 所有。

## 刷新資料流

1. 使用者執行 `RefreshTortoiseGitStatus` typed action。
2. reducer 關閉擴充功能選單，root action handler 將 global overlay epoch 推進到大於所有 per-item overlay epoch，並清除 per-item epoch map。
3. 清除 visible Shell texture cache、negative icon cache、pending visible icon keys 與 overlay 相關 thumbnail presentation；base icon cache與副檔名 association epoch 保留。
4. 以目前 active tab 的 generation 與 snapshot 重新提交可見圖示請求，並通知 GPUI rerender。舊世代完成事件因 key 世代較舊，不得覆蓋新結果。
5. 不重新列舉資料夾、不改變 selection、scroll offset、history 或檔案內容。

## 錯誤與邊界

- Program Files 環境變數不存在或安裝檔不可讀時視為未安裝，不阻止應用程式啟動。
- active location 為純 Shell namespace 時仍可刷新其現有 Shell 圖示；功能不宣稱該位置是 Git repository。
- Shell handler 暫時失敗時沿用既有 fallback／negative cache 行為；下一次手動刷新會以新 epoch 再嘗試。
- 偵測只代表 TortoiseGit 程式存在；Windows overlay slot 被其他 handler 排除時，應用程式仍不偽造 TortoiseGit badge。

## 驗證

- 純函式測試 TortoiseGit candidate path 偵測，包括存在、不存在與環境缺失。
- reducer 測試四個 command popup 互斥、關閉與 Refresh action enablement。
- root 測試刷新會推進 overlay epoch、清除 per-item／negative／pending 狀態，但不推進 association epoch。
- render contract 驗證「其它」文字、右側「擴充功能」按鈕、popup identity、可用命令與未安裝提示。
- Windows headful UITEST 在安裝 TortoiseGit 時開啟真實 Git fixture，展開選單、執行刷新並確認 overlay epoch 後重新載入；未安裝時明確記錄 prerequisite skip 或 disabled placeholder。
- 執行 targeted fmt、Clippy、model/UI/Shell tests、UITEST coverage 與 OpenSpec strict validation。
