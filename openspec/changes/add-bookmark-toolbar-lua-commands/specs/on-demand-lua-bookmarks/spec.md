## ADDED Requirements

### Requirement: 受限的按需 Lua 書籤執行

系統 SHALL 只在使用者點選 Lua 書籤時執行其原始碼，並 MUST 使用背景工作而不阻塞檔案總管介面。每次執行 MUST 僅提供唯讀 Lua 字串 `current_folder`，其值為目前分頁的實體資料夾路徑。

#### Scenario: 在目前資料夾執行 Lua

- **WHEN** 使用者在實體資料夾位置點選 Lua 書籤
- **THEN** 系統 SHALL 在背景執行該 Lua 原始碼並將該資料夾路徑提供為 `current_folder`

#### Scenario: Lua 嘗試改寫目前資料夾

- **WHEN** Lua 書籤嘗試重新指派 `current_folder`
- **THEN** 系統 MUST 拒絕該寫入且以 Lua 執行失敗結果完成該工作

#### Scenario: 非檔案系統位置

- **WHEN** 使用者在非實體資料夾位置點選 Lua 書籤
- **THEN** 系統 MUST 不啟動 Lua runtime 並顯示說明目前位置無法執行的非阻塞通知

### Requirement: Lua 書籤的受限 host 邊界

Lua 書籤 runtime MUST NOT 提供目前選取項目、檔案操作、Windows Shell、程序啟動或任何其他 Explorer host API。系統 MUST 在成功、例外、啟動失敗與逾時後提供可閱讀的非阻塞結果。

#### Scenario: 指令成功完成

- **WHEN** Lua 書籤在允許的執行時間內成功完成
- **THEN** 系統 SHALL 發布成功完成結果且不變更目前分頁位置

#### Scenario: 指令例外或逾時

- **WHEN** Lua 書籤拋出例外或超過設定的執行時間
- **THEN** 系統 MUST 終止該工作並發布可閱讀的失敗結果，而不阻塞介面

### Requirement: 不再執行資料夾自動化腳本

系統 MUST NOT 因進入資料夾而探索、讀取、載入或執行 `.explorer.lua`。SuperExplorer MUST NOT 修改或刪除使用者已有的 `.explorer.lua` 檔案。

#### Scenario: 進入含舊腳本的資料夾

- **WHEN** 使用者進入含有 `.explorer.lua` 的實體資料夾
- **THEN** 系統 MUST 不讀取或執行該檔案，且不得建立資料夾自動化工作

#### Scenario: 舊腳本檔案保留

- **WHEN** 使用者升級至包含此變更的版本
- **THEN** 系統 MUST 不修改、移動或刪除使用者檔案系統中的 `.explorer.lua`
