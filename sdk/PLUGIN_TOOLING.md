# P0 Rust plugin tooling

The three entry points accept only `-PluginRoot`. SDK paths, bundle identity, `x86_64-pc-windows-msvc`, release policy, locked/offline mode, and output locations are fixed by the SDK.

1. Materialize `plugin-project.json` from the shipped template using the current `sdk-lock.json` bundle ID, ABI schema, and `ui-abi-fingerprint.json` fingerprint.
2. Run `validate-plugin.ps1`; it invokes the exact Rust manifest and payload validator in `sdk/tools/plugin-tooling`.
3. Run `build-plugin.ps1`; it verifies the pinned toolchain, validates first, builds with an empty Cargo home and isolated target directory, then atomically publishes the DLL and build report under `target/superexplorer/<bundle-id>`.
4. Run `package-plugin.ps1`; it never rebuilds, revalidates the manifest and build hashes, creates a fixed-order/fixed-time `.sepack`, reopens and hashes every entry, and refuses to overwrite a different package.

The template placeholders are intentional: embedding the generated bundle ID in an inventoried SDK source file would create a self-referential bundle hash. A consumer copy must replace every placeholder before validation; the validator rejects unresolved or stale identities.

P0 accepts only Rust ABI-root/GPUI contributions and the payload kinds listed in `p0-manifest.schema.json`. Lua, Skin, bundled tools, signing inputs, arbitrary commands/environment, output overrides, and skip flags fail closed until the full package parser is delivered in Task 2.3.

Rust `build.rs` and proc macros execute native code. These scripts are reproducibility tools, not a sandbox. Official builds must run in an ephemeral guest with networking disabled and no secrets.
