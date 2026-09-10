# Import Explorer Tabs And Win+E

**Goal:** First SuperExplorer start recreates every open File Explorer window and every tab; afterwards Win+E opens a new SuperExplorer window on top.

**Decisions:** One Explorer window → one SuperExplorer process/window. Import all tabs via IShellWindows + IShellBrowser (cycle with WM_COMMAND 0xA221 if needed). Close CabinetWClass windows after success, never kill explorer.exe. Win+E → This PC. Hook owned by exclusive mutex.

**Files:**
- `crates/explorer-shell-win/src/explorer_tabs.rs` — enumerate + normalize
- `crates/explorer-app/src/explorer_import.rs` — spawn siblings, close Explorers, restore tabs
- `crates/explorer-app/src/win_e_hotkey.rs` — LL hook
- Wire `main.rs`, `application.rs`, `lib.rs`
