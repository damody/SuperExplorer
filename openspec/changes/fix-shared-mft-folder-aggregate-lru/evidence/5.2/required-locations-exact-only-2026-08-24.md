# Required-location exact-only acceptance — 2026-08-24

## Procedure

The newly built exact-only client queried four immediate child directories at each required location, waited until ten seconds from the start of that location, and queried those children again. The client executable was:

`D:\SuperExplorer\target\release\superexplorer-mft-service.exe --query-folder <child>`

## Actual result

The currently installed service returned typed partial aggregates for all twelve sampled children. The new client rejected every partial response with exit code 1 and a detailed message containing path, generation, logical bytes, allocated bytes, file count, and directory count. No partial numeric result is eligible for UI publication.

- `D:\`: 0 exact of 4 after ten seconds.
- `D:\SuperExplorer`: 0 exact of 4 after ten seconds.
- `D:\UE_5.7`: 0 exact of 4 after ten seconds.

All second queries terminated in 13–24 ms, so this is an installed-service exactness failure rather than an indefinite-loading failure.

Installed diagnostics reported volume `D` as `exact=false`, observed generation `1247`, durable generation `0`, service mode `2`, and recovery code `7`. The running installed service binary is older than the matched release artifact:

- Installed: `C:\Program Files\SuperExplorer\superexplorer-mft-service.exe`, SHA-256 `996C51A05BF2D7D1E9F5C5C95B1BA1BFF081C32836B0B6CC2B08F420BD0E1ABE`.
- Built: `D:\SuperExplorer\target\release\superexplorer-mft-service.exe`, SHA-256 `3A2929D0DC8EF230D5A2C1BDED39764749473878E1E7F12D98B860F81A1EDAAB`.

The current medium-integrity terminal cannot replace or restart the LocalSystem service, so installed acceptance is not claimed. Installing and restarting the matched service is the remaining deployment gate; source-level exact-only, terminal-state, diagnostics, LRU, scheduling, and telemetry checks pass.
