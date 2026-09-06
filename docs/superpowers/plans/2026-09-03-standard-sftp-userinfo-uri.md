# 標準 SFTP `username@host` URI 實作計畫

> **給代理執行者：** 必要子技能：使用 superpowers:subagent-driven-development（建議）或 superpowers:executing-plans，依任務逐步實作。步驟以核取方塊（`- [ ]`）追蹤。

**目標：** 讓 `sftp://username@host/path` 成為唯一帶帳號的 SFTP URI 解讀方式，用該帳號預填登入，並維持既有僅主機的書籤、歷程與已儲存連線可用。

**架構：** `SftpAddressInput::parse` 是唯一來源。它把一個 `@` 拆成 `(username, host)`，拒絕不安全的 user-info，並正規化成不含憑證的 `sftp://host/path`。網址列登入（`ExplorerRoot::begin_address_navigation`）與 `RemoteService::login_address` 必須使用這個解析器，且不得自行對調 authority 元件。

**技術棧：** Rust workspace crates `explorer-model`、`explorer-ui`、`explorer-app`；Windows Credential UI 登入路徑（不變）；OpenSpec 變更 `standardize-sftp-username-uri`。

**規格：** `docs/superpowers/specs/2026-09-03-standard-sftp-userinfo-uri-design.md`  
**OpenSpec：** `openspec/changes/standardize-sftp-username-uri/`  
**OpenSpec 規格：** `openspec/changes/standardize-sftp-username-uri/specs/standard-sftp-userinfo-addresses/spec.md`

## 全域約束

- 密碼不得出現在 SFTP URI 的接受、顯示、記錄或持久化內容中。
- 正規位置維持僅主機：`sftp://host/path`。帳號只是暫時的登入預填提示。
- 解析器不得猜測 `@` 哪一側是主機。舊的 `sftp://host@username/path` 一律依新標準解讀（`username = host`，`host = username`）。
- 既有僅主機的設定檔、憑證、書籤與歷程維持有效。不做憑證庫或設定檔別名遷移。
- 不變更驗證方式、主機金鑰政策、Credential Manager 目標，或設定檔 JSON 形狀。
- user-info 若帳號為空、含 `:`、第二個 `@` 或 NUL，拒絕為 `RemoteAddressError::InvalidAuthority`。
- 主機驗證沿用既有 `validate_authority` 規則（拒絕空值、`>255`、`@`、`:`、`\`、NUL、空白）。公開 URI 不新增埠號或 IPv6 字面值。
- 網址列 SFTP 登入必須離開 GPUI 執行緒。`begin_address_navigation` 仍須立即回傳 `None`。

## 檔案對照

- 修改：`crates/explorer-model/src/remote.rs` — `SftpAddressInput::parse` 與聚焦測試
- 修改：`crates/explorer-ui/src/lib.rs` — 網址列登入整合測試（呼叫端已轉送原始字串）
- 僅驗證：`crates/explorer-app/src/remote_service.rs` — `login_address` 已呼叫共用解析器
- 僅驗證：`crates/explorer-app/src/application.rs` — observer 為 `runtime.login_address(input)`，沒有改寫 authority
- 修改：`docs/superpowers/specs/2026-08-26-sftp-address-login-design.md` — 取代反序 `host@username` 例子
- 修改：`openspec/changes/add-sftp-address-login/specs/sftp-address-login/spec.md` — 替換帳號提示情境，避免兩個進行中變更互相矛盾
- 規劃：`openspec/changes/standardize-sftp-username-uri/{proposal,design,tasks}.md` 與 `specs/standard-sftp-userinfo-addresses/spec.md`

---

### 任務 1：共用解析器契約

**檔案：**
- 修改：`crates/explorer-model/src/remote.rs`
- 測試：`crates/explorer-model/src/remote.rs`（`#[cfg(test)]` 模組）

