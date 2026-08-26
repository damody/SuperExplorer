> **所有mutation leaves的共同完成條件：** 每個包含新增、移除、移動、更新或修改source/manifest/test的L3，在寫入前必須執行immediate expected-hash/preimage check；寫入後立即驗證intended hunk、保存new expected hash及外部hunks未變證據。若active-change ownership未解決或preimage drift，該leaf保持blocked並建立stale/replacement lineage。包含多個baseline items或commands的leaf，evidence index必須為每個item/command建立唯一immutable `subcheck_key`，不得只以task ID概括通過。

## 1. 基線、分類與治理工具

### 1.1 Target-aware immutable baseline

**目的：** 凍結每個 `dead_code` diagnostic、primary item、target topology及dirty-tree attribution。
**輸入：** Rust 1.97.1、locked workspace、目前工作樹、proposal/design/spec。
**產出：** `evidence/baseline.json`、structured compiler log、owned-file hashes與prechange diffs。
**依賴：** 無。
**Owner／Wave：** Primary／1。
**Gate／Evidence：** DCG-INVENTORY；`evidence/baseline.json`、`evidence/prechange-diffs/`。
**完成門檻：** 417 emitted diagnostics、322 canonical sites、primary items及完整target sets均可重現，所有owned files可歸屬且沒有未解釋差異。

- [ ] 1.1.1 記錄revision、dirty paths、rustc/cargo版本、host triple、Cargo config、features、相關environment與locked/offline baseline command。
- [ ] 1.1.2 執行normal workspace compiler JSON，保存原始事件並記錄各warning code的emitted count。
- [ ] 1.1.3 正規化 `src/bin/../` 路徑，以file、line、column、message建立canonical diagnostic IDs及emitting targets。
- [ ] 1.1.4 展開multi-method diagnostics的所有primary spans，為每個item記錄symbol/text、source range與parent diagnostic ID。
- [ ] 1.1.5 盤點每個MFT source實際會被App library、helper、service哪些targets編譯，區分target-local與all-target dead code。
- [ ] 1.1.6 對所有預定owned files保存SHA-256、scoped prechange diff及既有修改的歸屬說明。

### 1.2 歷史、OpenSpec lineage與唯一disposition

**目的：** 讓每個item在寫程式前已有可驗證的保留／移除理由。
**輸入：** 1.1 baseline、Git history、所有相關active/completed OpenSpec。
**產出：** `evidence/decision-ledger.json`與requirement lineage map。
**依賴：** 1.1。
**Owner／Wave：** Primary／1。
**Gate／Evidence：** DCG-INVENTORY、DCG-POLICY；`evidence/decision-ledger.json`。
**完成門檻：** 每個current item恰有一個合法disposition、replacement/consumer/spec證據及預定validation；沒有future-use或search-only判定。

- [ ] 1.2.1 對App Code Lines、Details cache、Size Map、Folder Size、MFT sidecar/SQLite、UI、bookmark及runtime authority建立Git introduction/supersession lineage。
- [ ] 1.2.2 對照現行OpenSpec requirements，標記legacy rollback readers、required-contract items及已supersede writers/helpers。
- [ ] 1.2.3 將每個item分類為 `remove-superseded`、`remove-unreferenced`、`retain-cross-target-live`、`test-only`、`retain-required-contract`或`retain-narrow-suppression`。
- [ ] 1.2.4 對每個removal chain記錄正式replacement與至少一個防回歸test；對每個retained item記錄實際consumer或normative requirement。
- [ ] 1.2.5 建立active-change ownership matrix，逐檔列出owning changes、未完成tasks、semantic responsibility、允許/禁止hunks與依賴順序。
- [ ] 1.2.6 為每個重疊檔案取得owner resolution並記錄需stale/replacement的既有evidence；沒有resolution者明確阻止後續相關wave。
- [ ] 1.2.7 驗證hash rebaseline沒有被當作semantic ownership替代品，並將DCG-OWNERSHIP gate結果寫入decision ledger。

### 1.3 Fail-closed evidence與suppression validator

