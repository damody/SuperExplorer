# MFT SQLite foreground persistence release report

## Candidate identity

- Installed service: `C:\Program Files\SuperExplorer\superexplorer-mft-service.exe`
- Service account/startup: `LocalSystem`, automatic, running
- Installed and release service SHA-256: `f13b4aac7f09f6420ea06c0cac0d210f75b12ab68a5443af0b356d3072bc5672`
- Installed and release application SHA-256: `a1a0eb2c8443ba4c8872e58142afe8887a580194c71d328a5d55242018934ecb`
- Installer SHA-256: `1eb6bf237cc887a1e420c011797e987817ae3c73c3e64c264001fe8aeaabcbfc`
- Version: `0.1.0`; environment: Windows 11 `10.0.26200`

The installed hashes equal the release artifacts. SQLite is bundled into the service and no external SQLite runtime is installed.

## Functional disposition

- Live USN ingestion remains memory-first.
- Durable SQLite writes require an authenticated foreground lease and the ten-minute attempt/success clocks.
- The active cache contains nine fixed SQLite members across C, D, and E and no legacy generation files.
- Folder-size queries remain service-owned. A budget-partial live topology may use the complete durable SQLite metadata when its durable and observed cursors match; it never reads user file contents recursively.
- Directory rows are excluded from the code-lines provider, preventing navigation from recursively opening a source tree and triggering Defender.
- Incomplete/unproven folder-size state is reported as partial/calculating rather than exact zero.
- Switching volumes gives the foreground volume first claim on the fixed live-memory budget and releases background topology with an O(1) snapshot swap.
- The legacy 512 MiB topology default migrates to 1024 MiB; both SQLite admission and direct MFT scanning use structure-derived per-entry accounting instead of the former 1024-byte row guess.
- Large exact folders that exceed the interactive subtree-walk bound build one in-memory volume aggregate and retain O(1) answers inside the independent aggregate budget; they no longer degrade to `Partial: 0 KB`.

The final installed candidate completed its first foreground ten-minute D-drive rebuild and atomically promoted the SQLite snapshot. Installed queries then returned `D:\code` as 236,490,954,612 logical bytes (515,415 files, 74,686 directories), `D:\SuperExplorer` as 116,314,655,258 logical bytes, and the controlled fixture as 2,080 bytes, all with `partial=false`. The fixture's recursive file sum independently matched 2,080 bytes.

## Defender and I/O evidence

The pre-fix D-drive navigation trace attributed the large recursive read set to SuperExplorer's code-lines directory measurement, not to the MFT service. After directory rows were excluded, a ten-second installed D-drive observation recorded zero SuperExplorer read MiB, zero MFT-service read MiB, and zero Defender CPU/read delta in that interval. The final ten-second idle observation after exact queries for `C:\Users`, `C:\Program Files`, and `D:\code` recorded zero service and Defender read/write delta.

The immediately preceding candidate's 1,201.232-second unfocused trace recorded 600 representative mutations, no MFT cache file events, a stable six-member active SQLite file set, 1,368 service read bytes, 10,944 service write bytes, zero Defender read bytes, and 181,280 Defender write bytes as environmental telemetry. It is retained as comparative evidence but is not counted for the final cursor/catch-up source fingerprint. Working-set size is never used as evidence of scanning.

## Verification

- MFT service unit/integration/fault tests: 94 passed; the opt-in elevated real-D scan test passed in 15.43 seconds and the final installed-canonical catch-up probe passed with 2,077 coalesced changes.
- Explorer app suites: 158, 20, 93, 2, 1, 9, 1, 1, and 2 passed with zero failures.
- Extension API: 32 passed; extension host: 296 passed; extension broker: 31 passed, all with zero failures.
- Tokei installed headful smoke: passed.
- `cargo fmt --all -- --check`: passed.
- Relevant locked package check: passed.
- OpenSpec strict and detailed validation: passed (66 detailed tasks recognized).
- Evidence-index validation: passed for the currently checked automated evidence leaves.

The repository-wide warnings-as-errors Clippy invocation remains non-countable because pre-existing shared extension/application targets produce numerous unrelated lint failures. This task remains unchecked until a scoped accepted-lint policy or repository cleanup passes honestly.

## Rollback

The installer stops and verifies the service before replacing or deleting its binary, and preserves `%ProgramData%\SuperExplorer\MftIndex`. An older build ignores the SQLite store and may rebuild its legacy MFT cache. Rollback does not reinterpret SQLite files as legacy sidecars and does not require stop-time cache deletion. Failed migration or replacement retains a typed recovery disposition and never admits an unverified store as exact.

## Approval disposition

Pending the exact-candidate installed foreground/background, lifecycle, reboot, and packaging evidence leaves listed in tasks 4.2.4 and 5.2.1–5.2.7. No unchecked task is represented as passed.
