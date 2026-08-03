# 0→1 Product Validation 執行計畫

目前只驗證一件事：一個外部 Rust Plugin 能以 `abi_stable` 進入真實 SuperExplorer 程序、完成註冊，並在現有 Extensions menu 顯示可見的 loaded summary。只支援 `p0-consumer` 一個 Plugin；不先抽象多 Plugin、動態欄位 provider、scheduler、evidence、snapshot、release 或測試 framework。

驗證只允許最小 unit、smoke 與人工 demo。Tasks 1–5 不執行 UITEST。Task 6 先完成整個 example；只有需要自動化重跑這個已完成 slice 時，最後的 6.5 才可使用既有 `explorer-uitest` 作單一 smoke case，且不得新增通用測試 framework。CI、GitHub Actions 與遠端 gate 永不執行。

## 1. 單一真實 Plugin DLL

- [x] 1.1 確認 `sdk/fixtures/p0-consumer` 只使用公開 `explorer-extension-api`、ordinary Rust `ExtensionRegistrarImplementationV1` 與 `#[export_root_module]`，外掛作者不手寫 callback table。
- [x] 1.2 直接使用現有 `p0_consumer.dll` 完成第一次 smoke；若 binary 與目前 API 不相容，才將 consumer 改成最小本機 public-crate path dependency 並執行一次 `cargo build --manifest-path sdk/fixtures/p0-consumer/Cargo.toml`。

## 2. 明確的單 Plugin 開發載入入口

- [x] 2.1 為 `explorer-app` 加入單一 `--plugin-dll <absolute-path>` 開發參數；沒有參數時不掃描、不載入任何 unsigned local DLL。
- [x] 2.2 參數只接受一個 DLL、拒絕不存在或非絕對路徑，直接回報可理解錯誤；不建立 package-source 或多 Plugin abstraction。

## 3. 重用真實 abi_stable loader

- [x] 3.1 Extension Host 重用現有 Windows loader、root layout validation、metadata validation 與 SDK-owned registrar factory，載入該 DLL並呼叫一次 registration。
- [x] 3.2 將成功結果複製成最小 owned summary：plugin ID、contribution ID、kind；不把 DLL handle、ABI object 或 host internals傳到 app/UI。

## 4. 真實 App 到可見 UI

- [x] 4.1 `explorer-app` 在 startup composition 接收 optional summary，沒有 Plugin 時維持目前行為。
- [x] 4.2 現有 Extensions menu 顯示一筆 read-only loaded entry，例如 `p0-consumer · abi-root · Column`；不宣稱 column provider、value 或 renderer 已完成。

## 5. 最小錯誤路徑與 demo 命令

- [x] 5.1 無效 DLL path／ABI reject 時 app 明確失敗或顯示診斷，不 crash、不 fallback 到其他 Plugin。
- [x] 5.2 文件只提供兩個 demo：帶 `--plugin-dll` 啟動並看到 entry；不帶參數重啟且 entry 消失。

## 6. 第一個 example 完整驗證

- [x] 6.1 建置或選定與目前程式碼相容的 `p0_consumer.dll`。
- [x] 6.2 以 `cargo run -p explorer-app -- --plugin-dll <absolute-dll>` 啟動真實 App。
- [x] 6.3 人工 smoke：帶 `p0-consumer` 的真實 App 顯示正確 plugin/contribution/kind，資料夾內容與 Shell icon 正常載入，且 app 可繼續一般操作。
- [x] 6.4 不帶參數重啟，確認沒有 Plugin registration、資料夾與 icon 仍正常；product validation 決策為 GO，下一個最小功能是讓單一 Plugin 提供一個真實可見欄位值。
- [x] 6.5 人工 demo 已足以驗證本 slice，明確決定不新增、不執行 UITEST smoke case。

## 7. Installer 內附單一 Plugin

- [x] 7.1 更新 `build/build_install.lua`：一般模式以 fixture manifest、release、固定 MSVC target 與 offline 建置 `p0_consumer.dll`；`--skip-build` 只重用明確 release 產物，`--check` 不要求既有 binary。
- [x] 7.2 驗證明確的 release Plugin DLL 是有效 Windows PE，並以 `PLUGIN_DLL` define 傳給 NSIS；缺失或失敗時不得 fallback 到舊、debug 或其它 Plugin。
- [x] 7.3 更新 NSIS：安裝 `plugins\p0_consumer.dll`，讓兩個捷徑與完成頁傳入已安裝絕對路徑，並在解除安裝時只刪除已知 DLL及空目錄。
- [x] 7.4 執行最小 build/package smoke，產生並驗證 `dist\SuperExplorer-Setup-1.2026.8.4-x64.exe`；不執行 CI、UITEST 或建立新測試 framework。

## 8. Visible folder-size GPUI example

- [x] 8.1 將尚未發布的 V1 擴充為 `abi_stable` Visual Column object；Plugin 作者只實作普通 Rust trait，取得公開 cell render context 並回傳 data-only proportional-bar render plan，不跨 DLL 傳遞 GPUI/private host types。
- [x] 8.2 將 `p0-consumer` 改為單一完整 folder-size example：背景遞迴計算資料夾 bytes、提供 exact byte sort value、以目前目錄最大 sibling bytes 計算比例，並支援一個最小顯示設定。
- [x] 8.3 讓 development DLL loader 保留單一 Plugin 的 Visual Column object，並由 application composition 傳入 `ExplorerRoot`；未載入 Plugin 時維持原本檔案總管行為。
- [x] 8.4 將 `p0-consumer:folder-size` 註冊到 production `ColumnRegistry` / Details layout，顯示動態 header、cell、column chooser，並以 host-owned GPUI element 畫出 Plugin render plan。
- [x] 8.5 以最小 Cargo check/unit 與真實視窗 smoke 驗證：一般資料夾與 icon 正常載入、資料夾大小逐步出現、比例條可見、設定生效、排序使用 exact bytes；不執行 CI 或 UITEST。
- [x] 8.6 更新 bundled Plugin 與安裝腳本輸入，重建並驗證 `dist\\SuperExplorer-Setup-1.2026.8.4-x64.exe` 包含完成的 example。

## 9. Visible Size Map GPUI example

- [x] 9.1 建立獨立 `rust-folder-size-map-view` consumer，透過公開 ordinary Rust trait 與 SDK-owned `abi_stable` adapter 註冊單一 view renderer；不引用 private host/UI crates。
- [x] 9.2 定義最小 public data-only view context/plan：owned node ID/name/type/exact bytes/status、viewport/theme/settings與treemap rectangle；Plugin callback不接收路徑、GPUI entity或native handle。
- [x] 9.3 讓單Plugin loader/application runtime保留Size Map renderer，重用有界且generation-aware的背景資料夾計量；partial/error不得冒充exact bytes，舊位置/F5結果不得覆寫目前畫面。
- [x] 9.4 在production View menu加入只有載入該Plugin才出現的 `Size Map`，以host-owned GPUI elements畫rectangles、label/bytes/percentage/status，並接回正式selection、double-click folder navigation與F5。
- [x] 9.5 完成missing/faulted/no-plugin fallback至Details、獨立README與最小build/package路徑；不修改目前installer的唯一bundled folder-size Plugin。
- [x] 9.6 在9.1–9.5全部完成後執行Cargo最小unit/smoke、真實視窗demo與既有本機UITEST runner單一Size Map case；驗證icon/一般資料夾行為不回歸，且永遠不跑CI。

## 10. Rust tokei Code lines column example

- [x] 10.1 定義最小 public `BatchColumnProviderV1`：ordinary Rust author trait、SDK-owned `abi_stable` adapter、每批最多128個host-attested item/`InputStreamV1`，結果重用既有typed outcome/value/stable-sort types；不傳路徑、native handle、future或runtime。
- [x] 10.2 讓尚未發布的V1 registration與單Plugin DLL loader保留一個batch provider及其Visual Column renderer；只支援 `rust-tokei:code-lines`，不建立多Plugin registry或通用framework。
- [x] 10.3 建立app-owned bounded batch runtime：只讀目前資料夾普通檔案、單檔上限8 MiB、每批128、generation-aware cancellation/stale rejection；unsupported/binary/unknown/oversized不得顯示為有效0。
- [x] 10.4 鎖定並靜態連結一個Rust `tokei` library及其精確dependency closure，使獨立consumer可在clean `CARGO_HOME`以`--locked --offline`建置；不得spawn `tokei.exe`或每檔建立process。
- [x] 10.5 建立獨立 `rust-tokei-code-lines-column` public-SDK consumer，對Rust、C/C++、Python、Lua與JavaScript輸出language/code/comment/blank/total，主要欄位與sort key使用exact code-lines integer。
- [x] 10.6 在production Details安裝唯一的 `Code lines` 動態欄位、column chooser、integer sorting與host-owned GPUI cell；提供一個最小設定切換comment/blank detail，並維持folder-size與無Plugin fallback。
- [x] 10.7 完成README與最小build/package路徑，加入mixed-language/empty/binary/unknown fixture及最小Cargo unit/smoke；明確觀察沒有child process，不擴充installer bundled Plugin。
- [x] 10.8 僅在10.1–10.7完整完成後，加入並執行一筆本機 `rust-tokei-code-lines-headful` UITEST，驗證真實欄位值、numeric sorting、設定切換與一般icon/資料夾行為；永遠不跑CI。

# Deferred roadmap（不屬於目前 apply scope）

以下內容保留為產品可行後的候選 roadmap。所有 `[deferred]` 都不是目前 task、completion gate 或子代理工作來源；除非使用者明確重新啟用，禁止實作、補測試或建立 framework。
## 1. 規劃治理、證據與既有完成狀態遷移

### 1.1 Evidence ledger 與舊任務 lineage

**目的：** 建立可讓每個新版 leaf 唯一結案、失效與重開的 append-only 證據帳本，並保留舊 1.1–4.8／5.1 的實作歷史。
**輸入：** 舊版 `tasks.md`、git history、現有測試／fixture、5.1 architecture FINAL GO。
**產出：** `evidence/evidence-index.jsonl` schema/validator、legacy-to-new mapping、stale/superseded lineage 規則。
**依賴：** 無；所有其他 work package 的 completion 前置。
**Owner／Wave：** `release-integrator`／W0；owned: 本 change 的 `evidence/` 與 mapping；forbidden: production crates。
**Gate／Evidence：** evidence schema unit test、duplicate task ID/hash/timestamp/status negative fixtures；本 L2 records 置於 `1.1.*`。
**完成門檻：** 每個 legacy `[x]` 有對應新版 leaf 或明確 superseded link，validator 拒絕 duplicate、missing field、mutable shared evidence 與非 terminal status。

- [deferred] 1.1.1 定義 `evidence-index.jsonl` record schema、terminal status、shared `subcheck_key` 與 append-only lineage，並以可重跑的本機 unit/contract 結果作為輸入。
- [deferred] 1.1.2 實作evidence record required-field與duplicate task ID validator；以直接可重跑結果、ledger command/result/hash 關閉 leaf。
- [deferred] 1.1.3 建立舊 task 1.1–4.8 與新版 L3 的 machine-readable lineage，保留原完成 commit 與原測試名稱，並以直接可重跑 lineage report 與 ledger hash 關閉 leaf。
- [deferred] 1.1.4 將 5.1 FINAL GO、242 UI tests、7 dynamic-column tests 與已知 5.2/5.4 reopen risks 寫成未勾選的 backfill candidate，使用本機可重跑結果與 ledger hash，不直接宣稱新版完成。
- [deferred] 1.1.5 驗證 failed、blocked、stale、unexecuted 與 trait/mock-only record 均不能讓 task 被解析為完成；以本機 negative fixtures 直接重跑並記錄結果/hash。
- [deferred] 1.1.6 實作每個L3恰好映射一個command/manual subcheck的cardinality validator，將 deterministic report 的 command/result/hash 寫入 ledger。
- [deferred] 1.1.7 驗證 mandatory P0/P1 不得 N/A，只有預先列出的 conditional 可 N/A，superseded 必須有唯一 replacement 且 dependents 轉 stale；以直接可重跑 validator result 關閉 leaf。
- [deferred] 1.1.8 建立通用、release-integrator-owned 的 signed retained-bundle verifier：只接受本機 `release://` locator，驗證簽章與信任主體、manifest/task/subcheck/source revision binding、SHA-256、canonical paths、retention metadata、path traversal/reparse escapes 與大小限制；verifier 供 L2/RC/release 組裝使用，不是任何 leaf 的循環前置條件。

### 1.2 Requirement／gate／task 雙向追蹤

**目的：** 讓 proposal、design、每個 Requirement/Scenario、blocking gate、L3 與 evidence 可雙向查詢。
**輸入：** proposal、design、十一份 delta specs、requirement selector scanner。
**產出：** traceability matrix 與 coverage validator。
**依賴：** 1.1。
**Owner／Wave：** `release-integrator`／W0；owned: change traceability files；forbidden: spec semantics，除非走 B/C 流程。
**Gate／Evidence：** `openspec validate build-extensible-plugin-platform --strict`、traceability validator；records `1.2.*`。
**完成門檻：** 每個 Requirement 與 Scenario 至少映射一個 L3、gate 和 evidence type；零命中、孤兒 task、只有未來文件承諾均失敗。

- [deferred] 1.2.1 列出十一個 capability 的每個 Requirement 與 Scenario stable selector。
- [deferred] 1.2.2 將每個 blocking design gate 映射到 requirement/scenario 與負向或恢復 leaf。
- [deferred] 1.2.3 將每個 L3 映射回唯一 requirement/gate，標記純 governance 或 integration leaf。
- [deferred] 1.2.4 實作 missing requirement、unknown selector、orphan leaf 與 mock-only coverage 的獨立失敗案例。
- [deferred] 1.2.5 產生可供 runner/release共用的 matrix；每個 gate_id具exact command/manual procedure、cwd、env、expected exit/artifacts並保存hash。

### 1.3 Multi-agent ownership 與調整控制

**目的：** 固定 wave、共享檔案 integrator、handoff 與 A/B/C correction 流程，避免平行代理互相覆蓋。
**輸入：** 本檔 ownership contract、repository `AGENTS.md`、worktree dirty-state policy。
**產出：** agent handoff template、owned/forbidden path matrix、adjustment log。
**依賴：** 1.1。
**Owner／Wave：** primary agent／W0；owned: plan/evidence governance；forbidden: delegating privileged operations。
**Gate／Evidence：** ownership collision fixture、B/C correction walkthrough、reviewer sign-off；records `1.3.*`。
**完成門檻：** 每個 L2 有一個 owner/wave；共享 manifests 只有 integrator；handoff 包含 diff、commands、results、risks、remaining tasks；B/C 不可繞過重驗或使用者核准。

- [deferred] 1.3.1 產生 role-to-owned-path 與 forbidden-path matrix，包含 shared manifest integrator。
- [deferred] 1.3.2 定義 agent handoff 必填 diff、test command/result、evidence IDs、known risks 與 remaining dependencies。
- [deferred] 1.3.3 驗證兩個 owner 宣告同一 mutable path 時 validator 明確失敗。
- [deferred] 1.3.4 演練 A refinement，確認永久 L3 ID 與舊 evidence lineage 保留。
- [deferred] 1.3.5 演練 B correction，確認 dependent evidence 轉 stale、affected work 暫停並重跑 OpenSpec validation。
- [deferred] 1.3.6 演練 C change，確認沒有使用者核准記錄時 public ABI/gate/permission 變更被拒絕。

## 2. P0-0 Rust、ABI 與 GPUI Snapshot 基線

### 2.1 Toolchain 與 protected dependency closure audit

**目的：** 以可重播證據確認 Rust 1.97.1、Cargo commit、MSVC target、`abi_stable 0.11.3` 與 protected closure。（legacy 1.1、1.2、1.5）
**輸入：** root/SDK toolchain files、Cargo locks、vendor metadata、protected graph validator。
**產出：** baseline audit report、positive/negative fixture logs、canonical graph hash。
**依賴：** 1.1–1.3。
**Owner／Wave：** `sdk-tooling-owner`／W0；owned: SDK locks/tooling；forbidden: ABI Rust source、release orchestration。
**Gate／Evidence：** isolated `cargo metadata --locked --offline`、toolchain commit validator、closure drift fixtures；records `2.1.*`。
**完成門檻：** host/SDK/fixture 使用相同 exact baseline；display-version spoof、feature drift、second GPUI/SDK edge 與 missing vendor source 分別 fail closed。

- [deferred] 2.1.1 核對 root、SDK、host fixture 與 plugin fixture 的 rustc/Cargo commit、target 和 toolchain file 完全一致。
- [deferred] 2.1.2 核對 `abi_stable = 0.11.3`、空 top-level features 與 protected dependency closure 的 canonical hash。
- [deferred] 2.1.3 在空 `CARGO_HOME` 執行 root 與 SDK `cargo metadata --locked --offline` 並保存 resolved graph。
- [deferred] 2.1.4 以相同顯示版本但不同 compiler commit fixture 證明 validator 拒絕。
- [deferred] 2.1.5 以protected feature drift fixture證明拒絕。
- [deferred] 2.1.6 稽核所有 vendored crate provenance/license 與 lock checksum 一致，缺一項即失敗。
- [deferred] 2.1.7 以第二份GPUI/SDK dependency edge fixture證明拒絕。