**介面：**
- 使用：既有 `RemoteAddress::parse`、`validate_authority`、`RemoteAddressError`
- 產出：`SftpAddressInput { address: RemoteAddress, username_hint: Option<String> }`，其中 `parse(&str) -> Result<Self, RemoteAddressError>` 把單一個 `@` 拆成 `(username, host)`

- [x] **步驟 1：寫出會失敗的測試**

```rust
#[test]
fn direct_sftp_username_hint_uses_standard_userinfo_order() {
    let input = SftpAddressInput::parse("sftp://root@45.32.49.125/").unwrap();
    assert_eq!(input.username_hint.as_deref(), Some("root"));
    assert_eq!(input.address.canonical(), "sftp://45.32.49.125");
    assert!(!format!("{:?}", input.address).contains("root"));
}

#[test]
fn direct_sftp_host_only_address_remains_compatible() {
    let input = SftpAddressInput::parse("sftp://45.32.49.125/home/linuxuser").unwrap();
    assert_eq!(input.username_hint, None);
    assert_eq!(
        input.address.canonical(),
        "sftp://45.32.49.125/home/linuxuser"
    );
}

#[test]
fn direct_sftp_reversed_legacy_order_is_not_inferred() {
    let input = SftpAddressInput::parse("sftp://45.32.49.125@root/").unwrap();
    assert_eq!(input.username_hint.as_deref(), Some("45.32.49.125"));
    assert_eq!(input.address.canonical(), "sftp://root");
}

#[test]
fn direct_sftp_rejects_password_bearing_or_malformed_user_info() {
    for input in [
        "sftp://root:secret@45.32.49.125/",
        "sftp://@45.32.49.125/",
        "sftp://root@@45.32.49.125/",
    ] {
        assert_eq!(
            SftpAddressInput::parse(input),
            Err(RemoteAddressError::InvalidAuthority)
        );
    }
}
```

- [x] **步驟 2：跑測試，確認舊的 `host@username` 拆法會失敗**

執行：`cargo test -p explorer-model --lib -- remote::tests::direct_sftp_username_hint_uses_standard_userinfo_order --exact --nocapture`

解析器對調前的預期：FAIL（`username_hint` 是 `45.32.49.125`，正規主機是 `root`）。

- [x] **步驟 3：實作標準拆法**

在 `SftpAddressInput::parse` 把反序拆法改成：

```rust
let (host, username_hint) = authority
    .split_once('@')
    .map_or((authority, None), |(username, host)| {
        (host, Some(username.to_owned()))
    });
validate_authority(host)?;
if username_hint.as_deref().is_some_and(|value| {
    value.is_empty() || value.len() > 255 || value.contains([':', '@', '\0'])
}) {
    return Err(RemoteAddressError::InvalidAuthority);
}
let canonical = if path.is_empty() {
    format!("sftp://{host}/")
} else {
    format!("sftp://{host}/{path}")
};
Ok(Self {
    address: RemoteAddress::parse(&canonical)?,
    username_hint,
})
```

`SFTP://` 前綴處理與路徑轉送維持不變。不要把 `username_hint` 放進 `RemoteAddress`。

- [x] **步驟 4：跑解析器測試**

執行：

```
cargo test -p explorer-model --lib -- remote::tests::direct_sftp_username_hint_uses_standard_userinfo_order remote::tests::direct_sftp_host_only_address_remains_compatible remote::tests::direct_sftp_reversed_legacy_order_is_not_inferred remote::tests::direct_sftp_rejects_password_bearing_or_malformed_user_info remote::tests::sftp_address_keeps_user_info_out_of_canonical_location remote::tests::sftp_profile_serialization_has_no_password_field -- --nocapture
```

預期：PASS。正規 `RemoteAddress::parse("sftp://root@45.32.49.125/root")` 仍須為 `Err`，讓持久化位置不能含 user-info。

- [x] **步驟 5：證明被拒絕的密碼不會出現在錯誤文字中**

