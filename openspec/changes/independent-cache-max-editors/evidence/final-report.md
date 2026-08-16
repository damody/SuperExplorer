# Independent cache budgets validation report

## Passed gates

- G-CONTRACT / G-MIGRATION: the versioned 14-budget model, bounds, 24 MB stop and session migration tests pass.
- G-EDITOR / G-COMMIT: headful evidence verifies all editors and sliders, Apply 2048, OK/restart 4096, and Cancel preserving 4096.
- G-RUNTIME / G-DISK: independent memory, GPU, Host and disk owners have bounded enforcement tests.
- G-MFT-IPC / G-MFT-TRIM / G-PARTIAL: versioned configuration, reconnect, five independent stores, atomic persisted pruning, and typed partial tests pass.
- G-AUTO: formatting, diff checks, targeted suites and the registered headful UITEST pass.
- G-INSTALL (build): `build_test_install.bat --no-launch` exits 0. Latest installer SHA-256 is `ED6C9988E7886D07F5223DC420CEC37DC35D560FF7F9D6EEB34D8D146C6A7350`.
- G-FINAL (spec): strict OpenSpec validation exits 0.

## Current installed validation and remaining risk

- Installed editor evidence proves Apply 2048 without navigation, OK/restart 4096, Cancel preservation, and representative UI/Host/GPU/disk settings.
- The current 2026-08-14 installer/app/service identities are recorded in `evidence-lineage-review.md`.
- The bounded inaccessible-subtree fixture exposed a real Details projection defect: a partial value with a diagnostic was discarded before the `Partial: <size>` renderer. The fix in `crates/explorer-ui/src/chrome.rs` passes the focused folder-size suite (9 tests) and the release candidate now visibly renders both Size Map partial state and `Folder size: Partial: 1.0 KB`; see `release-partial-visible-20260814/report.json` and its screenshots.
- Task 5.2.5 remains open only at the installed-build boundary. Replacing `C:\Program Files\SuperExplorer\SuperExplorer.exe` with the verified release candidate was denied by the current non-elevated token, so the release result is not mislabeled as installed evidence. Release candidate SHA-256: `BF3298DC4C26F0085EDCC54ED7066F072232EC9BC6E77A773E50ED95A3016DC3`.
