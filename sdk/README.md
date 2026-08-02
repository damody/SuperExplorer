# SuperExplorer SDK toolchain

The SDK is pinned to Rust `1.97.1` for `x86_64-pc-windows-msvc`; the exact
compiler and Cargo commit hashes are part of the toolchain contract. Run the
contract test from the repository root:

```powershell
powershell -NoProfile -File sdk/tests/toolchain-contract.ps1
```

The production entrypoint has no override or bypass parameters. Run synthetic
negative coverage separately:

```powershell
powershell -NoProfile -File sdk/tests/toolchain-contract-self-test.ps1
```

The GPUI baseline contract verifies the authorized local remote, full parent
gitlink/tree identity, and the resolved production feature graph:

```powershell
powershell -NoProfile -File sdk/tests/gpui-baseline-contract.ps1
```

GPUI snapshot updates use the protected two-stage
`.github/workflows/update-gpui-snapshot.yml` workflow. The first job resolves and
hashes an immutable update plan. The protected-environment job verifies that
plan and creates the runtime approval; non-fast-forward changes require the
exact expiring approval bound to the run nonce and candidate-plan digest. The
workflow produces reviewable promotion artifacts and does not directly publish
the protected SDK snapshot.

A materialized consumer project can be validated, built, and packaged only from
the SDK contract and offline vendor tree. The checked-in P0 fixture is a
placeholder template exercised by `sdk/tests/plugin-tooling-self-test.ps1`.

The automated `clean-readme-reproduction` gate runs this exact documented
reproduction command against a fresh materialized fixture:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/plugin-tooling-self-test.ps1
```

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot C:\path\to\plugin
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot C:\path\to\plugin
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot C:\path\to\plugin
```

Release freeze metadata is deliberately fail-closed. A production freeze
requires a protected annotated tag, a trusted Git signing keyring, detached
artifact provenance, and the immutable prior-release ledger. The canonical
`sdk/snapshot/release-freeze.json` must not be created from fixture evidence.
Use `sdk/scripts/freeze-release.ps1` only in that protected release context;
local coverage belongs in `sdk/tests/release-freeze-contract.ps1`.
The versioned [release policy](ci/release-policy.json) fixes the provider and
policy ID; signer name, primary fingerprint, keyring hash, protection-record
hash, builder, and predicate are injected only by the `sdk-release-freeze`
protected environment in `.github/workflows/freeze-gpui-release.yml`. The
freeze script compares every caller value and evidence hash against those
protected values before verifying the annotated tag and detached bundle.

## Extension API ABI contract

The extension root uses schema namespace/version `0x5345_0001`. SDK major
version 1.0 remains binary-compatible with 1.1: the latter adds only the
optional `describe_contract` registrar tail, which old plugins omit.

Plugins must construct registration callbacks with
`RegistrarCallbackV1::new`, the only safe trampoline. Fabricating the ABI
function pointer is raw unsafe and may abort the process; callback panics are
translated to the typed `Panicked` error.

Run the isolated, offline contract driver from the repository root:

```powershell
powershell -NoProfile -File sdk/tests/extension-api-abi-contract.ps1
```

The versioned `.sepack` manifest parser has deterministic multi-content,
canonical publisher/contact, payload-kind, GPUI fingerprint, and negative
fixtures. Verified publisher identity is intentionally opaque; end-to-end signer
identity coverage begins with the cryptographic verifier in task 2.5.
Validate it in an empty Cargo home with no network:

```powershell
powershell -NoProfile -File sdk/tests/package-manifest-v1-contract.ps1
```

Package content, target, path-containment, hash/size, and trust-store validation
fixtures run in an isolated offline Cargo home:

```powershell
powershell -NoProfile -File sdk/tests/package-validation-v1-contract.ps1
```
The fixture verifies fail-closed unsigned rejection; local-developer authorization
is intentionally host-source-issued and is covered by host integration tests.

## Package lifecycle

The versioned `.sepack` manifest, validation-to-resolution trust chain,
dependency semantics, bounds, and lifecycle scope are documented in
[PACKAGE_LIFECYCLE.md](PACKAGE_LIFECYCLE.md) (English and 繁體中文). Run its
offline resolver contract with:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/package-resolution-v1-contract.ps1
```
