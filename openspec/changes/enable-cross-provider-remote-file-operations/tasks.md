## 1. 共用契約與 Remote Mutation

### 1.1 操作能力與 typed request 契約

**目的：** 建立 Local／ADB／SFTP 共用且預設拒絕的 mutation、clipboard、transfer 與結果契約。
**輸入：** 已核准設計；現有 `LocationDescriptor`、`NamespaceCapabilities`、`FileOperationRequest`、`DataTransferRequest` 與 operation terminal 契約。
**產出：** 共用 capability predicate、typed source／destination、item outcome 與界限常數。
**依賴：** 無。
**Owner／Wave：** 主要代理程式／第 1 波。
**Gate／Evidence：** `REMOTE-MUTATION`、`TRANSFER-MATRIX`；`evidence/index.jsonl` 中 1.1.* 紀錄。
**完成門檻：** 所有 UI／service／provider 可用同一 typed 契約判斷與 dispatch，未知 provider 及缺少能力時預設拒絕。

- [x] 1.1.1 盤點並收斂背景目錄與選取項目的 create-directory、upload／paste、copy、cut、delete 能力，使用 provider 身分而非網址或顯示文字。
- [x] 1.1.2 擴充共用 request／outcome，使檔案與資料夾跨 provider Copy／Move、Remote permanent delete、Succeeded／Skipped／Partial／Failed／Cancelled 均能無損表達。
- [x] 1.1.3 定義 recursive traversal depth 64 與每來源樹 100000 nodes 的契約及 exact-boundary 模型測試。
- [x] 1.1.4 定義單檔／每操作／全 process staging bytes 與 free-space reserve admission 契約及 N+1 模型測試。
- [x] 1.1.5 定義 Remote／Windows component containment、identity revalidation 與 exactly-one terminal 契約及 fail-closed 模型測試。

### 1.2 ADB 與 SFTP mutation provider

**目的：** 讓兩個 provider 以一致、可取消且安全的方式建立資料夾與永久刪除檔案／樹。
**輸入：** 1.1；現有 ADB client、SFTP session 與 provider registry。
**產出：** validated create-directory、file/tree delete、取消與非秘密錯誤映射。
**依賴：** 1.1。
**Owner／Wave：** 主要代理程式／第 1 波。
**Gate／Evidence：** `REMOTE-MUTATION`；`evidence/1.2/` 與索引中的 1.2.*。
**完成門檻：** ADB／SFTP 均拒絕 traversal／authority mismatch，取消後不啟動後續破壞步驟，每個 request 只有一個 terminal。

- [x] 1.2.1 完成 ADB create-directory 的安全參數編碼、component／authority 驗證與 request cancellation。
- [x] 1.2.2 完成 ADB file／recursive-tree delete，拒絕 root／dot／parent／identity mismatch，並在每項 destructive commit 前重驗 cancellation／deadline。
- [x] 1.2.3 完成 SFTP mkdir 與 validated child path 建立，拒絕 root／authority／container mismatch。
- [x] 1.2.4 完成 SFTP lstat 驅動的 unlink／recursive-rmdir，使 symlink 只刪 link 且每項 commit 保留真實 outcome。
- [x] 1.2.5 將 ADB／SFTP enumeration、download、upload、delete 接入共用 deadline，阻止 deadline 後啟動後續步驟。
- [x] 1.2.6 將 provider mutation 接入 remote service 的 request context、逐項 terminal ledger 與受影響位置刷新資料。

### 1.3 永久刪除身分與確認安全

**目的：** 防止 Remote root、偽造／過期 descriptor 或 stale dialog 取得破壞性能力。
**輸入：** 1.1、1.2；既有 delete confirmation session。
**產出：** root guard、immutable confirmation targets、nonce／generation revalidation 與逐項 commit outcomes。
**依賴：** 1.1、1.2。
**Owner／Wave：** 主要代理程式／第 1 波。
**Gate／Evidence：** `REMOTE-MUTATION`、`DESTRUCTIVE-FIXTURE`；`evidence/1.3/` 與索引中的 1.3.*。
**完成門檻：** root／identity mismatch 在 dispatch 前失敗；dialog 只能刪核准集合；取消後已刪項目與未開始項目不會被混淆。