**目的：** 自動拒絕未分類、hash drift、廣域suppression、generic reason及不完整task evidence。
**輸入：** 1.1 baseline、1.2 decision ledger、dead-code-governance spec。
**產出：** `evidence/schema.json`、validator及正反fixtures。
**依賴：** 1.2。
**Owner／Wave：** Primary／1。
**Gate／Evidence：** DCG-POLICY；`evidence/governance-review.json`。
**完成門檻：** Validator接受完整fixture，拒絕duplicate/missing disposition、unknown ID、stale hash、無replacement lineage與不合法suppression。

- [ ] 1.3.1 定義diagnostic/item/disposition/task/hash/gate/timestamp/adjustment evidence schema。
- [ ] 1.3.2 實作source scan，拒絕新增crate/module-wide `allow(dead_code)`及無具體contract/removal condition的reason。
- [ ] 1.3.3 實作baseline與current source hash、唯一current task record及stale-to-replacement lineage驗證。
- [ ] 1.3.4 建立passing、missing、duplicate、unknown、hash-mismatch、generic-reason與broad-suppression fixtures並執行self-test。
- [ ] 1.3.5 實作task structure/atomic-subcheck validator，建立compound-leaf、missing-subcheck與duplicate-subcheck負向fixtures並執行self-test。

### 1.4 Legacy reader immutable golden chains

**目的：** 在刪除任何sidecar writer/encoder前，以獨立checked-in bytes凍結仍受migration/rollback規格保護的reader行為。
**輸入：** 現行/已發布 `.semftidx`、checkpoint、delta、status格式，migration specs與reader APIs。
**產出：** `crates/explorer-app/tests/fixtures/mft-legacy/`（或B-level核准等價路徑）、fixture manifest、SHA-256與reader-only tests。
**依賴：** 1.3。
**Owner／Wave：** Primary／1。
**Gate／Evidence：** DCG-OBSOLETE、DCG-OWNERSHIP；`evidence/legacy-golden-baseline.json`。
**完成門檻：** Golden bytes不由test執行時的待刪writer產生；正向與所有failure/boundary readers均可獨立重現，symbol keep/remove whitelist已凍結。

- [ ] 1.4.1 從目前或已發布相容writer產生最小valid base/checkpoint/delta/status chain，保存原始producer revision與format版本。
- [ ] 1.4.2 將golden chain以binary fixtures checked in並建立包含每檔用途、size與SHA-256的manifest。
- [ ] 1.4.3 建立corruption、wrong volume/journal identity、cursor non-contiguity及oversize/bounds golden variants。
- [ ] 1.4.4 建立unfocused no-delete與failed-promotion retry fixture，證明reader/migration failure不修改golden或canonical destination。
- [ ] 1.4.5 新增reader-only tests，禁止呼叫待刪writer/encoder，覆蓋 `load_legacy_memory_index`、`read_checkpoint`、`deltas_after`、`validate_delta_after`及必要decode closure。
- [ ] 1.4.6 建立legacy symbol keep/remove whitelist；將上述reader與decode closure列為禁止刪除，除非C-level修改migration capability。
- [ ] 1.4.7 執行golden reader tests兩次並比對fixture hashes，證明tests無自我改寫且結果deterministic。

## 2. 低風險UI、Bookmark與Runtime Authority清理

### 2.1 UI duplicate與registry-unaware wrappers

**目的：** 移除已被現行UI composition與column registry API取代的窄wrapper。
**輸入：** DCG baseline中 `explorer-ui/src/chrome.rs` 的items及現行cache editor/registry consumers。
**產出：** 最小UI source/test edits與 `evidence/batch-ui.json`。
**依賴：** 1.4。
**Owner／Wave：** Primary／2。
**Gate／Evidence：** DCG-OBSOLETE；`evidence/batch-ui.json`。
**完成門檻：** Duplicate renderer及舊wrappers移除，現行cache editor、details width、horizontal scroll tests通過且UI behavior不變。

- [ ] 2.1.1 比對 `chrome.rs` preimage與current diff，確認 `cache_usage_section` 沒有production consumer且新cache editor section完整取代。
- [ ] 2.1.2 移除 `cache_usage_section`及只服務該duplicate的private helpers/imports。
- [ ] 2.1.3 移除 `view_item_width`與`details_horizontal_maximum` wrappers，將有效tests改用registry-aware API與built-in registry fixture。
- [ ] 2.1.4 執行focused explorer-ui cache editor、details layout及horizontal scroll tests並記錄warning delta。

