# Independent cache budgets validation report

## Passed gates

- G-CONTRACT / G-MIGRATION: the versioned 14-budget model, bounds, 24 MB stop and session migration tests pass.
- G-EDITOR / G-COMMIT: headful evidence verifies all editors and sliders, Apply 2048, OK/restart 4096, and Cancel preserving 4096.
- G-RUNTIME / G-DISK: independent memory, GPU, Host and disk owners have bounded enforcement tests.
- G-MFT-IPC / G-MFT-TRIM / G-PARTIAL: versioned configuration, reconnect, five independent stores, atomic persisted pruning, and typed partial tests pass.
- G-AUTO: formatting, diff checks, targeted suites and the registered headful UITEST pass.
- G-INSTALL (build): `build_test_install.bat --no-launch` exits 0. Latest installer SHA-256 is `ED6C9988E7886D07F5223DC420CEC37DC35D560FF7F9D6EEB34D8D146C6A7350`.
- G-FINAL (spec): strict OpenSpec validation exits 0.

## Remaining privileged evidence

The latest installer was not elevated and installed during this run. Tasks 5.2.3 through 5.2.5 and the final traceability closure remain open until the newest package is installed and the installed app/service screenshots are captured. Existing installed-build evidence applies to the preceding package hash and is retained without being misrepresented as current.
