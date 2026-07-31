## ADDED Requirements

### Requirement: 外部互動結束網址編輯

網址 editor SHALL 在互動或焦點離開 editor 時恢復 resolved breadcrumb，除非 address child interaction 仍啟用。

#### Scenario: Pointer 離開網址編輯
- **WHEN** 使用者點擊 file、folder、background、navigation、command、search、tab 或其他非 address surface
- **THEN** draft 取消並顯示 resolved breadcrumb

#### Scenario: 視窗失去 activation
- **WHEN** 網址 editor 顯示時 Explorer window 失去作用中狀態
- **THEN** editing 與 breadcrumb child menu 關閉，重新作用前已恢復 resolved breadcrumb

追蹤來源：[`proposal.md`](../../proposal.md)、[`design.md`](../../design.md)、[`tasks.md`](../../tasks.md)、[`docs/EXPLORER_VISUAL_ADDRESS_BASELINE.md`](../../../../../docs/EXPLORER_VISUAL_ADDRESS_BASELINE.md)。

### Requirement: Per-tab 網址列雙模式狀態
每個 tab SHALL 擁有獨立 `AddressBarState`，至少區分 browsing breadcrumb、editing draft、enumerating menu 與 navigation error；切換 tab MUST 還原該 tab 的 resolved location 與未提交 draft。

#### Scenario: 兩分頁各自編輯
- **WHEN** 第一分頁保留未提交路徑後切到第二分頁並導覽
- **THEN** 回到第一分頁時 draft 必須原樣保留，第二分頁 event 不得覆寫它

### Requirement: 點擊空白區進入完整路徑編輯
使用者點擊 breadcrumb 右側未占用空白區、按 `Ctrl+L` 或按 `Alt+D` 時，網址列 MUST 切換為完整 parsing path editor、取得焦點並全選文字。

#### Scenario: 點擊右側空白
- **WHEN** breadcrumb 未占滿網址列且使用者點擊其右側空白區
- **THEN** 同一控制項必須顯示可編輯完整路徑並全選，不能先觸發 segment navigation

#### Scenario: breadcrumb 占滿寬度
- **WHEN** 路徑過長而沒有明顯右側空白
- **THEN** `Ctrl+L` 與 `Alt+D` 仍必須可靠進入 editor

### Requirement: 路徑提交、取消與錯誤
Enter SHALL 用 address parser 提交目前 draft，Esc SHALL 放棄未提交 draft並回復 resolved breadcrumb；無效、不存在或無權限 location MUST NOT 提交 history，且 editor MUST 保留失敗輸入與可理解錯誤。

#### Scenario: 成功提交真實路徑
- **WHEN** 使用者輸入存在且可導覽的 Windows 路徑並按 Enter
- **THEN** 系統必須成功導覽、提交一次 history 並顯示新 ancestry

#### Scenario: 取消編輯
- **WHEN** 使用者修改 draft 後按 Esc
- **THEN** 系統不得導覽或改變 history，並回復原 location 的 breadcrumb

#### Scenario: 無效路徑
- **WHEN** address parser 或 Shell resolve 回報錯誤
- **THEN** 系統不得轉成 search 或顯示假空資料夾，必須保留 draft 和錯誤

### Requirement: 可點擊 breadcrumb segment
每個 breadcrumb segment MUST 保存 stable identity、display name、`LocationDescriptor` 與 capability；點擊名稱 SHALL 導覽至該 segment 對應位置並使用既有 history/generation/cancellation contract。