### 2.2 Canonical bundle 與 UI fingerprint

**目的：** 驗證 canonical generator 只由核准 inputs 產生 deterministic bundle ID/fingerprint，且單因子 drift 精確不相容。（legacy 1.3、1.4、1.6）
**輸入：** GPUI authority/rev/tree、toolchain commits、protected graph、profiles、panic/allocator/CRT/LTO/codegen/rustflags、SDK public hashes。
**產出：** `sdk-lock.json`、`bundle-manifest.json`、`ui-abi-fingerprint.json` 與 determinism report。
**依賴：** 2.1。
**Owner／Wave：** `sdk-tooling-owner`／W0；owned: generator and canonical SDK outputs；shared output merge only by `release-integrator`。
**Gate／Evidence：** bundle-generator contract、fingerprint unit matrix、two-run byte comparison；records `2.2.*`。
**完成門檻：** 兩次 clean generation byte-identical；每個 required input 單獨改變均改 fingerprint；unrelated app build ID 不改 fingerprint。

- [deferred] 2.2.1 驗證 GPUI source authority 僅接受 `damody/gpui-ce-explorer.git` 的完整 commit/tree。
- [deferred] 2.2.2 逐欄核對 canonical bundle input inventory 與 serialization order。
- [deferred] 2.2.3 在兩個乾淨目錄生成 bundle 並比較所有 canonical output bytes/hash。
- [deferred] 2.2.4 建立只改一個input且輸出unique subcheck的fingerprint mismatch harness。
- [deferred] 2.2.5 證明只改 unrelated SuperExplorer build ID 時 fingerprint 與 compatibility decision 不變。
- [deferred] 2.2.6 由 release-integrator 同步 trust-root/bundle ID consumers 並驗證沒有 mixed snapshot。
- [deferred] 2.2.7 驗證rustc commit單因子drift改變fingerprint。
- [deferred] 2.2.8 驗證Cargo commit單因子drift改變fingerprint。
- [deferred] 2.2.9 驗證target triple單因子drift改變fingerprint。
- [deferred] 2.2.10 驗證GPUI commit/tree單因子drift改變fingerprint。
- [deferred] 2.2.11 驗證protected feature單因子drift改變fingerprint。
- [deferred] 2.2.12 驗證profile/panic strategy單因子drift改變fingerprint。
- [deferred] 2.2.13 驗證allocator/CRT policy單因子drift改變fingerprint。
- [deferred] 2.2.14 驗證LTO/codegen-units單因子drift改變fingerprint。
- [deferred] 2.2.15 驗證rustflags單因子drift改變fingerprint。
- [deferred] 2.2.16 驗證ABI schema version單因子drift改變fingerprint。

### 2.3 隔離 host/plugin fixture 與 Rust-first ABI baseline

**目的：** 從互不共享 cache 的 consumer 建置並載入 Rust-first `abi_stable` plugin，且 legacy raw root 在任何 native marker 前被拒絕。（legacy 1.2、1.7、2.2、3.3）
**輸入：** approved bundle、host fixture、plugin fixture、legacy raw-root fixture。
**產出：** 分離 build/load logs、ABI layout report、native marker assertions。
**依賴：** 2.1、2.2；contract changes 先由 contract-owner 完成。
**Owner／Wave：** `fixture-owner`／W0；owned: SDK ABI fixtures/tests；forbidden: public ABI definitions。
**Gate／Evidence：** `--locked --offline` builds with distinct empty `CARGO_HOME`、loader contract；records `2.3.*`。
**完成門檻：** current plugin loads and registers through SDK-owned factory；legacy/raw/layout/major/fingerprint mismatch all reject before accessor/factory/callback/marker。

- [deferred] 2.3.1 以獨立空`CARGO_HOME`、禁止網路建置host fixture。
- [deferred] 2.3.2 載入 current plugin 並證明 ordinary Rust trait 經 `RootModule`／`#[sabi_trait]` registrar 完成 registration。
- [deferred] 2.3.3 掃描 author-facing API/fixtures，拒絕手寫 `extern "C"` callback、layout 或 panic trampoline。
- [deferred] 2.3.4 載入 pre-callback legacy raw root，證明 layout reject 發生在所有 accessor/factory/callback/native marker 之前。
- [deferred] 2.3.5 驗證SDK major mismatch診斷與pre-callback拒絕。
- [deferred] 2.3.6 驗證 panic translation 使用 SDK-owned trampoline 且 ABI boundary 不跨 `String`/`Vec`/Future/private types。
- [deferred] 2.3.7 以另一個獨立空`CARGO_HOME`、禁止網路建置current plugin fixture。
- [deferred] 2.3.8 驗證root fingerprint mismatch診斷與pre-callback拒絕。
- [deferred] 2.3.9 驗證required numeric semantics mismatch診斷與pre-callback拒絕。
- [deferred] 2.3.10 驗證GPUI fingerprint mismatch診斷與pre-callback拒絕。

### 2.4 Snapshot update、rollback 與 Release freeze tooling

**目的：** 驗證 development candidate、non-fast-forward approval、atomic promotion、rollback 與 immutable RC freeze。（legacy 1.8、1.9）
**輸入：** current approved snapshot、locally retained fixture、candidate gates、freeze metadata。
**產出：** candidate/promotion/freeze reports、new bundle ID 或 byte-identical retained snapshot。
**依賴：** 2.2、2.3；external tag/publish 僅 primary 執行。
**Owner／Wave：** `release-integrator`／W0（W5只重驗）；owned: local snapshot and promotion manifests；forbidden for subagents: git tag/push/credentials。
**Gate／Evidence：** snapshot update/promotion/freeze contract scripts and offline rebuild; records `2.4.*`。
**完成門檻：** fast-forward candidate 只在所有 gates green 後原子切換；failure/non-approved rewrite 保留舊 snapshot；freeze 後 rev change 必須新 RC/bundle。

- [deferred] 2.4.1 驗證 snapshot metadata 將 approved source revision 解析為完整 commit/tree 且不直接以 branch 建置。
- [deferred] 2.4.2 驗證 fast-forward candidate 在 host、SDK、fixture、examples、package gates 前不修改 approved outputs。
- [deferred] 2.4.3 模擬 candidate gate failure，確認 rollback 後 canonical outputs 與原 snapshot byte-identical。
- [deferred] 2.4.4 模擬 non-fast-forward，確認無 approval 時拒絕、有可驗 proof 時才產生隔離 candidate。
- [deferred] 2.4.5 驗證 `release_frozen = true`、protected-tag metadata、signed release input inventory 與 offline rebuild。
- [deferred] 2.4.6 模擬 freeze 後 main advance，確認舊 release offline rebuild；模擬 frozen rev change，確認強制新 RC/bundle ID。

### 2.5 作者 scripts、診斷與 minimal prompt fixture

**目的：** 讓作者只用 approved bundle 執行 build/validate/package，並得到可操作而不洩密的 P0-0 診斷。（legacy 1.10）
**輸入：** bundle metadata、manifest schema、minimal provider+GPUI fixture。
**產出：** `build-plugin.ps1`、`validate-plugin.ps1`、`package-plugin.ps1` audit、診斷文件、future UI selector mappings。
**依賴：** 2.1–2.3。
**Owner／Wave：** `sdk-tooling-owner`／W0；owned: SDK scripts/docs fixture；forbidden: host runtime behavior。
**Gate／Evidence：** plugin-tooling self-test、isolated author reproduction、manifest selector check；records `2.5.*`。
**完成門檻：** 三支 script 各有成功與獨立失敗證據；minimal provider+renderer 在 clean consumer build/validate/package；diagnostic 不含 secrets/absolute private paths。

- [deferred] 2.5.1 以 approved bundle 與空 `CARGO_HOME` 執行 `build-plugin.ps1 --locked --offline` 成功案例。
- [deferred] 2.5.2 驗證toolchain mismatch的build/validate診斷。
- [deferred] 2.5.3 驗證 package script 產出 deterministic store-only `.sepack`、runtime manifest、DLL、SBOM/NOTICE 與 hashes。
- [deferred] 2.5.4 驗證缺manifest capability fail closed。
- [deferred] 2.5.5 依公開文件在 clean consumer 重現 minimal provider+GPUI renderer 並保存 command/output hashes。
- [deferred] 2.5.6 將每個 script gate 靜態映射至 requirement selector 與 `uitest/manifest.json` schema；只驗證 selector/artifact registration，不啟動 `explorer-uitest`（首次 execution 延後至 6.4.7）。
- [deferred] 2.5.7 驗證protected dependency mismatch的build/validate診斷。
- [deferred] 2.5.8 驗證ABI layout mismatch的build/validate診斷。
- [deferred] 2.5.9 驗證UI fingerprint mismatch的build/validate診斷。
- [deferred] 2.5.10 驗證private crate dependency fail closed。
- [deferred] 2.5.11 驗證unlocked dependency fail closed。
- [deferred] 2.5.12 驗證package path escape fail closed。
- [deferred] 2.5.13 驗證missing license/NOTICE fail closed。

## 3. Extension API、套件生命週期、Native Loader 與 Safe Mode

### 3.1 Rust-first public ABI 與 forbidden-surface audit

**目的：** 凍結首次發布的單一 Rust-first V1 root/registrar/data boundary，作者只實作 ordinary Rust traits。（legacy 2.1、2.2）
**輸入：** 2.3 ABI fixture、public API/UI API crates、ABI specs。
**產出：** reviewed public surface、layout/semantic ID report、compatibility fixtures。
**依賴：** 2.1–2.3；所有 consumer work 的 contract gate。
**Owner／Wave：** `contract-owner`／W1；owned: API crates；forbidden: host/model/UI consumers before contract approval。
**Gate／Evidence：** ABI layout/fingerprint/public-type scans；records `3.1.*`。
**完成門檻：** required root/factory/trait layout 固定；只使用 fixed-width 與 `abi_stable` owned types；無手寫 raw callback、private type、Future/closure/std collection 跨 DLL。

- [deferred] 3.1.1 盤點並凍結 `ExtensionRootModuleV1` required prefix、SDK-owned factory、registrar trait object 與 numeric semantics。
- [deferred] 3.1.2 定義 stable IDs、non-exhaustive value policy、unknown-value preserve/reject 規則與 major-version boundary。
- [deferred] 3.1.3 建立 public-surface scan，拒絕 `std::String/Vec`、ordinary trait object、Future、closure、GPUI entity、private model/native wrapper。
- [deferred] 3.1.4 建立 author fixture compile test，證明作者不需且不得手寫 `extern "C"` callback/layout/trampoline。
- [deferred] 3.1.5 建立 old-host/new-value、malformed descriptor、layout drift 與 numeric reinterpretation negative fixtures。
- [deferred] 3.1.6 取得 contract-owner 與 architecture reviewer 對 V1 baseline 的獨立簽核與 hash。
- [deferred] 3.1.7 凍結每個 ABI object/returned value 的 allocation origin、destructor owner、registrar/trait-object ownership、permitted drop thread與DLL lifetime。
- [deferred] 3.1.8 驗證host只在DLL resident且允許thread釋放plugin-created object。
- [deferred] 3.1.9 對factory執行panic fixture，證明無unwind跨ABI且matching marker/owned memory正確處理。
- [deferred] 3.1.10 驗證panic strategy進入fingerprint；`panic=abort` plugin不得宣稱typed panic recovery且mismatch在callback前拒絕。
- [deferred] 3.1.11 對registrar執行panic fixture並驗證no-unwind/marker/owned-memory。
- [deferred] 3.1.12 對provider執行panic fixture並驗證no-unwind/marker/owned-memory。
- [deferred] 3.1.13 對renderer執行panic fixture並驗證no-unwind/marker/owned-memory。
- [deferred] 3.1.14 對host service callback執行panic fixture並驗證no-unwind/marker/owned-memory。
- [deferred] 3.1.15 對destructor執行panic fixture並驗證no-unwind、DLL resident與safe terminal。
- [deferred] 3.1.16 驗證wrong-thread destruction被拒絕且object ownership未遺失。
- [deferred] 3.1.17 驗證after-unload destruction在lifetime model中不可構造或被拒絕。

### 3.2 `.sepack` manifest、publisher 與 content validation

**目的：** 將 Rust/Lua/Skin/locales/tools/licenses 置於單一原子信任邊界。（legacy 2.3–2.5）
**輸入：** manifest/schema、package importer、publisher keys、content fixtures。
**產出：** parser/validator audit、positive/negative package corpus、diagnostics。
**依賴：** 3.1 stable IDs；2.5 package tooling。
**Owner／Wave：** `host-owner`＋`fixture-owner`／W1；owned: host package validation and fixtures；forbidden: public ABI changes。
**Gate／Evidence：** package manifest/lifecycle/import contracts；records `3.2.*`。
**完成門檻：** valid multi-content package 可表示；duplicate/unknown-required/over-length ID、contact purpose、publisher mismatch、path/hash/signature/target/reparse escape 各自整包拒絕且零 callback。

- [deferred] 3.2.1 驗證 versioned manifest 對 entry points、dependencies、features、capabilities、data version 與 payload inventory 的 bounds。
- [deferred] 3.2.2 驗證 publisher ID/display name/contact kinds，且至少一筆 support/security purpose。
- [deferred] 3.2.3 驗證 signing identity 與 manifest publisher mismatch 在 registration 前拒絕。
- [deferred] 3.2.4 驗證absolute package path拒絕。
- [deferred] 3.2.5 驗證content hash mismatch整包拒絕。
- [deferred] 3.2.6 對 valid Rust+Lua+Skin package 做 deterministic import，確認一個 package generation 與獨立 feature declarations。
- [deferred] 3.2.7 驗證parent traversal package path拒絕。
- [deferred] 3.2.8 驗證NUL package path拒絕。
- [deferred] 3.2.9 驗證duplicate normalized package path拒絕。
- [deferred] 3.2.10 驗證symlink package path escape拒絕。
- [deferred] 3.2.11 驗證junction package path escape拒絕。
- [deferred] 3.2.12 驗證generic reparse-point package path escape拒絕。
- [deferred] 3.2.13 驗證signature mismatch整包拒絕。
- [deferred] 3.2.14 驗證target mismatch整包拒絕。
- [deferred] 3.2.15 驗證missing declared payload整包拒絕。
- [deferred] 3.2.16 驗證undeclared payload整包拒絕。

### 3.3 Package sources、resolver 與原子 registration

**目的：** 只選一個完整相容版本，先完成 source/dependency/entitlement resolution 再允許任何 contribution。（legacy 2.6、2.7）
**輸入：** validated package candidates、built-in/local sources、dependency graph、entitlement boundary。
**產出：** resolved package set、blocked diagnostics、atomic registration plan。
**依賴：** 3.2。
**Owner／Wave：** `host-owner`／W1；owned: resolver/source adapters；forbidden: Steamworks linkage、UI state。
**Gate／Evidence：** resolver/source integration matrix；records `3.3.*`。
**完成門檻：** 每 package ID 最多一版本；cycle/unsatisfied/hash/signature/target/entitlement failure 均無 partial contribution；built-in/local 在無 Steam 環境可用。

- [deferred] 3.3.1 驗證 built-in 與 local-developer discovery 的 deterministic candidate order 與 package identity。
- [deferred] 3.3.2 實作/稽核 replaceable Package Source 與 Entitlement Provider boundary，確認 root dependency graph 無 Steamworks。
- [deferred] 3.3.3 驗證 version selection、dependency range 與 transitive graph 的 deterministic resolution。
- [deferred] 3.3.4 驗證unsatisfied dependency的whole-package blocked state。
- [deferred] 3.3.5 驗證 resolver 成功後才建立 sealed atomic registration plan，途中 fault 不留下 half registry。
- [deferred] 3.3.6 驗證dependency cycle的whole-package blocked state。
- [deferred] 3.3.7 驗證duplicate selected version的whole-package blocked state。
- [deferred] 3.3.8 驗證target incompatibility的whole-package blocked state。
- [deferred] 3.3.9 驗證SDK/UI compatibility failure的whole-package blocked state。

### 3.4 Desired/effective state 與 contribution authority

**目的：** 將 global/package/feature desired state 與 effective lifecycle、capability authority 分離。（legacy 3.1、3.2）
**輸入：** resolved packages、manifest features/capabilities、state store。
**產出：** immutable effective snapshot、sealed contribution tokens、state transition diagnostics。
**依賴：** 3.2、3.3。
**Owner／Wave：** `host-owner`／W1；owned: host lifecycle/gate；forbidden: UI-local authoritative registries。
**Gate／Evidence：** feature-state/contribution-gate tests；records `3.4.*`。
**完成門檻：** enabled/disabled/disabling/pending-restart/blocked/faulted transitions deterministic；parent off 保留 child desired；undeclared/duplicate/capability-exceeding contribution 整包拒絕。

- [deferred] 3.4.1 驗證 global/package/feature desired state persistence 與 parent disable/re-enable child-state preservation。
- [deferred] 3.4.2 驗證 dependency/compatibility/capability/restart inputs 對 effective state 的 deterministic resolution。
- [deferred] 3.4.3 將每個 registrar descriptor 綁定 sealed package generation、feature ID 與 capability token。
- [deferred] 3.4.4 驗證unknown feature contribution整包拒絕。
- [deferred] 3.4.5 產生 host-owned immutable catalog snapshot；證明 model/UI 無法直接繞過 gate 修改 authority registry。
- [deferred] 3.4.6 驗證duplicate stable contribution ID整包拒絕。
- [deferred] 3.4.7 驗證undeclared capability contribution整包拒絕。
- [deferred] 3.4.8 驗證capability-exceeding callback registration整包拒絕。