### 2.2 Bookmark與runtime authority convenience APIs

**目的：** 刪除只有測試使用的舊mutation/replacement入口，同時保留正式安全語意測試。
**輸入：** `state.rs`、`runtime_authority.rs` baseline及bookmark/authority specs。
**產出：** Source/test edits與 `evidence/batch-bookmark-authority.json`。
**依賴：** 2.1。
**Owner／Wave：** Primary／2。
**Gate／Evidence：** DCG-OBSOLETE、DCG-TEST；`evidence/batch-bookmark-authority.json`。
**完成門檻：** 舊API移除，tests改走正式mutation、issue及feature/incarnation revoke路徑，bookmark persistence與authority fail-closed tests通過。

- [ ] 2.2.1 確認 `AppViewState::add_bookmark` 無production consumer，移除wrapper並將tests改用正式bookmark mutation入口。
- [ ] 2.2.2 確認 `RuntimeAuthorityV1::revoke`、`replace_current` 僅供tests，移除後以 `issue`、`revoke_feature`、`revoke_feature_incarnation`重建相同adversarial scenarios。
- [ ] 2.2.3 執行focused bookmark model/UI、independent persistence及runtime authority tests。
- [ ] 2.2.4 記錄public/internal API diff，證明沒有extension ABI、capability或revocation behavior改變。

## 3. Application舊Code Lines與Details路徑移除

### 3.1 App-owned Code Lines directory scan/cache

**目的：** 移除已被Host-prepared bounded snapshots取代的App tokei directory掃描與persistent cache chain。
**輸入：** `application.rs` baseline、Code Lines input-preparation及directory-count OpenSpecs。
**產出：** Cohesive source/test/dependency cleanup與 `evidence/batch-code-lines.json`。
**依賴：** 2.2。
**Owner／Wave：** Primary／3。
**Gate／Evidence：** DCG-OBSOLETE；`evidence/batch-code-lines.json`。
**完成門檻：** App不再直接掃描directory或維護Code Lines disk cache，official providers仍從Host-attested bounded input得到相同值，無stale dependency。

- [ ] 3.1.1 盤點Code Lines cache constants、key/hash/read/prune/store/replace、MoveFileExW與measure functions的完整call/test/dependency closure。
- [ ] 3.1.2 證明現行Rust/Lua folder Code Lines production dispatch只使用Host-prepared bounded snapshot及既有admission gates。
- [ ] 3.1.3 移除App-owned directory cache、direct tokei scan、atomic cache publication與只驗證舊架構的tests。
- [ ] 3.1.4 若 `explorer-app` 不再使用tokei，移除其direct dependency並更新lockfile；若仍有consumer，記錄精確保留原因。
- [ ] 3.1.5 執行Code Lines file/folder、8 MiB boundary、unsupported source、file-count admission及Rust/Lua parity tests。
- [ ] 3.1.6 執行locked/offline explorer-app check並記錄本批canonical dispositions、hashes與warning delta。

### 3.2 Details Host cache、TTL與pre-batch helpers

**目的：** 移除已由MFT Service batch stream取代的Details aggregate cache及單筆query程式。
**輸入：** `application.rs`、`fix-shared-mft-folder-aggregate-lru` design/spec、batch protocol tests。
**產出：** Minimal application edits與 `evidence/batch-details.json`。
**依賴：** 3.1。
**Owner／Wave：** Primary／3。
**Gate／Evidence：** DCG-OBSOLETE；`evidence/batch-details.json`。
**完成門檻：** Details只走current batch stream，Host aggregate cache/TTL/single-query chain不存在，exact/unavailable/cancel/stale semantics不變。

