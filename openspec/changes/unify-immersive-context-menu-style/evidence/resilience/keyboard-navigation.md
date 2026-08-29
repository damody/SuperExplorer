# Keyboard navigation and focus lifetime

The controlled Win32 modal-loop test
`keyboard_arrows_enter_and_nested_submenu_dispatch_through_the_real_modal_loop` posts real Down,
Right, Down, and Enter messages. It enters a nested `HMENU`, returns command 313 through the
entire popup chain, and proves that a selected child command is not overwritten by a queued
`WM_CANCELMODE`. The fix restores parent capture/focus only when a child is cancelled.

`mnemonic_matching_is_unique_case_insensitive_and_skips_disabled_rows` verifies accelerators,
including case folding, duplicates, and disabled rows. The 1,000-cycle modal test delivers Escape
through the same message loop. The retained headful built-in and replacement sessions verify that
the application remains responsive and receives the next pointer/keyboard interaction after
selection or dismissal.

Command:

`cargo test -p explorer-shell-win immersive_popup -- --test-threads=1`

Result: 12 passed, 0 failed, including the 1,000-cycle resource test.

The post-fix headful built-in command run also passed Copy, Cut, shortcut creation, rename,
file/folder/multi/executable/script Properties, fifteen placement targets, ten Properties
close/reopen cycles, Delete, selection retention, one-broker lifetime, and bounded resources.
Its report is `build/context-menu-final-scroll-fix/report.json` with SHA-256
`8EB8EC7120C0C517439B6C58ED3EC37DF2D7DB3E0FC77806407BBAFB65E1161C`.

Menus taller than the active monitor work area now use a bounded viewport. Mouse-wheel input
scrolls by three rows, Up/Down keeps the selected row visible, hit testing uses content
coordinates, child submenus anchor to the visible parent row, and the popup reserves right/bottom
space for its soft shadow. The post-fix headful run reached low commands such as Properties and
Delete through this viewport without clipping or changing command identity.
