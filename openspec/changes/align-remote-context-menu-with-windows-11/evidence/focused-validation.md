# Focused validation — 2026-08-29

## Automated results

- `cargo test -p explorer-ui remote_`: PASS — 10 passed, 0 failed.
- `cargo build -p explorer-app --locked`: PASS as part of the ADB headful attempt.
- `openspec validate align-remote-context-menu-with-windows-11 --strict`: PASS before apply; repeated at final review.

## Headful disposition

The existing `scripts/smoke_remote_background_context.ps1` harness was attempted against:

- `adb://emulator-5554/sdcard/Download`
- `sftp://45.32.49.125/home/linuxuser`

The ADB target navigated successfully in the first attempts, proving that the built application and provider path were available. The harness could not deliver its synthetic right-button gesture to the file-view background: application logs showed no `ShowContextMenu` dispatch, and UI Automation therefore could not discover `Remote file context menu`. Later retries also showed focus/address-navigation instability. The SFTP attempt hit the same harness navigation/focus instability before menu discovery.

No provider mutation was performed. Product-side command composition, icon/text grouping, placement, accessibility activation, inside-event ownership, and local-Shell isolation remain covered by focused compiled tests. The headful result is recorded as `environment-unavailable` rather than a product pass; no screenshot is claimed as successful evidence.

## Requirement traceability

- Windows 11 command strip and grouped text rows: `remote_menu_commands`, `remote_menu_command_strip_button`, `remote_menu_text_command`.
- Shared item/background surface and semantic states: `remote_context_menu` plus the reusable command renderers.
- Provider-aware dispatch: unchanged `ExplorerAction` callback path.
- Placement and dismissal: `remote_context_menu_position`, overlay handlers, menu-surface propagation stops, and existing Escape reducer.
- Local Shell isolation: unchanged `custom_virtual_parent` branch; focused source-contract test asserts both paths remain present.
