## Context

Rust 1.97.1 下執行正常 locked/offline workspace check 會產生 417 個 `dead_code` diagnostics。以 `file + line + message` 正規化後共有 322 個 canonical warning sites：251 個位於共用 MFT 原始碼，且只在部分編譯 target 發出；22 個在所有會編譯該程式的 MFT targets 都發出；49 個位於 application、Folder Size、UI、extension host 或 service 本體。多方法 diagnostic 展開後會對應更多 primary items，因此 evidence 必須同時保存 diagnostic 與 item 兩種視角，不能以單一數字推斷刪除量。

歷史追溯顯示，警告主要來自 `build-extensible-plugin-platform`、`centralize-shared-folder-size-service`、`event-driven-mft-index-updates`、`mft-sqlite-foreground-persistence`、`fix-shared-mft-folder-aggregate-lru`、cache editor 與 bookmark/runtime-authority 後續重構。部分舊程式已被新架構明確 supersede；部分是 production API 的測試 convenience seam；另有 `RecoveryReasonV1`、`MigrationStateV1`、remove delta 等仍受既有 requirement 要求但尚未接入正式路徑。

工作樹同時存在多個 active OpenSpec 及使用者修改。此變更不得把「目前沒有引用」直接當作刪除授權，也不得覆寫其他變更。每次寫入前須驗證 preimage hash/scoped diff，寫入後立即歸屬 hunk。

## Goals / Non-Goals

**Goals:**

- 對每個 canonical diagnostic 與 primary item建立唯一、可稽核的 disposition。
- 移除已 supersede 或真正無引用的 production code、過時測試及不再需要的 dependency。
- 把有效 test seam 移到 `#[cfg(test)]` 或讓測試直接使用 production API。
- 對既有規格仍要求的typed state/delta保留原active-change ownership；本change不越權新增行為，只建立窄item disposition與可追蹤到期條件。
- 將 MFT 共用實作收斂到單一 workspace-internal crate，使 App、helper、service 不再各自對同一份完整模組執行 dead-code reachability。
- 正常 workspace check 達到零 `dead_code`，並維持 `unsafe_code` 為零及其他 warning category 不增加。
- 保持公開 extension ABI、named-pipe protocol、SQLite schema、service/installer、cache/migration 與 filesystem semantics 不變。

**Non-Goals:**

- 處理 `unused_imports`、`unused_variables`、`unused_mut` 或 `unused_qualifications`。
- 改變 Folder Size、Size Map、Code Lines、bookmark、MFT durability 或 extension authorization 的使用者行為。
- 藉機完成其他 active OpenSpec 的產品功能或放寬其 blocking gates。
- 使用 crate/module-wide `allow(dead_code)`、無原因 suppression 或把 production 模組整體標成 test-only。
- 修復目前 workspace `--all-targets` 的範圍外 `folder_admission` initializer 錯誤；但必須誠實記錄其狀態。

## Decisions

### 1. Baseline 同時保存 diagnostic、item 與 target topology

Compiler JSON 將正規化 `src/bin/../` 路徑，對 diagnostic 建立穩定 ID，並展開所有 primary spans為 item records。每筆記錄包含 emitting targets、會編譯該 source 的完整 target set、symbol/text、Git introduction/removal history、相關 OpenSpec requirement/task，以及 source hash。

Disposition 僅允許：

- `remove-superseded`
- `remove-unreferenced`
- `retain-cross-target-live`
- `test-only`
- `retain-required-contract`
- `retain-narrow-suppression`

Alternative rejected：只以 `cargo check` 的文字輸出逐行處理。多 target 重複與 multi-method diagnostics 會讓刪除判斷失真。

### 2. 刪除以架構 supersession 為單位，不做孤立 symbol 猜測

第一批移除下列 cohesive chains：

