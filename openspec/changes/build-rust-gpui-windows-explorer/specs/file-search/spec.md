## ADDED Requirements

### Requirement: 可驗證的搜尋語法 AST
系統 SHALL 將純文字、quoted phrase、property filter、comparison、date/size shorthand 與 boolean operator 解析成 typed AST；invalid input MUST 回報位置與可修正訊息。

#### Scenario: 複合查詢
- **WHEN** 使用者輸入包含 quoted phrase、type/size/date filter 與 boolean operator 的合法 query
- **THEN** parser 必須產生 deterministic AST，backend 不得重新解析未驗證 raw string

#### Scenario: 未閉合引號
- **WHEN** query 包含未閉合 quoted phrase
- **THEN** UI 必須標示錯誤位置並保留輸入供修正，不得啟動 backend search

### Requirement: 地址列與搜尋列分離
地址 navigation parser 與 search parser SHALL 是不同入口；系統 MUST NOT 以模糊 heuristic 靜默把失敗地址改成搜尋或把 query 當成 location。

#### Scenario: 無效地址輸入
- **WHEN** 使用者在 address input 提交無法解析的字串
- **THEN** 系統必須顯示 address error 並保留目前 location，不得自動建立 search session

### Requirement: Per-tab 可取消搜尋 session
每個搜尋 SHALL 綁定 tab/request/generation 與 cancellation；提交新 query、離開搜尋、導覽或關閉分頁 MUST 取消舊 session，model MUST 拒絕 late results。

#### Scenario: 快速切換 query
- **WHEN** 使用者依序提交 query A、B 且 A 的結果晚於 B
- **THEN** active tab 只能顯示 B 的 results/status，A 的 late results 與 terminal event 必須被拒絕

### Requirement: Windows Search backend
系統 SHALL 將 validated AST 以安全 escape/bind 轉給 Windows Search backend，回傳增量 results 與 source status；不得拼接未驗證 query string。

#### Scenario: Indexed location 搜尋
- **WHEN** active location 可由 Windows Search 索引且 query 合法
- **THEN** backend 必須增量回傳符合項目、標記 Windows Search source，並在 terminal event 提供完成/錯誤/取消狀態

### Requirement: 有界 fallback search
當 Windows Search 不可用、location 未索引或 query 能力不支援時，系統 SHALL 提供可取消且有界的 filesystem fallback 或明確 unavailable 狀態；UI MUST 顯示能力差異與 scope。

#### Scenario: 未索引真實資料夾
- **WHEN** 使用者搜尋未索引的受控 temporary folder
- **THEN** 系統必須使用 fallback 或清楚說明不可用，不得顯示空結果冒充完整 Windows Search 結果

### Requirement: 增量結果、dedupe 與 identity
search results SHALL 使用 stable item identity 去重並增量呈現；不同 backend/source 回傳同一 item 時 MUST 合併，不得因 display name/path alias 顯示重複項目。

#### Scenario: 重複來源結果
- **WHEN** Windows Search 與 fallback/fake source 回傳相同 stable identity
- **THEN** result list 必須只有一個項目，並保留可診斷的 source attribution

### Requirement: 搜尋錯誤與取消不是假空結果
parser error、backend error、cancellation、partial source failure 與成功零結果 SHALL 有不同 terminal state；UI MUST NOT 將錯誤或取消顯示為「找不到項目」。

#### Scenario: Backend 中途失敗
- **WHEN** backend 已回傳部分結果後發生錯誤
- **THEN** UI 必須保留標記為 partial 的有效結果、顯示 source error 與重試選項，不得宣告完整成功

### Requirement: 真實搜尋測試
專案 SHALL 在受控真實資料夾測試 Unicode、quoted phrase、name/type/size/date filter、boolean query、零結果、取消、未索引 fallback、快速 query replacement 與 100,000 項目 first-result latency。

#### Scenario: 真實 query 與磁碟內容一致
- **WHEN** fixture 建立已知名稱、型別、大小與時間的檔案並執行支援 query
- **THEN** terminal result set 必須與 fixture oracle 一致，任何 Windows Search 索引延遲或 fallback 差異必須在結果與證據中明列