- [x] 1.3.1 實作 Remote delete root／empty／dot／parent 與 provider／authority／container identity／generation 硬拒絕。
- [x] 1.3.2 讓 delete confirmation 綁定 immutable typed targets、operation nonce 與 generation，拒絕 selection／tab／location 變更後的 stale dispatch。
- [x] 1.3.3 定義並實作 per-item destructive commit point，使 cancel-before-first、between-items、during-tree 保留已完成真實 outcome 且不啟動未開始項目。

## 2. Transfer Engine 與 Clipboard

### 2.1 檔案與 bounded directory tree 傳輸

**目的：** 讓 Transfer Engine 完成 Local／ADB／SFTP 六種跨邊界方向的檔案與資料夾 Copy。
**輸入：** 第 1 階段；現有 upload／download、Shell Local → Local 與 conflict decision。
**產出：** bounded recursive transfer、目的 component 建立、衝突與取消整合。
**依賴：** 1.1、1.2。
**Owner／Wave：** 主要代理程式／第 2 波。
**Gate／Evidence：** `TRANSFER-MATRIX`；`evidence/2.1/` 與索引中的 2.1.*。
**完成門檻：** 六種方向均保留相對樹，link target 不被追蹤，衝突不靜默覆寫，超界／取消能有界終止。

- [x] 2.1.1 擴充 provider download／upload 與 Transfer Engine，使檔案與目錄項目可判型並安全建立目的根名稱。
- [x] 2.1.2 實作不追蹤符號連結且深度 64／每樹 100000 nodes 的 recursive enumeration 與相對 component 驗證。
- [x] 2.1.3 實作 Windows staging 單一 component 驗證、ADS／reserved／trailing-dot-space 拒絕、normalization／case-fold conflict 與 canonical containment／reparse 防護。
- [x] 2.1.4 將 Prompt／Skip／Replace／KeepBoth 接入 copy pipeline，保留 `Skipped` 且任何 skipped descendant 禁止 Move 刪來源。
- [x] 2.1.5 將單檔 32 GiB、每操作 64 GiB、全 process 128 GiB 與 max(2 GiB, 5%) free-space reserve 以實際寫入 bytes 接入 N+1 admission。
- [x] 2.1.6 保持 Local → Local 使用 Windows Shell，並讓其與 remote pipeline 產生一致的 item outcomes／refresh metadata。

### 2.2 Scoped staging 與 copy-then-delete Move

**目的：** 安全完成 Remote → Remote 中轉與不誤刪來源的跨 provider Move。
**輸入：** 2.1；`tempfile::TempDir` 與 provider delete。
**產出：** scoped staging guard、Copy／Move 狀態機、Partial 結果及清理保證。
**依賴：** 2.1。
**Owner／Wave：** 主要代理程式／第 2 波。
**Gate／Evidence：** `TRANSFER-MATRIX`；`evidence/2.2/` 與索引中的 2.2.*。
**完成門檻：** staging 僅刪除驗證過的 owned root；Move 只有完整 copy 後才刪來源；刪除失敗必為 Partial。

- [x] 2.2.1 建立不含 host／user／serial／remote path 的每操作 scoped staging root、quota accounting 與 verified cleanup guard。
- [x] 2.2.2 將 ADB → SFTP、SFTP → ADB 及同／異 remote provider 路徑接到 download → staged tree → upload pipeline。
- [x] 2.2.3 實作 per-source copy-then-delete Move 狀態機，使 copy／取消／衝突失敗保留來源，delete 失敗回報 Partial。
- [x] 2.2.4 證明成功、Failed 與 early return 均由 verified guard 釋放一般 transfer staging。
- [x] 2.2.5 證明 Cancelled 與 deadline terminal 均釋放 staging 且不啟動 source delete。
- [x] 2.2.6 對 transfer panic boundary 做 containment 並釋放 owned staging，不跨 ABI／thread boundary unwind。
- [x] 2.2.7 將 staging／transfer 診斷 redaction 為不含 authority、remote path、秘密或檔案內容的有界訊息。

### 2.3 Typed clipboard 與 Paste routing

