## 1. P0-0 Rust 與 GPUI Snapshot 基線

- [x] 1.1 將 workspace、host 與 SDK consumer 的 Rust 基線統一為 `1.97.1`／`x86_64-pc-windows-msvc`，提交精確 `rust-toolchain.toml` 並測試 rustc/Cargo commit 驗證
- [x] 1.2 將 `abi_stable = 0.11.3` 加入 protected dependency closure，以空 top-level features 建立最小 root-module/layout/panic-boundary fixture
- [x] 1.3 將 GPUI source authority 統一為 `https://github.com/damody/gpui-ce-explorer.git`，解析核准 development commit 並讓 host 與 SDK 使用同一 rev
- [x] 1.4 實作 `sdk-lock.json`／`bundle-manifest.json` canonical generator，涵蓋 toolchain、GPUI rev/tree、ABI、features、profile、panic、allocator/CRT、rustflags 與檔案 SHA-256
- [x] 1.5 建立 SDK canonical `Cargo.lock`、`.cargo/config.toml`、protected dependency graph validator 與 offline `vendor/cargo-sources`
- [x] 1.6 實作 UI ABI fingerprint generator／loader comparison，加入每個單因子改變都產生不相容的單元測試
- [ ] 1.7 建立隔離的 host-fixture 與 plugin-fixture，於空 `CARGO_HOME`、禁止網路下以 `--locked --offline` 分開建置並驗證載入
- [ ] 1.8 實作 `update-gpui-snapshot.ps1` 與 CI job：解析完整 HEAD、拒絕未核准 non-fast-forward、產生新 bundle ID、失敗回退上一 snapshot
- [ ] 1.9 實作 Release freeze 流程與 fixture：protected tag metadata、`release_frozen = true`、遠端 main 移動後仍離線重建、換 rev 強制新 RC/bundle ID
- [ ] 1.10 建立 `build-plugin.ps1`、`validate-plugin.ps1`、`package-plugin.ps1` 與 P0-0 診斷文件，將 UITEST／CI requirements 映射到 manifest

## 2. Extension API、Host 與套件格式

- [x] 2.1 新增 `explorer-extension-api`、`explorer-extension-ui-api`、`explorer-extension-host` crates 與 composition-root wiring，建立私有 crate dependency 禁止規則
- [x] 2.2 定義 FFI-safe stable IDs、typed errors/outcomes、root module 與 prefix registrar，加入 SDK 1.x optional-tail/non-exhaustive 相容 fixture
- [x] 2.3 實作版本化 `.sepack`／`PackageManifestV1` parser，涵蓋 Rust/Lua/Skin/locale/tools/features/capabilities/dependencies/hash/signature/data-version
- [x] 2.4 實作 publisher/contact schema validation，涵蓋 email、網站、論壇、GitHub Issues、Discord、QQ、support/security purpose 與簽署 publisher mismatch
- [x] 2.5 實作 package content/path/hash/signature/target 驗證，拒絕絕對路徑、`..`、symlink/junction/reparse-point 逃逸
- [ ] 2.6 實作 built-in 與 local-developer Package Source adapters，以及不連結 Steamworks 的 Package Source／Entitlement Provider 抽象測試
- [ ] 2.7 實作 Package Resolver 的單版本選擇、dependency graph、cycle detection、atomic whole-package rejection 與診斷
- [ ] 2.8 撰寫 package lifecycle API／manifest schema 雙語文件與範例 manifest，加入 unit/integration/UITEST manifest mapping

## 3. Feature 狀態、Native Loader 與 Safe Mode

- [ ] 3.1 實作 global/package/feature `FeatureStateStore` 與 `EffectiveFeatureResolver`，涵蓋 enabled/disabled/disabling/pending-restart/blocked/faulted
- [ ] 3.2 實作 contribution-to-feature/capability 驗證與 `ContributionGate`，讓未宣告、重複或越權 registration 拒絕整包
- [ ] 3.3 實作 `abi_stable` DLL loader，在任何 callback 前完成 root layout、SDK major 與 GPUI fingerprint 驗證
- [ ] 3.4 實作 startup-only DLL load、resident-until-exit、runtime feature gate、bounded drain 與未載入/更新 DLL 的 restart semantics
- [ ] 3.5 實作 `PluginCallGuard`、panic translation、timing、native call marker 與下次啟動 Safe Mode
- [ ] 3.6 新增 synthetic panic/uncleared marker/slow callback/drain timeout integration tests 與 Safe Mode UITEST mapping
- [ ] 3.7 撰寫原生外掛風險、無 sandbox、無熱卸載與診斷操作文件

