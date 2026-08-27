# dead_code 改善執行清單

## 先讀這裡

請依編號執行，一次只做一個 L3。不要跳過工作包的完成門檻，也不要把多個 L3 合成一次修改。

常用詞：

- **consumer：** 現在真的會呼叫或讀取該 item 的程式。
- **replacement：** 已正式取代舊程式的新路徑。
- **disposition：** 這個 dead-code item 的保留或移除決定。
- **evidence：** 可重跑的 command、輸出、hash、diff 或測試結果。

每個會修改 source、test、manifest 或 lockfile 的步驟，都要遵守「共同寫入規則」：

1. 寫入前比對目前 SHA-256 與 evidence 內的預期 hash。
2. Hash 不同就停止該步驟，找出差異屬於誰，建立 stale/replacement 記錄。
3. 只重算 hash 不算解決 ownership；必須先確定哪個 change 負責該段行為。
4. 只修改該步驟點名的項目。
5. 寫入後立刻檢查 diff，只能有預期 hunk。
6. 記錄新 SHA-256，並證明其他既有 hunk 沒有改變。

每個完成的 L3 任務都要記錄：task ID、command、exit code、檔案 hash、warning 數量、結果。結果只能是 `passed`、有證據的 `not-applicable`，或有 replacement 的 `superseded`。

一個步驟若含多個 item 或 command，每一項都要有不同的 `subcheck_key`。

Disposition 只能使用：

- `remove-superseded`
- `remove-unreferenced`
- `retain-cross-target-live`
- `test-only`
- `retain-required-contract`
- `retain-narrow-suppression`

禁止 crate-wide 或 module-wide `allow(dead_code)`。只有明確契約需要時，才可對單一 item 使用帶原因、owner 與移除條件的 suppression。

## 1. 建立基線與安全工具

### 1.1 建立 compiler 基線

**目的：** 固定目前的 `dead_code` 警告與編譯 target。
**輸入：** Rust 1.97.1、目前 workspace、目前工作樹。
**產出：** `evidence/baseline.json`、compiler JSON、檔案 hash。
**依賴：** 無。
**Owner／Wave：** Primary／1。
**Gate／Evidence：** DCG-INVENTORY；`evidence/baseline.json`。
**完成門檻：** 可重現 417 筆 diagnostics、322 個 canonical sites，並列出每個 MFT source 的 targets。

- [x] 1.1.1 記錄 revision、dirty paths、rustc/cargo 版本、host triple、Cargo config、features 與 baseline command。證據：`baseline.json/environment`。
- [x] 1.1.2 執行 normal workspace compiler JSON check，保存原始輸出與各 warning code 數量。證據：`baseline.json/diagnostics`。
- [x] 1.1.3 正規化 `src/bin/../` 路徑，建立 canonical diagnostic ID。證據：`baseline.json/canonical_sites`。
- [x] 1.1.4 展開 multi-method diagnostic，讓每個 method 有獨立 item ID。證據：`baseline.json/items`。
- [x] 1.1.5 列出每個 MFT source 會被 App library、helper、service 的哪些 targets 編譯。證據：`baseline.json/target_topology`。
- [x] 1.1.6 保存預定修改檔案的 SHA-256、現有 diff 與修改歸屬。證據：`evidence/prechange-diffs/`。

### 1.2 建立 dead code 決策表

**目的：** 在改程式前，先決定每個 item 要刪除或保留。
**輸入：** 1.1、Git 歷史、相關 OpenSpec。
**產出：** `evidence/decision-ledger.json`、ownership matrix。
**依賴：** 1.1。
**Owner／Wave：** Primary／1。
**Gate／Evidence：** DCG-INVENTORY、DCG-OWNERSHIP；decision ledger。
**完成門檻：** 每個 item 只有一個 disposition，每個重疊檔案都有 owner。