**目的：** 讓右鍵與 `Ctrl+C/X/V` 支援 mixed-provider 檔案操作且不干擾文字／圖片 clipboard。
**輸入：** 1.1、2.1、2.2；現有 `ClipboardState`、Shell clipboard adapter 與 focus routing。
**產出：** typed remote file format、clipboard isolation、Paste → Transfer Engine dispatch。
**依賴：** 2.1、2.2。
**Owner／Wave：** 主要代理程式／第 2 波。
**Gate／Evidence：** `CLIPBOARD-ISOLATION`；`evidence/2.3/` 與索引中的 2.3.*。
**完成門檻：** file view Copy／Cut 可保存 Local／Virtual 來源；可寫 Local／Remote Paste 正確 dispatch；editable 與非檔案 clipboard 不被消耗或清除。

- [x] 2.3.1 建立 host-minted 256-bit process/session-bound clipboard token 與 immutable internal Local／Virtual source record，不把可執行 Cut descriptor 放入 native payload。
- [x] 2.3.2 對 forged、malformed、replayed、previous-process 與已消耗 token 預設拒絕，且 dispatch 前重驗 provider／authority／generation／capability。
- [x] 2.3.3 將 context menu 與 file-view `Ctrl+C/X/V` 導向同一 begin-copy／cut／paste request，並將 Paste dispatch 至 Shell 或 Transfer Engine。
- [x] 2.3.4 保留 editable text 的快捷鍵優先權，並讓 text／HTML／PNG／bitmap／unknown-only clipboard 對 file Paste 回傳 unsupported 而不改動內容。
- [x] 2.3.5 分離 view generation 與 operation／clipboard generation，使 stale view 不更新 snapshot 但匹配 terminal 仍冪等消耗完成的 Cut items。
- [x] 2.3.6 讓 Partial 只保留 Skipped／Failed／Cancelled／未開始 Cut items，並拒絕 terminal replay 再次傳輸或刪除。

## 3. UI、刪除確認與 Drag/Drop

### 3.1 Remote 右鍵、快捷鍵與永久刪除確認

**目的：** 在可寫遠端位置顯示正確命令，並以不可復原確認保護 Remote Delete。
**輸入：** 第 1–2 階段；現有 context menu、keyboard reducer、rename/new-folder editor 與 delete dialog。
**產出：** 背景／項目 capability UI、Remote confirmation、operation progress／result surface。
**依賴：** 1.2、2.3。
**Owner／Wave：** 主要代理程式／第 3 波。
**Gate／Evidence：** `REMOTE-MUTATION`、`CLIPBOARD-ISOLATION`；`evidence/3.1/` 與索引中的 3.1.*。
**完成門檻：** 支援的 ADB／SFTP 顯示並執行命令；unsupported provider 不顯示；Remote 刪除永久確認，Local delete 仍為 recycle。

- [x] 3.1.1 將背景右鍵的新增資料夾／貼上與項目右鍵的刪除／複製／剪下導向單一 capability predicate。
- [x] 3.1.2 讓 ADB／SFTP 新增資料夾 editor 提交 typed remote create request，並在成功／失敗後維持正確 editor 與 refresh 狀態。
- [x] 3.1.3 讓 Remote Delete 顯示不可復原確認並提交 permanent provider delete；Local Delete 保持 Windows Recycle Bin。
- [x] 3.1.4 將 item-level Succeeded／Skipped／Partial／Failed／Cancelled 與 aggregate Partial／failure 映射到 bounded progress／status。
- [x] 3.1.5 拒絕 stale view generation 更新目前 snapshot／selection，同時讓匹配 operation terminal 冪等完成 clipboard outcome bookkeeping。

### 3.2 應用程式內跨 provider 拖放

**目的：** 讓 SuperExplorer 內部 Local／ADB／SFTP 拖放與 clipboard Paste 共用 transfer semantics。
**輸入：** 2.1–2.3；現有 drag session、effect negotiation 與 drop request。
**產出：** typed internal sources、remote drop capability、Copy／Move dispatch。
**依賴：** 2.1、2.2、2.3。
**Owner／Wave：** 主要代理程式／第 3 波。
**Gate／Evidence：** `DRAG-INTEROP`；`evidence/3.2/` 與索引中的 3.2.*。
**完成門檻：** 所有 internal provider 組合在合法目的上產生與 Paste 相同結果；非法 self／descendant／unsupported drop 預設拒絕。

