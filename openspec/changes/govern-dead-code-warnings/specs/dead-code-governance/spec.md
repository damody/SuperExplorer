## ADDED Requirements

### Requirement: Target-aware dead-code inventory
系統 SHALL 以 structured compiler diagnostics建立 immutable baseline，分別記錄 emitted diagnostic、canonical warning site、所有 primary items、emitting targets與會編譯該 source的完整 target set；每筆記錄 SHALL 具有唯一ID、source hash、Git/OpenSpec lineage及唯一current disposition。

#### Scenario: 同一MFT項目只在部分targets發出警告
- **WHEN** 一個項目在 App library被報為dead，但在helper或service有正式consumer
- **THEN** inventory將其標為 `retain-cross-target-live`，且不得把library warning當成刪除證據

#### Scenario: Multi-method diagnostic
- **WHEN** rustc以一個diagnostic列出多個未使用methods
- **THEN** evidence同時保存單一canonical diagnostic及每個primary method item，使每個item都有可稽核處置

#### Scenario: Source在處理前發生drift
- **WHEN** owned file hash或scoped diff與baseline不同
- **THEN** 受影響批次停止寫入、重新歸屬差異並建立replacement baseline，舊evidence標為stale

### Requirement: Evidence-based removal and retention
每個 dead-code item SHALL 依現行consumer、Git history及既有OpenSpec被判定為 superseded、unreferenced、cross-target-live、test-only、retain-required-contract或narrow-suppression；系統 MUST NOT以「目前搜尋不到call」作為唯一刪除依據。

#### Scenario: 新架構明確supersede舊chain
- **WHEN** 現行OpenSpec指定正式replacement且舊chain沒有migration、rollback、ABI或test authority
- **THEN** production code、只驗證舊架構的tests與不再需要的dependency一併移除

#### Scenario: Rollback reader仍有規格責任
- **WHEN** legacy writer已supersede但reader仍被migration或rollback requirement引用
- **THEN** 只移除無consumer的writer，保留reader並以不依賴待刪writer的checked-in golden migration chain證明其有效

#### Scenario: Legacy golden chain先於writer刪除
- **WHEN** sidecar writer、encoder或round-trip helper預定移除
- **THEN** 系統先保存具SHA-256的正向、corruption、identity、cursor-contiguity、bounds、unfocused no-delete及failed-promotion retry golden fixtures，reader tests只讀這些immutable bytes

#### Scenario: 無現行consumer也無規格責任
- **WHEN** repository、all governed targets、tests、fixtures及OpenSpec均沒有合法consumer
- **THEN** item判為 `remove-unreferenced`並直接移除，不得以future-use reason壓制

### Requirement: Test-only code boundary
僅供unit/failure-injection/reference-fixture使用的程式 SHALL 不進入normal production build；測試 SHOULD 優先呼叫production bounded/linearized APIs，必要的控制seam SHALL受 `#[cfg(test)]`限制。

#### Scenario: SQLite failure injection
- **WHEN** atomicity test需要在mutation、cursor或commit前注入失敗
- **THEN** failure selector只在test build存在，而被測transaction path與production path共享相同核心實作

#### Scenario: Convenience wrapper重複production API
- **WHEN** test-only wrapper只轉呼叫現行production API且不增加必要failure seam
- **THEN** wrapper移除，測試直接使用production API

#### Scenario: Test-only移動後測試失效
- **WHEN** `#[cfg(test)]`重構使既有blocking scenario無法編譯或不再測到production核心
- **THEN** 該leaf不得完成，必須修正test boundary而非刪除scenario或降低gate

### Requirement: Required contracts retain their owning change
既有active OpenSpec仍要求的typed recovery、migration、delta或diagnostic contract SHALL 保留原change ownership；本dead-code change MUST NOT實作其production producer/consumer或改變runtime behavior。每個此類item SHALL 連結owning change/task/evidence及明確到期條件，並只允許具體item-level disposition。

#### Scenario: Machine-readable recovery state
- **WHEN** `RecoveryReasonV1`或`MigrationStateV1`仍由未完成的 `mft-sqlite-foreground-persistence`要求但尚未接線
- **THEN** 本change保留item、連結原owning tasks與移除suppression的到期條件，不新增diagnostic behavior

#### Scenario: Remove delta requirement
- **WHEN** remove delta仍由未完成的 `centralize-shared-folder-size-service`要求但production尚無emitter/consumer
- **THEN** 本change保留variant、連結原owning tasks與到期條件，不自行新增跨generation、authority、cancellation或re-add semantics

#### Scenario: 要求把行為移入本change
- **WHEN** 實作需要由本change接手原active change的product behavior、公開requirement、ABI、IPC或schema
- **THEN** 工作依C-level流程停止並要求使用者決策及跨change supersession/evidence invalidation