- [ ] 3.2.1 移除 `FolderSizeCachedValueV1`、TTL setters及folder-cache partition helpers與舊cache tests。
- [ ] 3.2.2 移除 `exact_directory_facts` 舊projection helper並將仍有效assertions移至current MFT response projection tests。
- [ ] 3.2.3 移除單筆deadline/receive helpers及只有舊worker使用的constants/imports。
- [ ] 3.2.4 移除 `take_folder_size_requests`、`take_folder_size_request`，保留並驗證current bounded batch claimant。
- [ ] 3.2.5 執行batch framing、visible order、per-item completion、timeout、cancellation與stale-generation tests。
- [ ] 3.2.6 執行Details Folder Size/File Count/Folder Count及Code Lines admission focused tests。
- [ ] 3.2.7 記錄source hashes、canonical dispositions、warning delta及無Host L1 cache的scoped diff review。

### 3.3 Application重複Size Map reference fixtures

**目的：** 清除已由shared Folder Size service擁有的重複recursive scanner，而保留正式Size Map projection與fallback authority。
**輸入：** `application.rs` Size Map test-only chain、shared service tests/spec。
**產出：** Source/test consolidation與 `evidence/batch-size-map-app.json`。
**依賴：** 3.2。
**Owner／Wave：** Primary／3。
**Gate／Evidence：** DCG-OBSOLETE、DCG-TEST；`evidence/batch-size-map-app.json`。
**完成門檻：** Application沒有第二套filesystem scanner；shared service仍測試reparse、hard-link、partial、cancel及resource limits。

- [ ] 3.3.1 比對App test-only scanner與FolderSizeService reference traversal的semantic coverage，列出需搬移的唯一fixtures。
- [ ] 3.3.2 將仍唯一的hard-link/reparse/boundary scenarios搬到shared service tests。
- [ ] 3.3.3 移除App的 `SizeMapHardLinkPolicyV1`、pending tree node、filesystem identity及舊scanner/test closure。
- [ ] 3.3.4 執行Size Map projection、recursive fallback、cancel與bounded tree tests並記錄warning delta。

## 4. Folder Size service與legacy snapshot責任整理

### 4.1 Aggregate-only APIs與obsolete Details persistence

**目的：** 從仍服務Size Map的FolderSizeService移除只屬舊Details路徑的API，保留必要snapshot/fallback與bounded cleanup。
**輸入：** `folder_size_service.rs` baseline、current Size Map consumer、centralize/fix-shared specs。
**產出：** Service/API/test edits與 `evidence/batch-folder-service.json`。
**依賴：** 3.3。
**Owner／Wave：** Primary／4。
**Gate／Evidence：** DCG-OBSOLETE；`evidence/batch-folder-service.json`。
**完成門檻：** 所有warned aggregate-only methods有removed disposition，Size Map `snapshot_or_scan`、fallback、leases及obsolete namespace cleanup仍通過。

- [ ] 4.1.1 建立FolderSizeService method reachability map，區分Size Map active path、Details obsolete path、cleanup與tests。
- [ ] 4.1.2 移除set-budget/generation/invalidate/subscribe/aggregate-only/publish/release/counters中沒有current consumer的完整closure。
- [ ] 4.1.3 移除無production consumer且重複實際persistence serializer的bounded encode/decode helpers與舊tests。
- [ ] 4.1.4 保留並驗證 `snapshot_or_scan`、canonical containment、MFT/helper/recursive fallback、leases及partial terminal semantics。
- [ ] 4.1.5 驗證obsolete Details snapshot cleanup仍只處理核准namespace、bounded files且不碰Size Map內容。
- [ ] 4.1.6 執行shared folder snapshot、Size Map consumer及cleanup focused tests並記錄hash/warning delta。

## 5. Superseded MFT sidecar與舊service路徑移除

### 5.1 Journal/service legacy writer與query chain

**目的：** 移除已被SQLite durability與live query取代、且不再承擔migration reader責任的sidecar writer及service程式。
**輸入：** `mft_journal.rs`、`mft_service.rs`、event-driven與SQLite OpenSpecs、migration consumers。
**產出：** Cohesive legacy removal與 `evidence/batch-mft-legacy.json`。
**依賴：** 4.1。
**Owner／Wave：** Primary／5。
**Gate／Evidence：** DCG-OBSOLETE；`evidence/batch-mft-legacy.json`。
**完成門檻：** 未使用的delta/status publication、legacy aggregate query/refresh/watch/publish/limit程式移除；仍受migration/rollback要求的readers通過fixtures。