- [x] 3.2.1 擴充 internal drag payload 以保留 Local／Virtual typed sources、來源 provider 與 Copy／Move intent。
- [x] 3.2.2 將 remote directory／row drop hit target、allowed effects 與 self／descendant 驗證導向共用 capability／location 邊界。
- [x] 3.2.3 將接受的 internal drop 轉成與 Paste 相同 Transfer Engine request，並共享 cancellation、conflict、Move Partial 與 refresh 行為。

### 3.3 Windows Explorer drag-in 與 staged drag-out

**目的：** 完成原生檔案總管 Local → Remote 與 Remote → Explorer 互動，並治理 staging lease 生命週期。
**輸入：** 2.1、2.2、3.2；既有 Shell `CF_HDROP`、OLE data object 與 drag loop。
**產出：** native Local typed imports、fully materialized remote exports、lease cleanup／failure behavior。
**依賴：** 2.2、3.2。
**Owner／Wave：** 主要代理程式／第 3 波。
**Gate／Evidence：** `DRAG-INTEROP`；`evidence/3.3/` 與索引中的 3.3.*。
**完成門檻：** Explorer Local files 可 upload 至 ADB／SFTP；Remote 拖出只發布完整 staged paths，lease 在 Shell 結束後清理，失敗時不發布不完整資料。

- [x] 3.3.1 將 Windows Explorer `CF_HDROP` Local paths 驗證並轉成 typed Local sources，再 routing 至 remote Transfer Engine。
- [x] 3.3.2 實作 Remote selection 的完整 staged materialization 與 OLE data object，禁止在完成前發布 Local path。
- [x] 3.3.3 將 Remote → Explorer 限制為 `DROPEFFECT_COPY`，不以 staged file 的 performed effect 刪除 Remote source。
- [x] 3.3.4 以 COM data object／drag source shared owner 持有 staging lease，等待 DoDragDrop terminal 與 final `IDataObject::Release` 後 exactly-once cleanup。
- [x] 3.3.5 實作 QueryInterface／AddRef／Release、STGMEDIUM ownership、STA affinity 與 callback `catch_unwind` → HRESULT 契約。
- [x] 3.3.6 讓 window teardown 只取消並釋放自己的 reference，不強制刪除仍由 COM 持有的 staging。
- [x] 3.3.7 對 materialization／quota／network 失敗取消 drag、顯示單一 bounded error，且不暴露不存在路徑、秘密或遠端內容。

## 4. 最後集中驗證與交付

### 4.1 聚焦自動化驗證

**目的：** 在所有實作整合完成後一次執行相關測試／編譯矩陣，不做完整 workspace 回歸。
**輸入：** 第 1–3 階段與同步加入的聚焦測試。
**產出：** `evidence/4.1/` 原始輸出及索引中的 pass／failure 紀錄。
**依賴：** 1.1–3.3 全部完成。
**Owner／Wave：** 主要代理程式／第 4 波，僅在最後執行。
**Gate／Evidence：** `REMOTE-MUTATION`、`TRANSFER-MATRIX`、`CLIPBOARD-ISOLATION`、`DRAG-INTEROP`、`FINAL-FOCUSED`。
**完成門檻：** 所有聚焦測試與相關 crate check 成功，原始命令／exit status 已保存，未啟動完整回歸。

