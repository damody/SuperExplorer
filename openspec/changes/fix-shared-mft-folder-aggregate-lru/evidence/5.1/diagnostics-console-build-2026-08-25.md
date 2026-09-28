# Test diagnostics console and matched build

Timestamp: `2026-08-25T00:29:32+08:00`

Focused verification:

- `scripts/test_installer_build_handoff.lua`: PASS.
- `cargo check -p explorer-app --bins --release`: PASS.
- `cargo build -p explorer-app --release --bin SuperExplorer --bin superexplorer-mft-service`: PASS.
- `build_test_install.bat --skip-build --no-launch`: PASS; NSIS output confirms `TEST_INSTALL`, `APP_ARGS=--diagnostics-console ...`, finish-page parameters, and both shortcut parameters.

Interactive `build_test_install.bat` runs now keep their command window open with `pause`; CI and argument-driven checks remain non-blocking. Test installs allocate a client diagnostics console for the application lifetime and print the selected persistent `error.log` path. Formal combined installers do not define `TEST_INSTALL` and keep the normal window-subsystem launch.

Artifacts:

- `target/release/SuperExplorer.exe`: 28,735,488 bytes, SHA-256 `8D1D15ED54B00E531D9FA02915A5E94EE8E690C78AA779B44D1E98BC8B6FF225`.
- `target/release/superexplorer-mft-service.exe`: 3,048,960 bytes, SHA-256 `45F157EEAF7B48E95CAA2575F18370A803247693A76D7E51DFEF3DF95854F34B`.
- `dist/SuperExplorer-Test-Setup-1.2026.8.24-x64.exe`: 9,611,662 bytes, SHA-256 `DAFB936279E1DFCF523DFF5189E9550519556A6FC7DE76270CDC27BCD5C151BF`.

Installed acceptance is intentionally not claimed here. The elevated NSIS window was launched and remained at its interactive welcome page; Windows UIPI correctly prevented the non-elevated verification process from driving the elevated button. Installed hashes therefore remained the previous matched build pending completion of the visible installer.