在 `direct_sftp_rejects_password_bearing_or_malformed_user_info`（或相鄰測試）加入此斷言。規格要求診斷輸出不得含密碼：

```rust
#[test]
fn direct_sftp_password_is_absent_from_error_diagnostics() {
    let error = SftpAddressInput::parse("sftp://root:secret@45.32.49.125/").unwrap_err();
    assert_eq!(error, RemoteAddressError::InvalidAuthority);
    let display = error.to_string();
    let debug = format!("{error:?}");
    assert!(!display.contains("secret"));
    assert!(!debug.contains("secret"));
    assert!(!display.contains("root:secret"));
    assert!(!debug.contains("root:secret"));
}
```

執行：`cargo test -p explorer-model --lib -- remote::tests::direct_sftp_password_is_absent_from_error_diagnostics --exact`

預期：測試尚未存在時 FAIL；加入後 PASS，因為 `RemoteAddressError` 是沒有 payload 的 unit enum。

- [x] **步驟 6：提交解析器契約**

```
git add crates/explorer-model/src/remote.rs
git commit -m "fix: parse sftp user-info as username@host"
```

已落在 `b928fd7`。步驟 5 完成後，另開只含診斷斷言的後續提交。

---

### 任務 2：網址列登入整合

**檔案：**
- 修改：`crates/explorer-ui/src/lib.rs`（測試 `standard_sftp_username_hint_is_intercepted_and_navigates_to_canonical_host`）
- 僅驗證：`crates/explorer-ui/src/lib.rs` 的 `begin_address_navigation`（轉送原始字串；不得自行拆 `@`）
- 僅驗證：`crates/explorer-app/src/remote_service.rs` 的 `login_address`
- 僅驗證：`crates/explorer-app/src/application.rs` 的 observer `runtime.login_address(input)`

**介面：**
- 使用：任務 1 的 `SftpAddressInput::parse`
- 產出：登入 observer 收到帳號提示 `root`；導航後的虛擬位置 `public_authority` 為 `45.32.49.125`

- [x] **步驟 1：寫出會失敗的 UI 測試**

```rust
#[test]
fn standard_sftp_username_hint_is_intercepted_and_navigates_to_canonical_host() {
    let mut root = ExplorerRoot::default();
    root.attach_sftp_address_login_observer(Arc::new(|input| {
        let parsed = explorer_model::SftpAddressInput::parse(input)
            .map_err(|error| error.to_string())?;
        assert_eq!(parsed.username_hint.as_deref(), Some("root"));
        parsed
            .address
            .to_deterministic_location(1)
            .map(Some)
            .map_err(|error| error.to_string())
    }));

    assert!(
        root.begin_address_navigation("sftp://root@45.32.49.125/")
            .is_none(),
        "SFTP login must not block the GPUI action callback"
    );
    let (_, result) = root
        .sftp_address_login
        .as_ref()
        .expect("login state")
        .receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("background login result");
    let location = result.expect("login succeeds").expect("canonical location");
    let explorer_model::LocationDescriptor::Virtual(location) = location else {
        panic!("expected virtual SFTP location");
    };
    assert_eq!(location.public_authority.as_deref(), Some("45.32.49.125"));
    assert!(location.components.is_empty());
}
```

不要讓 `begin_address_navigation` 自己解析 user-info。帳號提示的唯一解析呼叫點必須維持在 observer / `login_address` 路徑。

- [x] **步驟 2：跑 UI 測試**

執行：`cargo test -p explorer-ui --lib -- standard_sftp_username_hint_is_intercepted_and_navigates_to_canonical_host --exact --nocapture`

預期：任務 1 完成後 PASS。`begin_address_navigation` 仍回傳 `None`（背景執行緒）。

- [x] **步驟 3：確認應用程式協調器沒有第二次對調 authority**

`RemoteService::login_address` 必須是：