#### Scenario: 從子資料夾點擊磁碟 segment
- **WHEN** 使用者在 `D:\a\b` 點擊代表 `D:\` 的 segment 名稱
- **THEN** active tab 必須導覽至 `D:\`，Back 必須可回到先前位置

### Requirement: Shell-aware ancestry
breadcrumb ancestry SHALL 由 resolved Shell identity/metadata 建立，不得只以字串分隔符作唯一真相，並 MUST 支援 filesystem root、UNC、This PC、ZIP、Libraries 與可導覽 namespace location 的能力差異。

#### Scenario: 顯示磁碟根目錄
- **WHEN** active location 為 `D:\`
- **THEN** breadcrumb 必須包含可導覽的「本機」與磁碟 segment，且 display name 使用 Shell metadata

#### Scenario: filesystem metadata 延後到達
- **WHEN** 初始 ancestry 先由 path 建立而 Shell display metadata 稍後到達
- **THEN** UI 可更新 display name，但 segment target identity 與 history 不得改變

### Requirement: 可點擊 chevron 子資料夾選單
每個允許列舉 children 的 segment 右側 SHALL 顯示可操作 `>`，點擊後 MUST 非同步列出該 segment 的直接可導覽子資料夾／Shell containers；選取項目 SHALL 導覽至其 descriptor。

#### Scenario: 列舉 D 槽子資料夾
- **WHEN** 使用者點擊 `D:\` segment 右側的 `>`
- **THEN** menu 必須逐批顯示 `D:\` 的直接資料夾，不得包含一般檔案或遞迴 descendants

#### Scenario: 選取 menu child
- **WHEN** 使用者從 chevron menu 選取一個子資料夾
- **THEN** menu 必須關閉並以既有 navigation pipeline 導覽，不能建立平行 history

#### Scenario: Menu 位於最上層
- **WHEN** chevron menu 與 command bar、file view 或其他普通 scene content 的矩形重疊
- **THEN** menu 必須以 deferred anchored overlay 最後繪製並優先接收 hit-test，所有可見 menu item 都必須能點擊

### Requirement: Chevron 列舉可取消且拒絕 stale event
每次 menu enumeration MUST 建立 tab/request/generation context；關閉 menu、切 tab、導覽、重開另一 menu 或關窗 MUST 取消舊 request，所有 context 不符的 batch/terminal event MUST 被拒絕。

#### Scenario: 快速切換兩個 chevron
- **WHEN** 第一個 menu 尚在 loading 時開啟第二個 segment menu
- **THEN** 第一個 request 必須取消，其 late batches 不得出現在第二個 menu

#### Scenario: 關窗時仍在列舉
- **WHEN** application 關閉且 menu enumeration 尚未完成
- **THEN** request、menu entity 與 Shell resources 必須恰好清理一次

### Requirement: Chevron menu loading、empty、partial 與 error 語意
menu MUST 呈現 loading、partial results、empty、cancelled 與 recoverable error，慢速或失敗 provider MUST NOT 阻塞 GPUI thread、網址列 editor 或主要導覽。

#### Scenario: 無子資料夾
- **WHEN** segment enumeration 成功但沒有可導覽 child
- **THEN** menu 必須顯示 disabled empty 狀態而非永遠 loading

#### Scenario: provider 回傳部分結果後失敗
- **WHEN** menu 已收到部分 batches 後遇到 provider error
- **THEN** 已顯示結果必須保留並標示 partial/error，使用者仍可選取有效結果

### Requirement: 網址列鍵盤、IME 與 accessibility
breadcrumb、chevron、menu 與 editor MUST 支援正反向 focus traversal、左右／上下方向鍵、Enter、Esc、IME composition 與可辨識 accessibility role/name/state；快捷鍵 dispatcher MUST NOT 攔截進行中的 IME composition。

#### Scenario: 只用鍵盤完成導覽
- **WHEN** 使用者不使用滑鼠，聚焦網址列、選取 segment、開啟 chevron menu 並選取 child
- **THEN** focus、selection、navigation 與關閉 menu 行為必須完整且每個 action 只觸發一次

#### Scenario: 繁中 IME 編輯路徑
- **WHEN** editor 進行 Windows IME composition
- **THEN** composition text、caret 與 commit 必須正常，Back/Forward/shortcut action 不得誤觸

### Requirement: 響應式 breadcrumb overflow
當 ancestry 無法完整顯示時，網址列 SHALL 依 Explorer 式優先序保留目前位置、editor entry area 與 search，將較舊 segments 收合至可操作 overflow，而不得裁切 chevron hit target 或重疊搜尋框。

#### Scenario: 窄視窗長路徑
- **WHEN** 視窗縮小且 active location ancestry 很長
- **THEN** breadcrumb 必須收合舊 segments、保留目前 segment 與可操作 overflow，所有具名區域仍不得重疊

### Requirement: Breadcrumb icons identify the actual Shell item type
Every visible breadcrumb segment SHALL use the Shell icon for its exact location, with a type-correct geometry-stable fallback while the asynchronous icon is loading.

#### Scenario: A filesystem folder is active
- **WHEN** the address bar shows drive and nested folder segments
- **THEN** the drive SHALL use a drive icon and every folder SHALL use a folder icon
- **AND** no segment SHALL fall back to the unrelated Details-list glyph

#### Scenario: Navigation enters a compressed folder
- **WHEN** ZIP, RAR, 7z, TAR, or GZip is present in the resolved ancestry
- **THEN** that segment SHALL be classified as an archive
- **AND** the loaded Shell association icon SHALL replace the archive fallback without changing segment geometry

### Requirement: Stable local drive breadcrumb name
The address breadcrumb SHALL display a local filesystem drive using only its canonical uppercase drive designator, such as `D:`, throughout navigation and asynchronous Shell metadata updates.

#### Scenario: Shell volume metadata arrives after navigation
- **WHEN** a filesystem navigation first publishes `D:` and a later ancestry batch contains a volume title such as `新增磁碟區 (D:)`
- **THEN** the visible drive breadcrumb SHALL remain `D:` without changing text or width
- **AND** the Shell drive icon MAY update independently without changing the drive text

#### Scenario: Navigate between folders on the same drive
- **WHEN** the user navigates repeatedly between child folders on `D:`
- **THEN** every resolved breadcrumb SHALL retain exactly one drive segment named `D:`
- **AND** folder, archive, UNC, and namespace segment display names SHALL remain unaffected

### Requirement: Breadcrumb Shell overlays survive navigation round trips

Every breadcrumb, overflow, and breadcrumb-child folder icon SHALL use the newest compatible Windows Shell texture for its exact location, theme, and DPI. A newer association or overlay generation SHALL replace an older presentation without causing the address bar to fall back to an unbadged generic folder.

#### Scenario: Return to a TortoiseGit working tree

- **WHEN** TortoiseGit is installed and Windows Shell exposes a Git status overlay for a folder
- **AND** the user navigates from that Git folder to a drive or unrelated folder and then returns
- **THEN** the breadcrumb segment SHALL again display the current TortoiseGit-composited Shell icon
- **AND** it SHALL NOT disappear merely because the file view previously loaded a newer icon generation for the same path