- [x] 1.2.1 查 Git 與 OpenSpec，記錄 Code Lines、Details cache、Size Map、Folder Size、MFT、UI、bookmark、runtime authority 的形成與取代歷史。證據：`decision-ledger.json/history`。
- [x] 1.2.2 標出仍要保留的 legacy readers、required contracts，以及已被取代的 writers/helpers。證據：`decision-ledger.json/contracts`。
- [x] 1.2.3 為每個 item 指定一個合法 disposition。證據：`decision-ledger.json/items`。
- [x] 1.2.4 為刪除項寫 replacement 與回歸測試；為保留項寫 consumer 或 requirement。證據：每個 item 的 reason、replacement、validation。
- [x] 1.2.5 列出重疊檔案所屬 active change、未完成 task、可改 hunk、不可改 hunk、執行順序。證據：ownership matrix。
- [x] 1.2.6 解決每個重疊檔案的 owner；未解決的檔案標成 blocked。證據：ownership resolutions。

### 1.3 建立 evidence validator

**目的：** 自動擋下錯誤分類、hash drift 與不合法 suppression。
**輸入：** 1.1、1.2、dead-code-governance spec。
**產出：** Schema、validator、測試 fixtures。
**依賴：** 1.2。
**Owner／Wave：** Primary／1。
**Gate／Evidence：** DCG-POLICY；`evidence/governance-review.json`。
**完成門檻：** 正向 fixture 通過，負向 fixture 全部被拒絕。

- [x] 1.3.1 定義 diagnostic、item、disposition、task、hash、gate、timestamp、replacement 欄位。證據：schema test。
- [x] 1.3.2 實作 suppression scan，拒絕廣域 allow 與缺少原因、owner、移除條件的 reason。證據：scan fixtures。
- [x] 1.3.3 實作 hash、唯一 current task record、stale-to-replacement 驗證。證據：lineage fixtures。
- [x] 1.3.4 實作 `subcheck_key` 驗證。證據：missing/duplicate subcheck fixtures。
- [x] 1.3.5 執行 validator self-test，保存每個 fixture 的預期與實際結果。證據：governance review。

### 1.4 建立 legacy reader golden fixtures

**目的：** 刪除舊 writer 前，先保存仍要支援的 reader 格式。
**輸入：** 現行或已發布 sidecar 格式、migration specs、reader APIs。
**產出：** `crates/explorer-app/tests/fixtures/mft-legacy/`、manifest、reader-only tests。
**依賴：** 1.3。
**Owner／Wave：** Primary／1。
**Gate／Evidence：** DCG-OBSOLETE、DCG-OWNERSHIP；legacy golden evidence。
**完成門檻：** Reader tests 不呼叫待刪 writer，兩次測試後 fixture hash 不變。

- [x] 1.4.1 用相容 writer 產生最小 valid base/checkpoint/delta/status chain，記錄 producer revision 與格式版本。證據：fixture manifest。
- [x] 1.4.2 依共同寫入規則加入 binary fixtures，記錄用途、大小、SHA-256。證據：fixture manifest。
- [x] 1.4.3 依共同寫入規則加入 corruption、wrong identity、cursor 不連續、oversize fixtures。證據：逐 fixture subcheck。
- [x] 1.4.4 依共同寫入規則加入 unfocused-no-delete、failed-promotion-retry fixtures。證據：逐 fixture subcheck。
- [x] 1.4.5 依共同寫入規則新增 reader-only tests，覆蓋 `load_legacy_memory_index`、`read_checkpoint`、`deltas_after`、`validate_delta_after` 與必要 decode closure。證據：focused tests。
- [x] 1.4.6 建立 keep/remove whitelist，把上述 readers 與 decode closure 列為禁止刪除。證據：whitelist。
- [x] 1.4.7 連續跑 reader tests 兩次，比對執行前後 fixture hashes。證據：兩次 test log 與 hash。

## 2. 清理低風險程式

### 2.1 清理 UI 舊 wrapper

**目的：** 移除已被現行 UI 與 column registry 取代的程式。
**輸入：** `explorer-ui/src/chrome.rs`、UI tests。
**產出：** UI 修改、`evidence/batch-ui.json`。
**依賴：** 1.4。
**Owner／Wave：** Primary／2。
**Gate／Evidence：** DCG-OBSOLETE；batch evidence。
**完成門檻：** 舊 wrapper 消失，cache editor 與 details layout tests 通過。