### Requirement: Active-change ownership precedes mutation
所有與active OpenSpec重疊的owned files SHALL 在任何mutation前建立owner matrix、依賴順序、semantic ownership決議及stale/replacement evidence處置；hash rebaseline MUST NOT取代ownership resolution。每個mutation leaf SHALL 執行immediate expected-hash/preimage check、post-write intended-hunk verification及new expected hash。

#### Scenario: 重疊owner尚未解決
- **WHEN** `application.rs`、Folder Size或MFT storage/service檔案仍被另一active change管理且沒有owner resolution
- **THEN** 相關mutation wave保持blocked，即使current hash可重新baseline亦不得寫入

#### Scenario: 寫入期間source drift
- **WHEN** mutation leaf的immediate preimage與expected hash不符
- **THEN** 該leaf停止、重新歸屬drift並建立replacement evidence，不得套用原patch

#### Scenario: 寫入完成
- **WHEN** mutation leaf完成source edit
- **THEN** evidence保存intended-hunk verification、新hash及未變更的外部hunks，後續leaf以該hash作為expected preimage

### Requirement: Single MFT compilation authority
共用MFT protocol、focus、journal、migration、persistence、runtime、size-map及SQLite實作 SHALL 由一個或經B-level核准的少數workspace-internal crates提供，App、helper、service不得再以 `#[path]` 各自編譯完整重複模組。

#### Scenario: App與service使用不同API子集
- **WHEN** App只需要client API而service需要server/storage API
- **THEN** internal crate以明確client/core/service module與visibility提供各自consumer，normal build不因另一target的使用情況產生target-local dead-code warning

#### Scenario: Internal crate移動前後相容
- **WHEN** consumer切換至internal crate
- **THEN** named-pipeframe、constants、SQLite schema/admission、migration fixtures及service/client round trip結果保持一致

#### Scenario: Visibility不能用來隱藏dead code
- **WHEN** internal crate新增或提升一個public item
- **THEN** consumer whitelist validator必須找到核准的App/helper/service call site及允許的module surface；無consumer的public item使gate失敗

#### Scenario: 發現dependency cycle
- **WHEN** crate抽離造成 explorer-app、model、common或其他crate循環相依
- **THEN** 受影響工作停止並作B-level design correction；不得以廣域suppression或不必要公開API繞過

### Requirement: Narrow suppression policy
新增的 `dead_code` suppression SHALL 僅能附著於無法由consumer、visibility或cfg正確表達的單一item，並包含具體consumer/compatibility contract、OpenSpec來源及移除條件；crate/module-wide suppression與generic reason SHALL 被拒絕。

#### Scenario: Compatibility entry必須保留
- **WHEN** versioned ABI callback或platform hook必須存在但Rust看不到靜態call
- **THEN** 可使用item-level `allow`或在lint必然發生時使用`expect`，reason完整記錄保留契約

#### Scenario: Generic suppression
- **WHEN** reason僅為「暫時未用」「未來使用」「Windows需要」或等價文字
- **THEN** policy gate失敗且該item不得完成

#### Scenario: 廣域suppression
- **WHEN** diff新增crate-level或module-level `allow(dead_code)`
- **THEN** policy gate失敗，即使workspace warning count已下降亦不得接受

### Requirement: Warning and behavior regression gates
完成時normal `cargo check --workspace --locked --offline` SHALL 產生零 `dead_code`及零 `unsafe_code` diagnostics，其他warning code不得高於baseline；format、focused targets、normal lib/bins、相關tests及strict OpenSpec validation SHALL通過。

#### Scenario: Normal workspace成功
- **WHEN** 最終locked/offline normal workspace check完成
- **THEN** structured counts顯示 `dead_code = 0`、`unsafe_code = 0`，其他warning category不超過baseline

#### Scenario: All-targets有範圍外錯誤
- **WHEN** `--all-targets`仍因未由本change修改的既有initializer或其他外部狀態失敗
- **THEN** evidence記錄精確error、file、line、baseline關係與out-of-scope分類，且不得誤報為passing

#### Scenario: 行為或contract drift
- **WHEN** scoped diff或focused tests顯示extension ABI、IPC、SQLite schema、migration、filesystem semantics、service identity或使用者行為改變
- **THEN** integration gate失敗並依B/C級流程處理，不得以lint reduction作為接受理由

### Requirement: Dirty-tree preservation and traceability
每個atomic task SHALL 對應唯一current evidence record，並記錄procedure、expected/actual、exit status或reviewer、source hashes、gate IDs、timestamp及adjustment lineage；實作 SHALL 保存所有不屬於本change的dirty-tree修改。

#### Scenario: Owned file含使用者修改
- **WHEN** dead-code hunk與既有dirty file重疊
- **THEN** 寫入前比對preimage與scoped diff，寫入後只歸屬本change hunk且不提交或回復使用者內容

#### Scenario: Evidence缺漏或hash不符
- **WHEN** mandatory task沒有唯一passed record、source hash過期或stale record沒有replacement
- **THEN** fail-closed evidence validator拒絕final gate
