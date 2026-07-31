## ADDED Requirements

### Requirement: Explorer 視窗區域結構
系統 SHALL 在單一可調整大小視窗中依序呈現 title/tab chrome、command bar、navigation/address/search row、navigation pane、file view host 與 status bar；`ExplorerWindow` MUST 只負責區域協調，不包含 Shell 業務邏輯。

#### Scenario: 初次顯示完整 chrome
- **WHEN** application 完成首個 window render
- **THEN** 所有主要區域必須可見、順序正確、沒有重疊，且 file view 能呈現 loading、ready、empty 或 error 狀態

#### Scenario: 視窗 resize
- **WHEN** 使用者將視窗從基準尺寸縮小或放大
- **THEN** 固定高度區域必須保持 token 高度，content 區域重新配置，且不得產生負尺寸、裁切主要控制或 panic

### Requirement: 多分頁 chrome
系統 SHALL 顯示一至多個 tabs、唯一 active tab、可操作的新分頁與關閉按鈕，以及 Windows caption controls；tab 數量或寬度超出可見區域時 MUST 提供可存取的 overflow 行為。

#### Scenario: 建立並切換分頁
- **WHEN** 使用者建立第二個分頁並切換 active tab
- **THEN** tab strip 必須顯示兩個分頁、只有目標分頁呈現 active，且兩個分頁的 location/view state 保持獨立

#### Scenario: 關閉最後分頁
- **WHEN** 使用者關閉視窗中的最後一個分頁
- **THEN** application 必須依產品規則關閉視窗或建立明確預設分頁，不得留下沒有 active tab 的不可操作狀態

### Requirement: 靜態命令與導航控制
系統 SHALL 顯示 command bar、Back、Forward、Up、可輸入的 breadcrumb/address 與 search；Back/Forward/Up availability MUST 由 active tab 的真實 history/location 決定。

#### Scenario: 觸發不可用 navigation action
- **WHEN** 使用者點擊或透過鍵盤觸發 disabled Back、Forward 或 Up
- **THEN** application state 不得改變，且控制的 disabled semantics 必須可被測試觀察

### Requirement: Navigation pane 與 content split
系統 SHALL 顯示 navigation pane 與真實 file view host，並以集中式 min/default/max width 控制 pane；resize handle 拖曳 MUST clamp 在允許範圍。

#### Scenario: 調整 navigation pane
- **WHEN** 使用者拖曳 navigation pane divider 超過最小或最大界線
- **THEN** pane width 必須限制在 token 範圍，content 保持有效尺寸，互動結束後 divider 不得卡在 capture 狀態

### Requirement: Status bar
系統 SHALL 顯示 status bar，依 active tab 的真實 directory/search/selection/operation state 顯示 loading、item count、selected count 或錯誤，且不得顯示虛構數字。

#### Scenario: 真實資料夾完成列舉
- **WHEN** active tab 完成真實資料夾列舉且沒有選取
- **THEN** status bar 必須顯示該 snapshot 的真實 item count，不得使用測試假資料或上一個分頁的數字

### Requirement: Semantic theme tokens
系統 SHALL 以集中式 semantic tokens 提供 light、dark 及 high-contrast-ready 映射；feature component MUST NOT 直接散落固定 RGB 作為互動狀態來源。

#### Scenario: 切換 light 與 dark
- **WHEN** 使用者觸發 theme action
- **THEN** 所有可見區域必須在同一個 render cycle 使用一致 theme，文字與背景保持可辨識，focus/hover/disabled 狀態由 semantic token 更新

#### Scenario: Token 完整性
- **WHEN** 執行 theme contract test
- **THEN** surface、control、hover、pressed、selected、divider、text、focus、danger 與 accent token 都必須有 light/dark 值及 high-contrast 對應策略

### Requirement: 集中式 layout tokens 與 DPI 行為
系統 SHALL 以 logical pixel layout tokens 定義主要區域高度、pane 寬度、padding、radius 與 focus stroke，並透過 GPUI/Windows scale 在 100%、125%、150% 與 200% DPI 保持一致幾何語意。

#### Scenario: DPI 矩陣
- **WHEN** 在指定 DPI 啟動固定基準尺寸的視窗
- **THEN** logical layout 關係必須保持一致，文字、控制與 focus stroke 不得因重複縮放而異常放大或縮小

### Requirement: Typed actions 與快捷鍵
系統 SHALL 將滑鼠與鍵盤輸入映射為 typed actions，由 window/focused surface 決定處理者；至少涵蓋 Back、Forward、Up、focus address、focus search 與 theme toggle。

#### Scenario: Focus search action
- **WHEN** 使用者觸發規格指定的 search focus shortcut
- **THEN** focus 必須移至 search input，原控制失去 focus，且不得同時觸發 navigation action

#### Scenario: Action 衝突
- **WHEN** focused surface 與 window 都能看到同一 key event
- **THEN** action routing 必須依明確優先序只執行一次，並留下可測試的 handled 結果

### Requirement: Focus 與互動狀態
系統 SHALL 為所有可互動控制提供可見 focus、hover、pressed、disabled 與 active/inactive 狀態，且鍵盤焦點順序 MUST 能遍歷主要 chrome、tab strip、file view 與 operation/search surfaces。

#### Scenario: Keyboard-only traversal
- **WHEN** 使用者只用 Tab、Shift+Tab 與已定義快捷鍵巡覽視窗
- **THEN** focus 必須依文件順序移動、保持可見，且 disabled controls 不得取得可操作焦點

### Requirement: UI 與 Windows Shell 隔離
`explorer-ui` SHALL 只消費 application state 與 typed actions，M1 render/input callback MUST NOT 執行同步 filesystem I/O、Shell COM call 或直接依賴 Windows Shell 型別。

#### Scenario: 靜態 UI 操作
- **WHEN** 使用者重複 resize、切換 theme 與移動 focus
- **THEN** UI callback 不得啟動 directory enumeration 或 Shell operation，且一般 callback 的量測目標維持低於 4 ms