- [x] 2.1.1 確認 `cache_usage_section` 無 production consumer，且新 cache editor 已取代它。證據：references 與 replacement test。
- [x] 2.1.2 依共同寫入規則移除 `cache_usage_section` 與專用 private helpers/imports。證據：scoped diff。
- [x] 2.1.3 依共同寫入規則移除 `view_item_width`、`details_horizontal_maximum`，讓 tests 改用 registry-aware API。證據：scoped diff。
- [x] 2.1.4 執行 cache editor、details layout、horizontal scroll tests。證據：batch evidence。

### 2.2 清理 Bookmark 與 Runtime Authority 舊 API

**目的：** 移除只供舊測試使用的 convenience API。
**輸入：** `state.rs`、`runtime_authority.rs`、相關 tests。
**產出：** Source/test 修改、`evidence/batch-bookmark-authority.json`。
**依賴：** 2.1。
**Owner／Wave：** Primary／2。
**Gate／Evidence：** DCG-OBSOLETE、DCG-TEST；batch evidence。
**完成門檻：** Tests 改走正式 API，bookmark 與 authority 行為不變。

- [x] 2.2.1 確認 `AppViewState::add_bookmark` 無 production consumer。證據：references。
- [x] 2.2.2 依共同寫入規則移除 `add_bookmark`，tests 改用正式 bookmark mutation API。證據：scoped diff。
- [x] 2.2.3 確認 `RuntimeAuthorityV1::revoke`、`replace_current` 只供測試。證據：references。
- [x] 2.2.4 依共同寫入規則移除兩個舊 API，tests 改用 `issue`、`revoke_feature`、`revoke_feature_incarnation`。證據：scoped diff。
- [x] 2.2.5 執行 bookmark persistence、bookmark UI、runtime authority fail-closed tests。證據：batch evidence。

## 3. 移除 Application 舊路徑

### 3.1 移除 App-owned Code Lines cache

**目的：** 移除已被 Host-prepared bounded snapshot 取代的掃描與 cache。
**輸入：** `application.rs`、Code Lines OpenSpec、Cargo manifests。
**產出：** Source/test/dependency 修改、`evidence/batch-code-lines.json`。
**依賴：** 2.2。
**Owner／Wave：** Primary／3。
**Gate／Evidence：** DCG-OBSOLETE；batch evidence。
**完成門檻：** App 不再直接掃描 directory 或維護 Code Lines disk cache。

- [x] 3.1.1 列出 cache constants、read/store/prune/replace、MoveFileExW、tokei scan 的 call/test/dependency closure。證據：closure list。
- [x] 3.1.2 證明 Rust/Lua folder Code Lines 正式路徑只使用 Host-prepared bounded snapshot。證據：consumer map。
- [x] 3.1.3 依共同寫入規則移除 App-owned cache、direct tokei scan、atomic publication、舊 tests。證據：scoped diff。
- [x] 3.1.4 若 explorer-app 已無 tokei consumer，依共同寫入規則移除 dependency 並 offline 更新 lockfile；否則記錄保留原因。證據：manifest/lockfile diff。
- [x] 3.1.5 執行 file/folder、8 MiB、unsupported source、file-count admission、Rust/Lua parity tests。證據：逐 command subcheck。
- [x] 3.1.6 執行 explorer-app locked/offline check，記錄 hash、disposition、warning delta。證據：batch evidence。

### 3.2 移除 Details Host cache 與單筆 query

**目的：** 讓 Details 只使用現行 MFT batch stream。
**輸入：** `application.rs`、batch protocol tests。
**產出：** Application 修改、`evidence/batch-details.json`。
**依賴：** 3.1。
**Owner／Wave：** Primary／3。
**Gate／Evidence：** DCG-OBSOLETE；batch evidence。
**完成門檻：** Host cache、TTL、single-query chain 消失，batch semantics 不變。

