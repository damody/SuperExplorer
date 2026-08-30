# 遠端內容選單字型與排版校正設計

## 問題

ADB／SFTP 選單已採用經典 Windows 垂直選單結構，但目前把 `15px` 當作選單字級。Windows 原生選單實際使用系統 `lfMenuFont`；在繁體中文環境通常對應 Microsoft JhengHei UI 的一般選單尺寸。現有 `TypographyTokens::menu` 已定義為 `12px`、`16px` 行高，因此遠端選單的字體顯得過大，並連帶讓字形、圖示與列高比例失真。

## 方案比較與決策

- 只調整截圖對應的單一字級：修改最少，但字級、行高與字型來源仍分離，後續容易再次漂移。
- 直接以實體像素硬編碼：可對齊目前顯示器，卻會在 125%／150%／175% DPI 下失真。
- 採用 DPI 感知的 Windows 選單排版契約（採用）：幾何尺寸維持邏輯像素，字型由既有 `TypographyTokens::menu` 統一提供，讓 GPUI 自製遠端選單與 Windows owner-draw 備援共用合理的 12px 字級。

## 設計

### 字型

遠端選單明確使用：

- 字型：`Microsoft JhengHei UI`，沿用既有 Windows 繁中 fallback 順序。
- 字級：`12` 邏輯像素。
- 行高：`16` 邏輯像素。
- 字重：`400`。

GPUI 列項必須同時設定 font family、font size、line height 與 font weight，不再只設定字級並依賴預設行盒。

### 幾何

- 列高維持 `23` 邏輯像素，讓 16px 行盒在列內垂直置中，並保持 Windows 經典選單的緊湊節奏。
- 圖示槽維持 `42`、圖示 `16`、左偏移 `13` 邏輯像素；這些值已與兩張參考圖的文字起點相符。
- 選單外距維持 `3`，分隔線仍從圖示槽後開始。
- 寬度、陰影、顏色與命令順序不在這次修改範圍。

### 共用契約

`WINDOWS_CONTEXT_MENU_VISUAL_METRICS.font_size` 從 15 校正為 12，供 Windows owner-draw 無法取得系統 `lfMenuFont` 時使用。正常本機原生選單仍優先讀取 `NONCLIENTMETRICS.lfMenuFont`，不會被遠端 GPUI 排版覆蓋。

遠端 GPUI 呈現則以 `TypographyTokens::menu` 為字型權威，並測試其值與共用 fallback metric 一致。

## 錯誤與相容性

若系統缺少 Microsoft JhengHei UI，沿用 `Segoe UI Variable Text`、`Segoe UI`、`sans-serif` fallback。高對比、亮色與暗色只改變色彩，不改變字型及幾何。

此修改不改命令、檔案操作、右鍵事件、定位或剪貼簿行為。

## 驗證

最後集中驗證：

- 模型測試確認 12px fallback 字級與既有幾何。
- UI 測試確認遠端選單使用 menu typography 的 family、size、line height、weight。
- 編譯 `explorer-ui` 與 `explorer-shell-win` 相關路徑。
- 以 ADB／SFTP 截圖或現有 headful 工具檢查字形比例、垂直置中、文字起點及列間節奏；環境無法自動化時如實記錄，不宣稱視覺通過。

## 非目標

- 不重做 Windows Shell 擴充命令或命令分組。
- 不調整應用程式其他頁面的全域字型。
- 不針對單一 DPI 寫死實體像素。