### 3.5 Startup loader、resident lifecycle 與 bounded drain

**目的：** 只在 startup 載入 Rust DLL，先驗 ABI/fingerprint，再以 gate/cancel/remove/drain 停止 feature 而不卸載。（legacy 3.3、3.4）
**輸入：** sealed registration plan、validated DLL、effective state、job/callback trackers。
**產出：** loaded package lifecycle、drain report、restart state。
**依賴：** 3.1、3.4；4.x scheduler API。
**Owner／Wave：** `host-owner`／W1；owned: loader/native lifecycle；forbidden: runtime `FreeLibrary`/hot-load promises。
**Gate／Evidence：** DLL loader/lifecycle/drain integration tests；records `3.5.*`。
**完成門檻：** layout/major/fingerprint 在 callback 前 fail closed；loaded DLL resident 至 process exit；new/update/remove/unloaded enable 需 restart；drain timeout 轉 pending-restart。

- [deferred] 3.5.1 驗證 DLL content/import/root/SDK major/UI fingerprint 的 pre-callback load order。
- [deferred] 3.5.2 驗證 startup-only load 與 loaded module resident-until-exit invariants。
- [deferred] 3.5.3 實作/稽核 disable sequence：gate new dispatch、cancel jobs、remove contributions、bounded callback drain。
- [deferred] 3.5.4 驗證 successful drain 立即停止 contribution 但不 unload DLL。
- [deferred] 3.5.5 驗證 callback drain timeout/fault 轉 pending-restart 且不強制中止或 unload。
- [deferred] 3.5.6 驗證安裝、替換、移除與啟用本次 startup 未載入 DLL 均只設定 restart semantics。
- [deferred] 3.5.7 將package/hash/signature/target/PE-policy pre-load validation獨立成gate，失敗時不得呼叫`LoadLibrary`。
- [deferred] 3.5.8 在`LoadLibrary`前durably寫入package/incarnation-scoped load-attempt marker，寫入失敗時拒絕load。
- [deferred] 3.5.9 以child-process DLLMain/TLS abort fixture證明next-start Safe Mode抑制嫌疑package。
- [deferred] 3.5.10 驗證成功registration後matching load-attempt原子clear/registered且不與callback records混用。
- [deferred] 3.5.11 驗證typed post-load rejection後marker=`rejected-resident`、DLL resident且non-dispatchable。
- [deferred] 3.5.12 驗證abnormal load termination留下incomplete attempt供next-start Safe Mode。

### 3.6 Call guard、panic translation 與 Safe Mode

**目的：** 在每個 native callback 建立可復原邊界與 crash attribution，下一次 startup 可安全隔離嫌疑 contribution。（legacy 3.5–3.7）
**輸入：** native dispatcher、persistent marker、SDK panic trampoline、diagnostic store。
**產出：** call marker lifecycle、typed panic errors、Safe Mode offer/confirmation、雙語營運文件。
**依賴：** 3.1、3.5。
**Owner／Wave：** `host-owner`＋`docs-owner`／W1；owned: call guard/Safe Mode/docs；forbidden: claiming sandbox or forced recovery。
**Gate／Evidence：** panic/uncleared-marker/slow/drain integration fixtures；records `3.6.*`。
**完成門檻：** marker 在 callback 前 durable、正常 return 後 clear；recoverable panic typed；uncleared marker 只禁用嫌疑 contribution；重新啟用需 explicit confirmation；文件清楚無 sandbox/熱卸載。

- [deferred] 3.6.1 驗證 marker 寫入包含 package/interface/operation/generation，且在 callback 前可觀察。
- [deferred] 3.6.2 驗證正常 return、typed error 與 recoverable panic 的 marker 清理/translation。
- [deferred] 3.6.3 模擬 abnormal termination，驗證 next-start Safe Mode 識別並預先禁用嫌疑 contribution。
- [deferred] 3.6.4 驗證 Safe Mode confirm failure 不會清除 offer 或重新 dispatch callback。
- [deferred] 3.6.5 驗證 timing/slow callback diagnostics bounded、privacy-safe 且不 unsafe-interrupt native code。
- [deferred] 3.6.6 更新無 sandbox、無 hot unload、restart、diagnostic recovery 文件並由 security reviewer 簽核。
- [deferred] 3.6.7 驗證explicit package/interface-scoped confirmation成功後只重新啟用matching contribution。
- [deferred] 3.6.8 驗證successful confirmation清除matching Safe Mode offer/marker且不啟用unrelated faulted contributions。
- [deferred] 3.6.9 驗證re-enabled contribution再次crash會重新留下marker並在next-start re-suppress。

### 3.7 Unified runtime authority envelope

**目的：** 在所有services/handles/callbacks前建立共同runtime authorization primitive，不讓registration-time capability check取代use-time revalidation。
**輸入：** 3.4 sealed authority、package generations、resource generation domains。
**產出：** `AuthorityEnvelopeV1` internal contract、issue/revoke/revalidate API與adversarial tests。
**依賴：** 3.4；phases 4–12所有service/handle consumer前置。
**Owner／Wave：** `host-owner`／W1；owned: `crates/explorer-extension-host/src/**` authority module；forbidden: public ABI/model/UI changes。
**Gate／Evidence：** runtime-authority named test selectors；records `3.7.*`。
**完成門檻：** envelope綁package/feature/interface/incarnation/capability/authorized root與resource generations；dispatch/use/commit重驗；disable/update/generation change revoke；tamper/TOCTOU零side effect。

- [deferred] 3.7.1 定義package、feature、interface、incarnation、capability、authorized-root與location/item/refresh/container/job generation envelope。
- [deferred] 3.7.2 實作authority issue與每次dispatch/use revalidation，registration validation不能直接當runtime grant。
- [deferred] 3.7.3 實作feature disable、package update、folder/view/F5、container mutation時revoke/stale semantics。
- [deferred] 3.7.4 驗證tampered package欄位fail closed。
- [deferred] 3.7.5 驗證stream/tool/lock/navigation/plan/virtual/renderer adapters只能消費validated envelope。
- [deferred] 3.7.6 模擬validate-use identity race，確認use/commit前recheck拒絕替換資源。
- [deferred] 3.7.7 驗證tampered feature欄位fail closed。
- [deferred] 3.7.8 驗證tampered interface欄位fail closed。
- [deferred] 3.7.9 驗證tampered capability欄位fail closed。
- [deferred] 3.7.10 驗證tampered authorized-root欄位fail closed。
- [deferred] 3.7.11 驗證tampered resource-generation欄位fail closed。

### 3.8 Dispatch barrier、call leases 與 core drain primitive

**目的：** 在任何column/view/Lua/virtual consumer前提供linearizable disable primitive與concurrent call correlation；Options只compose不重新發明。
**輸入：** 3.5 native lifecycle、3.6 call journal、3.7 authority、4.1 cancellation hooks。
**產出：** dispatch gate、active-call leases、late publish rejection、drain state-machine tests。
**依賴：** 3.5–3.7、4.1 cancellation interface；phases 5–12前置。
**Owner／Wave：** `host-owner`／W1（W2 integration重驗）；owned: host lifecycle primitives；forbidden: Options UI/feature-specific logic。
**Gate／Evidence：** concurrent/nested/disable race selectors；records `3.8.*`。
**完成門檻：** close dispatch→cancel resources→request GPUI detach/virtual redirect→drain leases→disabled/pending-restart；nested/concurrent records獨立；late registration/sink/cache/invalidation一律reject；never unload。

- [deferred] 3.8.1 實作atomic new-dispatch barrier與correlation-scoped active call lease acquire/release。
- [deferred] 3.8.2 驗證nested/concurrent callback return只清除matching record/lease。
- [deferred] 3.8.3 在barrier後關閉incremental sinks並拒絕late registration/cache publish/invalidation。
- [deferred] 3.8.4 提供jobs/streams/processes cancellation與GPUI detach/virtual redirect的ordered coordinator hooks。
- [deferred] 3.8.5 實作bounded lease drain與timeout/fault→pending-restart，禁止unsafe interrupt/force unload。
- [deferred] 3.8.6 驗證callback-start與disable barrier race。
- [deferred] 3.8.7 驗證callback-return與drain-timeout race。
- [deferred] 3.8.8 驗證rapid enable/disable toggle race。
- [deferred] 3.8.9 驗證package update與active lease race。
- [deferred] 3.8.10 驗證late result在barrier後被拒絕。

## 4. Extension Jobs、Values、Streams 與 Cache

### 4.1 Bounded scheduler 與 cancellation

**目的：** 讓同步 ABI provider 只由 host worker 執行，CPU/I/O queue、global/per-package limits、priority/deadline/cancel 全部有界。（legacy 4.1）
**輸入：** `explorer-jobs`、package generation/effective state、visible item priorities。
**產出：** scheduler policy、typed admission/terminal diagnostics、quota calibration record。
**依賴：** 3.4；quota numeric value 是 primary 的 B/C decision leaf。
**Owner／Wave：** `jobs-owner`／W1；owned: explorer-jobs；forbidden: ABI/UI/host registry changes。
**Gate／Evidence：** scheduler unit/stress tests；records `4.1.*`。
**完成門檻：** queues/limits 不可繞過；visible work 優先但不 starvation；cancel/deadline/disable 產生 typed terminal；Future/runtime handle 不跨 ABI。

- [deferred] 4.1.1 記錄並核准 CPU/I/O global/per-package queue/concurrency/aging/default quota 數值與校準 evidence。
- [deferred] 4.1.2 驗證 CPU 與 I/O queues 的 bounded admission、fairness、visible-row priority 與 starvation prevention。
- [deferred] 4.1.3 驗證explicit cancellation terminal state。
- [deferred] 4.1.4 驗證 synchronous provider callback 僅在 host worker 執行，無 Future/runtime handle 跨 ABI。
- [deferred] 4.1.5 以多 package overload fixture 證明 global/per-package limits、progress 與 diagnostic counters 正確。
- [deferred] 4.1.6 驗證deadline terminal state與cooperative stuck-callback policy。
- [deferred] 4.1.7 驗證package disable terminal state與sink closure。
- [deferred] 4.1.8 驗證queue shutdown terminal state與pending work disposition。

### 4.2 ABI job context、incremental sink 與 typed values

**目的：** 用 generation-safe owned ABI records 傳遞 bounded incremental values/outcomes/sort keys。（legacy 4.2、4.3）
**輸入：** public API、scheduler、item/location handles。
**產出：** `JobContextV1`、sink/value/sort/outcome fixtures、opaque routing rules。
**依賴：** 3.1、4.1。
**Owner／Wave：** `contract-owner`→`jobs-owner`／W1；contract first；forbidden: UI-specific state in ABI。
**Gate／Evidence：** ABI round-trip/backpressure/value sorting contracts；records `4.2.*`。
**完成門檻：** every batch bounded/tagged；bool/int/float/bytes/time/duration/text/localized/structured/opaque round-trip；sort independent from display；unsupported 不冒充 0；opaque 僅回原 renderer。

- [deferred] 4.2.1 凍結 `JobContextV1`、generation-safe handles、incremental sink 與 terminal records 的 ABI layout。
- [deferred] 4.2.2 驗證 incremental batch size/backpressure/closed-sink/cancel race 的 typed outcomes。
- [deferred] 4.2.3 對每個 `PluginValueV1` variant 執行 ABI round-trip 與 malformed/unknown-value policy。
- [deferred] 4.2.4 驗證 `StableSortValueV1` 對 bytes/time/integer/text 的 exact deterministic ordering 與 missing-last policy。
- [deferred] 4.2.5 驗證 unsupported、unavailable、cancelled、plugin-error、incompatible 在 cache/UI/diagnostics 不互相混淆。
- [deferred] 4.2.6 驗證 opaque payload 只能由相同 package/interface/generation renderer 解讀。

### 4.3 UI batching、generation cache 與 InputStream

**目的：** 合併結果 invalidation、拒絕 stale generation、提供 capability-authorized bounded decoder streams。（legacy 4.4–4.6）
**輸入：** job batches、watcher/TTL/manual invalidation、filesystem handles、capability tokens。
**產出：** batcher/cache/stream behavior reports and negative fixtures。
**依賴：** 3.4、4.2。
**Owner／Wave：** `jobs-owner`＋`host-owner`／W1；owned: jobs batching/cache and host stream adapter；forbidden: raw paths/native handles in ABI。
**Gate／Evidence：** batching/cache/stream contract suites；records `4.3.*`。
**完成門檻：** invalidation window 16–50 ms；1,000 results 不造成 1,000 redraw；cache key 完整；navigation/F5/watcher/data-version stale results 不更新；stream bounded/cancellable/generation-safe。

- [deferred] 4.3.1 校準並驗證 16–50 ms coalescing window、max batch 與 overload recovery。
- [deferred] 4.3.2 驗證 1,000 rapid results 的 redraw/invalidation count 有界且結果增量可見。
- [deferred] 4.3.3 驗證 cache key 含 package/interface/data version/file identity/metadata/options，recursive scan 再含 watcher/TTL/manual generation。
- [deferred] 4.3.4 驗證navigation generation change的stale result rejection。
- [deferred] 4.3.5 驗證 `filesystem.read` capability 缺失時不發 stream handle。
- [deferred] 4.3.6 驗證 bounded read/seek/length/deadline/cancel/source-generation 且 source change 不回填 current metadata。
- [deferred] 4.3.7 驗證tab switch的stale result rejection。
- [deferred] 4.3.8 驗證F5 refresh generation的stale result rejection。
- [deferred] 4.3.9 驗證watcher invalidation generation的stale result rejection。
- [deferred] 4.3.10 驗證feature disable的stale result rejection。
- [deferred] 4.3.11 驗證package generation change的stale result rejection。

### 4.4 1,000-item integration、diagnostics 與文件

**目的：** 證明基本列表先可互動、visible priority/cancel/batching/cache 可觀測且公開文件可重現。（legacy 4.7、4.8）
**輸入：** 1,000-item deterministic fixture、4.1–4.3 APIs。
**產出：** performance report、public jobs/value/stream/cache docs。
**依賴：** 4.1–4.3。
**Owner／Wave：** `fixture-owner`＋`docs-owner`／W1；owned: fixtures/tests/docs；forbidden: weakening thresholds to pass。
**Gate／Evidence：** named 1,000-item contract/performance and docs reproduction；records `4.4.*`。
**完成門檻：** basic list interactive before extensions complete；visible-first/cancel latency/redraw bounds pass twice；diagnostics identify package/interface without private data；docs commands clean-run offline。

- [deferred] 4.4.1 固定 1,000-item fixture inventory、expected generations、supported/unsupported/partial outcomes 與 hash。
- [deferred] 4.4.2 測量 basic list readiness 與 extension completion，證明列表先可互動。
- [deferred] 4.4.3 測量 visible-row priority、cancel latency、queue bounds、redraw/invalidation counts 並保存 raw samples。
- [deferred] 4.4.4 驗證 slow callback/backpressure/cache diagnostics 只含 package/interface/timing/typed terminal。
- [deferred] 4.4.5 定義 future UI selector/artifact mapping（僅登記，不執行 headful tests）。
- [deferred] 4.4.6 依公開 jobs/value/stream/cache 文件在 clean fixture 重現並由 docs reviewer 簽核。

## 5. 動態欄位與 GPUI Contribution

### 5.1 Column/provider/aggregate/renderer public contract gate

**目的：** 在任何host/model/UI consumer前凍結Column、single/batch/aggregate provider、renderer descriptor/context/factory的Rust-first ABI與stable ID semantics。
**輸入：** 3.1 ABI ownership/no-unwind、3.7 authority envelope、4.2 values/jobs、column/GPUI specs。
**產出：** public ABI records、layout hashes、compatibility/compile-fail fixtures、contract-owner review。
**依賴：** 3.1、3.7、4.2；5.2–5.6與phase6/8所有consumer的contract-first gate。
**Owner／Wave：** `contract-owner`／W1；owned: `crates/explorer-extension-api/**`、`crates/explorer-extension-ui-api/**`；forbidden: host/model/UI/example edits。
**Gate／Evidence：** column-ui-abi exact selectors；records `5.1.*`。
**完成門檻：** stable ID/descriptor/value/applicability/cost/sort/aggregate/renderer context/factory/ownership/drop/non-exhaustive policy完整；no private/std/future/raw callback；current/unknown/malformed fixtures與independent ABI review pass。