- [x] 3.2.1 依共同寫入規則移除 `FolderSizeCachedValueV1`、TTL setters、cache partition helpers、舊 cache tests。證據：scoped diff。
- [x] 3.2.2 依共同寫入規則移除 `exact_directory_facts`，把有效 assertions 搬到現行 projection tests。證據：scoped diff。
- [x] 3.2.3 依共同寫入規則移除單筆 deadline/receive helpers 與舊 worker constants/imports。證據：scoped diff。
- [x] 3.2.4 依共同寫入規則移除 `take_folder_size_requests`、`take_folder_size_request`，保留 bounded batch claimant。證據：scoped diff。
- [x] 3.2.5 執行 batch framing、visible order、per-item completion、timeout、cancel、stale-generation tests。證據：逐 command subcheck。
- [x] 3.2.6 執行 Details Folder Size、File Count、Folder Count、Code Lines admission tests。證據：batch evidence。

### 3.3 移除 Application 重複 Size Map scanner

**目的：** 只保留 shared Folder Size service 的 reference traversal。
**輸入：** Application Size Map tests、shared service tests。
**產出：** Test consolidation、`evidence/batch-size-map-app.json`。
**依賴：** 3.2。
**Owner／Wave：** Primary／3。
**Gate／Evidence：** DCG-OBSOLETE、DCG-TEST；batch evidence。
**完成門檻：** Application 沒有第二套 filesystem scanner。

- [x] 3.3.1 比對兩套 scanner tests，列出 shared service 缺少的 scenario。證據：coverage table。
- [x] 3.3.2 依共同寫入規則把唯一的 hard-link、reparse、boundary scenarios 搬到 shared service tests。證據：scoped diff。
- [x] 3.3.3 依共同寫入規則移除 `SizeMapHardLinkPolicyV1`、pending tree node、filesystem identity、舊 scanner closure。證據：scoped diff。
- [x] 3.3.4 執行 Size Map projection、recursive fallback、cancel、bounded tree tests。證據：batch evidence。

## 4. 整理 Folder Size service

### 4.1 移除只屬舊 Details 的 API

**目的：** 移除 obsolete aggregate API，但保留 Size Map 與 cleanup。
**輸入：** `folder_size_service.rs`、現行 Size Map consumers。
**產出：** Service/test 修改、`evidence/batch-folder-service.json`。
**依賴：** 3.3。
**Owner／Wave：** Primary／4。
**Gate／Evidence：** DCG-OBSOLETE；batch evidence。
**完成門檻：** 舊 Details API 消失，`snapshot_or_scan`、fallback、leases、cleanup 通過。

- [x] 4.1.1 將 warned methods 分成 Size Map active、Details obsolete、cleanup、test-only四類。證據：method map。
- [x] 4.1.2 依共同寫入規則移除無 consumer 的 budget/generation/invalidate/subscribe/aggregate/publish/release/counter closure。證據：逐 item subcheck。
- [x] 4.1.3 依共同寫入規則移除重複 serializer 的 encode/decode helpers 與舊 tests。證據：scoped diff。
- [x] 4.1.4 驗證 `snapshot_or_scan`、containment、MFT/helper/recursive fallback、leases、partial terminal semantics。證據：focused tests。
- [x] 4.1.5 驗證 obsolete Details cleanup 只處理核准 namespace 與 bounded files，不碰 Size Map。證據：cleanup tests。
- [x] 4.1.6 執行 shared snapshot、Size Map consumer、cleanup tests，記錄 warning delta。證據：batch evidence。

## 5. 移除 superseded MFT legacy 路徑

### 5.1 移除 legacy writer 與 query chain

**目的：** 移除已由 SQLite 與 live query 取代的程式，保留 migration readers。
**輸入：** `mft_journal.rs`、`mft_service.rs`、1.4 whitelist。
**產出：** Legacy removal、`evidence/batch-mft-legacy.json`。
**依賴：** 4.1。
**Owner／Wave：** Primary／5。
**Gate／Evidence：** DCG-OBSOLETE；batch evidence。
**完成門檻：** 無 consumer 的 writers/query/watch/publish 消失，受保護 readers 通過 golden tests。