```rust
let parsed =
    explorer_model::SftpAddressInput::parse(input).map_err(|error| error.to_string())?;
let host = parsed.address.authority.clone();
let suggested_user = parsed
    .username_hint
    .or_else(|| saved.as_ref().map(|profile| profile.username.clone()))
    .unwrap_or_default();
```

`application.rs` 必須掛上 `runtime.login_address(input)`，不得額外拆分。僅主機的 `sftp://45.32.49.125/path` 仍以別名 `== host` 解析 `saved`；沒有提示時預填 `saved.username`。

- [x] **步驟 4：提交 UI 整合**

已落在 `b928fd7`（`crates/explorer-ui/src/lib.rs`）。

---

### 任務 3：取代反序文件

**檔案：**
- 修改：`docs/superpowers/specs/2026-08-26-sftp-address-login-design.md`
- 修改：`openspec/changes/add-sftp-address-login/specs/sftp-address-login/spec.md`
- 若情境仍缺漏，修改：`openspec/changes/standardize-sftp-username-uri/specs/standard-sftp-userinfo-addresses/spec.md`

**介面：**
- 使用：本變更已接受的形式
- 產出：進行中規格不得再把 `sftp://45.32.49.125@root/` 當成 `username = root`、`host = 45.32.49.125`

- [x] **步驟 1：更新 2026-08-26 網址登入設計例子**

在 `docs/superpowers/specs/2026-08-26-sftp-address-login-design.md`：

- 目的列目前寫 `sftp://<host>@<username>/`。改成 `sftp://<username>@<host>/`，並加一句說明 `standardize-sftp-username-uri` 已取代反序形式。
- 把帳號提示條目改成：

```
- `sftp://root@45.32.49.125/` opens the same surface with `root` prefilled and
  immediately canonicalizes the visible/persistable address to
  `sftp://45.32.49.125/`.
```

僅主機的 `sftp://45.32.49.125/`、Credential UI、主機金鑰與持久化行為維持不變。

- [x] **步驟 2：更新仍在進行中的 `add-sftp-address-login` 帳號提示情境**

在 `openspec/changes/add-sftp-address-login/specs/sftp-address-login/spec.md`，把：

```
#### Scenario: Username hint
- **WHEN** the user submits `sftp://45.32.49.125@root/`
- **THEN** the login surface SHALL prefill `root` and every persistable/display address SHALL be canonicalized to `sftp://45.32.49.125/`
```

改成：

```
#### Scenario: Username hint
- **WHEN** the user submits `sftp://root@45.32.49.125/`
- **THEN** the login surface SHALL prefill `root` and every persistable/display address SHALL be canonicalized to `sftp://45.32.49.125/`
```

不要改密文隔離、持久化或主機金鑰情境。

- [x] **步驟 3：確認本變更的 delta 規格涵蓋所有接受／拒絕形式**

`openspec/changes/standardize-sftp-username-uri/specs/standard-sftp-userinfo-addresses/spec.md` 必須包含：

- 標準 URI 預填登入（`sftp://root@45.32.49.125/home/linuxuser` → 帳號 `root`、主機 `45.32.49.125`）
- 僅主機 URI 維持相容
- 不推斷反序舊 authority
- 正規位址不含帳號
- 含密碼 URI 被拒絕，且診斷不含密碼
- 空帳號（`sftp://@45.32.49.125/`）被拒絕
- 多個 `@`（`sftp://root@@45.32.49.125/`）被拒絕

- [ ] **步驟 4：提交文件取代**

```
git add docs/superpowers/specs/2026-08-26-sftp-address-login-design.md openspec/changes/add-sftp-address-login/specs/sftp-address-login/spec.md openspec/changes/standardize-sftp-username-uri/specs/standard-sftp-userinfo-addresses/spec.md
git commit -m "docs: supersede reversed sftp username uri examples"
```

---

### 任務 4：聚焦工作區驗證