- [deferred] 5.1.1 凍結built-in/extension `ColumnId` namespace/grammar與column descriptor value/width/alignment/applicability/sort/cost records。
- [deferred] 5.1.2 凍結single provider descriptor/callback、authorized item input與typed result/terminal records。
- [deferred] 5.1.3 凍結batch provider descriptor、bounded batch-to-item mapping、cost/partial/unsupported semantics。
- [deferred] 5.1.4 凍結aggregate provider request/result/generation、dependency inputs與bounded output semantics。
- [deferred] 5.1.5 凍結renderer descriptor/factory/context的value/aggregate/loading/error/geometry/DPI/theme/action/invalidation records。
- [deferred] 5.1.6 定義所有ABI objects的allocation/drop thread/DLL lifetime/no-unwind policy。
- [deferred] 5.1.7 執行public column/provider/renderer ABI layout hash fixture。
- [deferred] 5.1.8 取得contract-owner與independent ABI reviewer簽核；任何後續contract change令5.2–16 dependent evidence stale。
- [deferred] 5.1.9 定義unknown non-exhaustive value preserve/reject policy。
- [deferred] 5.1.10 執行forbidden public type scan。
- [deferred] 5.1.11 執行handwritten raw author ABI compile-fail fixture。
- [deferred] 5.1.12 執行malformed column/provider/renderer descriptor fixture。
- [deferred] 5.1.13 執行unknown non-exhaustive value compatibility fixture。

### 5.2 Host authority registry 與 immutable model snapshot

**目的：** 建立 host-owned、package/feature/generation-scoped 欄位 authority，只投影 immutable owned catalog 給 model/UI；不得把 model registry 當 plugin authority。（legacy 5.1 foundation）
**輸入：** 3.4 ContributionGate、現有 `ColumnId`/descriptor/layout foundation、4.x values/jobs。
**產出：** host registry/reconciliation API、deterministic catalog snapshot、lifecycle tests。
**依賴：** 3.4、3.7、3.8、5.1；5.3 consumer 前置。
**Owner／Wave：** `host-owner`／W2；owned: host registries/adapters；forbidden: model/UI direct registration authority。
**Gate／Evidence：** registry/lifecycle integration tests、architecture review；records `5.2.*`。
**完成門檻：** validation/gate 後才 replace package；duplicate/ownership failure atomic；disable/revoke/fault/drain 不可 dispatch；reverse registration permutation 產生相同 catalog order。

- [deferred] 5.2.1 消費並驗證frozen 5.1 `ColumnId`/descriptor contract對host snapshot projection的相容性，回填legacy 5.1 evidence。
- [deferred] 5.2.2 在extension-host定義package/feature/interface/incarnation/generation scoped authority與sealed registration input。
- [deferred] 5.2.3 實作atomic package descriptor validation與replace，任何failure不改舊snapshot。
- [deferred] 5.2.4 實作package unregister並驗證visibility撤銷但layout intent保留。
- [deferred] 5.2.5 實作deterministic snapshot reconciliation，按stable package/descriptor order而非callback arrival order。
- [deferred] 5.2.6 以正反package registration permutation驗證snapshot/order/hash相同。
- [deferred] 5.2.7 驗證disabled feature snapshot/dispatch。
- [deferred] 5.2.8 architecture reviewer確認model `ColumnRegistry`只是projection/layout helper，不能繞過host authority。
- [deferred] 5.2.9 實作package revoke並驗證dispatch撤銷但layout intent保留。
- [deferred] 5.2.10 驗證blocked feature snapshot/dispatch。
- [deferred] 5.2.11 驗證faulted feature snapshot/dispatch。
- [deferred] 5.2.12 驗證Safe Mode suppressed feature snapshot/dispatch。
- [deferred] 5.2.13 驗證draining feature snapshot/dispatch。
- [deferred] 5.2.14 驗證package-update generation snapshot/dispatch。

### 5.3 Provider、typed sort 與 renderer binding

**目的：** 註冊 single/batch/aggregate providers 與 feature-gated renderer，display/value/sort semantics 分離。
**輸入：** 5.1 catalog、4.1–4.3 scheduler/value/cache、renderer descriptors。
**產出：** provider dispatch registries、typed sort pipeline、aggregate routing。
**依賴：** 3.7、3.8、5.1、5.2。
**Owner／Wave：** accountable `host-owner`／W2；contributor `model-ui-owner`只改`crates/explorer-model/**` projection；forbidden: UI direct provider dispatch與同檔共寫。
**Gate／Evidence：** provider/sort/aggregate integration matrix；records `5.3.*`。
**完成門檻：** provider selection deterministic；unsupported/unavailable/missing stable；aggregate generation-safe；renderer 只綁相同 package/feature/value contract；disabled/faulted 不 dispatch。

- [deferred] 5.3.1 實作single/batch/aggregate descriptor compatibility與cost/applicability validation。
- [deferred] 5.3.2 將provider dispatch接到scheduler與validated authority envelope。
- [deferred] 5.3.3 將provider dispatch接到generation/cache/cancellation與closed-sink late-result rejection。
- [deferred] 5.3.4 實作display value與stable sort key分離的typed ordering及missing/unsupported policy。
- [deferred] 5.3.5 實作aggregate request/result generation與bounded aggregate routing。
- [deferred] 5.3.6 驗證renderer package/feature/value-type binding，跨package opaque renderer被拒絕。
- [deferred] 5.3.7 驗證feature disable後無新provider/renderer callback。
- [deferred] 5.3.8 驗證plugin fault後無新provider/renderer callback。
- [deferred] 5.3.9 驗證feature draining期間無新provider/renderer callback。
- [deferred] 5.3.10 驗證barrier後late provider result被拒絕。
- [deferred] 5.3.11 驗證barrier後late renderer invalidation被拒絕。

### 5.4 Details UI 全動態化

**目的：** header、chooser、rows、virtualization、scroll、input 與 UIA 全部從 catalog/layout projection 驅動，不保留固定欄位分支。
**輸入：** 5.2 snapshot、5.3 value/sort pipeline、model ordered layout。
**產出：** dynamic details UI、built-in parity tests、headful evidence。
**依賴：** 5.2、5.3、5.5 persistence schema。
**Owner／Wave：** `model-ui-owner`／W2；owned: model/UI column consumers；forbidden: host registry/public ABI edits。
**Gate／Evidence：** model/UI unit+integration；records `5.4.*`。
**完成門檻：** built-in 與 extension descriptors 共用 header/row path；resize/reorder/visibility/sort/horizontal overflow/keyboard/UIA/session restore 可觀察且 virtualization 有界。

- [deferred] 5.4.1 將details header/order/width/visibility完全改由registry-effective ordered layout產生。
- [deferred] 5.4.2 將column chooser與feature unavailable/unknown hidden states接到immutable catalog snapshot。
- [deferred] 5.4.3 將virtual rows、horizontal extent、cell selection/hit testing與resize bounds改為descriptor-driven。
- [deferred] 5.4.4 將keyboard focus/sort commands/accessibility names/UIA grid semantics改為stable dynamic IDs。
- [deferred] 5.4.5 將session restore只消費5.5已遷移schema，未知ID不可在UI路徑被刪除。
- [deferred] 5.4.6 驗證built-in名稱/日期/類型/大小排序與既有UI文案/virtualization無回歸。
- [deferred] 5.4.7 以至少兩packages同local ID驗證column chooser不碰撞。
- [deferred] 5.4.8 以至少兩packages同local ID驗證ordered layout不碰撞。
- [deferred] 5.4.9 以至少兩packages同local ID驗證resize state不碰撞。
- [deferred] 5.4.10 以至少兩packages同local ID驗證typed sort不碰撞。
- [deferred] 5.4.11 以至少兩packages同local ID驗證UIA identities不碰撞。

### 5.5 Persistence migration 與 unknown-ID round-trip

**目的：** 將 fixed session schema 遷移為 extensible ordered map/list，未知 plugin ID 隱藏但 byte/semantic round-trip，重裝恢復。（legacy 5.4）
**輸入：** current session schema、5.2 descriptor snapshots、5.1 stable ID encoding。
**產出：** versioned persistence schema、migration/rollback fixtures、reinstall recovery tests。
**依賴：** 5.1、5.2；本L2完成後才解鎖5.4 UI/session restore。
**Owner／Wave：** `model-ui-owner`／W2；owned: model session/layout migration；forbidden: silently dropping extension IDs or reusing schema version。
**Gate／Evidence：** migration golden/round-trip/reinstall tests；records `5.5.*`。
**完成門檻：** legacy built-ins 一次遷移；extension sort/layout/width/visibility/order 保存；unknown hidden 不丟失；same ID reinstall 恢復；corrupt/over-limit data safe fallback。

- [deferred] 5.5.1 定義新session schema/version與built-in/extension column ID canonical encoding。
- [deferred] 5.5.2 實作舊fixed widths/visibility/order/sort到新schema的一次性migration golden fixture。
- [deferred] 5.5.3 實作unknown extension IDs的load/save semantic round-trip，UI hidden但persisted intent不刪除。
- [deferred] 5.5.4 驗證extension sort未安裝時safe fallback、重裝後可再次選取/恢復layout。
- [deferred] 5.5.5 驗證remove/reinstall同ID恢復width/order/visibility，換ID不錯誤繼承。
- [deferred] 5.5.6 驗證corrupt persisted entry bounded fallback且不破壞其他session state。
- [deferred] 5.5.7 驗證duplicate persisted ID deterministic reconciliation且保留其他entries。
- [deferred] 5.5.8 驗證over-length persisted ID bounded rejection/fallback。
- [deferred] 5.5.9 驗證over-count persisted entries有界截斷/diagnostic且不OOM。

### 5.6 GPUI renderer context、安全與 SDK 文件

**目的：** 只在 GPUI thread 以 public immutable state/theme/action/invalidation 渲染，自訂 cell 不得 I/O 或保留 private entity。
**輸入：** 5.1 renderer ABI、5.3 values/aggregates、UI theme/action facades。
**產出：** `GpuiColumnRendererV1` adapter、thread/timing/panic diagnostics、public docs/fixtures。
**依賴：** 3.6–3.8、4.3、5.1、5.3、5.4。
**Owner／Wave：** accountable `model-ui-owner`／W2；`contract-owner`只review frozen 5.1，`docs-owner`只改public docs；forbidden: host/API edits。
**Gate／Evidence：** compile forbidden-surface、GPUI thread/panic/slow/I/O fixtures；records `5.6.*`。
**完成門檻：** context 僅含 public value/loading/error/aggregate/selection/hover/DPI/theme/action/invalidation；wrong thread/forbidden I/O/panic/slow callback 可診斷且不破壞 host state。

- [deferred] 5.6.1 實作immutable value/aggregate/loading/error、selection/hover geometry、DPI/theme facade projection。
- [deferred] 5.6.2 實作scoped action sink/invalidation handle，拒絕stale/cross-package/retained-after-close use。
- [deferred] 5.6.3 加入GPUI-thread assertion與renderer filesystem/network/blocking-I/O negative fixture。
- [deferred] 5.6.4 加入renderer panic marker、slow threshold、invalidation throttling與typed fallback integration fixtures；UI execution deferred to the example final slice.
- [deferred] 5.6.5 驗證renderer create/drop遵守5.1 ownership/drop/no-unwind與DLL lifetime。
- [deferred] 5.6.6 完成dynamic column/renderer雙語SDK guide與clean consumer sample。

## 6. 第一垂直切片：Rust 資料夾大小欄位

### 6.1 Independent consumer framework 與 example validator

**目的：** 在第一個 slice 前固定獨立 workspace/templates/common checklist，拒絕 private crate、unwired interface 與缺少必要 artifacts。
**輸入：** approved bundle、public SDK、source-example spec。
**產出：** consumer workspace、Rust/Lua templates、example validator、common artifact rules。
**依賴：** 2.5、3.1；所有 example project 的前置。
**Owner／Wave：** `sdk-tooling-owner`／W1；owned: SDK example framework；forbidden: example feature logic、product crates。
**Gate／Evidence：** example-validator positive/negative selectors；records `6.1.*`。
**完成門檻：** 每 example 必須有 source/manifest/locales/zh-TW+en README/license/NOTICE/provenance/fixtures/unit/integration/UITEST/screenshots/package/modify guide；private/unwired/缺項各自 fail。

- [deferred] 6.1.1 建立 independent consumer workspace 與 Rust/Lua templates、directory/manifest/localization/license conventions。
- [deferred] 6.1.2 實作 private workspace dependency、path dependency、composition bypass 與 protected closure drift rejection。
- [deferred] 6.1.3 實作每 example required artifact/locale/doc/test/screenshot/package inventory validation。
- [deferred] 6.1.4 實作 public interface 必須有 production composition-root registration＋official example use，trait/mock-only fail。
- [deferred] 6.1.5 驗證八個 project metadata 互相獨立且不成為 root workspace members。

### 6.2 獨立 consumer 與 feature contract

**目的：** 先以 public SDK 建立可安裝、可修改、可封裝的 `rust-folder-size-visual-column`，不得引用 private workspace crate。
**輸入：** 2.5 tooling、5.x column/provider/renderer contracts、6.1 shared consumer rules。
**產出：** source、manifest、locales、zh-TW/en README、license/NOTICE/provenance、fixtures、package command。
**依賴：** 3.7、3.8、5.1–5.6、6.1；第一個 example gate，未過不得開始 phase 7 slice。
**Owner／Wave：** `example-owner`／W2；owned: 此 example directory；forbidden: product/API/shared manifests。
**Gate／Evidence：** example validator、clean consumer metadata/build；records `6.2.*`。
**完成門檻：** 專案不在主 workspace、不引用 private crates；三個 features/capabilities/contacts/locales/licensing 完整；README 可在 published bundle clean-run。

- [deferred] 6.2.1 建立獨立 project、locked dependencies、manifest 與 column/recalculate/settings 三個 stable feature IDs。
- [deferred] 6.2.2 宣告最小 capabilities、publisher contacts、zh-TW/en locales 與 package entry points。
- [deferred] 6.2.3 建立 LICENSE/NOTICE/SBOM/provenance，分類所有 static Rust dependencies。
- [deferred] 6.2.4 撰寫 zh-TW/en README 的 build/test/modify/validate/package/install 步驟。
- [deferred] 6.2.5 執行 private-crate/composition-bypass/unlocked-dependency validator negative fixtures。

### 6.3 Recursive provider、aggregation 與 cache

**目的：** 在 background 有界遞迴 folder bytes，回傳 exact sort/partial/cancelled results 與 largest-sibling aggregate。
**輸入：** 4.x jobs/cache/streams、5.3 provider/aggregate API、authorized item handles。
**產出：** provider/aggregator implementation、unit/integration fixtures。
**依賴：** 6.2、5.2。
**Owner／Wave：** `example-owner`／W2；owned: example source/tests only。
**Gate／Evidence：** 1,000-item/cycle/partial/cancel/cache tests；records `6.3.*`。
**完成門檻：** no renderer I/O；symlink/junction cycle bounded；inaccessible subtree partial；exact bytes sorting；watcher/F5/manual/data-version invalidation generation-safe。

- [deferred] 6.3.1 實作 authorized recursive byte scan、deadline/cancel 與 bounded incremental results。
- [deferred] 6.3.2 實作 symlink/junction no-follow default、identity cycle prevention 與 inaccessible partial outcomes。
- [deferred] 6.3.3 實作 exact byte `StableSortValueV1`、loading/unsupported/unavailable/cancelled/error states。
- [deferred] 6.3.4 實作 largest-sibling aggregate，對 missing/partial/generation mismatch 不產生錯誤比例。
- [deferred] 6.3.5 驗證 watcher、TTL、manual recalculate、F5 與 plugin data version cache invalidation。

### 6.4 GPUI cell/settings 與完整 example gate

**目的：** 用 public render context 畫比例條並完成 example 的 unit/integration/UITEST/screenshots/package gate。
**輸入：** 6.3 values、5.6 renderer/settings/action APIs、shared example checklist。
**產出：** cell renderer/settings/recalculate command、evidence artifacts、installable `.sepack`。
**依賴：** 6.2、6.3。
**Owner／Wave：** `example-owner`＋`fixture-owner`／W2；UITEST manifest merge only by integrator。
**Gate／Evidence：** renderer thread/I/O tests、integration/performance、README reproduction、package validation、inventory/composition GO，最後才是 1,000-item UITEST；records `6.4.*`。
**完成門檻：** valid rows exact sort、partial states truthful、renderer only consumes background results；所有共同 artifacts/測試/雙語 docs/screenshots/clean package 個別通過。

- [deferred] 6.4.1 實作 DPI/theme/selection-aware proportional cell，設定可調且無 filesystem/network I/O。
- [deferred] 6.4.2 實作 recalculate command/settings feature，驗證 disable/re-enable 與 stale invalidation。
- [deferred] 6.4.3 執行 unit tests：bytes/cycle/partial/cache/aggregate/sort/renderer states。
- [deferred] 6.4.4 執行 integration/performance tests，保存 raw timing。
- [deferred] 6.4.5 在空 consumer 執行 README build/test/validate/package 並驗證 deterministic `.sepack` hash。
- [deferred] 6.4.6 執行 `common_artifact_inventory`、記錄 production composition-root registration 與 interface coverage，取得 provisional slice GO。
- [deferred] 6.4.7 在 Task 6 除本 leaf 外的全部 leaves（6.1.1–6.4.6）均完成後，首次且唯一地執行 Task 6 的 1,000-item UITEST，保存 screenshots 與 accessibility evidence。

## 7. 第二垂直切片：Dynamic View、Tree Scan 與 Size Map

### 7.1 View ABI、host registry 與 model fallback

