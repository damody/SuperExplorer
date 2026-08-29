# `C:\appverifUI.dll` current-profile validation

The user's earlier screenshot was the native `#32768` fallback, not the application-owned
renderer. The persisted tabs had `immersive_native_context_menus: false`, and rebuilding only
`explorer-app` left the separately executed extension worker stale.

The correction rebuilt both `explorer-app` and `explorer-extension-broker`, repaired the current
session envelope with a fresh checksum, enabled the persisted setting for both restored tabs,
and aligned the application-owned popup geometry to the ExplorerPatcher reference.

- Target: `C:\appverifUI.dll`.
- Invocation: physical mouse right-click in a normally restored current profile.
- Observed class: `SuperExplorer.ImmersivePopup.v1` (application-owned), not `#32768`.
- Popup bounds: 494 x 958 physical pixels in the final run. Height is content-dependent; the
  final Shell snapshot exposed an enabled `Paste` row that was absent from the supplied capture.
- Supplied reference width: 495 physical pixels; delta: -1 pixel.
- Logical row height: 23 px before DPI scaling.
- Icon gutter uses transparent alpha bitmaps; no black icon tile was observed.
- Screenshot: `build/appverif-final/appverif-owned-popup.png`.
- Screenshot SHA-256: `A5C470A7BBFC0F260CC7A92580EF2154250EBDECE4E25CEF5BB97A76BB2C6C0C`.
- Machine report: `build/appverif-final/report.json`.
- Report SHA-256: `3CAF2C2992932B3B4C3B880DB76AC1A01DCE75552FD463892EE51E95F25B2769`.

Fresh-profile validation then ran ten replacement cycles with the new default and passed exact
clipboard targeting, process-tree popup ownership, per-cycle identity replacement, dismissal,
multi-selection, input replay protection, one-broker ownership, bounded resources, and
responsiveness. The complete report is
`build/context-menu-fresh-default/report.json`.

Focused validation:

- `cargo test -p explorer-shell-win immersive_popup -- --test-threads=1`: 14 passed.
- `cargo test -p explorer-model immersive_context_menu_setting -- --test-threads=1`: 1 passed.
- `cargo check -p explorer-uitest --bin explorer-session-repair --locked`: passed.
- `cargo build -p explorer-app -p explorer-extension-broker --locked`: passed.
- `cargo fmt --all -- --check`: passed.
- `openspec validate unify-immersive-context-menu-style --strict`: passed.
- `git diff --check`: passed (line-ending notices only).

The still-open multi-DPI, dark-theme, high-contrast, mixed-monitor, and remote capture-matrix tasks
remain explicitly open; this record proves the exact reported Local file and rollout path rather
than claiming those unrelated evidence matrices are complete.