**檔案：**
- 測試：`crates/explorer-model/src/remote.rs`
- 測試：`crates/explorer-ui/src/lib.rs`
- 驗證：`crates/explorer-app`（沒有解析器本地測試；跑 crate 測試防迴歸）
- 驗證：`openspec validate standardize-sftp-username-uri --strict`

- [x] **步驟 1：explorer-model SFTP 測試**

執行：`cargo test -p explorer-model --lib remote`

預期：PASS。截圖基準：8 項 SFTP 相關測試通過（解析、僅主機、反序不推斷、密碼／畸形拒絕、正規 `RemoteAddress` 拒絕 user-info、設定檔 JSON 無密碼欄、以及同模組相鄰 remote 測試）。

- [x] **步驟 2：網址列背景登入測試**

執行：`cargo test -p explorer-ui --lib -- standard_sftp_username_hint_is_intercepted_and_navigates_to_canonical_host --exact --nocapture`

預期：PASS。登入維持在 GPUI 執行緒外（`begin_address_navigation` 回傳 `None`）。

- [x] **步驟 3：explorer-app 迴歸**

執行：`cargo test -p explorer-app`

預期：PASS，允許既有環境測試略過（截圖基準：119 項通過、1 項既有環境測試略過）。不要新增略過。

- [x] **步驟 4：OpenSpec 嚴格驗證**

執行：`openspec validate standardize-sftp-username-uri --strict`

預期：PASS。

- [x] **步驟 5：使用者視角審查**

對 `sftp://root@45.32.49.125/` 手動檢查：

- 帳號欄預填 `root`
- 連線主機是 `45.32.49.125`
- 既有 `sftp://45.32.49.125/path` 書籤、歷程與已儲存連線仍可開啟
- `sftp://root:password@host/` 經可見的無效位址路徑拒絕
- 輸入舊的 `sftp://45.32.49.125@root/` **不會**連到 `45.32.49.125`

- [x] **步驟 6：任務 3 與診斷斷言完成後重跑解析器與 OpenSpec**

執行：

```
cargo test -p explorer-model --lib -- remote::tests::direct_sftp
cargo test -p explorer-ui --lib -- standard_sftp_username_hint_is_intercepted_and_navigates_to_canonical_host --exact
openspec validate standardize-sftp-username-uri --strict
openspec validate add-sftp-address-login --strict
```

預期：全部 PASS。`add-sftp-address-login` 不得再把 `sftp://45.32.49.125@root/` 當成帳號提示的 WHEN 子句。

---

## 規格涵蓋

| 規格／設計要求 | 任務 |
| --- | --- |
| `sftp://username@host/path` 預填帳號並使用主機 | 任務 1、任務 2 |
| `sftp://host/path` 維持相容 | 任務 1 僅主機測試 |
| 不推斷反序 `host@username` | 任務 1 反序測試 |
| 密碼不得出現在 URI／診斷／持久化 | 任務 1 拒絕 + 步驟 5 診斷斷言 |
| 不含憑證的正規 `sftp://host/path` | 任務 1 正規化 + `RemoteAddress::parse` 仍拒絕 user-info |
| 共用解析器是唯一來源 | 任務 2 呼叫點驗證 |
| 可見的無效位址路徑、不 panic | 任務 1 `InvalidAuthority`；UI／app 已把解析錯誤對應成 `String` |
| 更新被取代的反序文件 | 任務 3 |
| 聚焦測試 + 使用者視角檢查 | 任務 4 |

## 佔位與型別檢查

- 後續任務使用的型別與任務 1 一致：`SftpAddressInput`、`username_hint: Option<String>`、`address.canonical()`、`RemoteAddressError::InvalidAuthority`。
- 沒有 TBD／「處理邊界情況」步驟。空帳號、多餘 `@`、`user:pass@host` 都已具名。
- IPv6 字面值與 URI 內埠號仍由既有 `validate_authority` 拒絕（`:` 不合法）。本計畫不新增它們。
