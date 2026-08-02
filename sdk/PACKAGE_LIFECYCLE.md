# Extension package lifecycle / 擴充套件生命週期

## English

`manifest.json` is the versioned, data-only declaration inside one `.sepack`.
It declares package/publisher identity, SDK compatibility, Rust/Lua/Skin/
locale/tool components, features, dependencies, payload hashes, signature, and
plugin-owned `data_version`. Every top-level field is required; empty arrays
must be explicit. Unknown fields and non-normalized IDs are rejected.

The trust chain is `source.discover` → `DiscoveredPackageV1::validate`
(`PackageValidatorV1`) → sealed `PackageValidationResultV1` →
`PackageResolverV1` → `resolved_packages` → `activation_guard` → a future
loader → host `validate_root`/`register_root`. The resolver accepts only sealed
validation results. `parse_json` is deliberately not a proof of payload hash,
signature, reparse-point, target, or dependency resolution. Unsigned packages
are accepted only through opaque local-developer provenance; ordinary callers
cannot mint it.

The resolver chooses at most one SemVer version per package ID, requires a
closed acyclic required-dependency graph, reports stable diagnostics, and
returns an atomic registration set. Its fixed bounds are 128 candidates, 512
total dependency edges (including optional edges), and 65,536 search states. `activation_guard` is not a loader;
no UI contribution surface is available in this API yet.
Missing/incompatible optional dependencies do not block their owner; an
optional edge that would create a cycle is omitted and diagnosed.

No resolver output executes a DLL, Lua script, tool, or renderer. Loading,
feature state, runtime draining, and Safe Mode are separate later host stages.

## 繁體中文

`manifest.json` 是單一 `.sepack` 內具版本、純資料的宣告。它描述套件／發行者
識別、SDK 相容性、Rust／Lua／Skin／locale／tool 元件、feature、相依性、payload
hash、簽章及外掛自有的 `data_version`。所有頂層欄位都必須出現；空陣列也必須明確
寫出。未知欄位與未正規化 ID 都會被拒絕。

信任鏈是 `source.discover` → `DiscoveredPackageV1::validate`
（`PackageValidatorV1`）→ 封存的 `PackageValidationResultV1` →
`PackageResolverV1` → `resolved_packages` → `activation_guard` → 未來的 loader →
host `validate_root`／`register_root`。resolver 只接受封存的 validation result。
`parse_json` 刻意不證明 payload hash、簽章、reparse point、target 或 dependency
resolution；unsigned package 只可經由不透明的 local-developer provenance 接受，一般
呼叫端無法自行建立它。

每個 package ID 最多選一個 SemVer 版本；required dependency 必須完整且無 cycle，並
產生穩定診斷與原子化 registration set。固定上限為 128 candidates、512 條 dependency
edge（包含 optional edge）、65,536 search states。`activation_guard` 不是 loader；目前 API 也尚未提供 UI
contribution surface。遺失或不相容的 optional dependency 不會阻擋 owner；若 optional
edge 會形成 cycle，會略過該 edge 並記錄診斷。

resolver 的輸出不會執行 DLL、Lua、tool 或 renderer。載入、feature 狀態、runtime
drain 與 Safe Mode 是後續 host 階段。

## Source, validation, and activation APIs / 來源、驗證與啟用 API

`PackageSourceV1` is the replaceable discovery boundary. The shipped
`BuiltInPackageSourceV1` and `LocalDeveloperPackageSourceV1` inspect direct
child package directories only, reject reparse-point roots/candidates, and cap
one scan at 1,024 direct children. Local-developer provenance is minted inside
the host source adapter; it is not a public unsigned-package bypass.
`EntitlementProviderV1` is a separate replaceable policy boundary. This SDK
does not link or ship a Steamworks/store implementation.

`PackageValidationBudgetV1` and `PackageValidationCancellationV1` bound source
walking, hashing, sealing, deadlines, and cancellation. A successful validation
publishes one immutable sealed generation. Failed, cancelled, or expired work
does not publish a partial generation; staging cleanup preserves active owners.
Before native activation, call `activation_guard()` or
`activation_guard_with_budget()` to revalidate and hold the sealed bytes. The
guard still does not load a DLL or invoke a registrar.

`PackageSourceV1` 是可替換的探索邊界。內建的 `BuiltInPackageSourceV1` 與
`LocalDeveloperPackageSourceV1` 只掃描套件根目錄的直接子目錄，拒絕 reparse-point
根目錄與候選，且每次最多掃描 1,024 個直接子項。local-developer provenance 只能由
host 內的 source adapter 建立，不能當成公開的 unsigned-package 繞過方式。
`EntitlementProviderV1` 是另一個可替換的政策邊界；SDK 不連結也不提供 Steamworks
或商店實作。

`PackageValidationBudgetV1` 與 `PackageValidationCancellationV1` 限制目錄走訪、hash、
封存、deadline 與取消。成功時只發布一個不可變 sealed generation；失敗、取消或逾時
不會留下部分發布，staging cleanup 也必須保留仍由 active owner 使用的世代。native
啟用前須呼叫 `activation_guard()` 或 `activation_guard_with_budget()` 重新驗證並持有
sealed bytes；guard 本身仍不載入 DLL，也不呼叫 registrar。