- App-owned Code Lines directory tokei scan、persistent cache、MoveFileExW publication 與舊測試；正式路徑已由 Host-prepared bounded snapshot 取代。
- Details 的 `FolderSizeCachedValueV1`、TTL、Host cache partition、single-query deadline 與 pre-batch request helpers；正式路徑已改為 MFT Service batch stream。
- Application 內重複的舊 Size Map recursive fixture chain；reference traversal authority 留在 shared Folder Size service。
- MFT service/journal 中未再被 legacy migration reader使用的 `.semftidx`/delta/status writers、legacy query/refresh、舊 watch/publish 與 persisted limit helpers；正常 durability 已由 SQLite 取代。
- 舊 query server wrapper、UI cache renderer duplicate、registry-unaware width wrappers、bookmark convenience wrapper、runtime-authority test-only mutation methods及無引用常數/欄位。

每個 chain 必須先證明正式 replacement 存在、相關規格沒有要求 rollback reader/writer 繼續存在，並刪除或改寫只驗證舊架構的測試。

Alternative rejected：每遇到 warning 就刪一個 function。這會留下半套資料型別、測試或 migration graph。

### 3. 測試 seam 以 compile boundary 保留

SQLite failure injection、migration guard fixture、atomic commit/WAL tests及仍有效的 reference fixtures，若 production 不應呼叫，移入 `#[cfg(test)]`；production helper只因測試 convenience 而存在時，優先讓測試改呼叫正式 linearized/bounded API。測試專用型別不得留在正常 build再以 `allow(dead_code)` 壓制。

### 4. 規格要求的 dead code保留原change ownership

`RecoveryReasonV1`、`MigrationStateV1` 等machine-readable state，以及shared Folder Size delta的remove terminal，分別屬仍未完成的 `mft-sqlite-foreground-persistence` 與 `centralize-shared-folder-size-service`。本change不得實作其producer/consumer或改變runtime behavior；它只建立 `retain-required-contract` disposition、owning task/evidence連結及具到期條件的item-level `allow(dead_code, reason = "...")`。原owning change完成wiring後，本change的suppression必須移除並重新baseline。若要把行為實作移入本change，屬C-level scope/capability變更，必須先取得使用者批准並更新所有跨change supersession/evidence lineage。

### 5. MFT 共用程式移入單一 internal crate

建立 `crates/explorer-mft`（最終名稱可作 A-level調整），設定 `publish = false`，集中目前被 App library、helper、service以 `#[path]` 重複編譯的 protocol、focus、journal、migration、persistence、runtime、size-map、SQLite共用實作。App、helper、service改用該 crate；client/server API依模組分區，只暴露 workspace consumers所需項目。

移動前後須保存 protocol layout/constant snapshots、SQLite schema/admission tests、migration fixtures及 service/client round trip。若 dependency cycle或 Cargo target topology證明單 crate不可行，這是 B-level correction：設計可改為數個明確 client/core/service internal crates，但不得退回廣域 suppression。

Alternative rejected：在251個 target-local items上逐一加入 `allow(dead_code)`。它製造大量永久 lint債，且掩蓋未來真正死亡的項目。

### 6. Suppression 是最後的窄例外

只有 ABI callback、platform hook、generated registry入口或跨版本 compatibility symbol無法靠使用/visibility/cfg正確表達時，才可使用 item-level `#[allow(dead_code, reason = "...")]`；reason須指出 consumer/compatibility contract、相關 OpenSpec與移除條件。當該 target每次都必然發出 lint時可使用 `#[expect(dead_code, reason = "...")]`。禁止 crate/module-wide suppression與「暫時保留」「未來可能使用」等 generic reason。

### 7. 分批驗證與行為不變證據

每批先比對 owned file hash，再執行 focused build/tests、normal workspace structured warning count及 scoped diff review。Blocking gates：

- `DCG-INVENTORY`：322 canonical sites及其 primary items全部有唯一分類。
- `DCG-OBSOLETE`：superseded chains移除且 replacement tests通過。
- `DCG-TEST`：test-only seam不進 normal build，規格測試仍通過。
- `DCG-CONTRACT-OWNERSHIP`：required-contract items保留原active-change owner、具task/evidence/to-expiry連結，且本change未新增runtime behavior。
- `DCG-OWNERSHIP`：所有與active OpenSpec重疊的檔案在進入mutation wave前已有owner resolution、依賴順序及stale/replacement evidence決議。
- `DCG-MFT-TOPOLOGY`：MFT consumers使用單一 internal authority，protocol/storage/migration tests通過。
- `DCG-POLICY`：沒有新廣域 suppression、generic reason或無 disposition項目。
- `DCG-INTEGRATION`：正常 workspace `dead_code = 0`、`unsafe_code = 0`、其他 warning不高於 baseline。
- `DCG-FINAL`：所有 requirement→scenario→task→evidence可追溯，OpenSpec strict validation通過。