- [x] 5.1.1 為每個 journal read/write/encode/decode helper 列出 consumer，分開 reader 與 writer。證據：consumer map。
- [x] 5.1.2 依共同寫入規則與 whitelist，逐項移除 coalesce/publication/status/delta writers、writer-only encoder。證據：逐 item subcheck。
- [x] 5.1.3 依共同寫入規則移除 legacy `query`、`query_inner`、`refresh_volume`、writer-only checkpoint state。證據：scoped diff。
- [x] 5.1.4 確認 `load_legacy_memory_index`、`read_checkpoint`、`deltas_after`、`validate_delta_after`、decode closure 沒有被刪除。證據：whitelist check。
- [x] 5.1.5 依共同寫入規則移除 `watch_volume`、`publish_pending`、fallback exhaustion、persisted limit、unused lifecycle wrapper。證據：逐 item subcheck。
- [x] 5.1.6 執行 golden reader、SQLite startup、journal catch-up、restart、rollback reader tests。證據：逐 command subcheck。
- [x] 5.1.7 執行 MFT live query、journal、migration、persistence tests，記錄 warning delta。證據：batch evidence。

### 5.2 清理 MFT 小型 dead code

**目的：** 移除沒有 compatibility contract 的 wrapper、constant、field、helper。
**輸入：** Query、focus、size-map、service decision ledger。
**產出：** 小型修改、`evidence/batch-mft-small-dead.json`。
**依賴：** 5.1。
**Owner／Wave：** Primary／5。
**Gate／Evidence：** DCG-OBSOLETE；batch evidence。
**完成門檻：** Items 已移除、test-only 或 required-contract，沒有 generic suppression。

- [x] 5.2.1 依共同寫入規則移除 `serve_folder_queries`，確認 service 使用 `serve_queries`。證據：references 與 diff。
- [x] 5.2.2 依共同寫入規則移除 mft_focus 未使用的 pipe error constants。證據：scoped diff。
- [x] 5.2.3 逐項檢查 MFT size-map warned methods；無 consumer 就移除，只供測試就移到 test boundary。證據：逐 item subcheck。
- [x] 5.2.4 依共同寫入規則移除 service 未讀欄位與 initializer。證據：scoped diff。
- [x] 5.2.5 執行 query protocol、focus lease、size-map aggregate、service diagnostics tests。證據：batch evidence。

## 6. 將 test seam 移出 production build

### 6.1 整理 Migration 與 SQLite test seam

**目的：** 保留 failure/atomicity tests，但 test helper 不進 normal build。
**輸入：** `mft_migration.rs`、`mft_sqlite.rs`、production APIs。
**產出：** `#[cfg(test)]` 邊界、tests、`evidence/batch-test-seams.json`。
**依賴：** 5.2。
**Owner／Wave：** Primary／6。
**Gate／Evidence：** DCG-TEST；batch evidence。
**完成門檻：** Normal build 不含 test helper，tests 仍走 production transaction 核心。

- [x] 6.1.1 將 migration cleanup/quarantine wrappers、hash helpers 分成 production、test-only、unreferenced。證據：classification table。
- [x] 6.1.2 依共同寫入規則把必要 migration test helper 放進 `#[cfg(test)]`。證據：scoped diff。
- [x] 6.1.3 依共同寫入規則讓其餘 migration tests 直接呼叫 production guarded/linearized API。證據：scoped diff。
- [x] 6.1.4 將 SQLite convenience APIs、failure selectors 分成 production、test-only、unreferenced。證據：classification table。
- [x] 6.1.5 依共同寫入規則把 failure controls、fixture wrappers 放進 `#[cfg(test)]`。證據：scoped diff。
- [x] 6.1.6 依共同寫入規則移除無獨立價值的 wrapper，tests 改用 bounded/cancelled/focused/linearized API。證據：scoped diff。
- [x] 6.1.7 執行 commit failure、cursor atomicity、migration reopen、WAL threshold、shutdown no-write tests。證據：逐 command subcheck。
- [x] 6.1.8 分別執行 normal 與 test target structured checks，確認 normal build 不含 test seam。證據：batch evidence。