## 4. Extension Jobs、Values、Streams 與 Cache

- [ ] 4.1 在 `explorer-jobs` 實作 extension CPU/I/O queues、global/per-package limits、visible-row priority、deadline 與 cancellation
- [ ] 4.2 實作 `JobContextV1`、generation-safe handles、`IncrementalResultSinkV1`、backpressure 與 typed terminal states
- [ ] 4.3 實作 `PluginValueV1`、`StableSortValueV1`、opaque payload routing 與 unsupported/unavailable/cancelled/error sorting policies
- [ ] 4.4 實作 `UiInvalidationBatcher` 的 16–50 ms 合併與 extension timing/slow-callback diagnostics
- [ ] 4.5 實作 generation-aware result cache、watcher/TTL/manual/data-version invalidation 與 stale result rejection
- [ ] 4.6 實作 capability-authorized `InputStreamV1` 的 bounded read/seek/length/deadline/cancel/source-generation
- [ ] 4.7 以 1,000-item fixture 驗證基本列表先顯示、可見列優先、取消延遲與非 1,000 次同步 redraw，加入 UITEST mapping
- [ ] 4.8 撰寫 jobs/value/stream/cache 公開 API 與效能診斷文件

## 5. 動態欄位與 GPUI Contribution 基礎

- [ ] 5.1 將固定 `SortColumn`、`DetailsColumnWidths` 與 visibility bitmask 遷移為 built-in/extension `ColumnId`、descriptor registry 與 ordered layout
- [ ] 5.2 實作 single/batch/aggregate column provider registries、typed sort pipeline 與 feature-gated renderer binding
- [ ] 5.3 修改 details header、row virtualization、column chooser、resize、horizontal scroll、keyboard/UIA 及 session restore 使用 dynamic registry
- [ ] 5.4 實作舊欄位設定 migration、未知 plugin column 保存/隱藏與重新安裝恢復測試
- [ ] 5.5 實作 `GpuiColumnRendererV1`、preview/panel/settings/toolbar factories、public render context、theme facade、action sink 與 scoped invalidation
- [ ] 5.6 加入 GPUI-thread assertion、renderer I/O 禁止 fixture、panic/timing marker 與慢 renderer 診斷 UITEST
- [ ] 5.7 撰寫 dynamic column 與 GPUI contribution SDK 文件，不暴露 `ExplorerState`、private `Entity<T>` 或 private actions

## 6. 第一垂直切片：Rust 資料夾大小欄位

- [ ] 6.1 建立獨立 consumer `rust-folder-size-visual-column` 完整專案、manifest、feature/capability、locales、README、license 與 package script
- [ ] 6.2 實作背景遞迴 bytes provider、symlink/junction cycle、權限 partial、取消與 cache invalidation
- [ ] 6.3 實作 largest-sibling aggregator、精確 bytes sort value 與 loading/unavailable/cancelled states
- [ ] 6.4 使用公開 GPUI SDK 實作可替換的比例條 cell renderer、重新計算命令與設定頁
- [ ] 6.5 新增 1,000 items、長路徑、cycle、partial、aggregation、sorting、renderer 與 feature toggle unit/integration/UITEST
- [ ] 6.6 在空 consumer 環境執行 build/validate/package 並將範例要求映射到 CI release gate

## 7. 動態 View Mode 與 Size Map 垂直切片

