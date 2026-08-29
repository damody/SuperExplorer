# Post-validation remote interaction rerun

Recorded on 2026-08-30 after correcting global Escape routing for open remote and bookmark menus.

- ADB headful interaction matrix: passed.
- SFTP headful interaction matrix: passed.
- Both runs verified hover, pressed, Escape, outside-click dismissal, right-click replacement, exactly-once pointer dispatch, UIA focus, Enter activation, `Menu`/`MenuItem` accessible names and roles, and application-edge clamping.
- Both provisional Create Folder editors were cancelled; no remote object was committed.
- `cargo fmt --all -- --check`: passed.
- `cargo test -p explorer-ui remote_ -- --test-threads=1`: 12 passed, 0 failed.
- `cargo check -p explorer-app --locked`: passed.
- `openspec validate unify-immersive-context-menu-style --strict`: passed.
- Detailed OpenSpec progress: 98/109 leaves complete, with the remaining 11 exclusively in real visual/DPI evidence gates and their dependent traceability closure.
- `git diff --check`: passed with line-ending conversion warnings only.

The earlier full Shell compatibility and soak suite is still current because this post-validation product change is confined to `explorer-ui` action routing; no Shell implementation changed afterward.