## 7. 保留其他 OpenSpec 的 contract

### 7.1 保留 Recovery 與 Migration typed contract

**目的：** 保留原 OpenSpec ownership，本 change 不新增 runtime 行為。
**輸入：** `RecoveryReasonV1`、`MigrationStateV1`、`mft-sqlite-foreground-persistence`。
**產出：** `evidence/recovery-contract-ownership.json`、窄 item disposition。
**依賴：** 6.1。
**Owner／Wave：** Primary／7；產品 owner 是 `mft-sqlite-foreground-persistence`。
**Gate／Evidence：** DCG-CONTRACT-OWNERSHIP；ownership evidence。
**完成門檻：** 每個 item 有原 owner/task/evidence/expiry，本 change 不新增 producer 或 consumer。

- [x] 7.1.1 記錄 `RecoveryReasonV1` 的 requirement、未完成 task、wiring 缺口、到期條件。證據：ownership JSON。
- [x] 7.1.2 記錄 `MigrationStateV1` 的 requirement、未完成 task、wiring 缺口、到期條件。證據：ownership JSON。
- [x] 7.1.3 在原 owning change 記錄 disposition 依賴，不要把原產品 task 標成完成。證據：lineage link。
- [x] 7.1.4 必要時依共同寫入規則加入 item-level suppression；reason 寫 owner、task、wiring 完成即移除。證據：policy scan。
- [x] 7.1.5 執行 persistence/diagnostics tests 與 behavior diff，確認無新增 runtime 行為。證據：ownership evidence。

### 7.2 保留 Snapshot Remove delta contract

**目的：** 保留 shared Folder Size OpenSpec ownership，不實作 remove semantics。
**輸入：** `SnapshotDeltaV1::Remove`、`centralize-shared-folder-size-service`。
**產出：** `evidence/remove-delta-contract-ownership.json`、窄 item disposition。
**依賴：** 7.1。
**Owner／Wave：** Primary／7；產品 owner 是 `centralize-shared-folder-size-service`。
**Gate／Evidence：** DCG-CONTRACT-OWNERSHIP；ownership evidence。
**完成門檻：** Variant 有 owner/task/evidence/expiry，本 change 不新增 emitter/application。

- [x] 7.2.1 記錄 Remove variant 的 requirement、未完成 task、wiring 缺口、到期條件。證據：ownership JSON。
- [x] 7.2.2 在原 owning change 記錄 disposition 依賴，不要把原產品 task 標成完成。證據：lineage link。
- [x] 7.2.3 必要時依共同寫入規則加入 item-level suppression；reason 寫 owner、task、wiring 完成即移除。證據：policy scan。
- [x] 7.2.4 執行 shared snapshot 與 Size Map tests，確認無新增 remove semantics。證據：ownership evidence。
- [x] 7.2.5 若本 change 必須實作 remove behavior，停止本分支並建立 C-level blocker。證據：blocker record。

## 8. 建立單一 MFT internal crate

### 8.1 建立 crate 與依賴邊界

**目的：** 建立 `crates/explorer-mft`，先不搬行為。
**輸入：** 清理後 MFT modules、Cargo manifests、consumer map。
**產出：** Dependency map、crate scaffold、`evidence/mft-topology-baseline.json`。
**依賴：** 7.2。
**Owner／Wave：** Primary／8。
**Gate／Evidence：** DCG-MFT-TOPOLOGY；topology evidence。
**完成門檻：** Crate 可 locked/offline 編譯，沒有 dependency cycle。

- [x] 8.1.1 列出 MFT module dependencies 與 App/helper/service 使用的 symbols。證據：dependency/consumer map。
- [x] 8.1.2 保存 pipe frame/constants、SQLite schema/admission、migration fixtures、focused test baseline。證據：topology baseline。
- [x] 8.1.3 定義 client/core/service API，列出禁止公開的 handles、paths、mutation/storage internals。證據：visibility table。
- [x] 8.1.4 依共同寫入規則新增 `publish = false` 的 `crates/explorer-mft`、workspace member、最小 dependencies。證據：manifest diff。
- [x] 8.1.5 依共同寫入規則 offline 更新 `Cargo.lock`，確認只新增預期 path package/dependency edge。證據：lockfile diff/hash。
- [x] 8.1.6 執行 Cargo metadata 與 crate check；有 cycle 就走 B-level correction。證據：topology evidence。