- [ ] 7.1 實作 `ViewModeRegistrationV1`、dynamic view switcher、session persistence、missing/faulted/disabled fallback
- [ ] 7.2 實作 `GpuiViewModeRendererV1` lifecycle、public view context、current-location subscription 與 view settings persistence
- [ ] 7.3 實作 `ViewSelectionBridgeV1` 與 `NavigationRequestV1`，接回正式 selection、open、breadcrumb、address 與 history
- [ ] 7.4 實作 `DirectoryTreeScanServiceV1`、bounded deltas、terminal states、quotas、symlink/hard-link policies 與 generation-aware scan cache
- [ ] 7.5 建立獨立 consumer `rust-folder-size-map-view` 完整專案與 Size Map feature/settings manifest
- [ ] 7.6 實作 squarified treemap、bytes area、folder nesting、file-type colors、tooltips、small-item accessibility、keyboard/UIA 與 GPUI-only rendering
- [ ] 7.7 實作 F5/watcher/location/view-disable generation invalidation，驗證 late scan/layout 永不覆寫新狀態
- [ ] 7.8 新增 100,000-node、deep/wide tree、cycle、hard link、partial、selection/navigation、fallback、memory/layout batching unit/integration/UITEST
- [ ] 7.9 完成 view/scan 公開文件、範例 README 與 clean consumer package gate

## 8. Batch Columns：Rust tokei 與 Lock Owner

- [ ] 8.1 完成 bounded `BatchColumnProviderV1` API、batch-to-item mapping 與 provider cost/unsupported semantics 測試
- [ ] 8.2 建立 `rust-tokei-code-lines-column` 完整 consumer 專案，以鎖定 Rust library 回傳 language/code/comments/blanks/total
- [ ] 8.3 實作 Rust tokei numeric sorting/settings/GPUI renderer，加入 mixed-language/encoding/unknown/1,000 files 且無 per-file process 測試
- [ ] 8.4 將既有 Restart Manager adapter 包裝為 read-only `LockOwnerQueryServiceV1`，加入 bounded input/result、deadline/cancel/session cleanup
- [ ] 8.5 建立 `rust-lock-owner-column` 完整 consumer 專案，實作多 owner display、short TTL 與無 process-control surface
- [ ] 8.6 將 Lock Owner manual refresh 與 F5 接到同一 refresh-generation/cache pipeline
- [ ] 8.7 使用 helper processes 驗證 acquire/release、多 owner、process-exit race、access denial、resource cleanup 與 stale-generation rejection
- [ ] 8.8 完成兩個範例 README、manifest、UITEST mapping 與 clean build/validate/package gate

## 9. Commands、Forms、Operation Plans 與 EXIF

- [ ] 9.1 實作 command/button descriptors、placement、selection predicate、shortcut 與 feature-scoped registries
- [ ] 9.2 實作 versioned `FormSchemaV1`／typed values/submission、host validation/localization 與 Rust GPUI form adapter
- [ ] 9.3 實作 `OperationPlanV1`／preview／validator，涵蓋 path normalization、Windows names、escape、case collision、permissions、limits 與 conflicts
- [ ] 9.4 將 `OperationPlanExecutor` 接到既有 file-operation pipeline，加入 progress/cancel/partial terminal 與 conservative undo journal
- [ ] 9.5 實作 batch `CreateDirectoryStep` 與只刪除本次建立且仍為空目錄的 undo 規則
- [ ] 9.6 實作 `FileDecoderV1`、typed metadata map、rename template/token parser、basename sanitizer 與 collision graph
- [ ] 9.7 建立 `rust-exif-rename-command` 完整 consumer 專案，將鎖定 Rust EXIF parser 靜態連結進 plugin.dll
- [ ] 9.8 實作 rawname/extension/X/YResolution/PixelX/YDimension/DateTimeOriginal 預覽與 undoable batch rename
- [ ] 9.9 新增 empty PATH/no network/no exiftool/no external EXIF DLL、PE import allowlist、missing tags、rational、Unicode、collision/undo tests
- [ ] 9.10 完成 commands/forms/plans/EXIF API 文件、範例 README、UITEST mapping 與 clean package gate

## 10. Lua Registrar 與 Bundled Tool 執行

