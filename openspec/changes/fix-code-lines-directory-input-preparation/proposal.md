## Why

Folder Code Lines accepts a directory payload up to the 64 MiB batch limit, but each Host input stream is capped at 8 MiB. Real repositories such as `D:\code\file_explorer` can therefore pass collection and then fail dispatch preparation, while unsupported binaries and repository metadata consume most of the payload.

## What Changes

- Filter directory snapshots to files that tokei recognizes from their relative paths before reading or packing their contents.
- Bound every directory snapshot by the single-stream 8 MiB contract so successful collection is always dispatchable.
- Treat an empty or oversized supported-source snapshot as unsupported without failing other rows in the same batch.
- Isolate canonicalization, filename, and stream-construction preparation failures to the affected row.
- Add regression coverage and a real-folder diagnostic for `D:\code\file_explorer`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `source-example-plugin-suite`: Require official Code Lines directory inputs to contain only recognized source files, remain within the single-stream bound, and isolate preparation failures per row.

## Impact

- `crates/explorer-app/src/application.rs`: directory snapshot collection and batch preparation.
- Official Rust and Lua Code Lines providers remain wire-compatible and receive the existing `SECLDIR1` format.
- No public ABI, permission, MFT admission, or localization changes.
- Focused application tests and real-folder validation gain coverage for binary-heavy repositories and per-row failure isolation.
