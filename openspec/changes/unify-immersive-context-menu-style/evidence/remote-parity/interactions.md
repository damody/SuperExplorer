# ADB and SFTP interaction evidence

Captured on 2026-08-30 with `scripts/smoke_remote_background_context.ps1 -InteractionMatrix` against the built SuperExplorer application.

## Results

| Provider | Location | Pointer states | Dismissal and replacement | Dispatch | Keyboard and accessibility | Edge clamp |
|---|---|---|---|---|---|---|
| ADB | `adb://emulator-5554/sdcard/Download` | hover and pressed captured | Escape, outside click, and second right click passed | exactly one provisional Create Folder editor, then cancelled | `Remote file context menu`; command role `MenuItem`; focus plus Enter passed | menu rectangle remained inside the application work area |
| SFTP | `sftp://production/root` | hover and pressed captured | Escape, outside click, and second right click passed | exactly one provisional Create Folder editor, then cancelled | `Remote file context menu`; command role `MenuItem`; focus plus Enter passed | menu rectangle remained inside the application work area |

The first SFTP `/` attempt landed on an occupied item row, correctly producing an item menu. The final background-menu matrix therefore used the existing `/root` location, whose empty viewport area was already established by the earlier capture. No remote object was committed: both pointer and keyboard dispatch checks cancelled the provisional rename editor.

## Product correction found by the matrix

The initial Escape check exposed that the global `CancelScrollbarDrag` action consumed Escape before an open remote or bookmark menu could dismiss. `explorer-ui` now closes those menus first. The successful reruns prove the fixed route in the real application, rather than only in a model test.

## Artifacts

- ADB report: `build/remote-context-adb-interactions5/report.json` (SHA-256 `412F24CAF8D0B188D49EF05E2BCFAEF80CC2A45F210AA7D29485025178C2E996`)
- SFTP report: `build/remote-context-sftp-interactions3/report.json` (SHA-256 `FC8A0A77FEDAB2474FEC2F3F836FC05CB9CEA9475C3231ECD1D0DCA20A2766C1`)
- Each artifact directory also contains the base, hover, and pressed PNG captures.
- Harness SHA-256: `8463A506A8C0C4CF0EEC5B9A718A5D46EAFE24ADDE8C19327A568E9817906645`.
- Escape-routing implementation SHA-256 (`crates/explorer-ui/src/lib.rs`): `F20DCFF4C4712078DFFEC4CD71755F76B7D0F82564C32676BC65FC1EF64EE3D6`.

## Shared-style rerun

After Local and remote menus were moved onto the same provider-neutral metric and palette
contract, the interaction matrix passed again at
`adb://emulator-5554/sdcard/Download` and `sftp://production/tmp`. The corresponding ADB/SFTP
background PNGs are byte-identical, as are the ADB/SFTP folder-item PNGs. See
`shared-style-current-environment.md` for dimensions, hashes, and the explicit matrix limitation.