- [ ] 10.1 擴充 Lua registration phase，加入 single/batch columns、commands、buttons、forms、operation-plan callbacks 與 feature/capability validation
- [ ] 10.2 建立 `PluginValueV1`／terminal／`OperationPlanV1` Lua serde mirrors 與相容 round-trip tests
- [ ] 10.3 實作 `BundledToolDescriptorV1` 與安裝時 tool validator，涵蓋 target/path/size/hash/protocol/license/reparse escape
- [ ] 10.4 實作 package-generation-scoped `ToolResolver`／`ToolHandleV1`，禁止 PATH/Registry/common path/network/user substitute
- [ ] 10.5 實作 shell-free `ProcessRequestV2`、authorized cwd/environment/stdin、timeout/output bounds 與 typed terminals
- [ ] 10.6 實作 Windows Job Object `ProcessLease`，在 cancel/drop/feature disable/folder change 時終止並回收 child process tree
- [ ] 10.7 新增 capability denial、shell injection、tampered/missing tool、stale handle、timeout/output truncation與 process cleanup tests/UITEST
- [ ] 10.8 撰寫 Lua registrar、capability、bundled tool packaging與 diagnostics 雙語文件

## 11. Lua tokei 與大量建立資料夾範例

- [ ] 11.1 建立 `lua-tokei-code-lines-column` 完整 `.sepack`，封裝精確 windows-x64 `tokei.exe`、SHA-256、來源、LICENSE/NOTICE
- [ ] 11.2 實作 shell-free argument batches、JSON result mapping、numeric sort/settings 與 unknown/binary outcomes
- [ ] 11.3 使用 fake/real tool fixtures 驗證 1,000 files、default 128 batch、command-line limit、special filenames、cancel/reap 與 no PATH fallback
- [ ] 11.4 建立 `lua-bulk-folder-generator` 完整 `.sepack`、extension button 與 host parameter form
- [ ] 11.5 實作 1–100,000 naming plan、zero padding、suffix、conflict policies、>1,000 second confirmation 與 preview
- [ ] 11.6 驗證 reserved names、trailing dot/space、long paths、duplicates、cancel、partial success 與 conservative undo
- [ ] 11.7 完成兩個 Lua 範例 README、manifest capability mapping、UITEST 與 clean validate/package gate

## 12. Virtual Folder、Streams、Mutation 與 7z 範例

- [ ] 12.1 新增 virtual variants 到 location、tab history、breadcrumb、address、session restore 與 stable entry/container generation model
- [ ] 12.2 實作 `VirtualProviderRegistrationV1`／enumeration 與 entry path normalization，拒絕 absolute/drive/`..`/NUL/collision
- [ ] 12.3 實作 bounded `VirtualFileStreamProviderV1` 與 quota-managed preview materializer/cleanup
- [ ] 12.4 將 extract/copy/drag-out 接到 typed plans，加入 escape/conflict/space/quota/progress/cancel validation
- [ ] 12.5 實作 `VirtualMutationProviderV1`、mutation preview、same-volume staging、flush/reopen/verify、identity recheck 與 atomic replace
- [ ] 12.6 實作 `SecretHandleV1`、encryption policy、archive resource policy 與 whole-container quota-managed undo
- [ ] 12.7 建立 `rust-7z-virtual-folder` 完整 consumer 專案，以鎖定純 Rust backend 支援 browse/preview/extract/add/mkdir/delete/rename/move
- [ ] 12.8 新增 normal/nested/empty/Unicode/solid/AES/corrupt/CRC/deep/traversal/bomb/low-space/race/failure-keeps-original tests
- [ ] 12.9 新增 virtual navigation、breadcrumb/history/sort/preview/drag/mutation/undo/password-no-log UITEST mapping
- [ ] 12.10 完成 Virtual Folder API、安全文件、7z README 與 clean build/validate/package gate

## 13. Folder Options／Extensions 管理頁