## Verification / 驗證

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/package-resolution-v1-contract.ps1
```

The example at `sdk/fixtures/package-resolution-v1/example-manifest.json` is
documentation only; validation fixtures must be signed and sealed by the host.

## V1 manifest schema reference / V1 Manifest 結構參考

All fields below are required unless the type explicitly says `null` is
allowed. `deny_unknown_fields` applies at every object level; arrays are not
defaulted and must be present even when empty. 所有欄位皆為必要，除非型別明確允許
`null`；每個物件都拒絕未知欄位，陣列不可省略，即使沒有項目也必須寫成 `[]`。

| Top-level field / 頂層欄位 | JSON type | Meaning / 意義 |
| --- | --- | --- |
| `manifest_version` | `u32`, exactly `1` | Schema revision / 結構版本。 |
| `package` | `PackageIdentityV1` | `id`, `version` / 套件識別與版本。 |
| `publisher` | `PublisherV1` | Public accountable publisher / 可公開追責的發行者。 |
| `sdk` | `SdkCompatibilityV1` | Bundle, target and ABI declaration / bundle、target、ABI 宣告。 |
| `rust`, `lua`, `skins`, `locales`, `tools`, `features`, `dependencies`, `payloads` | arrays | Explicit component inventories; use `[]` when none / 明確清單，無項目時使用 `[]`。 |
| `signature` | `SignatureV1` | `unsigned` or `ed25519` declaration / 未簽章或 Ed25519 宣告。 |
| `data_version` | `u64` | Plugin-owned data/cache generation / 外掛自有資料與快取世代。 |

| Object / 物件 | Required fields and types / 必要欄位與型別 |
| --- | --- |
| `package` | `id: string`, `version: string`。 |
| `publisher` | `id: string`, nonblank `display_name: string`, `contacts: PublisherContactV1[]`。 |
| `publisher.contacts[]` | `kind` enum, `value: string`, non-empty `purposes: ContactPurposeV1[]`。Kinds: `email`, `website`, `support_forum`, `github_issues`, `discord_server`, `discord_user`, `qq_group`, `other`; purposes: `support`, `security`, `community`。至少一個聯絡方式必須有 `support` 或 `security`。 |
| `sdk` | `bundle_id: string`, `target: string`, `abi_schema: u32`, `gpui: bool`, `ui_abi_fingerprint: string|null`。`gpui=true` 時 fingerprint 必填且為 hash；`false` 時必須為 `null`。 |
| `rust[]` | `id`, `entrypoint`, `root_module` strings; `sdk_major: u16`。 |
| `lua[]`, `skins[]` | each has `id: string`, `entrypoint: string`。 |
| `locales[]` | `locale: string`, `path: string`, `sha256: string`。 |
| `tools[]` | `id`, `target`, `path`, `version`, `sha256`, `source` strings; `size: u64`; `output_protocol` enum `json|text|line_delimited_json`; `license_paths: string[]`。 |
| `features[]` | `id: string`, `capabilities: string[]`, `dependencies: string[]`。 |
| `dependencies[]` | `package_id: string`, `version_requirement: string` (SemVer requirement), `optional: bool`。 |
| `payloads[]` | `path: string`, `size: u64`, `sha256: string`, `kind` enum `rust_dll|lua_script|skin_asset|locale|tool|license|notice|data`。 |
| `signature` | tagged `kind`: `{ "kind":"unsigned" }` or `{ "kind":"ed25519", "key_id": string, "signature": string }`。 |

### Bounds, normalization, and errors / 限制、正規化與錯誤

`manifest.json` is at most 256 KiB. IDs use lowercase ASCII
`[a-z0-9][a-z0-9._-]{0,63}`. Display name ≤256 bytes; contact/source ≤2048;
version/target ≤128; path ≤1024; locale ≤64; signature ≤1024. Each top-level
array ≤128; contacts ≤32; contact purposes ≤8; tool licences ≤32; feature
capabilities and feature dependencies ≤64. IDs are unique within every
inventory, dependency package IDs and payload paths are unique, and SHA-256 is
exactly 64 lowercase hexadecimal characters. Manifest 最大為 256 KiB；ID、字串和
陣列上限如上，重複 ID／路徑或非小寫 64 位 SHA-256 都會被拒絕。

`parse_json` returns typed JSON, version, length, identifier, duplicate,
contact-policy, GPUI-fingerprint, and hash-format errors. It neither reads a
payload nor proves its hash/signature/path/target/dependency safety. Discovery
returns source/provenance/reparse/bounded-scan errors; validation returns trust,
signature, containment, digest, target, cancellation and sealing errors; the
resolver returns deterministic blocked-package diagnostics. `parse_json` 僅做
結構驗證；來源、驗證、解析階段各自回傳其型別錯誤，不能把 parse 成功當作已信任或
可載入。