- [ ] 5.1.1 對每個journal encode/decode/read/write helper建立current migration/service consumer map，明確分離reader與writer。
- [ ] 5.1.2 依1.4 keep/remove whitelist逐item移除無consumer的coalesce/publication/status/delta writers及其writer-only encode chain；每個item使用唯一subcheck。
- [ ] 5.1.3 依1.4 whitelist逐item移除service legacy `query/query_inner/refresh_volume`與writer-only checkpoint cache state；禁止刪除 `load_legacy_memory_index`及其reader closure。
- [ ] 5.1.4 移除舊 `watch_volume`、`publish_pending`、fallback exhaustion、persisted limit與unused lifecycle wrapper closure。
- [ ] 5.1.5 驗證SQLite startup、legacy migration input、journal catch-up、restart及rollback reader scenarios不依賴已移除writers。
- [ ] 5.1.6 執行MFT service live query、journal、SQLite migration與persistence focused tests。
- [ ] 5.1.7 記錄每個legacy item disposition、replacement requirement、source hashes與warning delta。

### 5.2 Query、focus、size-map及service小型殘留

**目的：** 清除沒有compatibility contract的小型wrapper、constant、field與helper。
**輸入：** MFT query/focus/size-map/service decision ledger。
**產出：** Minimal edits與 `evidence/batch-mft-small-dead.json`。
**依賴：** 5.1。
**Owner／Wave：** Primary／5。
**Gate／Evidence：** DCG-OBSOLETE；`evidence/batch-mft-small-dead.json`。
**完成門檻：** 所有small all-target-dead items已移除或轉為test-only/required-contract，無generic suppression。

- [ ] 5.2.1 移除 `serve_folder_queries` 舊wrapper並驗證service仍使用完整versioned `serve_queries`。
- [ ] 5.2.2 移除mft_focus未使用pipe error constants及其他沒有current error-path consumer的常數。
- [ ] 5.2.3 對MFT size-map warned methods逐一確認active/test consumer，移除無引用helpers或移到test boundary。
- [ ] 5.2.4 移除service未讀欄位及其initializer，確認diagnostic/accounting輸出不變。
- [ ] 5.2.5 執行query protocol、focus lease、size-map index/aggregate及service diagnostics tests。

## 6. Test-only seam與production API邊界

### 6.1 Migration與SQLite test seam

**目的：** 保留atomicity、migration、failure及WAL測試能力，但使其不進normal build。
**輸入：** `mft_migration.rs`、`mft_sqlite.rs` decision ledger及現行linearized/bounded APIs。
**產出：** `#[cfg(test)]` boundaries、rewritten tests與 `evidence/batch-test-seams.json`。
**依賴：** 5.2。
**Owner／Wave：** Primary／6。
**Gate／Evidence：** DCG-TEST；`evidence/batch-test-seams.json`。
**完成門檻：** Normal build不含test-only wrappers/failure selectors；atomic commit、migration、rebuild、WAL與lifecycle tests仍測到production core。

- [ ] 6.1.1 分類migration cleanup/quarantine wrappers及hash helpers為production core、test convenience或真正unreferenced。
- [ ] 6.1.2 將必要test convenience移入 `#[cfg(test)]`，其餘tests改呼叫production guarded/linearized APIs。
- [ ] 6.1.3 分類SQLite create/load/migrate/install/commit/truncate convenience APIs及failure selectors的production/test reachability。
- [ ] 6.1.4 將failure injection控制面與只供fixtures的wrappers移入 `#[cfg(test)]`，共用transaction核心保持production一致。
- [ ] 6.1.5 移除沒有獨立測試價值的wrapper，更新tests使用bounded、cancelled、focused或linearized APIs。
- [ ] 6.1.6 執行commit failure points、cursor atomicity、migration promotion/reopen、WAL threshold與shutdown no-write tests。
- [ ] 6.1.7 分別執行normal與test target structured checks，證明test seam不再貢獻normal `dead_code`。

## 7. 仍有效OpenSpec contract的ownership與窄保留

### 7.1 Typed recovery與migration contract ownership