**目的：** feature-scoped dynamic view 可註冊、保存 unknown ID、fault/disable 時 safe fallback，且不持有 private tab state。
**輸入：** 3.4 authority、3.7/3.8 runtime gates、5.6 GPUI boundary、session/navigation model。
**產出：** view descriptor/registry snapshot、built-in/extension ID persistence/fallback。
**依賴：** 6.4 GO；contract-owner 先凍結 view records。
**Owner／Wave：** accountable `contract-owner`／W1 contract gate；`host-owner`與`model-ui-owner`為W3 non-overlap consumers，各只改全域owned paths並handoff給integrator。
**Gate／Evidence：** ABI/registry/session/fallback tests；records `7.1.*`。
**完成門檻：** view 必須通過 fingerprint/feature/capability 才可見；missing/incompatible/faulted/disabled fallback 到 last usable built-in/Details 且保留 unknown ID。

- [deferred] 7.1.1 凍結 `ViewModeRegistrationV1` stable ID/name/icon/location/priority/selection/factory records 與 ownership/drop rules。
- [deferred] 7.1.2 實作 host view registry 的 capability/duplicate/fingerprint validation 與 deterministic snapshot。
- [deferred] 7.1.3 將 model/session view state 遷移為 built-in/extension ID 並 round-trip unknown ID。
- [deferred] 7.1.4 實作 missing/incompatible/faulted/disabled fallback，不自動強迫 re-enabled view 成為 current。
- [deferred] 7.1.5 驗證 reverse registration order、remove/reinstall、startup unavailable 與 active disable transitions。

### 7.2 GPUI view lifecycle、selection 與 formal navigation

**目的：** renderer 只收 owned public context，selection/navigation 回到正式 tab/history/address/breadcrumb。
**輸入：** 7.1 view snapshot、navigation/session state、public GPUI facade。
**產出：** renderer lifecycle dispatcher、selection bridge、navigation request adapter。
**依賴：** 7.1。
**Owner／Wave：** `model-ui-owner`／W3；owned: view UI/model adapter；forbidden: renderer retaining ExplorerState/entity。
**Gate／Evidence：** lifecycle/generation/navigation/UIA tests；records `7.2.*`。
**完成門檻：** create/render/focus/blur/location/selection/refresh/suspend/resume/close ordered；old context unusable；single click shares selection；double click uses formal navigation/history/open policy。

- [deferred] 7.2.1 實作 public view context 的 location/refresh generations、viewport/DPI/theme/focus/selection/action sink。
- [deferred] 7.2.2 實作 lifecycle transition table與 create/close/drop thread/no-unwind tests。
- [deferred] 7.2.3 實作 opaque selection exchange並拒絕 unknown/stale node IDs。
- [deferred] 7.2.4 實作 open/enter/new-tab/reveal requests 的 authorization 與 formal model dispatch。
- [deferred] 7.2.5 驗證 double-click folder 更新 address/breadcrumb/history，file 使用 existing open policy。

### 7.3 Host-owned recursive tree scan

**目的：** off-GPUI-thread 產生 bounded generation-tagged tree deltas，處理 quotas、partial、symlink/hard-link policy。
**輸入：** 4.x scheduler/cache、authorized location handles、7.1 view feature。
**產出：** `DirectoryTreeScanServiceV1`、delta/terminal/cache implementation、calibration report。
**依賴：** 7.1；memory/cancel/quota thresholds 需 primary 核准。
**Owner／Wave：** `host-owner`＋`jobs-owner`／W3；owned: host scan/jobs adapters；forbidden: UI I/O。
**Gate／Evidence：** deep/wide/cycle/partial/stale/resource tests；records `7.3.*`。
**完成門檻：** bounded add/update/remove/partial/subtree-complete/complete；default no-follow；cycles prevented；hard-link policy visible；terminal states distinct；late deltas rejected。

- [deferred] 7.3.1 核准 scan node/memory/deadline/cancel/delta batch/cache bounds 與 hardware calibration profile。
- [deferred] 7.3.2 實作 authorized request validation與 owned node/parent IDs，不暴露 unauthorized path/native handle。
- [deferred] 7.3.3 實作 bounded deltas與 complete/partial/cancelled/unavailable/resource-limited/failed terminals。
- [deferred] 7.3.4 實作 symlink/junction cycle detection與 default/identity-once hard-link accounting。
- [deferred] 7.3.5 驗證F5 refresh的stale delta/cache rejection。
- [deferred] 7.3.6 驗證location change的stale delta/cache rejection。
- [deferred] 7.3.7 驗證tab switch的stale delta/cache rejection。
- [deferred] 7.3.8 驗證view switch的stale delta/cache rejection。
- [deferred] 7.3.9 驗證feature disable的stale delta/cache rejection。
- [deferred] 7.3.10 驗證package update的stale delta/cache rejection。

### 7.4 `rust-folder-size-map-view` 與 100,000-node gate

**目的：** 第二個獨立 example 以 incremental squarified treemap 證明 view/scan/navigation/accessibility 完整垂直路徑。
**輸入：** 7.1–7.3、6.1 shared example rules、synthetic 100k fixture。
**產出：** complete consumer project、treemap renderer/settings、package/evidence/screenshots。
**依賴：** 7.1–7.3。
**Owner／Wave：** `example-owner`＋`fixture-owner`／W3；example paths only。
**Gate／Evidence：** unit/integration/UITEST/performance/docs/package；records `7.4.*`。
**完成門檻：** area=logical bytes、nesting=folders、color=file type；partial/tooltips/Other accessible；100k memory/layout/redraw/cancel thresholds pass；formal navigation與stale rejection pass。

- [deferred] 7.4.1 建立完整 project/manifest/features/locales/license/NOTICE/SBOM/zh-TW/en README。
- [deferred] 7.4.2 實作 incremental squarified treemap、stable colors、exact tooltip、partial/loading/error legend。
- [deferred] 7.4.3 實作 keyboard/UIA traversal與 aggregated `Other` 的 data/search/accessibility representation。
- [deferred] 7.4.4 在該 example 完成 production wiring、SDK、fixtures、docs、package 後，執行 selection/double-click/history/F5 race/disable-active/fallback 的 final-slice UITEST。
- [deferred] 7.4.5 執行 100,000-node raw memory/layout/redraw/cancel benchmark，對核准 hardware profile 判定。
- [deferred] 7.4.6 執行`common_artifact_inventory`、provenance/modify-guide/screenshots、clean README/validate/package，由integrator merge manifest後取得provisional slice GO。

## 8. 第三、四垂直切片：Rust tokei 與 Lock Owner

### 8.1 Batch provider contract 與 Rust tokei example

**目的：** 完成 bounded batch-to-item mapping/cost semantics，並以 static Rust tokei library 證明多 typed numeric values、無 per-file process。
**輸入：** 5.3 provider API、4.x scheduler、6.1 example rules。
**產出：** batch API tests、`rust-tokei-code-lines-column` complete consumer/package。
**依賴：** 7.4 GO。
**Owner／Wave：** `contract-owner`→`example-owner`／W3。
**Gate／Evidence：** mixed-language/1,000-file/process-observation/docs/package；records `8.1.*`。
**完成門檻：** batch bounds/map stable；language/code/comment/blank/total typed；unsupported binary/unknown 不報 0；無 OS process per file；full common example gate passes。

- [deferred] 8.1.1 消費並驗證frozen 5.1 `BatchColumnProviderV1` contract，不重新定義其ABI或numeric semantics。
- [deferred] 8.1.2 建立 complete consumer、manifest/settings/features/locales/README/license/provenance。
- [deferred] 8.1.3 鎖定並靜態連結 Rust tokei library，驗證 protected closure 不漂移。
- [deferred] 8.1.4 實作 language/code/comment/blank/total values與可選 exact numeric sort metric。
- [deferred] 8.1.5 驗證 Rust/C/C++/Python/Lua/JS/empty/invalid-text/unknown/1,000 files 與 no-process observation。
- [deferred] 8.1.6 完成renderer/settings/toggle implementation、`common_artifact_inventory`、provenance/modify-guide、screenshots與clean package，由integrator merge manifest後取得provisional GO。
- [deferred] 8.1.7 在8.1.1–8.1.6完成後，執行renderer/settings/toggle final-slice UITEST。

### 8.2 LockOwner host service contract

**目的：** 將 Restart Manager 包成只讀、bounded、generation-safe service，public surface 無 process control。
**輸入：** existing Windows adapter、4.x handles/jobs/cache、capability authority。
**產出：** `LockOwnerQueryServiceV1`、Windows adapter tests、TTL calibration。
**依賴：** 8.1 GO；contract-owner 先凍結 owned result records。
**Owner／Wave：** accountable `contract-owner`／W1 service contract；`windows-owner`只改shell-win adapter、`host-owner`只改host service adapter並在W3 consumer gate重驗。
**Gate／Evidence：** public-surface scan/helper-process integration；records `8.2.*`。
**完成門檻：** bounded authorized inputs/results、deadline/cancel/session cleanup；empty/unavailable/error distinct；無 terminate/shutdown/close-handle；F5/manual同 generation pipeline。

- [deferred] 8.2.1 凍結 authorized item request與 PID/safe name/application type/status owned result ABI。
- [deferred] 8.2.2 公開 surface scan 證明無 shutdown/terminate/close-handle/native Restart Manager handle。
- [deferred] 8.2.3 實作 max input/result、deadline/cancel與 success/error/race 全路徑 session cleanup。
- [deferred] 8.2.4 核准 short TTL 數值並接 watcher/F5/manual refresh generation/cache invalidation。
- [deferred] 8.2.5 驗證 no owner=valid empty、denied/protected=unavailable、adapter fault=plugin error。

### 8.3 `rust-lock-owner-column` 完整 gate

**目的：** 以第四個 example 顯示單/多 owner、details/manual refresh 並證明 stale result 不復活。
**輸入：** 8.2 service、5.x columns、helper process fixtures。
**產出：** complete consumer/package、Windows evidence/screenshots/docs。
**依賴：** 8.2。
**Owner／Wave：** `example-owner`＋`fixture-owner`／W3。
**Gate／Evidence：** helper acquire/release/multi/race/denied/cleanup/UITEST；records `8.3.*`。
**完成門檻：** acquire後顯示、release+F5後清除；late old generation不能恢復；feature無 process-control capability；full common example gate passes。

- [deferred] 8.3.1 建立 complete consumer/manifest/features/locales/README/license/NOTICE/provenance。
- [deferred] 8.3.2 實作 background batch provider、single/multiple owner display、details與manual refresh command。
- [deferred] 8.3.3 執行 acquire/release、multiple owners、owner exit race、access denied與 resource cleanup tests。
- [deferred] 8.3.4 在該 example 全部 artifacts 完成後，執行 rapid F5與tab/folder/disable stale-generation rejection final-slice UITEST。
- [deferred] 8.3.5 執行`common_artifact_inventory`、provenance/modify-guide/screenshots、clean README/validate/package，由integrator merge manifest後取得provisional GO。

## 9. Commands、Forms 與 Operation Plans 核心

### 9.1 Command/button 與 typed form registries

**目的：** feature-scoped commands/buttons/forms 只經 host-owned registry與 declarative validation，Lua只能 host-rendered forms。
**輸入：** 3.4 authority、public ABI、selection/action systems。
**產出：** descriptors/registries、form schema/submission validation、placement/UI adapters。
**依賴：** 8.3 GO；contract-owner first。
**Owner／Wave：** accountable `contract-owner`／W1 command/form contract；`host-owner`與`model-ui-owner`在W3只改各自owned consumers，shared composition由integrator。
**Gate／Evidence：** registry/form/accessibility/capability tests；records `9.1.*`。
**完成門檻：** stable ID/feature/capability/placement/predicate/shortcut validated；text/int/bool/choice/authorized path/template bounded；invalid field不產 plan；disable立即移除且不 dispatch。

- [deferred] 9.1.1 凍結 command/button descriptor與 placement/selection/shortcut conflict semantics。
- [deferred] 9.1.2 實作 host registry duplicate/capability/feature/effective-state validation與 UI snapshot。
- [deferred] 9.1.3 凍結 `FormSchemaV1`/typed values/submission/localized error records與 Rust GPUI optional adapter boundary。
- [deferred] 9.1.4 實作 field bounds、choice membership、authorized path/template validation與 focus/UIA semantics。
- [deferred] 9.1.5 驗證 disabled feature entries disappear、shortcut conflict deterministic、invalid count 1–100,000 fail before plan。

### 9.2 Operation plan authorization、preview 與 TOCTOU validation

**目的：** extensions只描述 typed intent；host在 preview與execute前以 authorized handles驗 path/identity/conflict/permissions/limits。
**輸入：** command/form submission、filesystem authorization、file-operation pipeline。
**產出：** `OperationPlanV1`/preview/validator、adversarial plan corpus。
**依賴：** 9.1。
**Owner／Wave：** `contract-owner`→`host-owner`／W3；forbidden: extension direct OS mutation。
**Gate／Evidence：** normalization/collision/TOCTOU/confirmation tests；records `9.2.*`。
**完成門檻：** absolute/device/drive/parent/separator/reserved/trailing dot-space/case collision fail；target identity execute前 recheck；>1,000 second confirmation；無 approval零 mutation。

- [deferred] 9.2.1 凍結 create/rename/copy/move/delete/extract/archive-mutation steps、preview與 terminal records。
- [deferred] 9.2.2 將 plan roots/items 綁 authorized opaque handles，而非任意 raw paths。
- [deferred] 9.2.3 驗證path escape plan rejection。
- [deferred] 9.2.4 驗證 permissions/conflict/estimated work/warnings/irreversible reasons 完整呈現在 preview。
- [deferred] 9.2.5 驗證 >1,000 steps second confirmation與 representative names。
- [deferred] 9.2.6 模擬 preview後 external identity/permission/conflict變更，execute前拒絕且零非授權 mutation。
- [deferred] 9.2.7 驗證Windows reserved basename plan rejection。
- [deferred] 9.2.8 驗證trailing dot/space basename plan rejection。
- [deferred] 9.2.9 驗證case-insensitive duplicate target plan rejection。
- [deferred] 9.2.10 驗證operation count bounds plan rejection。

### 9.3 Executor、progress、cancellation 與 conservative undo

**目的：** approved plans 經既有 file-operation pipeline bounded execution，partial結果與undo不刪使用者內容。
**輸入：** validated plan、model/shell operation adapters、undo journal。
**產出：** executor adapter、progress/cancel/partial/journal evidence。
**依賴：** 9.2。
**Owner／Wave：** accountable `host-owner`／W3；`model-ui-owner`只交付model operation adapter、`windows-owner`只交付shell operation adapter，shared composition由integrator。
**Gate／Evidence：** plan execution/undo destructive-fixture suite；records `9.3.*`。
**完成門檻：** bounded batches/progress/cancel；completed/failed/unattempted 精確；undo只處理可安全恢復項；本計畫建立但已有內容的目錄保留並報告。

- [deferred] 9.3.1 將每個 plan step映射既有 file-operation request，拒絕 private model/shell references回傳 extension。
- [deferred] 9.3.2 實作 bounded scheduling/progress/cancel barrier與 partial terminal summary。
- [deferred] 9.3.3 實作 operation identity journal與 reversible/irreversible classification。
- [deferred] 9.3.4 實作 CreateDirectory undo只刪本 plan 建立且仍空的目錄。
- [deferred] 9.3.5 驗證 cancel中途、單步失敗、使用者新增內容、late terminal與重複 undo idempotence。

## 10. Lua Registrar、Bundled Tools 與第五、六、七垂直切片

### 10.1 Restricted Lua registrar 與 shared typed semantics

**目的：** Lua registrations 使用相同 host authority/value/plan semantics，無 arbitrary GPUI/private/filesystem/network/process APIs。
**輸入：** 5.x columns、9.x commands/forms/plans、3.4 authority、restricted VM。
**產出：** registrar adapter、Lua serde mirrors、capability denial diagnostics。
**依賴：** 8.3 GO、9.1–9.3 core contracts。
**Owner／Wave：** `host-owner`／W3；owned: Lua host adapter；forbidden: arbitrary GPUI element API。
**Gate／Evidence：** registration/serde/capability tests；records `10.1.*`。
**完成門檻：** single/batch columns、commands/buttons/forms/plans immutable descriptors；every call rechecks feature/capability/incarnation；Rust/Lua typed round-trip equivalent。

- [deferred] 10.1.1 實作 Lua single/batch column、command、button、host form與 plan-provider registrations。
- [deferred] 10.1.2 將 registration與每次 callback綁 package/feature/interface/incarnation/capability authority。
- [deferred] 10.1.3 實作 `PluginValueV1`、terminal、`OperationPlanV1` Lua serde mirrors與 unknown/malformed policy。
- [deferred] 10.1.4 驗證 undeclared filesystem/network/process/private-model call denied、scoped diagnostic、零 side effect。
- [deferred] 10.1.5 驗證 disable/update/stale generation 後既有 Lua handles/callbacks 不可使用。

### 10.2 Bundled tool validation、opaque resolver 與 TOCTOU identity

**目的：** 只執行 `.sepack/tools/<target>/<id>` 已驗證 payload，handle scoped且spawn前重驗 identity，永不 fallback系統工具。
**輸入：** package content authority、tool descriptors、package generations。
**產出：** validator/resolver/opaque handles、tamper/no-fallback corpus。
**依賴：** 3.2、10.1。
**Owner／Wave：** `host-owner`＋`fixture-owner`／W3。
**Gate／Evidence：** tool validation/identity/path security tests；records `10.2.*`。
**完成門檻：** target/path/version/size/hash/protocol/source/license/reparse validated；stale/tampered/missing/wrong-target拒絕；PATH/Registry/common/network/user substitute零查詢。

