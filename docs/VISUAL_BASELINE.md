# Windows Explorer 視覺量測基線

## 2026-07-26 light theme 初始量測

- 環境：Windows 11 Professional x64，Version 10.0.26200，100% DPI，Microsoft JhengHei UI。
- 來源：同機 Windows Explorer `CabinetWClass`，開啟真實資料夾 `D:\test`；不是網路截圖。
- 視窗 capture：1534×926；本機 evidence `target/explorer-theme-evidence/explorer-light-2026-07-26.png`（不提交衍生 screenshot）。
- DWM accent：registry `ColorizationColor = 0xC40078D4`，semantic accent/focus 的不透明 RGB 使用 `#0078D4`。
- 取樣：title surface `#E8E8E8`、tab/control band `#F8F8F8`、address control fill `#FDFDFD`、navigation/file surface `#FFFFFF`、content divider `#D6D6D6`、可見 secondary text `#5A5A5A`。

`ThemeTokens::light()` 以以上取樣作為 surface/control/divider/text/accent 基礎。hover、pressed、selected、disabled 與 danger 的第一版採相鄰 Windows Fluent semantic 色階；這些互動值仍需在 task 10.4/12.7 的 deterministic interaction fixture 與 Explorer 並排 capture 後校正，不視為已完成 visual parity。

## Dark 與 high contrast 邊界

Dark palette 是獨立定義的 Windows Fluent 深色階，不由 light RGB 反相產生；尚未在本機切換 OS app theme 擷取 Explorer dark baseline，因此 parity matrix 保持部分完成。High contrast 不固定第二套 RGB，而由 `HighContrastMappings` 指向 Windows `Window`、`WindowText`、`ButtonFace`、`GrayText`、`Highlight`、`HighlightText`、`Hotlight` semantic roles；實際系統色解析與 high-contrast 實機驗收屬後續 tasks 10.8/13.7。

## Token review 規則

- Feature UI 的色彩必須來自 `ThemeTokens`；固定 RGB 只能出現在 `theme.rs`。
- 主要高度、寬度、padding、gap 必須來自 `LayoutTokens`；固定 logical dimensions 只能出現在 `layout.rs`。
- `scripts/check_ui_tokens.ps1` 在 CI 掃描 raw `rgb/rgba/Rgba`、hex color 與直接套用 numeric `px(...)` 的主要 layout calls。
- 極少數無法語意化的例外必須在同一行加上 `token-lint: allow` 與具體理由，review 時不得只寫「特殊情況」。
