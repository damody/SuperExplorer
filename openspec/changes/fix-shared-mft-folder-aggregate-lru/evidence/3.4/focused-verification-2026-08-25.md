# Completion-order MFT batch verification — 2026-08-25

## Implemented behavior

- One Client request carries up to 256 absolute folder identities.
- Every item has a local request ID; the Service streams exact results or bounded detailed errors in completion order and finishes with an end frame.
- One Client dispatcher preserves visible order and publishes each completion through the existing per-item UI result channel.
- The Service runs at most four aggregate queries per volume across all pipe connections.
- Existing generation-bound LRU and same-key single-flight remain shared by every Client and batch.
- A failed/incomplete canonical store no longer delays foreground exact-memory recovery behind quarantine or persistence cadence.
- Full MFT enumeration treats `ERROR_HANDLE_EOF` as successful completion; partial numeric results are never published as exact.

## Focused automated checks

- Batch codec bounds, duplicate request IDs, truncated items, cross-volume mismatch, bounded UTF-8 detail, request identity, and end frame: 2 passed.
- Local named-pipe compatibility plus synthetic `fast -> medium -> slow` completion order, per-item failure isolation, and parallelism: 1 passed.
- Host visible-order/current-generation batching: 1 passed.
- Host cancelled-generation retirement: 1 passed.
- Service per-volume concurrency cap: 1 passed; observed maximum exactly 4.
- Service exact-only LRU, live-generation rejection, and active-volume exact wait: 3 passed.
- Release `cargo check` for the application library and MFT Service: passed.
- Full workspace tests were intentionally not run, per user request.

## Installed acceptance

Installer: `SuperExplorer-Test-Setup-1.2026.8.25-x64.exe`

- Installer SHA-256: `FFF5236868C5DA464814EA697D335066324AE72AACC55337D8FB16BE44850157`
- App SHA-256: `B18B6B98FF526A2D6BC3156D1526A4D13196C52207B3E5825D3583EEAF735881`
- MFT Service SHA-256: `AD662E18EFA3EC49E85C82862E6573FB9075AA0EF32590BE6C8D657D9A885C09`
- Silent installer exit: 0.
- Installed App and Service hashes matched the release artifacts.
- `SuperExplorerMft` state after installation: `RUNNING`.

After an explicit Service restart, the real installed-Service batch test returned:

| Root | First exact child | Exact children | Batch terminal |
|---|---:|---:|---|
| `D:\` | 3707 ms | 20 | complete |
| `D:\SuperExplorer` | 268 ms | 23 | complete |
| `D:\UE_5.7` | 266 ms | 7 | complete |

All returned aggregates were exact (`partial == false`). The complete three-root acceptance took 8.24 seconds total.

## Intentionally remaining broader change tasks

This focused delivery does not claim the unrelated full OpenSpec matrix is complete. Screenshot capture, two-process `D:\trace` pressure evidence, explicit mid-stream disconnect injection, and the broader final archive gates remain unchecked in `tasks.md`.