**目的：** 保留 `mft-sqlite-foreground-persistence` 對machine-readable recovery/migration state的產品責任，本change不越權新增behavior。
**輸入：** `RecoveryReasonV1`、`MigrationStateV1`、owning change未完成tasks/evidence及decision ledger。
**產出：** `evidence/recovery-contract-ownership.json`與窄item disposition。
**依賴：** 6.1。
**Owner／Wave：** Primary／7；產品行為owner仍為 `mft-sqlite-foreground-persistence`。
**Gate／Evidence：** DCG-CONTRACT-OWNERSHIP；`evidence/recovery-contract-ownership.json`。
**完成門檻：** 每個typed item連結原owner/task/evidence及suppression到期條件；本change scoped diff沒有新增producer、consumer、IPC/schema或runtime semantics。

- [ ] 7.1.1 對 `RecoveryReasonV1` 建立owning requirement、未完成task、current producer/consumer缺口及到期條件記錄。
- [ ] 7.1.2 對 `MigrationStateV1` 建立owning requirement、未完成task、current producer/consumer缺口及到期條件記錄。
- [ ] 7.1.3 在原owning change的evidence/task lineage中記錄此dead-code disposition依賴，不將其task標成由本change完成。
- [ ] 7.1.4 僅對必要item加入具owning change、task及「wiring完成即移除」文字的item-level suppression；不得修改diagnostic behavior。
- [ ] 7.1.5 執行MFT persistence/diagnostics focused tests與scoped behavior diff，證明本change沒有接手product behavior。

### 7.2 Shared snapshot remove-delta contract ownership

**目的：** 保留 `centralize-shared-folder-size-service` 對remove delta的產品責任，本change不實作跨generation mutation semantics。
**輸入：** `SnapshotDeltaV1::Remove`、owning change requirements/tasks/evidence及decision ledger。
**產出：** `evidence/remove-delta-contract-ownership.json`與窄item disposition。
**依賴：** 7.1。
**Owner／Wave：** Primary／7；產品行為owner仍為 `centralize-shared-folder-size-service`。
**Gate／Evidence：** DCG-CONTRACT-OWNERSHIP；`evidence/remove-delta-contract-ownership.json`。
**完成門檻：** Variant連結原owner/task/evidence與到期條件；本change沒有新增emitter/application、ancestor invalidation、authority/cancellation或re-add behavior。

- [ ] 7.2.1 建立remove-delta owning requirement、未完成task、current producer/consumer缺口及到期條件記錄。
- [ ] 7.2.2 在原owning change的evidence/task lineage中記錄此dead-code disposition依賴，不將產品task標成完成。
- [ ] 7.2.3 僅對必要variant加入具owning change、task及「production wiring完成即移除」文字的item-level suppression。
- [ ] 7.2.4 執行shared snapshot與Size Map現行focused tests，證明沒有新增remove semantics或改變既有delta behavior。
- [ ] 7.2.5 若要求本change實作remove behavior，建立C-level blocker並等待使用者批准及跨change supersession，不得直接繼續。

## 8. 單一MFT internal crate編譯authority

### 8.1 Dependency/visibility freeze與crate scaffold

**目的：** 在移動程式前凍結依賴方向、consumer surface與protocol/storage snapshots。
**輸入：** 清理後MFT modules、Cargo manifests、App/helper/service consumers。
**產出：** Dependency map、API inventory、`crates/explorer-mft` scaffold與 `evidence/mft-topology-baseline.json`。
**依賴：** 7.2。
**Owner／Wave：** Primary／8。
**Gate／Evidence：** DCG-MFT-TOPOLOGY；`evidence/mft-topology-baseline.json`。
**完成門檻：** 無未解 dependency cycle；每個consumer所需API、forbidden visibility及behavior snapshot明確，internal crate可locked/offline編譯。

- [ ] 8.1.1 產生MFT module dependency graph及App/helper/service symbol consumer inventory。
- [ ] 8.1.2 凍結named-pipe frame sizes/constants、SQLite schema/admission、migration fixture及current focused test結果。
- [ ] 8.1.3 定義client/core/service module visibility，列出不得公開的raw handles、paths及mutation/storage internals。
- [ ] 8.1.4 新增 `publish = false` internal crate、workspace membership及最小dependencies，不移動behavior。
- [ ] 8.1.5 以offline可重現方式更新新增path package所需的 `Cargo.lock`，審核lockfile只含預期workspace package/dependency edge並保存diff/hash。
- [ ] 8.1.6 執行Cargo metadata/check偵測cycle；若失敗，依B-level流程修正設計與evidence lineage。

