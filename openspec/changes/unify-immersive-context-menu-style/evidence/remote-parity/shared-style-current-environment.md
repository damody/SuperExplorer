# Shared Local/ADB/SFTP menu style — current environment

Recorded on 2026-08-30 after replacing duplicated Local and remote metrics with
`WINDOWS_CONTEXT_MENU_VISUAL_METRICS` plus the matching light/dark palettes in
`explorer-model`.

## Shared contract

- Logical row height: 23 px.
- Logical font size: 15 px.
- Logical minimum/maximum width: 282/520 px.
- Text/icon gutter: 42 px; 16 px icon at logical x=13.
- Divider: logical x=42 through width minus 8 px.
- Vertical outer padding: 3 px; no extra horizontal padding.
- Light surface/divider/text/hover: `#F9F9F9`, `#D7D7D7`, `#1A1A1A`, `#E9E9E9`.
- Dark surface/divider/text/hover: `#2B2B2B`, `#484848`, `#F2F2F2`, `#3D3D3D`.
- Local Win32 and remote GPUI renderers consume the same provider-neutral metrics and palettes.

## Headful results

Environment: Windows 10.0.26200.0, light theme, active 175% display scale. The ADB emulator
`emulator-5554` and saved SFTP profile `production` were exercised through physical right-clicks.

| Provider | Variant | Result | PNG dimensions | SHA-256 |
|---|---|---|---:|---|
| ADB | background | passed | 493×54 | `96DEA5FC1B44E1EF2D6B3619449EFD70B5DFA66FD8C34D41684266F5C1202CAC` |
| SFTP | background | passed | 493×54 | `96DEA5FC1B44E1EF2D6B3619449EFD70B5DFA66FD8C34D41684266F5C1202CAC` |
| ADB | folder item | passed | 493×416 | `484A154AC9A66C8ECDA7FC18C25323A9CBFDB14C9BDE6EB6C77930D5E013D6D7` |
| SFTP | folder item | passed | 493×416 | `484A154AC9A66C8ECDA7FC18C25323A9CBFDB14C9BDE6EB6C77930D5E013D6D7` |

The byte-identical provider pairs prove that ADB and SFTP render the same pixels for matching
command membership. Item menus retain the required blank icon gutter for rows without icons.
The Local `appverifUI.dll` popup measured 494 px wide in the same environment; the remote GPUI
crop is 493 px because its one-pixel border is represented inside the logical width.

Artifacts:

- `build/remote-parity-final/adb-aligned/remote-background-context-menu.png`
- `build/remote-parity-final/adb-item-only/remote-item-context-menu.png`
- `build/remote-parity-final/sftp-tmp/remote-background-context-menu.png`
- `build/remote-parity-final/sftp-item-only/remote-item-context-menu.png`

The ADB and SFTP interaction reports passed hover, pressed, Escape dismissal, outside-click
dismissal, right-click replacement, single dispatch, keyboard focus/Enter, accessible menu/item
roles, and edge clamping. No remote object was committed; provisional folder creation remained
inside inline rename and was cancelled.

This record covers the active light-theme/DPI environment only. The still-open all-theme/all-DPI
capture matrix remains unchecked and is not implied complete by this evidence.
