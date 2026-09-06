# 實作計畫

詳細執行計畫：`docs/superpowers/plans/2026-09-03-standard-sftp-userinfo-uri.md`。

**目標：** 讓 `sftp://username@host/path` 成為唯一帶帳號的 SFTP URI 解讀方式，用該帳號預填登入，並維持既有僅主機的書籤、歷程與已儲存連線可用。

**架構：** `SftpAddressInput::parse` 是唯一來源。網址列登入與 `RemoteService::login_address` 使用它，且不得自行對調 authority 元件。

**規格：** `docs/superpowers/specs/2026-09-03-standard-sftp-userinfo-uri-design.md`

## 全域約束

- 密碼不得出現在 SFTP URI 的接受、顯示、記錄或持久化內容中。
- 正規位置維持僅主機：`sftp://host/path`。
- 不得猜測 `@` 哪一側是主機。舊的 `sftp://host@username/` 一律依新標準解讀。
- 不做憑證庫、主機金鑰或設定檔別名遷移。
- 網址列 SFTP 登入必須離開 GPUI 執行緒。

## 1. 共用解析器契約

### 1.1 標準 user-info 順序

**目的：** 把一個 `@` 拆成 `(username, host)`，拒絕不安全的 user-info，並維持僅主機的正規身分。
**輸入：** 已核准設計；`crates/explorer-model/src/remote.rs` 的 `SftpAddressInput::parse`。
**產出：** 解析器 + 聚焦 model 測試。
**依賴：** 無。
**完成門檻：** `sftp://root@45.32.49.125/` 得到提示 `root` 與正規 `sftp://45.32.49.125`；僅主機路徑仍可解析；不推斷反序輸入；`:`、空帳號與多餘 `@` 回傳 `InvalidAuthority`。

- [x] 1.1 更新 `SftpAddressInput::parse`，拆成 `username@host`，拒絕不安全 user-info，並保留僅主機的正規遠端身分。以 `cargo test -p explorer-model --lib -- remote::tests::direct_sftp_username_hint_uses_standard_userinfo_order --exact` 驗證。
- [x] 1.2 新增 model 測試：標準輸入、僅主機相容、不推斷舊順序、安全拒絕密碼。以 `cargo test -p explorer-model --lib -- remote::tests::direct_sftp` 驗證。
- [x] 1.3 斷言被拒絕的密碼不得出現在 `RemoteAddressError` 的 Display 或 Debug（`sftp://root:secret@45.32.49.125/` → 不含 `secret`）。以 `cargo test -p explorer-model --lib -- remote::tests::direct_sftp_password_is_absent_from_error_diagnostics --exact` 驗證。

## 2. 應用程式整合

### 2.1 網址列與協調器呼叫點

**目的：** 帳號預填與主機解析只使用共用解析器。
**輸入：** 任務 1 解析器；`ExplorerRoot::begin_address_navigation`；`RemoteService::login_address`。
**產出：** UI 整合測試；確認 app 程式碼沒有第二次對調 authority。
**依賴：** 1.1、1.2。
**完成門檻：** `sftp://root@45.32.49.125/` 預填 `root`、導航到主機 `45.32.49.125`，且不阻擋 GPUI callback。

- [x] 2.1 更新網址登入整合測試，證明標準帳號預填會到達以主機為準的導航。以 `cargo test -p explorer-ui --lib -- standard_sftp_username_hint_is_intercepted_and_navigates_to_canonical_host --exact --nocapture` 驗證。
- [x] 2.2 確認 `RemoteService::login_address` 與 `application.rs` 的 observer 呼叫 `SftpAddressInput::parse` / `login_address(input)`，沒有額外改寫 `@`。以程式檢視確認 `suggested_user` 來自 `username_hint`，其次才是已儲存設定檔帳號。

## 3. 取代反序文件

### 3.1 進行中規格仍在教 `host@username`

**目的：** 避免兩份進行中文件指定相反的帳號提示順序。
**輸入：** 本變更已接受的形式；`docs/superpowers/specs/2026-08-26-sftp-address-login-design.md`；`openspec/changes/add-sftp-address-login/specs/sftp-address-login/spec.md`。
**產出：** 改用 `sftp://root@45.32.49.125/` 的例子。
**依賴：** 1.x（行為已決定）。
**完成門檻：** 沒有進行中規格再把 `sftp://45.32.49.125@root/` 當成 `username = root`。

- [x] 3.1 把 `docs/superpowers/specs/2026-08-26-sftp-address-login-design.md` 的反序例子改成 `sftp://root@45.32.49.125/`，並註明舊形式已被取代。以搜尋該檔的 `host@username` 與 `45.32.49.125@root` 驗證。
- [x] 3.2 把仍在進行中的 `add-sftp-address-login` 帳號提示情境 WHEN 子句改成 `sftp://root@45.32.49.125/`。以 `openspec validate add-sftp-address-login --strict` 驗證。
- [x] 3.3 維持本變更 delta 規格情境：標準輸入、僅主機相容、不推斷反序、拒絕密碼、空帳號、多個 `@`、正規位址不含帳號。

## 4. 完成檢查

### 4.1 聚焦測試與使用者審查

**目的：** 證明解析器、登入預填、相容性與 OpenSpec 產物保持一致。
**輸入：** 任務 1–3。
**產出：** 通過的聚焦 cargo 測試與 OpenSpec 嚴格驗證。
**依賴：** 1–2 完成；最終重驗證前先完成 3。
**完成門檻：** model／UI／app 測試通過（允許既有環境測試略過）；OpenSpec 嚴格驗證通過；使用者視角審查符合標準 URI。

- [x] 4.1 跑聚焦 model、UI 與應用程式測試並修正失敗。驗證：`cargo test -p explorer-model --lib remote`；`cargo test -p explorer-ui --lib -- standard_sftp_username_hint_is_intercepted_and_navigates_to_canonical_host --exact`；`cargo test -p explorer-app`（119 項通過、1 項既有環境測試略過）。
- [x] 4.2 跑 OpenSpec 嚴格驗證，並檢查已完成產物有無契約矛盾。以 `openspec validate standardize-sftp-username-uri --strict` 驗證。
- [x] 4.3 做使用者視角的網址輸入、登入與導航審查：`sftp://root@45.32.49.125/` 預填 `root` 並連到 `45.32.49.125`；僅主機書籤／歷程／已儲存連線仍有效；`root:password@host` 被拒絕；不推斷舊的 `host@username`。
- [x] 4.4 在 1.3 與 3.x 之後，重跑 `cargo test -p explorer-model --lib -- remote::tests::direct_sftp`、UI 登入測試、`openspec validate standardize-sftp-username-uri --strict` 與 `openspec validate add-sftp-address-login --strict`。
