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
