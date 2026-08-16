# G-BASELINE status

Recorded 2026-08-14 on Windows, repository commit `fd66c9e8a759ca903b55b1e9ea156c42d37518e1` with the working-tree implementation under review.

- The prerequisite change `independent-cache-budgets-telemetry-webp` reports `68/68` and `all_done` through `openspec instructions apply`.
- Icon ownership is in `explorer-shell-win` (provider/disk codec), `explorer-ui` (memory/GPU presentation caches), and Host-owned settings/telemetry. Thumbnail ownership follows the same boundary but uses independent memory, disk, and GPU limits.
- Production derived-data roots are selected independently by the Shell disk-cache constructors; entries are private `.bc7cache` data and are not a public interchange contract.
- Public plugin/provider contracts remain RGBA-capable. The compressed descriptor is an internal GPUI/SuperExplorer seam.
- `cargo build --release --locked --offline -p explorer-app --bins` passed.
- Release hashes: `SuperExplorer.exe` `132A37AAF9B3144A3B0F564A500035F60BE3ABD32B345CA0D003DEB052859FB7`; MFT service `84D5CF929A21226821C086321C852C884AEB4D72580D942C3439C7FBE2D677C4`; MFT helper `F1CED9ABEC7987B03B47DFBB264449FCD441ECECE96596625D00C2AF1F3382B9`; `Cargo.lock` `88712FD906212125103720CCE48D8AEA58DD57FEA1207B20D0EECB71E674FE3F`.

G-BASELINE remains blocked only on task 1.1.4: no frozen pre-change CPU working-set, upload-byte, frame-time, and cold/warm latency capture exists. Current diagnostic latency is recorded under G-PERF without being relabelled as a historical baseline.
