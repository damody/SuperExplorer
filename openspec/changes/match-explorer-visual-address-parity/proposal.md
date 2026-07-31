## Why

目前程式已具備檔案總管主要功能，但 GPUI chrome 與 Windows 11 檔案總管在區域比例、按鈕順序、圖示、配色、字級及網址列互動上仍有明顯差異。使用者要求在相同資料夾與設定下，把具名控制項的座標誤差控制在 10% 內，並讓網址列具備 Explorer 式可點擊麵包屑、空白處路徑編輯及 `>` 子資料夾列舉。

核准來源為 [`docs/superpowers/specs/2026-07-27-explorer-visual-address-parity-design.md`](../../../docs/superpowers/specs/2026-07-27-explorer-visual-address-parity-design.md)；技術設計、能力規格、任務與實作前盤點分別見 [`design.md`](design.md)、[`specs/`](specs/)、[`tasks.md`](tasks.md) 與 [`docs/EXPLORER_VISUAL_ADDRESS_BASELINE.md`](../../../docs/EXPLORER_VISUAL_ADDRESS_BASELINE.md)。

## What Changes

- 以 Windows 11 build 26200、Explorer `10.0.26100.8875`、繁體中文、淺色、175% DPI、`D:\` Details view 建立主要參考契約。
- 重整 title/tab、navigation、command、navigation pane、details header、file rows、status 與 caption controls，使名稱、順序、比例及位置通過 10% 幾何門檻。
- 將 Unicode 暫代圖示替換為集中管理的 Fluent／Windows Shell 原生圖示，並校準可見尺寸、中心與線寬。
- 依 Windows theme/system colors 與 Explorer 實測建立色彩、字型 family、字級、字重及行高 tokens。
- 將網址列改為 per-tab 雙模式控制項：可互動麵包屑與完整路徑 editor。
- 讓麵包屑名稱可直接導覽，`>` 可非同步列出直接子資料夾／Shell containers，並支援取消、錯誤、鍵盤、IME 與 accessibility。
- 擴充視覺工具，輸出具名 region 幾何、icon bounds、色差與 typography 報告，而非只看全圖 pixel diff。
- 使用真實 `D:\`、多分頁、檔案操作、Clipboard、OLE drag-and-drop、context menu 與 search regression gates 驗證 chrome 重構。

## Capabilities

### New Capabilities

- `explorer-visual-parity`: Windows Explorer chrome、icon、theme、typography、具名區域量測與 10% 幾何驗收。
- `interactive-breadcrumb-address`: per-tab 麵包屑／路徑編輯雙模式、segment 導覽、chevron child-container menu、取消與錯誤語意。

### Modified Capabilities

無；既有 change 尚未封存成主規格，本 change 以新增 delta 能力追蹤，不回寫已完成 change 的 301 個任務。

## Impact

- 主要影響 `explorer-ui` 的 chrome、layout、theme、typography、actions、per-tab state 與 GPUI input/menu rendering。
- `explorer-common`／`explorer-model` 增加 breadcrumb segment、address mode 與 typed request/event；`explorer-shell-win` 增加 Shell ancestry、child-container enumeration 與 native icon pipeline。
- `explorer-app` composition root 需接線新 command/event 與 theme/DPI invalidation。
- 視覺 capture/compare scripts、diagnostics schema、真實 Explorer 證據與 parity/manual/status 文件會更新。
- 不嵌入或控制 Explorer，不讀取 Explorer 私有二進位資產，不修改真實 `D:\` 內容；破壞性測試仍限於已驗證 fixture root。