- [deferred] 10.2.1 凍結 `BundledToolDescriptorV1`與 package-generation-scoped opaque `ToolHandleV1` records。
- [deferred] 10.2.2 驗證 tool path containment、reparse、size/hash/target/protocol/source/license/NOTICE。
- [deferred] 10.2.3 issuance時綁 payload identity，spawn前以已驗 object/identity重驗，拒絕 name substitution race。
- [deferred] 10.2.4 驗證missing tool payload fail closed。
- [deferred] 10.2.5 放置 PATH/Registry/common-path decoy，證明 resolver不查詢、不下載、不提示 substitute。
- [deferred] 10.2.6 驗證tampered tool payload fail closed。
- [deferred] 10.2.7 驗證wrong-target tool payload fail closed。
- [deferred] 10.2.8 驗證stale-generation tool handle fail closed。

### 10.3 Shell-free process request 與 Job Object lifecycle

**目的：** direct executable+argument array受 cwd/env/stdin/time/output bounds控制，child tree在所有終止路徑回收。
**輸入：** validated ToolHandle、Windows process/Job Object APIs、cancellation tokens。
**產出：** `ProcessRequestV2`/`ProcessLease`、process fixture logs。
**依賴：** 10.2。
**Owner／Wave：** `windows-owner`＋`host-owner`／W3。
**Gate／Evidence：** injection/timeout/cancel/truncation/tree cleanup tests；records `10.3.*`。
**完成門檻：** no cmd/PowerShell string；metacharacter one literal arg；process created suspended→assigned Job→resumed；cancel/timeout/drop/disable/folder change全樹reap；pipes不deadlock。

- [deferred] 10.3.1 凍結 executable handle、argument array、authorized cwd、env allowlist、stdin、deadline/output bounds/terminal records。
- [deferred] 10.3.2 實作 direct shell-free spawn並驗 quotes/ampersand/command-substitution filenames為單一literal arg。
- [deferred] 10.3.3 實作 suspended create、Job Object assign成功後resume；assign failure不得執行child code。
- [deferred] 10.3.4 實作 cancel、timeout、lease drop、feature disable、folder change的tree termination/reap。
- [deferred] 10.3.5 驗證 exit/timeout/cancelled/spawn-failed/output-truncated terminals與stdout/stderr bound、無pipe deadlock/leak。

### 10.4 `lua-tokei-code-lines-column` 完整 gate

**目的：** 第五個 example 封裝 exact tokei.exe，以 bounded shell-free JSON batches映射 stable handles。
**輸入：** 10.1–10.3、5.x batch columns、6.1 rules。
**產出：** complete Lua package、tool payload/provenance、evidence/screenshots。
**依賴：** 10.1–10.3。
**Owner／Wave：** `example-owner`＋`fixture-owner`／W3。
**Gate／Evidence：** fake/real tool、1,000 files、batch/command length/no fallback/docs/package；records `10.4.*`。
**完成門檻：** default max128 subject command length；typed numeric mapping；unknown/binary unsupported；tamper/missing blocks before callback；full example gate passes。

- [deferred] 10.4.1 建立 complete package/manifest/features/locales/README/license/NOTICE並封裝 exact windows-x64 tokei/hash/source。
- [deferred] 10.4.2 實作 authorized handle batches、default 128與Windows command-line length subdivision。
- [deferred] 10.4.3 實作 JSON mapping、stable item identity、numeric values/sort/settings與unknown/binary outcomes。
- [deferred] 10.4.4 驗證 1,000 files、special filenames、cancel/reap、fake protocol errors與無 one-process-per-item。
- [deferred] 10.4.5 驗證tamper/missing+PATH decoy與`common_artifact_inventory`、provenance/modify-guide/screenshots/clean package，由integrator merge manifest後取得provisional GO。
- [deferred] 10.4.6 在10.4.1–10.4.5完成後，執行該 example 的 final-slice UITEST。

### 10.5 `lua-bulk-folder-generator` 完整 gate

**目的：** 第六個 example 用 extension button+host form提交1–100,000 create-directory plan，preview/confirm/cancel/undo全走 host。
**輸入：** 9.x form/plan/executor、10.1 Lua registrar、6.1 rules。
**產出：** complete Lua package、form/plan logic、destructive-fixture evidence。
**依賴：** 10.4 GO。
**Owner／Wave：** `example-owner`＋`fixture-owner`／W3。
**Gate／Evidence：** naming/conflict/confirmation/cancel/undo/docs/package；records `10.5.*`。
**完成門檻：** parent/prefix/start/count/padding/suffix/conflict typed；>1000 second confirm；reserved/escape/collision fail；partial cancel truthful；undo只刪仍空plan-created dirs。

- [deferred] 10.5.1 建立 complete package/manifest/features/locales/README/license/NOTICE/provenance。
- [deferred] 10.5.2 實作 extension button與host form fields/bounds/localized validation。
- [deferred] 10.5.3 實作 naming plan、zero padding/suffix/conflict policies與1–100,000 preview。
- [deferred] 10.5.4 驗證 >1,000 second confirmation、reserved/trailing dot-space/long/duplicate/escape rejection。
- [deferred] 10.5.5 驗證 cancel/partial result與user-content-preserving conservative undo。
- [deferred] 10.5.6 完成`common_artifact_inventory`、provenance/modify-guide/screenshots、clean README/validate/package，由integrator merge manifest後取得provisional GO。
- [deferred] 10.5.7 在10.5.1–10.5.6完成後，執行該 example 的 final-slice UITEST。

### 10.6 第七垂直切片 `rust-exif-rename-command`

**目的：** 在兩個Lua slices後靜態連結Rust EXIF parser，以`InputStreamV1` preview/sanitize/collision後提交undoable rename plan。
**輸入：** 4.3 stream、9.1–9.3、6.1 common rules、10.5 GO。
**產出：** complete consumer/package、metadata/token decoder、clean-machine evidence。
**依賴：** 9.1–9.3、10.5 GO；未過不得開始7z slice。
**Owner／Wave：** `example-owner`（`fixture-owner` reviewer）／W3；owned: EXIF example only；forbidden: shared manifests/product crates。
**Gate／Evidence：** exif-example exact selectors；records `10.6.*`。
**完成門檻：** required tokens正確且density≠pixels；missing tag阻擋ambiguous target；無exiftool/external EXIF DLL；`common_artifact_inventory`、provenance、screenshots、modify guide、clean package全部pass，slice GO只為provisional且W5重驗。

- [deferred] 10.6.1 建立complete consumer/manifest/features/locales/zh-TW+en README/license/NOTICE/SBOM/provenance/modify guide。
- [deferred] 10.6.2 鎖定並靜態連結Rust EXIF parser，PE import allowlist拒絕undeclared specialist DLL。
- [deferred] 10.6.3 實作`FileDecoderV1` metadata map與rawname/extension/X/YResolution/PixelX/YDimension/DateTimeOriginal tokens。
- [deferred] 10.6.4 驗證rational density與pixel dimensions為不同typed metadata，missing token產explicit blocked preview。
- [deferred] 10.6.5 實作basename sanitizer、case-insensitive collision graph與undoable rename preview。
- [deferred] 10.6.6 在empty PATH/no network/no exiftool執行valid/missing/rational/Unicode/collision/apply/undo tests。
- [deferred] 10.6.7 完成screenshots/clean README/validate/package與`common_artifact_inventory`，由integrator merge manifest後取得provisional GO。
- [deferred] 10.6.8 在10.6.1–10.6.7完成後，執行該 example 的 final-slice UITEST。

## 11. 第八垂直切片：Virtual Folder、Streams、Mutation 與 7z

### 11.1 Virtual location、entry identity 與 navigation

**目的：** virtual container成為正式 location variant，具 tab/history/address/breadcrumb/parent/session semantics。
**輸入：** navigation/session model、provider IDs、container file identity/generation。
**產出：** virtual location schema/migration、navigation adapters、fallback tests。
**依賴：** 10.6 GO；contract-owner先凍結 location/entry records。
**Owner／Wave：** `contract-owner`→`model-ui-owner`／W3。
**Gate／Evidence：** model/session/navigation tests；records `11.1.*`。
**完成門檻：** provider/container/generation/entry/components canonical；open archive root/history/parent/restore正常；missing provider safe fallback；stale entry不導航。

- [deferred] 11.1.1 凍結 virtual location、container identity/generation、stable entry ID與normalized components records。
- [deferred] 11.1.2 實作 tab open/address/breadcrumb/parent/back/forward與formal history integration。
- [deferred] 11.1.3 實作 session persistence/migration與provider unavailable/incompatible fallback。
- [deferred] 11.1.4 驗證 container move/change、provider disable、old generation entry與restore race。

### 11.2 Provider enumeration、normalization 與 bounded streams

**目的：** provider只回 normalized owned metadata與authorized generation-safe streams，materialization quota-managed/cleaned。
**輸入：** 11.1 location、4.3 stream/cancel、package authority。
**產出：** provider registration/enumeration/stream/materializer services。
**依賴：** 11.1。
**Owner／Wave：** `host-owner`／W3。
**Gate／Evidence：** path corpus/stream/materialization cleanup tests；records `11.2.*`。
**完成門檻：** absolute/device/drive/NUL/parent/normalized collision rejected；metadata不需extract；stream bounds/cancel/CRC/generation；temp cleanup all terminals。

- [deferred] 11.2.1 凍結 provider registration、rich entry metadata、stream request/result records與 capability binding。
- [deferred] 11.2.2 驗證absolute virtual entry path rejection。
- [deferred] 11.2.3 實作 stable entry/name/kind/path/sizes/CRC/time/encryption/allowed-ops enumeration。
- [deferred] 11.2.4 實作 bounded read/seek/length/CRC/cancel/generation stream與 stale handle rejection。
- [deferred] 11.2.5 實作 quota-managed physical materialization，只給需要 path的consumer，success/error/cancel/close全cleanup。
- [deferred] 11.2.6 驗證drive/device-prefixed virtual entry path rejection。
- [deferred] 11.2.7 驗證NUL virtual entry path rejection。
- [deferred] 11.2.8 驗證parent traversal virtual entry path rejection。
- [deferred] 11.2.9 驗證invalid virtual entry component rejection。
- [deferred] 11.2.10 驗證normalized virtual entry collision rejection。

### 11.3 Extract/drag-out typed plans 與 resource policy

**目的：** virtual→filesystem輸出只經 host extract plan，path/space/quota/conflict/cancel fail closed。
**輸入：** 9.x plan validator/executor、11.2 metadata/streams。
**產出：** extract steps/preview/executor adapter、bomb/escape fixtures。
**依賴：** 11.2。
**Owner／Wave：** `host-owner`＋`windows-owner`／W3。
**Gate／Evidence：** traversal/conflict/space/ratio/cancel tests；records `11.3.*`。
**完成門檻：** destination authorized；declared/observed output limits enforced；no entry escapes；cancel stops scheduling/cleans partial safely；terminal diagnostics bounded。

- [deferred] 11.3.1 凍結 extract plan/declared output/resource terminal records與 authorized destination handle。
- [deferred] 11.3.2 驗證 normalized destination、case conflict、path escape、existing target policy與space preflight。
- [deferred] 11.3.3 校準 entry/depth/per-entry/total/ratio/CPU/memory/temp limits並取得 primary approval。
- [deferred] 11.3.4 執行 traversal、compression-bomb、low-space、cancel與observed-output-exceeds-declared fixtures。

### 11.4 Transactional mutation、secret 與 whole-container undo

**目的：** 每個archive mutation用same-volume staging/verify/identity-recheck/atomic-replace；任一pre-commit failure原檔bit-identical且無resource/secret leak。
**輸入：** 9.x plans、11.2 streams、resource policy、filesystem identity APIs。
**產出：** mutation provider/protocol、per-failure hashes、undo/cleanup reports。
**依賴：** 11.2、11.3。
**Owner／Wave：** `host-owner`＋`windows-owner`／W3；destructive fixtures限定owned temp root。
**Gate／Evidence：** staged commit fault-injection matrix；records `11.4.*`。
**完成門檻：** preview→staging→flush→reopen/header/entry/CRC verify→original identity recheck→atomic replace線性；成功增generation；失敗原檔bit-identical/staging clean；password不serialize/log。

- [deferred] 11.4.1 凍結 mutation preview/steps/backup/undo/secret/resource records與non-undoable confirmation。
- [deferred] 11.4.2 建立same-volume quota-managed staging並在 rebuild/flush各注入 failure驗原檔/cleanup。
- [deferred] 11.4.3 注入staging reopen failure，驗原檔bit-identical與cleanup。
- [deferred] 11.4.4 commit前重驗original identity/size/mtime，模擬race並拒絕replace。
- [deferred] 11.4.5 驗證atomic replace成功才advance container generation並revokes old streams/locations/cache。
- [deferred] 11.4.6 實作quota-managed whole-container backup/atomic undo與over-quota explicit confirmation。
- [deferred] 11.4.7 驗證短生命secret handle在wrong password/success/cancel/error後destroy，manifest/settings/log/diagnostics零secret。
- [deferred] 11.4.8 注入header verification failure，驗原檔bit-identical與cleanup。
- [deferred] 11.4.9 注入entry inventory verification failure，驗原檔bit-identical與cleanup。
- [deferred] 11.4.10 注入CRC verification failure，驗原檔bit-identical與cleanup。

### 11.5 `rust-7z-virtual-folder` 完整 gate

**目的：** 第八個 example以locked pure-Rust backend完成browse/preview/extract/add/mkdir/delete/rename/move安全垂直切片。
**輸入：** 11.1–11.4、6.1 rules。
**產出：** complete consumer/package、archive corpus、UITEST/screenshots/docs。
**依賴：** 11.1–11.4。
**Owner／Wave：** `example-owner`＋`fixture-owner`／W3。
**Gate／Evidence：** full archive/security/mutation/navigation/docs/package matrix；records `11.5.*`。
**完成門檻：** normal/nested/empty/Unicode/solid/AES/corrupt/CRC/deep/traversal/bomb/low-space/race all terminal as specified；all common example artifacts pass。

- [deferred] 11.5.1 建立 complete consumer/manifest/features/locales/README/license/NOTICE/SBOM/provenance並鎖定pure-Rust backend。
- [deferred] 11.5.2 實作 browse/details sort/preview stream/extract/copy/drag-out。
- [deferred] 11.5.3 實作 add/mkdir/delete/rename/move preview與 mutation provider。
- [deferred] 11.5.4 執行 archive corpus unit/integration與每個pre-commit failure original-hash assertions。
- [deferred] 11.5.5 在該 example 全部 implementation、fixtures、docs與package artifacts 完成後，執行 navigation/breadcrumb/history/preview/drag/mutation/undo/password-no-log/disable final-slice UITEST。
- [deferred] 11.5.6 執行`common_artifact_inventory`、provenance/modify-guide/screenshots、clean README/validate/package，由integrator merge manifest後取得第八provisional GO。

## 12. Folder Options／Extensions 管理頁與 lifecycle composition

### 12.1 Catalog snapshot/draft 與 transaction model

**目的：** dynamic catalog使用獨立 immutable snapshot/draft，Apply/OK/Cancel/Close與catalog race可交易化。
**輸入：** package/effective-state snapshots、column/view/virtual impacts、existing folder-options dialog semantics。
**產出：** `ExtensionOptionsSnapshot`/Draft/Transaction actions/persistence。
**依賴：** phases 3、5、7、11；不得塞入 fixed Copy draft。
**Owner／Wave：** accountable `model-ui-owner`／W4；`host-owner`只提供immutable catalog/transaction adapter，不修改model/UI files。
**Gate／Evidence：** reducer/persistence/catalog-race tests；records `12.1.*`。
**完成門檻：** global/package/feature desired states、filters/unsaved changes獨立；Apply保存/啟用並留dialog；OK保存close；Cancel只丟last Apply後變更；close prompts三選。

- [deferred] 12.1.1 定義 snapshot/draft/generation/reconciliation/actions，不修改 fixed `FolderOptionsDraft` dynamic ownership。
- [deferred] 12.1.2 實作 global/package/feature desired state、search/type/status filters與unsaved-change tracking。
- [deferred] 12.1.3 實作validate/persist/activate與Apply/OK/Cancel/Close state machine。
- [deferred] 12.1.4 驗證 apply-then-edit-cancel、persist failure、activation partial failure與catalog generation race。

### 12.2 Searchable accessible Extensions tab

**目的：** virtualized catalog顯示identity/status/contacts/capabilities/tools/licenses/diagnostics/restart impact且不阻塞GPUI。
**輸入：** 12.1 model、host catalog/diagnostics、UI accessibility primitives。
**產出：** third tab、package/feature rows、filters、links與impact UI。
**依賴：** 12.1。
**Owner／Wave：** `model-ui-owner`／W4。
**Gate／Evidence：** long-list/keyboard/UIA/DPI/contrast/localization/link-safety UITEST；records `12.2.*`。
**完成門檻：** all effective states/reasons visible；child desired preserved；contacts只display且永不auto-open/join/message；long catalog virtualized；high contrast不只靠color。