### 8.2 Protocol、query與focus模組移動

**目的：** 將client/service共用IPC與focus實作切換至單一crate authority。
**輸入：** 8.1 scaffold及API inventory。
**產出：** Moved modules、updated consumers與 `evidence/batch-mft-protocol-move.json`。
**依賴：** 8.1。
**Owner／Wave：** Primary／8。
**Gate／Evidence：** DCG-MFT-TOPOLOGY；`evidence/batch-mft-protocol-move.json`。
**完成門檻：** App/service不再各自 `#[path]` 編譯query/focus完整模組；frame/auth/lifecycle tests通過且layout hashes不變。

- [ ] 8.2.1 移動query protocol/types/client/server模組並維持versioned frame layout與error bounds。
- [ ] 8.2.2 更新App、helper、service imports及visibility，刪除對舊path modules的引用。
- [ ] 8.2.3 移動focus lease/reporting/auth protocol並維持process/token/session validation boundary。
- [ ] 8.2.4 執行query round trip、batch streaming、malformed bounds、focus auth/expiry/disconnect與service stop tests。
- [ ] 8.2.5 記錄source moves、old/new hashes、API surface及本批warning delta。

### 8.3 Journal、persistence與runtime模組移動

**目的：** 將journal cursor、scheduler、lifecycle與live runtime收斂到internal crate。
**輸入：** 8.2完成crate及清理後modules。
**產出：** Moved modules、consumer updates與 `evidence/batch-mft-runtime-move.json`。
**依賴：** 8.2。
**Owner／Wave：** Primary／8。
**Gate／Evidence：** DCG-MFT-TOPOLOGY；`evidence/batch-mft-runtime-move.json`。
**完成門檻：** Journal/live runtime只編譯一次，cursor/pending/lifecycle/restart tests通過，無target-local dead code。

- [ ] 8.3.1 移動journal types/readers與normalization，保持legacy migration reader及Windows IO boundaries。
- [ ] 8.3.2 移動persistence scheduler、focus lease registry與lifecycle barrier。
- [ ] 8.3.3 移動volume memory runtime並更新service consumers，App不再編譯server-only runtime。
- [ ] 8.3.4 執行journal normalization/catch-up、pending batch、commit success/failure及stop/restart tests。
- [ ] 8.3.5 記錄API/behavior snapshots、hashes與warning delta。

### 8.4 Size-map、migration與SQLite模組移動

**目的：** 完成storage/index core的單一編譯authority並保持migration/atomicity契約。
**輸入：** 8.3 internal crate、6.1 test boundaries。
**產出：** Moved modules、consumer updates與 `evidence/batch-mft-storage-move.json`。
**依賴：** 8.3。
**Owner／Wave：** Primary／8。
**Gate／Evidence：** DCG-MFT-TOPOLOGY；`evidence/batch-mft-storage-move.json`。
**完成門檻：** Size-map/migration/SQLite不再由多targets重複path-compile，schema、atomic replace、legacy promotion及budget semantics不變。

- [ ] 8.4.1 移動MFT index/aggregate/volume reader及其Windows helper boundaries。
- [ ] 8.4.2 移動migration inventory/quarantine/promotion core並維持path/file-identity safety。
- [ ] 8.4.3 移動SQLite store/admission/transaction/WAL core與test-only failure seams。
- [ ] 8.4.4 更新service/helper/App consumers並移除所有舊MFT `#[path]` module duplication。
- [ ] 8.4.5 執行index parsing/aggregate、migration、SQLite atomicity/WAL、budget及helper tests並記錄layout/hash/warning evidence。

### 8.5 Consumer、manifest與topology closure