### 8.2 移動 Protocol、Query 與 Focus

**目的：** 讓 IPC 與 focus 程式只編譯一次。
**輸入：** 8.1 crate 與 API inventory。
**產出：** Moved modules、consumer 修改、`evidence/batch-mft-protocol-move.json`。
**依賴：** 8.1。
**Owner／Wave：** Primary／8。
**Gate／Evidence：** DCG-MFT-TOPOLOGY；batch evidence。
**完成門檻：** App/service 不再各自 path-compile query/focus，layout 不變。

- [x] 8.2.1 依共同寫入規則移動 query protocol/types/client/server modules。證據：old/new hash 與 move map。
- [x] 8.2.2 依共同寫入規則更新 App、helper、service imports，移除舊 query path modules。證據：scoped diff。
- [x] 8.2.3 依共同寫入規則移動 focus lease/reporting/auth protocol。證據：old/new hash 與 move map。
- [x] 8.2.4 執行 query round-trip、batch、malformed bounds、focus auth/expiry/disconnect、service stop tests。證據：逐 command subcheck。
- [x] 8.2.5 比對 frame layout、API surface、warning delta。證據：batch evidence。

### 8.3 移動 Journal、Persistence 與 Runtime

**目的：** 讓 journal、scheduler、runtime 只編譯一次。
**輸入：** 8.2 crate、清理後 modules。
**產出：** Moved modules、consumer 修改、`evidence/batch-mft-runtime-move.json`。
**依賴：** 8.2。
**Owner／Wave：** Primary／8。
**Gate／Evidence：** DCG-MFT-TOPOLOGY；batch evidence。
**完成門檻：** App 不再編譯 server-only runtime，reader 與 lifecycle tests 通過。

- [x] 8.3.1 依共同寫入規則移動 journal types/readers/normalization，保留 legacy reader、Windows IO 邊界。證據：move map。
- [x] 8.3.2 依共同寫入規則移動 persistence scheduler、focus registry、lifecycle barrier。證據：move map。
- [x] 8.3.3 依共同寫入規則移動 volume memory runtime 並更新 consumers。證據：scoped diff。
- [x] 8.3.4 執行 journal catch-up、pending batch、commit failure/success、stop/restart tests。證據：逐 command subcheck。
- [x] 8.3.5 比對 API、behavior snapshots、hash、warning delta。證據：batch evidence。

### 8.4 移動 Size Map、Migration 與 SQLite

**目的：** 讓 storage/index core 只編譯一次。
**輸入：** 8.3 crate、6.1 test boundaries。
**產出：** Moved modules、consumer 修改、`evidence/batch-mft-storage-move.json`。
**依賴：** 8.3。
**Owner／Wave：** Primary／8。
**Gate／Evidence：** DCG-MFT-TOPOLOGY；batch evidence。
**完成門檻：** Schema、atomic replace、legacy promotion、budget semantics 不變。

- [x] 8.4.1 依共同寫入規則移動 MFT index/aggregate/volume reader、Windows helpers。證據：move map。
- [x] 8.4.2 依共同寫入規則移動 migration inventory/quarantine/promotion core。證據：move map。
- [x] 8.4.3 依共同寫入規則移動 SQLite store/admission/transaction/WAL core、test-only seams。證據：move map。
- [x] 8.4.4 依共同寫入規則更新 service/helper/App consumers，移除舊 MFT `#[path]` duplication。證據：scoped diff。
- [x] 8.4.5 執行 index、aggregate、migration、SQLite atomicity/WAL、budget、helper tests。證據：逐 command subcheck。

### 8.5 關閉 MFT topology