- [deferred] 12.2.1 新增 keyboard/UIA accessible `Extensions` tab與global switch。
- [deferred] 12.2.2 實作 search、type/status filters與virtualized expandable package/feature rows。
- [deferred] 12.2.3 顯示publisher contacts、source/signature、content/capability/tool/license/fingerprint/diagnostic/restart data。
- [deferred] 12.2.4 驗證contact links render但validation/selection絕不auto-open、join或message。
- [deferred] 12.2.5 在long-catalog example全部 artifacts完成後，執行long catalog、keyboard/UIA、DPI、high contrast、localization與non-blocking final-slice UITEST。

### 12.3 Impact preview、drain linearization 與 type-specific switching

**目的：** disable transaction依固定順序close dispatch→cancel resources→GPUI detach→virtual redirect→drain→terminal，Rust不卸載。
**輸入：** 3.5 lifecycle、jobs/processes、column/view/panel/virtual contributions、12.1 transaction。
**產出：** `FeatureDrainCoordinator`、impact preview、race matrix。
**依賴：** 12.1、12.2；drain thresholds需primary approval。
**Owner／Wave：** accountable `host-owner`／W4；`model-ui-owner`只交付GPUI detach/virtual redirect adapter，primary整合shared composition。
**Gate／Evidence：** concurrent/nested callback/toggle/virtual-tab tests；records `12.3.*`。
**完成門檻：** linearizable sequence；late outputs/invalidations rejected；Lua immediate re-register、Skin fallback、loaded Rust gate/drain、unloaded Rust pending-restart；virtual navigation decline aborts transaction。

- [deferred] 12.3.1 核准 dispatch/drain deadlines與定義原子transition table、active call lease/correlation journal。
- [deferred] 12.3.2 實作 close dispatch gate後才cancel jobs/streams/processes並在GPUI thread detach UI contributions。
- [deferred] 12.3.3 實作virtual tabs impact preview/redirect；使用者取消navigation時整個disable不commit。
- [deferred] 12.3.4 實作active leases bounded drain與timeout/fault→pending-restart，永不force unload。
- [deferred] 12.3.5 驗證callback-start/disable、nested/concurrent calls、late sink/invalidation、rapid toggle/update races。
- [deferred] 12.3.6 驗證Lua immediate stop/re-register switching semantics與parent desired restore。
- [deferred] 12.3.7 驗證Skin active/default fallback switching semantics與parent desired restore。
- [deferred] 12.3.8 驗證loaded Rust gate/drain/no-unload switching semantics與parent desired restore。
- [deferred] 12.3.9 驗證unloaded Rust pending-restart switching semantics與parent desired restore。
- [deferred] 12.3.10 驗證virtual provider impact/redirect switching semantics與parent desired restore。

### 12.4 P0 pre-Skin acceptance barrier

**目的：** 依design先完成全部P0 host＋八examples＋options的offline/security/performance/UITEST/docs gates，才允許開始P1 Skin。
**輸入：** phases 2–12、6.4.6、7.4.6、8.1.6、8.3.5、10.4.5、10.5.6、10.6.7、11.5.6 provisional gates與其immutable provenance artifacts。
**產出：** P0 candidate report、per-example common-artifact/provenance revalidation、Skin-start GO/NO-GO。
**依賴：** 12.1–12.3、6.4.6、7.4.6、8.1.6、8.3.5、10.4.5、10.5.6、10.6.7、11.5.6；任何failed/stale/missing/skip阻擋phase13。
**Owner／Wave：** `release-integrator`（architecture reviewer independent）／W4；owned: candidate/evidence composition；forbidden: weakening thresholds。
**Gate／Evidence：** P0-pre-skin exact matrix selectors；records `12.4.*`。
**完成門檻：** host/SDK/八packages offline build；ABI/capability/Safe Mode/security；1k/100k performance；八unit/integration/UITEST/docs/screenshots/common inventory/provenance全pass；P0 candidate零未解P0/P1 review findings，但尚未開始的phase-13 planned Skin leaves明確排除於此判定。

- [deferred] 12.4.1 在empty Cargo home/network-denied環境建host。
- [deferred] 12.4.2 重跑folder-size `common_artifact_inventory`驗證。
- [deferred] 12.4.3 執行P0 ABI compatibility gate。
- [deferred] 12.4.4 執行1,000-item list-readiness performance threshold gate。
- [deferred] 12.4.5 實際執行folder-size mandatory UITEST。
- [deferred] 12.4.6 由independent architecture reviewer確認P0 candidate零未解P0/P1 findings（不含未開始phase13）並簽發Skin-start GO。
- [deferred] 12.4.7 在另一empty Cargo home/network-denied環境建SDK host fixture。
- [deferred] 12.4.8 重跑Size Map `common_artifact_inventory`與immutable provenance/package hash驗證。
- [deferred] 12.4.9 重跑Rust tokei `common_artifact_inventory`與immutable provenance/package hash驗證。
- [deferred] 12.4.10 重跑Lock Owner `common_artifact_inventory`與immutable provenance/package hash驗證。
- [deferred] 12.4.11 重跑Lua tokei `common_artifact_inventory`與immutable provenance/package hash驗證。
- [deferred] 12.4.12 重跑bulk-folder `common_artifact_inventory`與immutable provenance/package hash驗證。
- [deferred] 12.4.13 重跑EXIF `common_artifact_inventory`與immutable provenance/package hash驗證。
- [deferred] 12.4.14 重跑7z `common_artifact_inventory`與immutable provenance/package hash驗證。
- [deferred] 12.4.15 執行P0 package atomicity security gate。
- [deferred] 12.4.16 執行P0 Safe Mode recovery security gate。
- [deferred] 12.4.17 執行P0 process-containment security gate。
- [deferred] 12.4.18 執行100,000-node核准memory threshold gate。
- [deferred] 12.4.19 實際執行Size Map mandatory UITEST。
- [deferred] 12.4.20 實際執行Rust tokei mandatory UITEST。
- [deferred] 12.4.21 實際執行Lock Owner mandatory UITEST。
- [deferred] 12.4.22 實際執行Lua tokei mandatory UITEST。
- [deferred] 12.4.23 實際執行bulk-folder mandatory UITEST。
- [deferred] 12.4.24 實際執行EXIF mandatory UITEST。
- [deferred] 12.4.25 實際執行7z mandatory UITEST。
- [deferred] 12.4.26 實際執行Options mandatory UITEST，不以mapping或provisional GO取代。
- [deferred] 12.4.27 驗證folder-size immutable provenance/package hash。
- [deferred] 12.4.28 在另一empty Cargo home/network-denied環境建SDK plugin fixture。
- [deferred] 12.4.29 執行P0 ABI boundary security gate。
- [deferred] 12.4.30 執行1,000-item cancel threshold gate。
- [deferred] 12.4.31 執行1,000-item redraw/invalidation threshold gate。
- [deferred] 12.4.32 執行P0 capability authority security gate。
- [deferred] 12.4.33 執行P0 drain linearization security gate。
- [deferred] 12.4.34 執行P0 late-publish rejection security gate。
- [deferred] 12.4.35 執行P0 operation authorization security gate。
- [deferred] 12.4.36 執行P0 archive mutation security gate。
- [deferred] 12.4.37 執行100,000-node layout/redraw threshold gate。
- [deferred] 12.4.38 執行100,000-node cancellation threshold gate。

## 13. P1 純資料 Skin

### 13.1 Schema、asset validator 與 executable rejection

**目的：** 凍結versioned data-only schema，逐asset驗path/type/dimension/size/reference/budget，拒絕code/shader。
**輸入：** package manifest、skin spec、UI asset capabilities。
**產出：** schema/validator/corpus/author limits。
**依賴：** 12.4 P0 pre-Skin GO；contract-owner first。
**Owner／Wave：** `contract-owner`→`host-owner`／W4。
**Gate／Evidence：** schema/security/asset corpus；records `13.1.*`。
**完成門檻：** images/icons/fonts/button states/nine-slice/vector/colors/spacing/transparency/acrylic/hit masks bounded；Rust/Lua/JS/native shader rejected before apply；each invalid asset identifiable。

- [deferred] 13.1.1 凍結skin schema/version與所有asset/style/state references、defaults/non-exhaustive policy。
- [deferred] 13.1.2 拒絕任何Rust/Lua/JS/native executable/shader content或undeclared content type。
- [deferred] 13.1.3 驗證path containment、decode type、dimensions、file/decoded size、reference cycles與global/per-asset budgets。
- [deferred] 13.1.4 建立missing/corrupt/oversized/wrong-type/path-escape/reference-cycle fixtures。

### 13.2 Host-owned rendering、hit testing 與 fallback

**目的：** 視覺可客製但command/focus/UIA/window geometry/resize/titlebar永遠由host擁有；invalid asset局部fallback。
**輸入：** 13.1 validated data、window/chrome semantics、default skin。
**產出：** skin projection、button states、transparent mask protection、runtime switching。
**依賴：** 13.1。
**Owner／Wave：** `model-ui-owner`／W4；owned: skin UI adapter；forbidden: changing OS window to irregular geometry。
**Gate／Evidence：** rendering/hit/fallback unit+headful evidence；records `13.2.*`。
**完成門檻：** normal/hover/pressed/focused/disabled distinct；OS window rectangular；pass-through不能覆蓋required resize/command；single corrupt asset只fallback itself；disable active skin立即default。

- [deferred] 13.2.1 實作button visual states且retain host command/keyboard focus/UIA role。
- [deferred] 13.2.2 實作rectangular OS window上的transparent visual outline與validated pass-through mask。
- [deferred] 13.2.3 保護title drag、window commands、resize edges、shortcuts、UIA與high-contrast overrides。
- [deferred] 13.2.4 實作per-asset/state default fallback，不因單一decode/budget failure停用整skin。
- [deferred] 13.2.5 實作enable=selectable但不auto-activate、disable active=immediate default與setting persistence。

### 13.3 Skin quality matrix 與作者文件

**目的：** 每個平台/accessibility/fallback gate獨立結案，不用一個 broad “suite passed” 掩蓋子失敗。
**輸入：** 13.1–13.2、supported Windows/DPI/monitor fixtures。
**產出：** per-subcheck screenshots/reports、author schema/limits/hit-test docs。
**依賴：** 13.1、13.2。
**Owner／Wave：** `fixture-owner`＋`docs-owner`／W4。
**Gate／Evidence：** distinct UITEST subchecks `13.3.*`。
**完成門檻：** DPI、contrast、Snap、maximize、resize、multi-monitor、keyboard、UIA、mask、corrupt、oversized各有passed record；docs reproduction成功。

- [deferred] 13.3.1 執行100% DPI rendering/hit/accessibility evidence。
- [deferred] 13.3.2 執行high contrast required-control distinguishability evidence。
- [deferred] 13.3.3 執行Windows Snap geometry/interaction evidence。
- [deferred] 13.3.4 執行transparent pass-through/required hit-area protection與pointer interaction evidence。
- [deferred] 13.3.5 執行missing asset per-item fallback evidence。
- [deferred] 13.3.6 完成schema/asset limits/button states/transparent hit-test/fallback雙語文件與clean reproduction。
- [deferred] 13.3.7 執行125% DPI rendering/hit/accessibility evidence。
- [deferred] 13.3.8 執行150% DPI rendering/hit/accessibility evidence。
- [deferred] 13.3.9 執行175% DPI rendering/hit/accessibility evidence。
- [deferred] 13.3.10 執行200% DPI rendering/hit/accessibility evidence。
- [deferred] 13.3.11 執行mixed-DPI monitor move與rescale evidence。
- [deferred] 13.3.12 執行keyboard focus/shortcuts preservation evidence。
- [deferred] 13.3.13 執行UIA roles/names/states/actions evidence。
- [deferred] 13.3.14 執行maximize/restore geometry evidence。
- [deferred] 13.3.15 執行all resize edges/corners hit-test evidence。
- [deferred] 13.3.16 執行multi-monitor work-area/negative-coordinate evidence。
- [deferred] 13.3.17 執行corrupt asset per-item fallback evidence。
- [deferred] 13.3.18 執行oversized asset budget fallback evidence。
- [deferred] 13.3.19 執行wrong-type asset fallback evidence。

## 14. 八個範例共同 SDK、AI fixture、文件與 traceability

### 14.1 Dependency provenance 與 package reproducibility

**目的：** 每個dependency分類static Rust library或bundled executable，SBOM/NOTICE/hash/license完整且無runtime download。
**輸入：** eight example Cargo metadata、tool payload manifests、package artifacts。
**產出：** per-example SBOM/provenance/license report、determinism hashes。
**依賴：** 各example project建立後增量執行。
**Owner／Wave：** `sdk-tooling-owner`（`docs-owner` reviewer）／W2（每slice增量、W4 final重驗）。
**Gate／Evidence：** provenance validator/package two-run comparison；records `14.1.*`。
**完成門檻：** every transitive dep/payload represented；static parser linked/import-audited；executable target/hash/license validated；two package runs deterministic。

- [deferred] 14.1.1 對八個examples生成transitive Cargo SBOM與static-library classification。
- [deferred] 14.1.2 對Lua tokei等executables生成target/version/size/hash/source/license/NOTICE classification。
- [deferred] 14.1.3 執行PE import audit，拒絕static parser以undeclared specialist DLL masquerade。
- [deferred] 14.1.4 掃描runtime code/docs，拒絕download、PATH fallback與unversioned dependency instruction。
- [deferred] 14.1.5 各example連續兩次clean package比較entry inventory、bytes/hash與manifest binding。

### 14.2 Rust-only AI prompt fixture

**目的：** prompt讀machine-readable bundle/manifest/examples並生成可build/validate/package的minimal provider+GPUI renderer，不追main或私有crate。
**輸入：** current sdk-lock/schema/scripts/examples。
**產出：** `AI_RUST_PLUGIN_PROMPT.md`、generated fixture、negative prompts。
**依賴：** 6.1、5.6、2.5。
**Owner／Wave：** `docs-owner`＋`fixture-owner`／W4。
**Gate／Evidence：** prompt fixture offline build/package and rejection cases；records `14.2.*`。
**完成門檻：** current bundle ID/rev、official scripts、manifest/tests/locales使用正確；no fake contacts/private crates/main/unlocked deps；fixture complete offline。

- [deferred] 14.2.1 撰寫prompt要求讀sdk-lock/manifest schema/official examples/scripts並使用current bundle ID/rev。
- [deferred] 14.2.2 明確禁止private crates、branch HEAD、unlocked deps、虛構publisher contacts與缺tests/locales。
- [deferred] 14.2.3 生成minimal provider+GPUI renderer fixture並執行isolated build/test/validate/package。
- [deferred] 14.2.4 以GPUI main/private crate/unlocked dependency/invalid contact generated variants證明validator拒絕並給修正診斷。

### 14.3 Public docs 與 requirement/interface matrix

**目的：** 所有public interfaces/spec scenarios/examples/local validation/README雙向可追，作者文件clean reproduction。
**輸入：** 1.2 matrix、public rustdoc、eight examples、diagnostic/security docs。
**產出：** zh-TW/en guides、migration/restart/Safe Mode/security/support docs、final traceability matrix。
**依賴：** phases 3–14 relevant APIs/examples。
**Owner／Wave：** `docs-owner`＋`release-integrator`／W4。
**Gate／Evidence：** docs link/command/reproduction/interface coverage；records `14.3.*`。
**完成門檻：** every requirement/scenario有task/test/doc；every public interface有production+example use；八README各clean-run；無未決佔位文字或過度安全宣稱。

- [deferred] 14.3.1 完成public API rustdoc與zh-TW/en author lifecycle/ABI/jobs/columns/views/commands/Lua/virtual/Skin guides。
- [deferred] 14.3.2 完成migration/restart/Safe Mode/no-sandbox/security limitations/diagnostics/support contact文件。
- [deferred] 14.3.3 在clean consumer執行folder-size README全部commands並保存actual/hash。
- [deferred] 14.3.4 驗證十一capabilities每個Requirement/Scenario映射unit/integration/UITEST或明確non-test evidence。
- [deferred] 14.3.5 驗證每public interface映射production composition path與至少一official example，unwired阻擋release。
- [deferred] 14.3.6 掃描全部artifacts的未決佔位文字、矛盾、broken links與unsupported production claims。
- [deferred] 14.3.7 在clean consumer執行Size Map README全部commands並保存actual/hash。
- [deferred] 14.3.8 在clean consumer執行Rust tokei README全部commands並保存actual/hash。
- [deferred] 14.3.9 在clean consumer執行Lock Owner README全部commands並保存actual/hash。
- [deferred] 14.3.10 在clean consumer執行Lua tokei README全部commands並保存actual/hash。
- [deferred] 14.3.11 在clean consumer執行bulk-folder README全部commands並保存actual/hash。
- [deferred] 14.3.12 在clean consumer執行EXIF README全部commands並保存actual/hash。
- [deferred] 14.3.13 在clean consumer執行7z README全部commands並保存actual/hash。

## 15. System integration、安全、效能、UITEST 與候選證據

### 15.1 完整 offline build/package matrix