**目的：** 關閉crate extraction後的所有consumer、manifest、docs與dead-code dispositions。
**輸入：** 8.2–8.4 moved modules及baseline ledger。
**產出：** Final manifests/API map與 `evidence/mft-topology-final.json`。
**依賴：** 8.4。
**Owner／Wave：** Primary／8。
**Gate／Evidence：** DCG-MFT-TOPOLOGY；`evidence/mft-topology-final.json`。
**完成門檻：** 每個cross-target-live item有單一crate consumer證據，沒有舊duplicate modules、dependency cycle或不必要public API。

- [ ] 8.5.1 掃描workspace確認App/helper/service已完全改用internal crate且無殘留 `#[path]` MFT duplication。
- [ ] 8.5.2 建立consumer whitelist/visibility validator，要求每個new或提升visibility的public item都有核准App/helper/service call site及允許module surface。
- [ ] 8.5.3 執行consumer whitelist、Cargo metadata、dependency-direction與duplicate-module scans，拒絕以無consumer `pub` 隱藏dead code。
- [ ] 8.5.4 執行explorer-mft、explorer-app library、所有normal binaries及extension-host locked/offline checks。
- [ ] 8.5.5 將251個target-local baseline diagnostics逐一對應至消失的duplicate compilation或窄approved disposition。

## 9. 整合、零警告與最終traceability

### 9.1 Source policy與workspace integration gates

**目的：** 證明格式、suppression policy、target coverage及warning regression全部通過。
**輸入：** 所有batch evidence、current source及baseline counts。
**產出：** `evidence/integration-validation.json`與formatted source。
**依賴：** 8.5。
**Owner／Wave：** Primary／9。
**Gate／Evidence：** DCG-POLICY、DCG-INTEGRATION；`evidence/integration-validation.json`。
**完成門檻：** Normal workspace `dead_code=0`、`unsafe_code=0`、其他warning不增加，所有new suppression均為核准窄例外。

- [ ] 9.1.1 只格式化本change修改的Rust paths，再以 `cargo fmt --all --check`驗證且不做repository-wide rewrite。
- [ ] 9.1.2 執行suppression/governance scan與manual review，拒絕廣域allow、generic reason、未分類item及unfulfilled expectations。
- [ ] 9.1.3 執行explorer-mft、explorer-app lib/bins、explorer-ui、explorer-extension-host的focused locked/offline checks。
- [ ] 9.1.4 執行所有受影響的Code Lines、Folder Size、Size Map、MFT、SQLite、bookmark、authority focused tests。
- [ ] 9.1.5 執行 `cargo check --workspace --lib --bins --locked --offline`並保存structured diagnostics及exit status。
- [ ] 9.1.6 執行normal `cargo check --workspace --locked --offline`，斷言 `dead_code=0`、`unsafe_code=0`且其他warning code不高於baseline。

### 9.2 All-target狀態、evidence index與final review

**目的：** 誠實分類範圍外結果並完成requirement-to-evidence閉環。
**輸入：** 9.1 results、全部artifacts、batch evidence及dirty-tree attribution。
**產出：** `evidence/all-target-status.json`、`evidence/final-validation.json`、`evidence/index.json`。
**依賴：** 9.1。
**Owner／Wave：** Primary／10。
**Gate／Evidence：** DCG-FINAL；final evidence files。
**完成門檻：** 每個mandatory leaf有唯一passed record，所有blocking requirements通過，範圍外錯誤精確記錄，scoped diff與strict OpenSpec validation通過。

- [ ] 9.2.1 執行 `cargo check --workspace --all-targets --locked --offline`並記錄passing結果或精確out-of-scope errors及baseline關係。
- [ ] 9.2.2 逐項核對decision ledger，證明每個baseline diagnostic/item都有terminal disposition與current source evidence。
- [ ] 9.2.3 建立evidence index，確保每個task ID有唯一current passed record且所有stale/superseded record有distinct replacement。
- [ ] 9.2.4 審核scoped diff的behavior、ABI、IPC、schema、persistence、migration、filesystem、dependency、process topology及unrelated dirty-tree preservation。
- [ ] 9.2.5 執行fail-closed evidence validator、task structure validator與 `openspec validate govern-dead-code-warnings --strict`。
- [ ] 9.2.6 記錄final revision、toolchain/build environment、warning counts、approved narrow suppressions、removed dependencies、all-target status及residual risks。
