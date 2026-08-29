# Application-owned popup host contract

Adjustment B-003 replaces the discarded public owner-draw experiment. `immersive_popup.rs` uses documented Win32/GDI calls only and never resolves private `twinui.pcshell.dll` symbols, injects hooks, copies ExplorerPatcher implementation material, or mutates HMENU metadata.

The caller retains ownership of HMENU, IContextMenu/IContextMenu3, canonical verbs, and invocation. The host reads command IDs, state/type flags, strings, submenu handles, and bitmap handles into scoped rows; dynamic children receive `WM_INITMENUPOPUP` before rematerialization. Selection returns the original command ID. Extension-owned owner-draw, invalid, empty, enumeration-failed, and window-creation-failed sessions use unchanged `TrackPopupMenuEx` fallback.

`PopupState` owns only its row vector, font, HWND, capture lifetime, and down/right shadow HWNDs. The popup supports hover, press/release activation, outside click, right-click replacement cancellation, app deactivation, arrows, mnemonic characters, Enter, Escape, and nested submenus. High contrast bypasses custom rendering. Light/dark palettes and per-monitor DPI geometry are evaluated for every popup.

Current verification:

- `cargo test -p explorer-shell-win immersive_popup --lib`: passed.
- `cargo test -p explorer-shell-win context_menu --lib -- --test-threads=1`: 19 passed, 1 installation-gated ignored.
- Headful image `build/context-menu-alpha-size/after-first-right-click.png` proves transparent alpha icons, compact system-menu typography, reserved icon gutter, divider geometry, and live third-party rows.

Hashes at this evidence revision:

- `immersive_popup.rs`: `C270085ECB837E7B5B7F4A7F4395A057D8304F0BDF92394FF7864B9D7450ED9B`
- `context_menu.rs`: `ACE4FD131DAC801A1407E99E2D20D9763D7CCFEC11BE810B0E36E6F48CE48199`
- headful screenshot: `CA929BF1D6FD94E3C5E2A1604A7DB6454697B9F1198C33B9854B0F1F247FB572`