- [x] 4.1.1 執行 ADB create-directory component／authority validation 測試。
- [x] 4.1.2 執行 ADB root guard 與 file／recursive-tree delete containment 測試。
- [x] 4.1.3 執行 ADB cancel-before／between-items／during-tree、deadline 與 exactly-one-terminal 測試。
- [x] 4.1.4 執行 SFTP mkdir component／authority validation 測試。
- [x] 4.1.5 執行 SFTP root guard、lstat symlink unlink 與 recursive-rmdir containment 測試。
- [x] 4.1.6 執行 SFTP cancel-before／between-items／during-tree、deadline 與 exactly-one-terminal 測試。
- [x] 4.1.7 執行 delete confirmation selection change 與 immutable original-target dispatch／fail-closed 測試。
- [x] 4.1.8 執行 delete confirmation tab／navigation／generation change 與 nonce replay 拒絕測試。
- [x] 4.1.9 在唯一 marker 驗證的 owned ADB fixture subtree 執行破壞性 integration 與 containment oracle。
- [x] 4.1.10 在唯一 marker 驗證的 owned SFTP fixture subtree 執行破壞性 integration 與 containment oracle。
- [x] 4.1.11 執行六種 provider 方向的 file copy matrix。
- [x] 4.1.12 執行六種 provider 方向的 recursive tree／link-not-followed matrix。
- [x] 4.1.13 執行 Windows separator／ADS／reserved／trailing／normalization／case-fold／reparse malicious-name matrix。
- [x] 4.1.14 執行 Prompt／Skip／Replace／KeepBoth 與 skipped-descendant-prevents-delete 測試。
- [x] 4.1.15 執行 depth／node／per-file／per-operation quota exact-boundary 與 N+1 測試。
- [x] 4.1.16 執行 lying-size、全 process concurrent quota 與 free-space reserve 測試。
- [x] 4.1.17 執行 success／Failed／early-return／Cancelled／deadline／panic staging cleanup 測試。
- [x] 4.1.18 執行 Copy／Move／Partial／Skipped／Failed／Cancelled 與 delete-commit outcome 測試。
- [x] 4.1.19 執行 terminal replay、stale view／operation generation 與 Cut partial-retention 測試。
- [x] 4.1.20 執行 context menu、Ctrl+C/X/V 與 editable focus routing 測試。
- [x] 4.1.21 執行 text／HTML／PNG／bitmap／unknown clipboard isolation 測試。
- [x] 4.1.22 執行 forged／malformed／replayed／previous-process／consumed clipboard token 測試。
- [x] 4.1.23 執行 internal drag 的 typed routing／effect／self-descendant／failure contract 測試。
- [x] 4.1.24 執行 Explorer drag-in 的 CF_HDROP validation／remote upload／failure contract 測試。
- [x] 4.1.25 執行 OLE QueryInterface／AddRef／Release 與 exactly-once final-release unit tests。
- [x] 4.1.26 執行 OLE STGMEDIUM ownership、STA affinity 與 callback panic → HRESULT unit tests。
- [x] 4.1.27 執行 drag terminal／window teardown／COM-held staging lease lifetime unit tests。
- [x] 4.1.28 以真實 Windows Explorer 與 disk/content oracle 執行 mandatory headful drag-in；無真實輸入能力時將 gate 記為 Blocked。
- [x] 4.1.29 以真實 Windows Explorer 與 disk/content oracle 執行 mandatory headful remote staged drag-out；不得以 synthetic 取代。
- [x] 4.1.30 執行相關 model、remote、shell-win、ui、app crate 編譯／檢查矩陣，不得執行完整 workspace 回歸。
- [x] 4.1.31 執行 `cargo fmt --check` 與 `git diff --check` 並保存輸出。

### 4.2 規格、差異與安全審查

**目的：** 證明最終實作符合核准契約、安全／隱私邊界及 dirty worktree 保護要求。
**輸入：** 4.1 evidence；proposal、design、spec、tasks 與最終差異。
**產出：** strict validation、placeholder／traceability／security review 與完整 evidence index。
**依賴：** 4.1。
**Owner／Wave：** 主要代理程式／第 4 波。
**Gate／Evidence：** `FINAL-FOCUSED`；`evidence/4.2/` 與 `evidence/index.jsonl`。
**完成門檻：** apply artifacts 完整、所有 scenarios 可追溯至 task／evidence、無未解決 P0／P1、無秘密或無關使用者變更。

- [x] 4.2.1 執行 `openspec validate enable-cross-provider-remote-file-operations --strict` 並保存成功輸出。
- [x] 4.2.2 掃描 artifacts／changed source 的 placeholder、矛盾、弱化 fail-closed／取消／清理／permanent-delete 契約與缺少 traceability。
- [x] 4.2.3 驗證並更新既有 `traceability.md` 的 requirement／scenario key → implementation task → validation task → gate／evidence subcheck 映射；scenario heading 不得靜默改名。
- [x] 4.2.4 審查最終差異的 staging target、recursive delete、clipboard authenticity／isolation、credential／diagnostic redaction、Local Shell 相容性及無關 dirty-worktree 覆寫。
- [x] 4.2.5 完成 `evidence/index.jsonl`，為每個 L3 納入唯一 task ID／subcheck、命令或程序、預期／實際、exit status／reviewer、gate、hash、timestamp 與 adjustment lineage，並解決所有 P0／P1。