**目的：** 所有 consumers 使用 internal crate，不能用亂加 `pub` 消除警告。
**輸入：** 8.2–8.4、baseline ledger。
**產出：** Final API map、`evidence/mft-topology-final.json`。
**依賴：** 8.4。
**Owner／Wave：** Primary／8。
**Gate／Evidence：** DCG-MFT-TOPOLOGY；topology evidence。
**完成門檻：** 無 duplicate modules、dependency cycle、不必要 public API。

- [x] 8.5.1 掃描 workspace，確認 App/helper/service 使用 internal crate，沒有 MFT `#[path]` duplication。證據：scan result。
- [x] 8.5.2 建立 consumer whitelist；每個新增或提高 visibility 的 item 都要有核准 call site。證據：whitelist。
- [x] 8.5.3 執行 visibility、dependency direction、duplicate module validators。證據：topology evidence。
- [x] 8.5.4 執行 explorer-mft、explorer-app lib、normal binaries、extension-host locked/offline checks。證據：逐 command subcheck。
- [x] 8.5.5 將 251 個 target-local diagnostics 對應到 duplicate compilation 消失或核准 disposition。證據：closure map。

## 9. 最終驗證

### 9.1 執行 workspace gates

**目的：** 證明 normal workspace 的 `dead_code`、`unsafe_code` 都是零。
**輸入：** 所有 batch evidence、目前 source、baseline counts。
**產出：** `evidence/integration-validation.json`。
**依賴：** 8.5。
**Owner／Wave：** Primary／9。
**Gate／Evidence：** DCG-POLICY、DCG-INTEGRATION；integration evidence。
**完成門檻：** `dead_code=0`、`unsafe_code=0`，其他 warning 不高於 baseline。

- [x] 9.1.1 只格式化本 change 修改的 Rust files，再執行 `cargo fmt --all --check`。證據：format log 與 scoped diff。
- [x] 9.1.2 執行 suppression/governance scan，拒絕廣域 allow、generic reason、未分類 item、未達成 expect。證據：policy result。
- [x] 9.1.3 執行 explorer-mft、explorer-app、explorer-ui、explorer-extension-host focused locked/offline checks。證據：逐 command subcheck。
- [x] 9.1.4 執行 Code Lines、Folder Size、Size Map、MFT、SQLite、bookmark、authority tests。證據：逐 command subcheck。
- [x] 9.1.5 執行 `cargo check --workspace --lib --bins --locked --offline`，保存 structured diagnostics。證據：integration evidence。
- [x] 9.1.6 執行 `cargo check --workspace --locked --offline`，檢查 dead_code、unsafe_code、其他 warning 數量。證據：integration evidence。

### 9.2 完成 all-target 與 OpenSpec 驗證

**目的：** 記錄 all-target 狀態，關閉所有 task/evidence。
**輸入：** 9.1、所有 artifacts、dirty-tree attribution。
**產出：** `evidence/all-target-status.json`、`evidence/final-validation.json`、`evidence/index.json`。
**依賴：** 9.1。
**Owner／Wave：** Primary／10。
**Gate／Evidence：** DCG-FINAL；final evidence。
**完成門檻：** 每個 task 有唯一 terminal record，strict validation 通過。

- [x] 9.2.1 執行 `cargo check --workspace --all-targets --locked --offline`；若仍有既存 `folder_admission` initializer errors，記錄精確錯誤與 baseline 關係。證據：all-target status。
- [x] 9.2.2 核對 decision ledger，確認每個 baseline diagnostic/item 有 terminal disposition。證據：closure report。
- [x] 9.2.3 建立 evidence index；每個 task 只有一個 current record，stale/superseded 都有 replacement。證據：evidence index。
- [x] 9.2.4 審核 scoped diff，確認 ABI、IPC、schema、persistence、migration、filesystem、process topology、其他人的 dirty hunks 不變。證據：final diff review。
- [x] 9.2.5 執行 fail-closed evidence validator 與 task structure validator。證據：validator logs。
- [x] 9.2.6 執行 `openspec validate govern-dead-code-warnings --strict`。證據：strict validation log。
- [x] 9.2.7 記錄 final revision、toolchain、warning counts、窄 suppression、移除 dependencies、all-target 狀態、剩餘風險。證據：final validation。
