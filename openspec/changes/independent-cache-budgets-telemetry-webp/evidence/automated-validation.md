# Automated validation

Validation date: 2026-08-14 (Asia/Taipei)

| Gate | Command / evidence | Result |
|---|---|---|
| G-SETTINGS | `cargo test -p explorer-model session --lib` | PASS, 12 tests including current/prior golden fixtures and cache-budget round trip |
| G-MEMORY-LRU | `cargo test -p explorer-jobs --lib` | PASS, 33 tests including byte-cost LRU, reduction, oversized entry, promotion, cancellation, and bounded replacement |
| G-WEBP-STORAGE replacement | `cargo test -p explorer-shell-win icon_disk_cache --lib` | PASS, 9 current BC7 disk-cache tests; WebP disposition is in `supersession.md` |
| G-MFT-DIAGNOSTICS | `cargo test -p explorer-app mft_query::tests --lib` | PASS, 9 tests including fixed/path-free frames, malformed frames, limits, and local pipe security |
| G-HOST-REPORTERS | `cargo test -p explorer-app cache --lib` | PASS, 13 tests |
| G-FOLDER-OPTIONS | `cargo test -p explorer-ui folder_options --lib` | PASS, 13 tests |
| Affected compile | `cargo check -p explorer-model -p explorer-jobs -p explorer-shell-win -p explorer-app -p explorer-ui` | PASS; existing warnings remain non-blocking |
| Patch integrity | `git diff --check` | PASS |
| OpenSpec | `openspec validate independent-cache-budgets-telemetry-webp --strict` | PASS |

The MFT named-pipe implementation centralizes the protected ACL as `D:P(A;;GA;;;SY)(A;;GRGW;;;IU)` and sets message mode plus `PIPE_REJECT_REMOTE_CLIENTS`; the test asserts both properties and ensures no anonymous/network/broad-authenticated grant is introduced.

`cargo fmt --all -- --check` initially identified only the newly added MFT security assertion layout. That line was corrected and the final validation rerun is recorded in `final-review.md`.
