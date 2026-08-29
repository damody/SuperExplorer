# Installed Shell handler compatibility

Command:

`cargo test -p explorer-shell-win installed_ -- --ignored --test-threads=1 --nocapture`

Result: four installed-handler tests passed on the current Windows profile.

- 7-Zip initialized its nested menu, retained identity through the application-owned popup,
  and created a non-empty archive in an owned temporary fixture.
- WinRAR received `WM_INITMENUPOPUP`, exposed its lazy submenu, retained identity through the
  popup, and created `winrar-safe.rar` only inside an owned temporary fixture.
- TortoiseGit retained identity through the popup, invoked its non-mutating About command,
  and the test closed the resulting dialog.
- VS Code exposed canonical verb `VSCode`, retained identity through the popup, opened the
  uniquely named owned fixture folder, and the test closed only that matching window.

For every presentation, the before/after snapshot compared command ID, submenu handle,
canonical verb, and bitmap presence. No snapshot changed. Cancellation and later installed
handler sessions remained available; the four tests ran serially in one process.
