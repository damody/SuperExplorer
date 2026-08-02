# P0 plugin diagnostics

The validator emits stable JSON containing `schema_version`, `valid`, and sorted diagnostics. Each diagnostic contains a stable `code`, `severity`, `phase`, package-relative `path`, and remediation-oriented `message`; it never records environment variables, secrets, file contents, or user absolute paths.

Important code families are:

- `SESDK-INPUT` / `SESDK-MANIFEST`: missing or unknown manifest structure.
- `SESDK-SDK` / `SESDK-FINGERPRINT`: bundle, target, ABI, or GPUI identity drift.
- `SESDK-ID` / `SESDK-FEATURE` / `SESDK-CAPABILITY`: invalid or unbound identifiers.
- `SESDK-PATH` / `SESDK-PAYLOAD` / `SESDK-HASH`: unsafe, undeclared, missing, or changed content.
- `SESDK-EVIDENCE`: unknown, missing, or stale trusted CI/UITEST mapping.

Build and package reports contain only bundle-relative output paths, sizes, and SHA-256 values. A toolchain commit mismatch, unsafe Cargo environment override, offline dependency failure, changed build input, archive collision, or partial staging failure blocks publication and preserves an existing package.

The trusted evidence IDs are executable gates, not free-form labels. They are
registered in `uitest/manifest.json`, mirrored by named steps in the offline CI
workflow, and must match `sdk/ci/plugin-gates.json` exactly. The local
reproduction command is:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/plugin-tooling-self-test.ps1
```
