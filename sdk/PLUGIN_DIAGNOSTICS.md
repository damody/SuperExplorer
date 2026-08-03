# P0 plugin diagnostics

For the public jobs/value/stream/cache API and timing limits, see
[EXTENSION_API_GUIDE.md](EXTENSION_API_GUIDE.md). This guide is author-facing;
host-internal scheduler/cache records remain diagnostic implementation facts.

The validator emits stable JSON containing `schema_version`, `valid`, and sorted diagnostics. Each diagnostic contains a stable `code`, `severity`, `phase`, package-relative `path`, and remediation-oriented `message`; it never records environment variables, secrets, file contents, or user absolute paths.

Important code families are:

- `SESDK-INPUT` / `SESDK-MANIFEST`: missing or unknown manifest structure.
- `SESDK-SDK` / `SESDK-FINGERPRINT`: bundle, target, ABI, or GPUI identity drift.
- `SESDK-ID` / `SESDK-FEATURE` / `SESDK-CAPABILITY`: invalid or unbound identifiers.
- `SESDK-PATH` / `SESDK-PAYLOAD` / `SESDK-HASH`: unsafe, undeclared, missing, or changed content.
- `SESDK-PRIVATE`: invalid private-vendor metadata, checksum, provenance, license, or tree binding.
- `SESDK-EVIDENCE`: unknown, missing, or stale trusted CI/UITEST mapping.

Build and package reports contain only bundle-relative output paths, sizes, and SHA-256 values. A toolchain commit mismatch, unsafe Cargo environment override, offline dependency failure, changed build input, archive collision, or partial staging failure blocks publication and preserves an existing package.

The P0 project manifest's `payloads` list is intentionally restricted to declared
`rust-source` build inputs. Runtime package contents are synthesized separately by
the core tooling as `manifest.json` plus the bound `plugin/plugin.dll` payload.

The wrappers compile only a private, no-reparse snapshot of the consumer input.
They pin actual absolute, non-reparse Cargo and rustc executables from one
SDK-owned 1.97.1 toolchain bin directory, check signed SHA-256 and `-Vv`
release/commit records, seal Cargo, and hold rustc deny-write/delete through
the invocation. The core validator accepts `RUSTC` only when the wrapper sets
that exact trusted rustc path and hash, and rejects caller compiler/linker/Cargo authority overrides (including
`RUSTC_BOOTSTRAP` and `CARGO_INCREMENTAL`), consumer Cargo configuration,
junctions/symlinks, output-root escapes, and post-validation mutation. Package
publication stages all bytes on the destination volume and publishes sidecars before
the `.sepack` completion marker; a failed transaction removes only the new sidecars
and leaves any existing immutable package unchanged.

The trusted evidence IDs are executable gates, not free-form labels. They are
registered in `uitest/manifest.json`, mirrored by named steps in the offline CI
workflow, and must match `sdk/ci/plugin-gates.json` exactly. The local
reproduction command is:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/plugin-tooling-self-test.ps1
```

## Native runtime diagnostics

Native Rust callback incidents and timing records are deliberately separate from
package-validator JSON. They use path-free identities and bounded terminal
classes so a support report does not disclose local paths, marker contents, or
secrets. See [NATIVE_PLUGIN_OPERATIONS.md](NATIVE_PLUGIN_OPERATIONS.md) for the
in-process threat model, Safe Mode confirmation limits, redaction allow/deny
rules, restart semantics, and the operator runbook.

<!-- zh-TW-p0-diagnostics -->
## 繁體中文：P0 診斷與修復

validator 固定輸出含 `schema_version`、`valid` 與排序後 `diagnostics` 的 JSON。每筆診斷只有穩定 `code`、嚴重度、phase、package-relative path 與修復訊息；不得包含環境變數、秘密、檔案內容或使用者絕對路徑。

- `SESDK-INPUT`／`SESDK-MANIFEST`：補齊或移除 manifest 欄位，使其符合 exact schema。
- `SESDK-SDK`／`SESDK-FINGERPRINT`：改用同一個 SDK bundle、target、ABI 與 GPUI snapshot 後重建。
- `SESDK-ID`／`SESDK-FEATURE`／`SESDK-CAPABILITY`：修正 identifier，並將 contribution 綁到已宣告 feature/capability。
- `SESDK-PATH`／`SESDK-PAYLOAD`／`SESDK-HASH`：移除絕對路徑、`..`、大小寫碰撞、symlink/junction/reparse point，重新計算 size/hash。
- `SESDK-PRIVATE`：修正 exact version、patch path、crate/tree checksum、provenance、SPDX 與 license hashes；private tree 必須可離線重現。
- `SESDK-EVIDENCE`：使用 `ci/plugin-gates.json` 已註冊且在 UITEST／CI 可執行的 evidence ID。

build/package report 只記 bundle-relative output、size 與 SHA-256。toolchain commit 不符、Cargo override、離線 dependency 缺失、輸入在驗證後改變、archive collision 或 staging 中斷都會 fail closed。`build.complete.json` 與最後發布的 `.sepack` 分別是 build/package 的完成 marker；sidecar 本身不是完成證明。wrapper 在同一 volume 原子發布並只回收有界、未標記的本次 staging，不刪除既有 immutable package。

Rust callback／Safe Mode 診斷與 package-validator JSON 分離，僅使用有界、無路徑的 package/interface/operation identity。使用者明確確認前不得重啟 callback；詳見 [NATIVE_PLUGIN_OPERATIONS.md 的繁體中文章節](NATIVE_PLUGIN_OPERATIONS.md#繁體中文zh-tw)。