**目的：** 同一approved bundle在隔離空Cargo homes、禁止network/global cache下建host、SDK fixtures、八examples並package。
**輸入：** complete bundle/host/fixtures/examples。
**產出：** per-consumer logs/hashes、artifact inventory、two-run reproducibility report。
**依賴：** phases 2–14；任何 failure阻擋candidate。
**Owner／Wave：** `release-integrator`＋`fixture-owner`／W5。
**Gate／Evidence：** distinct subchecks `15.1.*`。
**完成門檻：** host、fixture與每個example獨立passed；no network/cache/PATH fallback；two runs reproduce locks/artifacts；無mandatory skip。

- [deferred] 15.1.1 在network-denied/empty `CARGO_HOME`建置host與SDK host/plugin fixtures。
- [deferred] 15.1.2 建置/測試/validate/package folder-size example並保存unique subcheck。
- [deferred] 15.1.3 建置/測試/validate/package Size Map example並保存unique subcheck。
- [deferred] 15.1.4 建置/測試/validate/package Rust tokei example並保存unique subcheck。
- [deferred] 15.1.5 建置/測試/validate/package Lock Owner example並保存unique subcheck。
- [deferred] 15.1.6 建置/測試/validate/package Lua tokei example並保存unique subcheck。
- [deferred] 15.1.7 建置/測試/validate/package bulk-folder example並保存unique subcheck。
- [deferred] 15.1.8 建置/測試/validate/package EXIF example並保存unique subcheck。
- [deferred] 15.1.9 建置/測試/validate/package 7z example並保存unique subcheck。
- [deferred] 15.1.10 重複完整matrix並比較bundle ID、locks、package inventory與artifact hashes。
- [deferred] 15.1.11 驗證所有commands含`--locked --offline`且無global Cargo/git/PATH tool依賴。

### 15.2 Security、ABI、lifecycle 與 recovery matrix

**目的：** 每個trust/ABI/capability/TOCTOU/panic/drain/process/archive/secret gate獨立執行並保存actual。
**輸入：** adversarial fixture corpus、host/package/runtime。
**產出：** security/compatibility report與per-gate evidence。
**依賴：** 15.1 buildable candidate。
**Owner／Wave：** `architecture_reviewer`＋`fixture-owner`／W5；read-only reviewer independent。
**Gate／Evidence：** records `15.2.*`，任何P0/P1 unresolved阻擋。
**完成門檻：** pre-load/load/root/callback markers、forbidden ABI types、atomic package, stale handles, concurrent drain, shell/process, plan/archive, secret全pass；no false sandbox claim。

- [deferred] 15.2.1 執行package path/containment/reparse atomic rejection gate。
- [deferred] 15.2.2 執行load-attempt crash與next-start Safe Mode gate。
- [deferred] 15.2.3 執行concurrent/nested call journal correlation gate。
- [deferred] 15.2.4 執行stale package-incarnation authority handle gate。
- [deferred] 15.2.5 執行shell injection literal-argument gate。
- [deferred] 15.2.6 執行archive traversal normalization gate。
- [deferred] 15.2.7 執行package content-hash atomic rejection gate。
- [deferred] 15.2.8 執行package target atomic rejection gate。
- [deferred] 15.2.9 執行legacy raw root pre-accessor rejection gate。
- [deferred] 15.2.10 執行root layout compatibility gate。
- [deferred] 15.2.11 執行factory no-unwind gate。
- [deferred] 15.2.12 執行recoverable panic typed-translation gate。
- [deferred] 15.2.13 執行disable/drain race gate。
- [deferred] 15.2.14 執行stale location generation gate。
- [deferred] 15.2.15 執行stale container generation gate。
- [deferred] 15.2.16 執行validate-use TOCTOU identity gate。
- [deferred] 15.2.17 執行PATH decoy/no-fallback gate。
- [deferred] 15.2.18 執行Job Object assign-before-resume gate。
- [deferred] 15.2.19 執行operation-plan authorization gate。
- [deferred] 15.2.20 執行archive bomb/resource-limit gate。
- [deferred] 15.2.21 執行archive pre-commit failure original-bit-preservation gate。
- [deferred] 15.2.22 執行secret no-log/destroy gate。
- [deferred] 15.2.23 執行registrar no-unwind gate。
- [deferred] 15.2.24 執行provider no-unwind gate。
- [deferred] 15.2.25 執行renderer no-unwind gate。
- [deferred] 15.2.26 執行host service callback no-unwind gate。
- [deferred] 15.2.27 執行destructor no-unwind gate。
- [deferred] 15.2.28 執行Safe Mode successful scoped re-enable gate。
- [deferred] 15.2.29 執行Safe Mode repeated-crash re-suppression gate。
- [deferred] 15.2.30 執行restart-state transition race gate。
- [deferred] 15.2.31 執行stale item generation gate。
- [deferred] 15.2.32 執行stale refresh generation gate。
- [deferred] 15.2.33 執行stale job generation gate。
- [deferred] 15.2.34 執行stale tool generation gate。
- [deferred] 15.2.35 執行stale stream generation gate。
- [deferred] 15.2.36 執行preview-execute TOCTOU identity gate。
- [deferred] 15.2.37 執行verify-commit TOCTOU identity gate。
- [deferred] 15.2.38 執行Job Object all-terminal process-tree cleanup gate。
- [deferred] 15.2.39 執行large-plan second-confirmation gate。
- [deferred] 15.2.40 執行conservative undo user-content preservation gate。
- [deferred] 15.2.41 執行archive staging cleanup gate。
- [deferred] 15.2.42 執行whole-container undo gate。
- [deferred] 15.2.43 執行package signature atomic rejection gate。
- [deferred] 15.2.44 執行signed publisher identity mismatch rejection gate。
- [deferred] 15.2.45 執行package dependency atomic rejection gate。
- [deferred] 15.2.46 執行package capability atomic rejection gate。
- [deferred] 15.2.47 執行required numeric-semantics compatibility gate。
- [deferred] 15.2.48 執行GPUI fingerprint compatibility gate。
- [deferred] 15.2.49 執行late-publish rejection gate。

### 15.3 Approved performance/hardware gates

**目的：** 以固定hardware profile/raw samples/approved thresholds判定1k columns、100k Size Map、long catalog、process/virtual limits；不得臨時降gate。
**輸入：** calibration decisions、release candidate、deterministic scale fixtures。
**產出：** raw metrics、percentiles/memory/redraw/latency report、threshold decisions。
**依賴：** 15.1；threshold change屬C類。
**Owner／Wave：** `fixture-owner`＋`architecture_reviewer`／W5。
**Gate／Evidence：** records `15.3.*`。
**完成門檻：** hardware/OS/build profile完整；basic list readiness、visible priority、cancel、queue/memory/redraw/layout/catalog responsiveness各獨立pass；machine-speed variance不隱藏algorithmic regression。

- [deferred] 15.3.1 記錄hardware/OS/power/build/profile與核准threshold/version/adjustment lineage。
- [deferred] 15.3.2 執行1,000-item basic-list/visible-priority/cancel/queue/cache/redraw raw measurements。
- [deferred] 15.3.3 執行100,000-node memory/delta/layout/redraw/cancel/navigation raw measurements。
- [deferred] 15.3.4 執行long Extensions catalog virtualization/filter/keyboard responsiveness measurements。
- [deferred] 15.3.5 執行tool process/archive resource/cleanup bounds並驗無unbounded growth/leak。
- [deferred] 15.3.6 對每個threshold輸出actual/expected/pass，任何降threshold要求C approval與dependent evidence stale。

### 15.4 Full UITEST、accessibility 與 docs reproduction

**目的：** mapping與execution分離；每個mandatory case產JSON/JUnit/Markdown/artifacts，八README與Skin矩陣實際重現。
**輸入：** UITEST manifest、candidate binaries/packages、fixtures/docs。
**產出：** coverage report、headful evidence/screenshots、docs reproduction report。
**依賴：** 15.1–15.3。
**Owner／Wave：** `uitest-owner`＋`docs-owner`／W5。
**Gate／Evidence：** records `15.4.*`。
**完成門檻：** requirement selectors zero-uncovered for this change或有approved non-UITest evidence；all mandatory cases executed/no unjustified skip；keyboard/UIA/DPI/contrast/localization/screenshots/docs pass。

- [deferred] 15.4.1 驗證manifest schema、case IDs、selectors、required artifacts與本change coverage零unknown/zero-hit。
- [deferred] 15.4.2 實際執行dynamic columns lifecycle/selection UITEST case。
- [deferred] 15.4.3 實際執行commands/forms/operation-plan UITEST case。
- [deferred] 15.4.4 實際執行Skin DPI UITEST subcheck。
- [deferred] 15.4.5 重現folder-size README commands，不以mapping代替execution。
- [deferred] 15.4.6 驗證folder-size required screenshots/content hashes收集完整。
- [deferred] 15.4.7 實際執行jobs priority/cancel/batching UITEST case。
- [deferred] 15.4.8 實際執行views selection/navigation/F5/fallback UITEST case。
- [deferred] 15.4.9 實際執行Options lifecycle/transaction UITEST case。
- [deferred] 15.4.10 實際執行Lua registrar/capability UITEST case。
- [deferred] 15.4.11 實際執行bundled tools/process cleanup UITEST case。
- [deferred] 15.4.12 實際執行virtual navigation/preview/drag UITEST case。
- [deferred] 15.4.13 實際執行virtual mutation/original-preservation UITEST case。
- [deferred] 15.4.14 實際執行operation/archive undo UITEST case。
- [deferred] 15.4.15 實際執行package/ABI/security denial UITEST case。
- [deferred] 15.4.16 實際執行Safe Mode/recovery UITEST case。
- [deferred] 15.4.17 實際執行Skin accessibility UITEST subcheck。
- [deferred] 15.4.18 實際執行Skin window-behavior UITEST subcheck。
- [deferred] 15.4.19 實際執行Skin hit-test protection UITEST subcheck。
- [deferred] 15.4.20 實際執行Skin per-asset fallback UITEST subcheck。
- [deferred] 15.4.21 重現Size Map README commands。
- [deferred] 15.4.22 重現Rust tokei README commands。
- [deferred] 15.4.23 重現Lock Owner README commands。
- [deferred] 15.4.24 重現Lua tokei README commands。
- [deferred] 15.4.25 重現bulk-folder README commands。
- [deferred] 15.4.26 重現EXIF README commands。
- [deferred] 15.4.27 重現7z README commands。
- [deferred] 15.4.28 重現AI prompt fixture commands。
- [deferred] 15.4.29 驗證Size Map required screenshots/content hashes收集完整。
- [deferred] 15.4.30 驗證Rust tokei required screenshots/content hashes收集完整。
- [deferred] 15.4.31 驗證Lock Owner required screenshots/content hashes收集完整。
- [deferred] 15.4.32 驗證Lua tokei required screenshots/content hashes收集完整。
- [deferred] 15.4.33 驗證bulk-folder required screenshots/content hashes收集完整。
- [deferred] 15.4.34 驗證EXIF required screenshots/content hashes收集完整。
- [deferred] 15.4.35 驗證7z required screenshots/content hashes收集完整。
- [deferred] 15.4.36 驗證all JSON/JUnit/Markdown/reports/artifacts retention完整。

## 16. RC freeze、原子 promotion 與最終完成

### 16.1 Candidate composition 與 independent final review

**目的：** 將所有passed evidence組成immutable candidate；failed/stale/skip/P0/P1不容許promotion。
**輸入：** 1.x ledger、15.x reports、canonical bundle outputs、traceability matrix。
**產出：** candidate manifest/compatibility report/rollback plan、review findings。
**依賴：** 15.1–15.4。
**Owner／Wave：** `release-integrator`＋independent `architecture_reviewer`／W5。
**Gate／Evidence：** evidence validator/hash verification/OpenSpec/task validator/review；records `16.1.*`。
**完成門檻：** every leaf terminal evidence valid；zero mandatory failure/stale/skip/unresolved P0/P1；candidate不修改active snapshot；rollback complete。

- [deferred] 16.1.1 驗證evidence index每個completed L3唯一、hash存在、actual/expected一致且無stale dependency。
- [deferred] 16.1.2 產生requirement→task→test/doc/artifact compatibility report與candidate manifest。
- [deferred] 16.1.3 驗證canonical lock/vendor/fingerprint/package hashes/signature inputs與rollback inventory。
- [deferred] 16.1.4 執行`openspec validate build-extensible-plugin-platform --strict`與detailed-task validator。
- [deferred] 16.1.5 由independent architecture reviewer審ABI/security/concurrency/lifecycle/tests/release，修完所有P0/P1再GO。

### 16.2 RC selection、protected tag、signing 與 offline freeze

**目的：** 只選完整passing development candidate，經primary authority記錄protected tag/signature，設freeze後offline重建；外部write不委派。
**輸入：** 16.1 GO candidate、approval/non-FF proof（若需要）、release credentials held by primary。
**產出：** protected-tag metadata、`release_frozen=true` release bundle、signed artifacts、offline rebuild evidence。
**依賴：** 16.1；external tag/sign/publish需要實際authority。
**Owner／Wave：** primary `release-integrator`／W5；subagents forbidden: tag/push/sign/publish/credentials。
**Gate／Evidence：** freeze/update/promotion/offline guest contracts；records `16.2.*`。
**完成門檻：** non-FF有explicit C approval；tag/sign actions可稽核；freeze inputs immutable；host/fixtures/eight examples offline rebuild；rev change強制new RC/bundle。

- [deferred] 16.2.1 確認candidate為latest fully-passing snapshot且active snapshot在approval前未變。
- [deferred] 16.2.2 若remote non-fast-forward，取得explicit C approval與可驗proof；否則記錄not-applicable evidence。
- [deferred] 16.2.3 由primary建立/記錄protected tag metadata，保存exact target與result。
- [deferred] 16.2.4 設`release_frozen=true`並驗證canonical freeze metadata。
- [deferred] 16.2.5 在remote main advance fixture下offline rebuild host。
- [deferred] 16.2.6 模擬post-freeze rev change，確認拒絕覆寫舊release並要求new RC/bundle全gate重跑。
- [deferred] 16.2.7 由primary執行signing action並保存exact inputs/outputs/result。
- [deferred] 16.2.8 驗證signed manifest綁定frozen bundle ID、tag、locks、vendor與artifact hashes。
- [deferred] 16.2.9 在remote main unavailable fixture下offline rebuild SDK host fixture。
- [deferred] 16.2.10 在remote main unavailable fixture下offline rebuild folder-size example。
- [deferred] 16.2.11 在remote main unavailable fixture下offline rebuild Size Map example。
- [deferred] 16.2.12 在remote main unavailable fixture下offline rebuild Rust tokei example。
- [deferred] 16.2.13 在remote main unavailable fixture下offline rebuild Lock Owner example。
- [deferred] 16.2.14 在remote main unavailable fixture下offline rebuild Lua tokei example。
- [deferred] 16.2.15 在remote main unavailable fixture下offline rebuild bulk-folder example。
- [deferred] 16.2.16 在remote main unavailable fixture下offline rebuild EXIF example。
- [deferred] 16.2.17 在remote main unavailable fixture下offline rebuild 7z example。
- [deferred] 16.2.18 生成new immutable release/RC bundle ID並驗證未覆寫舊bundle。
- [deferred] 16.2.19 在remote main unreachable fixture下offline rebuild host。
- [deferred] 16.2.20 在remote main unavailable fixture下offline rebuild SDK plugin fixture。

### 16.3 Publication readiness、non-goals 與 apply completion

**目的：** 只有全部契約與P1 Skin完成、無Steamworks dependency、artifacts可下載/回滾時才標記change apply完成。
**輸入：** 16.2 frozen bundle、docs/support/migration/security reports、workspace dependency graph。
**產出：** final acceptance report、artifact index、known limitations/non-goals、archive-readiness decision。
**依賴：** 16.2。
**Owner／Wave：** primary `release-integrator`／W5。
**Gate／Evidence：** final graph/docs/artifact/OpenSpec status checks；records `16.3.*`。
**完成門檻：** no Steamworks linkage；Package Source/Entitlement abstract only；all public artifacts/hashes/docs/support present；OpenSpec applyRequires done；goal僅在真正完成後complete。

- [deferred] 16.3.1 掃描workspace/PE/license graph，確認core無Steamworks且Steam/Pro只保留未wired abstractions/non-goals。
- [deferred] 16.3.2 驗證final SDK/eight examples/packages/SBOM/docs/reports的download inventory與SHA-256。
- [deferred] 16.3.3 驗證migration/restart/Safe Mode/no-sandbox/security/support/rollback limitations與實作一致。
- [deferred] 16.3.4 執行final `cargo fmt --all -- --check`並索引evidence。
- [deferred] 16.3.5 執行final workspace `cargo check --workspace --all-targets --locked --offline`並索引evidence。
- [deferred] 16.3.6 執行final workspace clippy核准command與warning policy並索引evidence。
- [deferred] 16.3.7 執行final workspace tests核准command並索引evidence。
- [deferred] 16.3.8 執行all SDK/host contract scripts並以unique subcheck索引evidence。
- [deferred] 16.3.9 執行full mandatory UITEST suite並索引JSON/JUnit/Markdown/artifacts。
- [deferred] 16.3.10 執行OpenSpec strict/status/detailed-task/traceability validators並索引evidence。
- [deferred] 16.3.11 確認所有L3已passed、唯一預核N/A或有replacement的superseded，沒有failed/blocked/stale/unexecuted。
- [deferred] 16.3.12 產生final handoff與archive-readiness review；只有此leaf完成後才可宣稱change apply完成。