### 8. 調整與 evidence lineage

- **A — task refinement：** 可調整 task拆分、執行順序、internal crate名稱、focused command或 evidence機制，但不得改變 scope、disposition規則、blocking gate、公開契約或零警告目標。
- **B — design/spec correction：** 若 source/target topology、現行 consumer或 active OpenSpec證明分類錯誤，暫停受影響分支，同步更新 proposal/design/spec/tasks，將 dependent evidence標 stale並建立 replacement lineage。
- **C — material change：** 改變產品 requirement、公開 ABI/IPC/schema、migration/rollback支援、平台、權限、破壞性檔案操作、blocking gate或要求的 evidence，必須先取得使用者批准。

## Risks / Trade-offs

- [刪除的程式其實被另一 target或 build mode使用] → 以完整 target set、structured compiler evidence、repository references與focused target checks共同判定。
- [MFT crate抽離造成 visibility或dependency cycle] → 先建立 dependency/consumer map；cycle發現視為 B-level correction，不以 public API擴張或 suppression硬繞過。
- [舊 sidecar程式仍是 rollback migration reader] → writer與reader分開盤點；只有沒有現行 migration/rollback requirement及consumer的部分才能移除。
- [Round-trip fixture與writer一起刪除後reader失去獨立驗證] → Wave 1先以目前/已發布格式產生checked-in golden legacy chains並固定SHA-256；後續reader tests只讀goldens，不呼叫待刪writer。
- [把 test seam移入 cfg後降低測試真實性] → 測試優先呼叫 production linearized/bounded APIs，只將 failure injection控制面放在 test cfg。
- [required-contract wiring改變 protocol/schema] → 限制在既有versioned欄位/內部狀態；任何 frame/schema變更升級為 C-level。
- [其他 active OpenSpec同時修改相同檔案] → Wave 1建立active-change ownership matrix；未取得owner resolution、依賴順序及stale/replacement evidence決議前禁止進入相關mutation wave。每個mutation leaf執行immediate expected-hash/preimage check、post-write intended-hunk verification及new expected hash；單純rebaseline不能取代semantic ownership。
- [一次移動大量MFT檔案難以review] → 先移除真正dead chain，再抽離剩餘live core；每個module移動都有獨立compile/test/evidence leaf。

## Migration Plan

1. 凍結 structured baseline、target topology、Git/OpenSpec lineage、active-change ownership、dirty-tree attribution及checked-in legacy reader golden chains。
2. 先移除低風險 UI/bookmark/authority wrappers與無引用常數，建立 removal流程信心。
3. 移除 App Code Lines及Details Host cache/pre-batch chains，驗證正式 replacement。
4. 清理 Folder Size、journal/service sidecar legacy chain，保留必要 migration readers。
5. 將有效 test seam移入 `#[cfg(test)]`並維持規格測試。
6. 對仍有效但屬其他active change的typed contract items建立owner/task/evidence/to-expiry連結與窄item disposition，不新增production行為。
7. 抽離 MFT internal crate並逐 consumer切換。
8. 執行全 workspace policy、format、locked/offline checks及all-target status分類。

Rollback以批次為單位：尚未提交前由保留的 prechange patch/hash還原本變更的hunks；已整合批次則以正常revert commit回退。不得使用 `git reset --hard` 或覆寫使用者dirty work。刪除cache writer不會刪除現存使用者/cache檔案；任何runtime檔案清理仍受既有migration規格控制。

## Open Questions

無產品層未決事項。Internal crate最終名稱、module分組及個別test seam採直接production API或 `#[cfg(test)]` wrapper，僅在不改變上述契約與gates時屬A-level refinement。
