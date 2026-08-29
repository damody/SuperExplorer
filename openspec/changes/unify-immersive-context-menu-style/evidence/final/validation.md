# Final scoped validation

Recorded on 2026-08-30 and supplemented after the default-enabled rollout correction.

- `cargo fmt --all -- --check`: passed.
- `cargo test -p explorer-shell-win context_menu -- --test-threads=1 --nocapture`:
  18 passed, 0 failed, 4 installed-handler tests ignored by the default runner. Those four
  were then run explicitly with `--ignored`; all passed.
- `cargo test -p explorer-ui remote_ -- --test-threads=1`: 12 passed, 0 failed.
- `cargo check -p explorer-app`: passed.
- Detailed task validator: L1=7, L2=17, leaves=109, unique IDs=109; every checked leaf had
  a valid task evidence record before this final record update.
- `openspec validate unify-immersive-context-menu-style --strict`: passed.
- `git diff --check`: passed; CRLF conversion notices were warnings, not whitespace errors.

The scoped diff contains the popup host, Shell/broker setting propagation, remote visual
tokens, tests, and headful harnesses. Pre-existing changes under
`openspec/changes/fix-interactive-bookmark-windows-and-paths`, `SuperDesktop`, and unrelated
build/evidence directories were neither reverted nor claimed by this change.

Final source review found no TODO/TBD placeholder, private immersive helper, copied
ExplorerPatcher implementation, dead global circuit, or unresolved P0/P1. The 134 audited
`unsafe` blocks in `immersive_popup.rs` stay inside the documented Win32/GDI boundary and are
covered by module-level safety policy plus focused lifetime tests.