- [ ] 13.1 新增 `FolderOptionsPage::Extensions` 與獨立 `ExtensionOptionsSnapshot`／`ExtensionOptionsDraft`／actions，不把動態 catalog 塞入固定 Copy draft
- [ ] 13.2 實作 global switch、search、type/status filters、virtualized package rows與 expandable feature rows
- [ ] 13.3 顯示 publisher contacts、source/signature、content types、capabilities、bundled tools/licenses、fingerprint、diagnostics 與 restart impact
- [ ] 13.4 實作 `ExtensionSettingsTransaction` 的 validate/impact preview/apply/persist/draft rollback 及 Apply/OK/Cancel/Close semantics
- [ ] 13.5 實作 `FeatureDrainCoordinator`，處理 jobs/callbacks/columns/views/panels/virtual tabs 與 pending restart
- [ ] 13.6 實作 Lua/Skin/loaded Rust/unloaded Rust/Virtual Folder 的個別切換語意與 parent desired-state preservation
- [ ] 13.7 新增 long catalog、keyboard/UIA、high DPI/high contrast/localization、draft/catalog race 與不阻塞 GPUI thread tests
- [ ] 13.8 新增 UITEST manifest mapping與使用者／作者診斷文件

## 14. P1 純資料 Skin

- [ ] 14.1 定義 versioned Skin schema，涵蓋 images/icons/fonts/button states/nine-slice/vector/colors/spacing/transparency/acrylic/hit-test masks
- [ ] 14.2 實作資產 path/type/dimension/size/budget validator，拒絕任何 executable/script/shader content
- [ ] 14.3 實作 normal/hover/pressed/focused/disabled button state與 host-owned command/focus/UIA 保留
- [ ] 14.4 實作矩形 OS window 上的不規則透明視覺外框、pass-through mask 與 resize/title/window-command 保護
- [ ] 14.5 實作 per-asset default fallback、active-skin disable immediate fallback 與 setting persistence
- [ ] 14.6 新增 DPI/high contrast/Snap/maximize/resize/multi-monitor/keyboard/UIA/corrupt/oversized asset UITEST suite
- [ ] 14.7 撰寫 Skin schema、asset limits、transparent hit testing與 fallback 作者文件

## 15. 八個範例的共同 SDK 與文件

- [ ] 15.1 建立 SDK 獨立 consumer workspace、Rust/Lua templates與八個範例共同目錄/manifest/license/localization規則
- [ ] 15.2 實作 example validator，拒絕 private workspace crates、未宣告 capabilities、unwired interfaces、缺少來源/測試/文件/授權
- [ ] 15.3 產生所有範例的第三方 SBOM/provenance，區分 static Rust libraries 與 bundled executables
- [ ] 15.4 建立 Rust-only `AI_RUST_PLUGIN_PROMPT.md`，要求讀 sdk-lock/manifest/examples、使用 snapshot rev、禁止虛構作者聯絡資料與私有 crates
- [ ] 15.5 建立 AI prompt fixture，自動產生最小 provider＋GPUI renderer並通過 build/validate/package
- [ ] 15.6 為十一份 capability specs 建立 requirement-to-unit/integration/UITEST/README traceability matrix
- [ ] 15.7 驗證每個公開 interface 至少由一個獨立官方範例與 production composition-root 路徑使用，拒絕 trait-only/mock-only 完成

## 16. P0 與 Release 驗收

- [ ] 16.1 在隔離空 `CARGO_HOME`、禁止網路下以同一 snapshot bundle建置 host、SDK fixtures與八個 `.sepack`
- [ ] 16.2 執行 package/manifest/contact/hash/signature/dependency/capability/tool/ABI/fingerprint/Safe Mode strict integration suite
- [ ] 16.3 執行 1,000-item column jobs與100,000-node Size Map效能、memory、cancel、UI batching gate並保存基準報告
- [ ] 16.4 執行八個範例的 unit/integration/UITEST/security/docs reproduction suite，確認任一失敗阻擋 stable SDK
- [ ] 16.5 執行 Skin P1 DPI/accessibility/window behavior/fallback suite，確認第一階段完成條件
- [ ] 16.6 完成 public API docs、zh-TW/en author guides、migration/restart/Safe Mode/security limitations與support contact文件
- [ ] 16.7 產生最終 compatibility report、canonical lock/vendor/hash/signature與可下載 SDK/example artifacts
- [ ] 16.8 執行 RC GPUI freeze、受保護 tag記錄、`release_frozen = true`離線重建與final bundle ID驗證
- [ ] 16.9 確認核心無 Steamworks dependency且Steam/Pro僅保留抽象接點，標記 change apply工作完成
