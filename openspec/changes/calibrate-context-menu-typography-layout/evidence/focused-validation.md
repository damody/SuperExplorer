# Focused validation

Validated on 2026-08-30 after the implementation was complete.

## Automated checks

- `cargo test -p explorer-model context_menu`: 5 passed, 0 failed.
- `cargo test -p explorer-ui remote_context_menu`: 5 passed, 0 failed.
- `cargo check -p explorer-shell-win`: passed.

## Headful remote checks

- ADB item menu at `adb://emulator-5554/sdcard/Download`: passed. Screenshot: `build/context-menu-typography-adb/remote-item-context-menu.png`.
- SFTP item menu at `sftp://45.32.49.125/home/linuxuser`: passed. Screenshot: `build/context-menu-typography-sftp/remote-item-context-menu.png`.

Both captures render the same Microsoft JhengHei UI menu typography, 12 logical-pixel font size, 16 logical-pixel line height, normal weight, 23 logical-pixel row height, and unchanged icon/text gutters. The SFTP run reused the existing saved profile; no credentials are recorded in this evidence.
