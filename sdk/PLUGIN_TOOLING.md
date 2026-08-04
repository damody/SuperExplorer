# P0 Rust plugin tooling

The three entry points accept only `-PluginRoot`. SDK paths, bundle identity, `x86_64-pc-windows-msvc`, release policy, locked/offline mode, and output locations are fixed by the SDK.

1. Materialize `plugin-project.json` from the shipped template using the current `sdk-lock.json` bundle ID, ABI schema, and `ui-abi-fingerprint.json` fingerprint.
2. Run `validate-plugin.ps1`; it invokes the exact Rust manifest and payload validator in `sdk/tools/plugin-tooling`.
3. Run `build-plugin.ps1`; it verifies the pinned toolchain, validates first, builds with an empty Cargo home and isolated target directory, then publishes `build/plugin.dll` and `reports/build.json` under `target/superexplorer/<bundle-id>`. Consumers must require the final `reports/build.complete.json` marker: it binds the immutable report and the private `inputs.consumer_tree_sha256` snapshot. A later wrapper startup boundedly removes interrupted `.build-stage-*` attempts and unmarked build payloads before retrying; it never overwrites a marked generation.
4. Run `package-plugin.ps1`; it never rebuilds, revalidates the manifest and build hashes, recomputes the live bounded source digest and requires it to equal the marked build snapshot, asks the core `stage-package` command for the exact runtime inventory, then creates a fixed-order/fixed-time store-only `.sepack`, reopens and hashes every entry, and stages the package, checksum, and report before publishing them together. The `.sepack` is the final complete-publication marker; stale sidecars without that marker and stale `.stage-*` attempts are boundedly removed on the next startup, while a marked package is never overwritten.

The wrapper self-test then feeds the produced `.sepack` into the production
`LocalDeveloperPackageStoreV1` importer, validator, resolver, and
`NativeExtensionLifecycleV1` loader/registrar through the ignored host
integration test
`script_produced_sepack_reaches_production_native_lifecycle`. This is a real
host admission gate, not a ZIP-only fixture check; the test also confirms the
transient import source is removed after admission and that exactly one root and
one declared feature are admitted.

Before staging any bytes, `stage-package` applies the same V1 producer bounds as
the host: at most 128 runtime payloads, 1,024 UTF-8 bytes per archive path, a
256 KiB `manifest.json`, and a 512 MiB store-only ZIP including each local and
central-directory header plus the end record. A package that the host would
reject is not staged or published.

Private Rust dependencies are permitted only as direct exact-version registry
dependencies patched through `[patch.crates-io]` to an exact
`vendor/private/<crate-version>` path. Declare the matching
`private_dependencies` record in `plugin-project.json`, including its vendor
tree SHA-256, crates.io checksum, SPDX license expression, and hashes for each
declared license file. The wrapper snapshots this ignored local source cache before Cargo or
package synthesis; undeclared, changed, reparse-point, oversized, or
non-canonical private trees are rejected offline.

The template placeholders are intentional: embedding the generated bundle ID in an inventoried SDK source file would create a self-referential bundle hash. A consumer copy must replace every placeholder before validation; the validator rejects unresolved or stale identities.

P0 accepts only Rust ABI-root/GPUI contributions and the payload kinds listed in `p0-manifest.schema.json`. Lua, Skin, bundled tools, signing inputs, arbitrary commands/environment, output overrides, and skip flags fail closed until the full package parser is delivered in Task 2.3.

The wrappers resolve the OS-profile installation
`~/.rustup/toolchains/1.97.1-x86_64-pc-windows-msvc/bin` directly. They do not
consult caller `PATH`, `rustup.exe`, `RUSTUP_HOME`, or a rustup shim. Before any
Cargo operation they require the actual `cargo.exe` and `rustc.exe` to share that
one non-reparse `bin` directory, match the signed `sdk-lock.json` SHA-256 and
`-Vv` release/commit records, seal an identical Cargo copy, and set `RUSTC` to
the verified absolute rustc path. The rustc file remains deny-write/delete
opened while Cargo runs.

Rust `build.rs` and proc macros execute native code. So do the MSVC linker and
Windows SDK tools required by the pinned Windows target. These are trusted build
prerequisites, not a sandbox; official builds must run in an ephemeral guest
with networking disabled and no secrets.

## Evidence and CI gate IDs

The P0 gate mapping in `ci/plugin-gates.json` is executable: the same IDs are
registered in the UITEST manifest and named as steps in
`.github/workflows/sdk-offline-windows.yml`.

| ID | Executable proof |
| --- | --- |
| `plugin-root-unit` | Offline `cargo test` for `sdk/tools/plugin-tooling` |
| `plugin-load-compatible` | Isolated offline ABI host/plugin fixture |
| `plugin-tooling-wrapper-contract` | `plugin-tooling-self-test.ps1` wrapper and atomic-publication contract |
| `ui-fingerprint-mismatch-rejected` | UI ABI fingerprint mismatch rejection contract |
| `clean-readme-reproduction` | Documented validate/build/package fixture flow |

Run the complete local wrapper gate with
`powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/plugin-tooling-self-test.ps1`.

<!-- zh-TW-p0-tooling -->
## 繁體中文：P0 Rust 外掛工具流程

三個入口都只接受 `-PluginRoot`；SDK 路徑、bundle ID、Windows MSVC target、release profile、`--locked --offline` 與輸出位置皆由 SDK 固定。

1. 以目前的 `sdk-lock.json`、ABI schema 與 UI fingerprint 套用 `plugin-project.json` 範本。
2. 執行 `validate-plugin.ps1`，先驗證 manifest、payload、private dependency provenance 與 CI／UITEST evidence mapping。
3. 執行 `build-plugin.ps1`。它只編譯 bounded、無 reparse point 的私有 snapshot，鎖定 Rust 1.97.1 的實體 Cargo/rustc 路徑、hash 與 commit，並在空 Cargo home、隔離 target、禁止網路下建置。只有最後的 `reports/build.complete.json` 才代表 generation 完成；marker 會綁定 build report 與 `consumer_tree_sha256`。中斷後只清除未標記的 staging，不覆寫已完成 generation。
4. 執行 `package-plugin.ps1`。它不會偷偷重建，而是重新驗證 live tree、完成 marker、DLL 與 report，再由 core 產生唯一 runtime inventory。wrapper 以固定順序／時間的 store-only ZIP 建立 `.sepack`，重開逐項驗證後才發布 sidecars，最後發布 `.sepack` completion marker；失敗會回復本次 staging，既有 package 不受影響。

Rust `build.rs`、proc macro、MSVC linker 與 Windows SDK 工具會執行 native code，因此這是「可信建置」而不是 sandbox。正式 CI 必須使用無秘密、停用網路的 ephemeral guest；caller 的 `PATH`、rustup shim、Cargo config、`RUSTC_BOOTSTRAP`、`CARGO_INCREMENTAL` 與 compiler/linker override 都不構成 authority。

P0 gate 對應不是自由文字：`plugin-root-unit`、`plugin-load-compatible`、`plugin-tooling-wrapper-contract`、`ui-fingerprint-mismatch-rejected` 與 `clean-readme-reproduction` 必須同時存在於 `ci/plugin-gates.json`、UITEST manifest 與 offline CI named steps。完整本機命令為：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/plugin-tooling-self-test.ps1
```
